// SPDX-License-Identifier: GPL-3.0-only

//! 每日交战、列阵、士气崩溃和撤退。
//!
//! 将领、地形、渡河、火炮后排、堡垒区域和国家骑兵比例都还没有独立系统。
//! 这些入口保留中性输入，不用另一套固定公式代替已经支持的步骑规则。

use std::collections::{BTreeSet, VecDeque};

use crate::combat::{
    CombatantRef, DELIBERATE_RETREAT_PENALTY_MILLI, FRONT_SCALAR, LOW_MORALE_THRESHOLD,
    MoraleStrike, NEUTRAL_ABILITY, NEUTRAL_DISCIPLINE, PhaseKind, RETREAT_LOCK_DAYS,
    SHATTERED_CLEAR_RELATIVE, StrengthStrike, cavalry_support_penalty, center_out,
    daily_morale_loss, effective_tactics, enemy_strength_blocks_retreat, far_retreat_score,
    flank_distance, front_ratio_loser, initial_overrun_loser, line_plan, morale_damage,
    recovered_morale, regiment_sort_score, strength_damage, strength_quarter,
    strength_weighted_morale, thousandths, trunc_div, trunc_mul, winner_morale_floor,
    wipe_refund_men, wipes_after_defeat,
};
use crate::{
    ArmyMovement, Battle, BattleOutcome, BattleReport, GameDate, Money, MovementError,
    REGIMENT_STRENGTH, ReportedSide, SaveError, UNSET_MORALE, UnitKind,
};

use super::World;

#[derive(Clone)]
struct ListedRegiment {
    unit: CombatantRef,
    kind: UnitKind,
    score: u128,
    order: u32,
}

enum RetreatPlan {
    NoDestination,
    RouteFailed,
    Ready(ArmyMovement),
}

impl World {
    pub fn battles(&self) -> &std::collections::BTreeMap<u32, Battle> {
        &self.military.battles
    }

    pub fn battle(&self, id: u32) -> Option<&Battle> {
        self.military.battles.get(&id)
    }

    pub fn battle_in_province(&self, province: u32) -> Option<&Battle> {
        self.military
            .battles
            .values()
            .find(|battle| battle.province() == province)
    }

    pub fn battle_report(&self, province: u32) -> Option<&BattleReport> {
        self.military.battle_reports.get(&province)
    }

    /// 省份上正在进行或刚刚结束的战斗说明。没有战斗时为空。
    pub fn province_combat_status(&self, province: u32) -> String {
        if let Some(battle) = self.battle_in_province(province) {
            return self.ongoing_text(battle);
        }
        self.battle_report(province)
            .map(|report| self.report_text(report))
            .unwrap_or_default()
    }

    /// 这支军队的士气、交战或撤退说明。
    pub fn army_combat_status(&self, army_id: u32) -> String {
        let Some(army) = self.armies().get(&army_id) else {
            return String::new();
        };
        let max = self.max_morale();
        let mut lines = vec![format!(
            "士气：{}",
            format_morale(army.average_morale(max), max)
        )];
        if army.battle_id().is_some() {
            lines.push(self.province_combat_status(army.province));
        } else if army.is_retreating() {
            lines.push("正在撤退，到达前不能接受命令，也不会交战".to_owned());
            if let Some(report) = self.report_for_army(army_id) {
                lines.push(self.report_text(report));
            }
        } else if let Some(report) = self.report_for_army(army_id) {
            lines.push(self.report_text(report));
        }
        lines.join("\n")
    }

    pub(crate) fn recover_monthly_morale(&mut self) {
        let max = self.max_morale();
        let ids: Vec<u32> = self.military.armies.keys().copied().collect();
        for id in ids {
            let army = &self.military.armies[&id];
            if army.battle_id().is_some() {
                continue;
            }
            let province = army.province;
            let country = army.country.clone();
            let rate = self.recovery_rate(&country, province);
            self.recover_army(id, max, rate);
        }
    }

    /// 停在当前省、而且 203/204 仍在时补的那一次恢复。正常走到终点没有这次。
    pub(super) fn recover_army_now(&mut self, id: u32) {
        let Some(army) = self.military.armies.get(&id) else {
            return;
        };
        if army.battle_id().is_some() {
            return;
        }
        let province = army.province;
        let country = army.country.clone();
        let rate = self.recovery_rate(&country, province);
        self.recover_army(id, self.max_morale(), rate);
    }

    /// 进入本国控制区且相对士气达到 990/1000 时，先清 204 再清 203。
    /// 余下路段改按普通速度重算。友方通行控制尚未单独实现，目前只认本国控制者。
    pub(super) fn maybe_clear_shattered(&mut self, id: u32) {
        let Some(army) = self.military.armies.get(&id) else {
            return;
        };
        if !army.retreating {
            return;
        }
        let max = self.max_morale();
        let country = army.country.clone();
        let province = army.province;
        let at_home = self
            .province(province)
            .is_some_and(|province| province.controller.as_deref() == Some(country.as_str()));
        if !at_home {
            return;
        }
        let weighted = self.army_weighted_morale(id, max);
        let relative = u64::try_from(weighted.max(0))
            .unwrap_or(0)
            .saturating_mul(1_000)
            / u64::from(max);
        if relative >= SHATTERED_CLEAR_RELATIVE {
            {
                let army = self
                    .military
                    .armies
                    .get_mut(&id)
                    .expect("解除溃退的军队存在");
                army.retreat_shattered = Some(false);
                army.retreating = false;
            }
            self.retime_remaining(id);
        }
    }

    /// 204 清除后，还没走完的路段不再沿用出发时的 1.5 倍速度。
    fn retime_remaining(&mut self, id: u32) {
        let Some(army) = self.military.armies.get(&id) else {
            return;
        };
        let Some(movement) = army.movement.clone() else {
            return;
        };
        if movement.leg_index >= movement.legs.len() {
            return;
        }
        let speed = army.march_speed(&self.military_rules);
        let mut started = movement.legs[movement.leg_index].started_on;
        let mut updated = movement;
        for leg in &mut updated.legs[updated.leg_index..] {
            let distance = self.land_routes[&leg.from][&leg.to];
            let days = crate::MilitaryRules::travel_days(distance, speed);
            let arrives_on = started
                .checked_add_days(days)
                .expect("余路抵达日在历法范围内");
            leg.started_on = started;
            leg.arrives_on = arrives_on;
            started = arrives_on;
        }
        self.military
            .armies
            .get_mut(&id)
            .expect("重算余路的军队存在")
            .movement = Some(updated);
    }

    /// 抵达或停在敌军所在省份时接战。只锁定参战军队，开场十倍留到当天初始化。
    pub(super) fn engage_province(&mut self, province: u32, date: GameDate, arriver: Option<u32>) {
        self.engage(province, date, arriver, false);
    }

    /// 满十二个战斗日后脱离。先找好退路再离场；没有退路就留在战斗里。
    /// 同侧其他军队按退出兵力占比承担士气，写入待结算，离开者自己不承担。
    pub fn retreat_from_battle(
        &mut self,
        country: &str,
        id: u32,
        date: GameDate,
    ) -> Result<(), MovementError> {
        self.country(country).ok_or(MovementError::UnknownCountry)?;
        let army = self.armies().get(&id).ok_or(MovementError::UnknownArmy)?;
        if army.country != country {
            return Err(MovementError::NotPlayerArmy);
        }
        if army.is_retreating() {
            return Err(MovementError::Retreating);
        }
        let Some(battle_id) = army.battle_id() else {
            return Err(MovementError::NotInBattle);
        };
        if self.military.battles[&battle_id].elapsed_days < RETREAT_LOCK_DAYS {
            return Err(MovementError::RetreatLocked);
        }
        let RetreatPlan::Ready(movement) = self.plan_retreat(id, date) else {
            return Err(MovementError::NoRetreatRoute);
        };
        self.apply_deliberate_penalty(battle_id, id);
        self.remove_from_battle(battle_id, id);
        let army = self
            .military
            .armies
            .get_mut(&id)
            .expect("主动撤退的军队存在");
        army.battle_id = None;
        army.movement = Some(movement);
        army.retreating = true;
        army.retreat_shattered = Some(true);
        Ok(())
    }

    pub(crate) fn resolve_battles(&mut self, date: GameDate) {
        // 按军队编号遍历。当天轮到的这支军队是进攻方，原驻敌军是防守方。
        let army_ids: Vec<u32> = self.military.armies.keys().copied().collect();
        for id in army_ids {
            let Some(army) = self.military.armies.get(&id) else {
                continue;
            };
            if army.battle_id().is_some()
                || army.is_retreating()
                || army.is_exiled()
                || army.strength() == 0
            {
                continue;
            }
            let province = army.province;
            self.engage(province, date, Some(id), false);
        }
        // 先记下今天要处理的战斗。结束时新开的续战不在这张表里，下一日才初始化和结算。
        let today: Vec<u32> = self
            .military
            .battles
            .iter()
            .filter(|(_, battle)| battle.last_resolved != Some(date))
            .map(|(id, _)| *id)
            .collect();
        for id in &today {
            if self
                .military
                .battles
                .get(id)
                .is_some_and(|battle| battle.elapsed_days == 0)
            {
                self.overrun(*id, date);
            }
        }
        for id in today {
            self.resolve_existing(id, date);
        }
    }

    pub(super) fn validate_battles(&self, date: GameDate) -> Result<(), SaveError> {
        let max = self.max_morale();
        let mut seen = BTreeSet::new();
        for (&id, battle) in &self.military.battles {
            if id == 0
                || id != battle.id()
                || !seen.insert(id)
                || !self.active_provinces.contains(&battle.province())
                || battle.started_on() > date
                || battle
                    .last_resolved
                    .is_some_and(|resolved| resolved > date || resolved < battle.started_on())
                || battle.rolls.iter().any(|roll| u32::from(*roll) >= 10)
            {
                return Err(SaveError::InvalidMilitary);
            }
            let mut listed = BTreeSet::new();
            for side in 0..2 {
                if battle.side_armies(side).is_empty() {
                    return Err(SaveError::InvalidMilitary);
                }
                for &army_id in battle.side_armies(side) {
                    let Some(army) = self.armies().get(&army_id) else {
                        return Err(SaveError::InvalidMilitary);
                    };
                    if !listed.insert(army_id)
                        || army.battle_id() != Some(id)
                        || army.province != battle.province()
                        || army.is_retreating()
                        || army.movement.is_some()
                    {
                        return Err(SaveError::InvalidMilitary);
                    }
                }
            }
        }
        for army in self.armies().values() {
            if army.is_retreating() && (army.movement.is_none() || army.battle_id().is_some()) {
                return Err(SaveError::InvalidMilitary);
            }
            if army
                .retreat_previous
                .is_some_and(|province| !self.active_provinces.contains(&province))
                || army
                    .retreat_return
                    .is_some_and(|province| !self.active_provinces.contains(&province))
            {
                return Err(SaveError::InvalidMilitary);
            }
            if let Some(battle_id) = army.battle_id() {
                let listed = self.military.battles.get(&battle_id).is_some_and(|battle| {
                    (0..2).any(|side| battle.side_armies(side).contains(&army.id()))
                });
                if !listed {
                    return Err(SaveError::InvalidMilitary);
                }
            }
            if army
                .regiments
                .iter()
                .any(|regiment| regiment.morale != UNSET_MORALE && regiment.morale > max)
            {
                return Err(SaveError::InvalidMilitary);
            }
        }
        for report in self.military.battle_reports.values() {
            if report.battle_id == 0
                || report.on > date
                || !self.active_provinces.contains(&report.province)
                || report.outcome.loser().is_some_and(|loser| loser > 1)
            {
                return Err(SaveError::InvalidMilitary);
            }
        }
        let highest = self
            .military
            .battles
            .keys()
            .chain(
                self.military
                    .battle_reports
                    .values()
                    .map(|report| &report.battle_id),
            )
            .max()
            .copied()
            .unwrap_or(0);
        if self.military.next_battle_id == 0 || self.military.next_battle_id <= highest {
            return Err(SaveError::InvalidMilitary);
        }
        Ok(())
    }

    pub(crate) fn fill_unset_morale(&mut self) {
        let max = self.max_morale();
        for army in self.military.armies.values_mut() {
            for regiment in &mut army.regiments {
                if regiment.morale == UNSET_MORALE {
                    regiment.morale = max;
                }
            }
        }
    }

    fn max_morale(&self) -> u32 {
        self.military_rules
            .combat
            .morale_milli()
            .expect("陆战规则已检查")
    }

    fn recovery_rate(&self, country: &str, province: u32) -> i64 {
        let rules = &self.military_rules.combat;
        let mut rate = thousandths(rules.recovery);
        if self
            .province(province)
            .is_some_and(|province| province.controller.as_deref() == Some(country))
        {
            rate += thousandths(rules.recovery_own_territory);
        }
        rate
    }

    fn recover_army(&mut self, id: u32, max: u32, rate: i64) {
        let Some(army) = self.military.armies.get_mut(&id) else {
            return;
        };
        for regiment in &mut army.regiments {
            let current = i64::from(regiment.effective_morale(max));
            let next = recovered_morale(current, i64::from(max), 1_000, rate);
            regiment.morale = u32::try_from(next).unwrap_or(max).min(max);
        }
    }

    fn army_weighted_morale(&self, id: u32, max: u32) -> i64 {
        let Some(army) = self.armies().get(&id) else {
            return 0;
        };
        let pairs: Vec<(u32, u32)> = army
            .regiments
            .iter()
            .map(|regiment| (regiment.strength, regiment.effective_morale(max)))
            .collect();
        strength_weighted_morale(&pairs)
    }

    /// 野战仍不按堡垒反转攻守。无要塞占领另行判断首都和建筑要塞。
    fn effective_fort_level(&self, province: u32) -> u32 {
        let _ = self.province(province);
        0
    }

    fn engage(&mut self, province: u32, date: GameDate, arriver: Option<u32>, defer: bool) {
        if let Some(id) = self.battle_in_province(province).map(Battle::id) {
            self.admit(id);
            return;
        }
        let Some(arriver) = arriver else {
            return;
        };
        self.try_start(province, date, arriver, defer);
    }

    /// 接战只建战斗并锁住军队。十倍覆灭要等当天军队都更新完，在初始化里做。
    fn try_start(
        &mut self,
        province: u32,
        date: GameDate,
        arriver: u32,
        defer: bool,
    ) -> Option<u32> {
        let (defenders, attackers) = self.arrival_pair(province, arriver)?;
        if defenders.is_empty() || attackers.is_empty() || self.military.next_battle_id == u32::MAX
        {
            return None;
        }
        let id = self.military.next_battle_id;
        self.military.next_battle_id += 1;
        let mut battle = Battle::new(id, province, date);
        battle.side_mut(0).armies = defenders;
        battle.side_mut(1).armies = attackers;
        self.military.battles.insert(id, battle);
        self.lock_side(id, 0);
        self.lock_side(id, 1);
        self.admit(id);
        if defer && let Some(battle) = self.military.battles.get_mut(&id) {
            // 当天结束战斗后新开的续战，下一日才初始化和结算。
            battle.last_resolved = Some(date);
        }
        Some(id)
    }

    /// 新到的军队是进攻方。同省原驻、且与它交战的军队是防守方。
    fn arrival_pair(&self, province: u32, arriver: u32) -> Option<(Vec<u32>, Vec<u32>)> {
        let country = self.armies().get(&arriver)?.country.clone();
        let present = self.present_armies(province);
        let mut defenders: Vec<u32> = present
            .iter()
            .copied()
            .filter(|&id| id != arriver && self.at_war(&country, &self.armies()[&id].country))
            .collect();
        let mut attackers: Vec<u32> = present
            .iter()
            .copied()
            .filter(|&id| self.armies()[&id].country == country)
            .collect();
        if !attackers.contains(&arriver) && self.armies().contains_key(&arriver) {
            attackers.push(arriver);
        }
        if defenders.is_empty() || attackers.is_empty() {
            return None;
        }
        if self.effective_fort_level(province) > 0 {
            let controller = self
                .province(province)
                .and_then(|province| province.controller.clone());
            let reverse = controller.as_deref().is_some_and(|controller| {
                defenders.iter().any(|id| {
                    self.armies()
                        .get(id)
                        .is_some_and(|army| self.at_war(&army.country, controller))
                })
            });
            if reverse {
                std::mem::swap(&mut defenders, &mut attackers);
            }
        }
        Some((defenders, attackers))
    }

    /// 行军中的敌军也可以被卷入。撤退、已参战和没有兵力的军队除外。
    fn present_armies(&self, province: u32) -> Vec<u32> {
        self.military
            .armies
            .values()
            .filter(|army| {
                army.province == province
                    && !army.is_retreating()
                    && !army.is_exiled()
                    && army.battle_id().is_none()
                    && army.strength() > 0
            })
            .map(|army| army.id())
            .collect()
    }

    fn lock_side(&mut self, battle_id: u32, side: usize) {
        let armies = self.military.battles[&battle_id].side_armies(side).to_vec();
        let mut joined = 0_u64;
        for army_id in armies {
            let Some(army) = self.military.armies.get_mut(&army_id) else {
                continue;
            };
            joined += army.strength();
            army.battle_id = Some(battle_id);
            army.movement = None;
        }
        self.military
            .battles
            .get_mut(&battle_id)
            .expect("战斗存在")
            .side_mut(side)
            .joined_strength += joined;
    }

    fn admit(&mut self, battle_id: u32) {
        if !self.military.battles.contains_key(&battle_id) {
            return;
        }
        let province = self.military.battles[&battle_id].province();
        let candidates: Vec<u32> = self
            .military
            .armies
            .values()
            .filter(|army| {
                army.province == province
                    && !army.is_retreating()
                    && !army.is_exiled()
                    && army.battle_id().is_none()
                    && army.strength() > 0
            })
            .map(|army| army.id())
            .collect();
        for army_id in candidates {
            let country = self.armies()[&army_id].country.clone();
            let Some(side) = self.reinforcement_side(battle_id, &country) else {
                continue;
            };
            let joined = self.armies()[&army_id].strength();
            {
                let battle = self.military.battles.get_mut(&battle_id).expect("战斗存在");
                let battle_side = battle.side_mut(side);
                battle_side.armies.push(army_id);
                battle_side.joined_strength += joined;
            }
            let army = self
                .military
                .armies
                .get_mut(&army_id)
                .expect("增援军队存在");
            army.battle_id = Some(battle_id);
            army.movement = None;
        }
    }

    /// 同国军队加入已有一侧。否则必须与恰好一侧的全部军队交战。
    fn reinforcement_side(&self, battle_id: u32, country: &str) -> Option<usize> {
        let battle = &self.military.battles[&battle_id];
        let same = |side: usize| {
            battle.side_armies(side).iter().any(|id| {
                self.armies()
                    .get(id)
                    .is_some_and(|army| army.country == country)
            })
        };
        if same(0) {
            return Some(0);
        }
        if same(1) {
            return Some(1);
        }
        let hostile_all = |side: usize| {
            let armies = battle.side_armies(side);
            !armies.is_empty()
                && armies.iter().all(|id| {
                    self.armies()
                        .get(id)
                        .is_some_and(|army| self.at_war(country, &army.country))
                })
        };
        match (hostile_all(0), hostile_all(1)) {
            (true, false) => Some(1),
            (false, true) => Some(0),
            _ => None,
        }
    }

    fn overrun(&mut self, battle_id: u32, date: GameDate) -> bool {
        let strengths = [
            self.side_strength(battle_id, 0),
            self.side_strength(battle_id, 1),
        ];
        // 军事科技尚未分国，两侧都是 0，高两级保护不会触发。
        let loser = initial_overrun_loser(
            strengths,
            u64::from(self.military_rules.combat.width),
            [0, 0],
            [
                self.side_fightable(battle_id, 0),
                self.side_fightable(battle_id, 1),
            ],
        );
        let Some(loser) = loser else {
            return false;
        };
        self.finish(
            battle_id,
            BattleOutcome::Overrun {
                loser: u8::try_from(loser).expect("阵营只有两方"),
            },
            date,
            true,
        );
        true
    }

    fn resolve_existing(&mut self, battle_id: u32, date: GameDate) {
        if !self.military.battles.contains_key(&battle_id) {
            return;
        }
        self.prune_battle(battle_id);
        if !self.military.battles.contains_key(&battle_id) {
            return;
        }
        self.admit(battle_id);
        if self.military.battles[&battle_id].last_resolved == Some(date) {
            return;
        }
        if !self.sides_still_hostile(battle_id) {
            self.disengage(battle_id);
            return;
        }
        let empty = [
            self.military.battles[&battle_id].side_armies(0).is_empty(),
            self.military.battles[&battle_id].side_armies(1).is_empty(),
        ];
        if empty == [true, true] {
            self.military.battles.remove(&battle_id);
            return;
        }
        if let Some(loser) = empty.iter().position(|side| *side) {
            self.finish(
                battle_id,
                BattleOutcome::Retreat {
                    loser: u8::try_from(loser).expect("阵营只有两方"),
                },
                date,
                false,
            );
            return;
        }
        self.fight_day(battle_id, date);
    }

    fn fight_day(&mut self, battle_id: u32, date: GameDate) {
        self.advance_phase(battle_id);
        self.prepare_lines(battle_id);
        self.accumulate_pending(battle_id);
        self.apply_pending(battle_id);
        if let Some(battle) = self.military.battles.get_mut(&battle_id) {
            battle.last_resolved = Some(date);
        }
        self.collapse_or_continue(battle_id, date);
    }

    fn advance_phase(&mut self, battle_id: u32) {
        let elapsed = self.military.battles[&battle_id].elapsed_days;
        if elapsed.is_multiple_of(3) {
            let attacker = self.military.random.dice();
            let defender = self.military.random.dice();
            let battle = self.military.battles.get_mut(&battle_id).expect("战斗存在");
            battle.rolls = [defender, attacker];
        }
        self.military
            .battles
            .get_mut(&battle_id)
            .expect("战斗存在")
            .elapsed_days = elapsed + 1;
    }

    fn prepare_lines(&mut self, battle_id: u32) {
        let deployed = [
            self.military.battles[&battle_id].side(0).deployed,
            self.military.battles[&battle_id].side(1).deployed,
        ];
        if !deployed[0] || !deployed[1] {
            self.deploy_side(battle_id, 0);
            self.deploy_side(battle_id, 1);
            return;
        }
        self.drop_invalid_from_lines(battle_id, 0);
        self.drop_invalid_from_lines(battle_id, 1);
        let snapshots = [
            self.military.battles[&battle_id].side(0).front.clone(),
            self.military.battles[&battle_id].side(1).front.clone(),
        ];
        self.rearrange_side(battle_id, 0, &snapshots[1]);
        self.rearrange_side(battle_id, 1, &snapshots[0]);
    }

    fn deploy_side(&mut self, battle_id: u32, side: usize) {
        let enemy = self.valid_regiments(battle_id, 1 - side).len();
        let mut listed = self.valid_regiments(battle_id, side);
        listed.sort_by(|left, right| {
            right
                .score
                .cmp(&left.score)
                .then(right.order.cmp(&left.order))
        });
        let infantry: Vec<_> = listed
            .iter()
            .filter(|regiment| regiment.kind == UnitKind::Infantry)
            .cloned()
            .collect();
        let cavalry: Vec<_> = listed
            .iter()
            .filter(|regiment| regiment.kind == UnitKind::Cavalry)
            .cloned()
            .collect();
        let width = self.military_rules.combat.width as usize;
        let flank = flank_distance(
            self.military_rules.combat.infantry.maneuver,
            REGIMENT_STRENGTH,
            REGIMENT_STRENGTH,
        );
        let plan = line_plan(infantry.len(), cavalry.len(), enemy, width, flank);
        let order = center_out(width);
        let mut front = vec![None; width];
        let mut cursor = 0;
        let mut placed = BTreeSet::new();
        let place_one = |regiment: &ListedRegiment,
                         front: &mut Vec<Option<CombatantRef>>,
                         placed: &mut BTreeSet<(u32, usize)>,
                         cursor: &mut usize| {
            if *cursor >= order.len() {
                return;
            }
            front[order[*cursor]] = Some(regiment.unit.clone());
            placed.insert((regiment.unit.army_id, regiment.unit.index));
            *cursor += 1;
        };
        for regiment in infantry.iter().take(plan.core) {
            place_one(regiment, &mut front, &mut placed, &mut cursor);
        }
        for regiment in cavalry.iter().take(plan.cavalry) {
            place_one(regiment, &mut front, &mut placed, &mut cursor);
        }
        // 后排支撑列尚未实现，中性为没有后排。额外步兵只补在前排外侧。
        for regiment in infantry.iter().skip(plan.core).take(plan.extra_infantry) {
            place_one(regiment, &mut front, &mut placed, &mut cursor);
        }
        let reserve = listed
            .into_iter()
            .filter(|regiment| !placed.contains(&(regiment.unit.army_id, regiment.unit.index)))
            .map(|regiment| regiment.unit)
            .collect();
        let battle_side = self
            .military
            .battles
            .get_mut(&battle_id)
            .expect("战斗存在")
            .side_mut(side);
        battle_side.front = front;
        battle_side.reserve = reserve;
        battle_side.deployed = true;
    }

    fn drop_invalid_from_lines(&mut self, battle_id: u32, side: usize) {
        let province = self.military.battles[&battle_id].province();
        let front = self.military.battles[&battle_id].side(side).front.clone();
        let reserve = self.military.battles[&battle_id].side(side).reserve.clone();
        let keep = |unit: &CombatantRef| self.regiment_on_line(battle_id, side, province, unit);
        let front: Vec<_> = front
            .into_iter()
            .map(|slot| slot.filter(|unit| keep(unit)))
            .collect();
        let reserve: Vec<_> = reserve.into_iter().filter(|unit| keep(unit)).collect();
        let battle_side = self
            .military
            .battles
            .get_mut(&battle_id)
            .expect("战斗存在")
            .side_mut(side);
        battle_side.front = front;
        battle_side.reserve = reserve;
    }

    fn rearrange_side(&mut self, battle_id: u32, side: usize, enemy: &[Option<CombatantRef>]) {
        self.pull_outermost(battle_id, side, enemy);
        self.add_unlisted_to_reserve(battle_id, side);
        self.refill_front(battle_id, side, enemy);
        self.compress_front(battle_id, side);
    }

    fn pull_outermost(&mut self, battle_id: u32, side: usize, enemy: &[Option<CombatantRef>]) {
        let width = self.military.battles[&battle_id].side(side).front.len();
        let center = width / 2;
        loop {
            let front = self.military.battles[&battle_id].side(side).front.clone();
            let occupied: Vec<usize> = front
                .iter()
                .enumerate()
                .filter_map(|(index, slot)| slot.as_ref().map(|_| index))
                .collect();
            if occupied.is_empty() {
                break;
            }
            let mut ends = vec![occupied[0]];
            let right = occupied[occupied.len() - 1];
            if right != ends[0] {
                ends.push(right);
            }
            ends.sort_by_key(|slot| std::cmp::Reverse(slot.abs_diff(center)));
            let mut pulled = None;
            for slot in ends {
                let Some(unit) = front[slot].clone() else {
                    continue;
                };
                if self.attack_target(slot, &unit, enemy).is_none() {
                    pulled = Some((slot, unit));
                    break;
                }
            }
            let Some((slot, unit)) = pulled else {
                break;
            };
            let battle_side = self
                .military
                .battles
                .get_mut(&battle_id)
                .expect("战斗存在")
                .side_mut(side);
            battle_side.front[slot] = None;
            battle_side.reserve.push(unit);
        }
    }

    fn add_unlisted_to_reserve(&mut self, battle_id: u32, side: usize) {
        let listed = self.listed_keys(battle_id, side);
        let fresh: Vec<_> = self
            .valid_regiments(battle_id, side)
            .into_iter()
            .filter(|regiment| !listed.contains(&(regiment.unit.army_id, regiment.unit.index)))
            .map(|regiment| regiment.unit)
            .collect();
        self.military
            .battles
            .get_mut(&battle_id)
            .expect("战斗存在")
            .side_mut(side)
            .reserve
            .extend(fresh);
    }

    fn refill_front(&mut self, battle_id: u32, side: usize, enemy: &[Option<CombatantRef>]) {
        let enemy_count = enemy
            .iter()
            .filter(|slot| {
                slot.as_ref()
                    .is_some_and(|unit| self.regiment_fightable(unit))
            })
            .count();
        let mut reserve_units = self.reserve_listed(battle_id, side);
        reserve_units.sort_by(|left, right| {
            right
                .score
                .cmp(&left.score)
                .then(right.order.cmp(&left.order))
        });
        let infantry = reserve_units
            .iter()
            .filter(|regiment| regiment.kind == UnitKind::Infantry)
            .count();
        let cavalry = reserve_units
            .iter()
            .filter(|regiment| regiment.kind == UnitKind::Cavalry)
            .count();
        let on_line = self.military.battles[&battle_id]
            .side(side)
            .front
            .iter()
            .filter(|slot| slot.is_some())
            .count();
        let width = self.military_rules.combat.width as usize;
        let flank = flank_distance(
            self.military_rules.combat.infantry.maneuver,
            REGIMENT_STRENGTH,
            REGIMENT_STRENGTH,
        );
        let plan = line_plan(
            infantry + self.front_kind_count(battle_id, side, UnitKind::Infantry),
            cavalry + self.front_kind_count(battle_id, side, UnitKind::Cavalry),
            enemy_count,
            width,
            flank,
        );
        let cap = plan.core + plan.cavalry + plan.extra_infantry;
        if on_line >= cap {
            return;
        }
        let mut placed = on_line;
        let slots = center_out(self.military.battles[&battle_id].side(side).front.len());
        let mut used = BTreeSet::new();
        for slot in slots {
            if placed >= cap {
                break;
            }
            if self.military.battles[&battle_id].side(side).front[slot].is_some() {
                continue;
            }
            let Some(choice) = reserve_units.iter().find(|regiment| {
                !used.contains(&(regiment.unit.army_id, regiment.unit.index))
                    && self.attack_target(slot, &regiment.unit, enemy).is_some()
            }) else {
                continue;
            };
            let unit = choice.unit.clone();
            used.insert((unit.army_id, unit.index));
            self.military
                .battles
                .get_mut(&battle_id)
                .expect("战斗存在")
                .side_mut(side)
                .front[slot] = Some(unit);
            placed += 1;
        }
        if used.is_empty() {
            return;
        }
        self.military
            .battles
            .get_mut(&battle_id)
            .expect("战斗存在")
            .side_mut(side)
            .reserve
            .retain(|unit| !used.contains(&(unit.army_id, unit.index)));
    }

    fn compress_front(&mut self, battle_id: u32, side: usize) {
        let front = self.military.battles[&battle_id].side(side).front.clone();
        let width = front.len();
        let present: Vec<_> = front.into_iter().flatten().collect();
        let mut compressed = vec![None; width];
        let start = width.saturating_sub(present.len()) / 2;
        for (offset, unit) in present.into_iter().enumerate() {
            compressed[start + offset] = Some(unit);
        }
        self.military
            .battles
            .get_mut(&battle_id)
            .expect("战斗存在")
            .side_mut(side)
            .front = compressed;
    }

    fn accumulate_pending(&mut self, battle_id: u32) {
        let phase = PhaseKind::from_battle_day(self.military.battles[&battle_id].elapsed_days);
        let rolls = self.military.battles[&battle_id].rolls;
        let elapsed = i64::from(self.military.battles[&battle_id].elapsed_days);
        let average = self.average_max_morale(battle_id);
        let penalties = [
            self.support_penalty(battle_id, 0),
            self.support_penalty(battle_id, 1),
        ];
        let fronts = [
            self.military.battles[&battle_id].side(0).front.clone(),
            self.military.battles[&battle_id].side(1).front.clone(),
        ];
        let reserves = [
            self.military.battles[&battle_id].side(0).reserve.clone(),
            self.military.battles[&battle_id].side(1).reserve.clone(),
        ];
        for attacker_side in 0..2 {
            let defender_side = 1 - attacker_side;
            let die = i32::from(rolls[attacker_side]);
            for (slot, unit) in fronts[attacker_side].iter().enumerate() {
                let Some(unit) = unit else {
                    continue;
                };
                let Some(target_slot) = self.attack_target(slot, unit, &fronts[defender_side])
                else {
                    continue;
                };
                let Some(target) = fronts[defender_side].get(target_slot).cloned().flatten() else {
                    continue;
                };
                let same_column_empty = fronts[defender_side]
                    .get(slot)
                    .and_then(|entry| entry.as_ref())
                    .is_none();
                self.queue_strike(PendingStrike {
                    attacker: unit,
                    defender: &target,
                    phase,
                    die,
                    elapsed,
                    defender_penalty: penalties[defender_side],
                    same_column_empty,
                });
            }
            for unit in fronts[attacker_side].iter().flatten() {
                self.add_pending(unit, 0, daily_morale_loss(average, false));
            }
            for unit in &reserves[attacker_side] {
                self.add_pending(unit, 0, daily_morale_loss(average, true));
            }
        }
    }

    fn queue_strike(&mut self, strike: PendingStrike<'_>) {
        let PendingStrike {
            attacker,
            defender,
            phase,
            die,
            elapsed,
            defender_penalty,
            same_column_empty,
        } = strike;
        let max = self.max_morale();
        let Some(attacker_regiment) = self.regiment_snapshot(attacker) else {
            return;
        };
        let Some(defender_regiment) = self.regiment_snapshot(defender) else {
            return;
        };
        let rules = &self.military_rules.combat;
        let attacker_unit = rules.unit(attacker_regiment.kind);
        let defender_unit = rules.unit(defender_regiment.kind);
        let (phase_off, _) = attacker_unit.phase_pips(phase);
        let (_, phase_def) = defender_unit.phase_pips(phase);
        // 将领、地形和渡河还没有数值，中性修正是 0。骰子单独相加，不放进这里。
        let margin = phase_off - phase_def;
        let (morale_off, _) = attacker_unit.morale_pips();
        let (_, morale_def) = defender_unit.morale_pips();
        let tech = thousandths(match phase {
            PhaseKind::Fire => attacker_unit.fire_damage,
            PhaseKind::Shock => attacker_unit.shock_damage,
        });
        let tactics = effective_tactics(
            thousandths(rules.tactics),
            defender_regiment.kind == UnitKind::Cavalry,
            defender_penalty,
        );
        let men = strength_damage(StrengthStrike {
            die,
            margin,
            strength: i64::from(attacker_regiment.strength),
            discipline: NEUTRAL_DISCIPLINE,
            front_scalar: FRONT_SCALAR,
            defender_tactics: tactics,
            tech_multiplier: tech,
            ability: NEUTRAL_ABILITY,
            defender_received: 0,
            attacker_dealt: 0,
            elapsed_days: elapsed,
            same_column_empty,
            quarter: strength_quarter(attacker_regiment.strength, REGIMENT_STRENGTH),
        });
        let morale = morale_damage(MoraleStrike {
            die,
            margin: margin + morale_off - morale_def,
            max_morale: i64::from(max),
            strength: i64::from(attacker_regiment.strength),
            discipline: NEUTRAL_DISCIPLINE,
            front_scalar: FRONT_SCALAR,
            defender_tactics: tactics,
            ability: NEUTRAL_ABILITY,
            tech_multiplier: tech,
            elapsed_days: elapsed,
            defender_received: 0,
            attacker_dealt: 0,
        });
        self.add_pending(defender, men, morale);
    }

    fn add_pending(&mut self, unit: &CombatantRef, men: i64, morale: i64) {
        let Some(regiment) = self
            .military
            .armies
            .get_mut(&unit.army_id)
            .and_then(|army| army.regiments.get_mut(unit.index))
        else {
            return;
        };
        if men > 0 {
            regiment.strength_pending = regiment
                .strength_pending
                .saturating_add(u32::try_from(men).unwrap_or(u32::MAX));
        }
        if morale > 0 {
            regiment.morale_pending = regiment
                .morale_pending
                .saturating_add(u32::try_from(morale).unwrap_or(u32::MAX));
        }
    }

    fn apply_pending(&mut self, battle_id: u32) {
        let max = self.max_morale();
        let armies = self.battle_armies(battle_id);
        for army_id in armies {
            let Some(army) = self.military.armies.get_mut(&army_id) else {
                continue;
            };
            let mut lost = 0_u64;
            for regiment in &mut army.regiments {
                let drop_men = regiment.strength_pending.min(regiment.strength);
                regiment.strength -= drop_men;
                let current = regiment.effective_morale(max);
                let drop_morale = regiment.morale_pending.min(current);
                regiment.morale = current - drop_morale;
                regiment.strength_pending = 0;
                regiment.morale_pending = 0;
                lost += u64::from(drop_men);
            }
            if lost == 0 {
                continue;
            }
            let battle = self.military.battles.get_mut(&battle_id).expect("战斗存在");
            for side in 0..2 {
                if battle.side_armies(side).contains(&army_id) {
                    battle.side_mut(side).strength_lost += lost;
                }
            }
        }
    }

    fn collapse_or_continue(&mut self, battle_id: u32, date: GameDate) {
        if !self.military.battles.contains_key(&battle_id) {
            return;
        }
        let elapsed = self.military.battles[&battle_id].elapsed_days;
        if self.side_failed(battle_id, 1, elapsed) {
            self.finish_loss(battle_id, 1, date);
            return;
        }
        if self.side_failed(battle_id, 0, elapsed) {
            self.finish_loss(battle_id, 0, date);
            return;
        }
        if elapsed < RETREAT_LOCK_DAYS {
            let front = [
                self.front_strength(battle_id, 0),
                self.front_strength(battle_id, 1),
            ];
            let fightable = [
                self.side_fightable(battle_id, 0),
                self.side_fightable(battle_id, 1),
            ];
            if let Some(loser) = front_ratio_loser(front, [0, 0], fightable) {
                self.finish(
                    battle_id,
                    BattleOutcome::Stackwipe {
                        loser: u8::try_from(loser).expect("阵营只有两方"),
                    },
                    date,
                    true,
                );
            }
        }
    }

    fn finish_loss(&mut self, battle_id: u32, loser: usize, date: GameDate) {
        let elapsed = self.military.battles[&battle_id].elapsed_days;
        let wiped = wipes_after_defeat(
            elapsed,
            self.side_strength(battle_id, loser),
            self.side_strength(battle_id, 1 - loser),
        );
        let loser = u8::try_from(loser).expect("阵营只有两方");
        let outcome = if wiped {
            BattleOutcome::Stackwipe { loser }
        } else {
            BattleOutcome::Retreat { loser }
        };
        self.finish(battle_id, outcome, date, wiped);
    }

    fn side_failed(&self, battle_id: u32, side: usize, elapsed: u32) -> bool {
        if self.military.battles[&battle_id]
            .side_armies(side)
            .is_empty()
            || self.side_strength(battle_id, side) < 10
        {
            return true;
        }
        if elapsed < RETREAT_LOCK_DAYS {
            return false;
        }
        let max = self.max_morale();
        let mut total = 0_u64;
        let mut count = 0_u64;
        for unit in self.line_units(battle_id, side) {
            let Some(regiment) = self.regiment_snapshot(&unit) else {
                continue;
            };
            let morale = if regiment.morale == UNSET_MORALE {
                max
            } else {
                regiment.morale
            };
            if regiment.strength > 0 && morale >= 10 {
                total += u64::from(morale);
                count += 1;
            }
        }
        count == 0 || total / count <= 10
    }

    fn finish(
        &mut self,
        battle_id: u32,
        outcome: BattleOutcome,
        date: GameDate,
        refund_wipe: bool,
    ) {
        if !self.military.battles.contains_key(&battle_id) {
            return;
        }
        if let Some(loser) = outcome.loser() {
            self.apply_winner_floor(battle_id, usize::from(1 - loser));
        }
        let mut snapshot = self.snapshot(battle_id);
        let battle = self
            .military
            .battles
            .remove(&battle_id)
            .expect("结束的战斗存在");
        for side in 0..2 {
            for army_id in battle.side_armies(side) {
                if let Some(army) = self.military.armies.get_mut(army_id) {
                    army.battle_id = None;
                }
            }
        }
        let mut outcome = outcome;
        match outcome {
            BattleOutcome::Retreat { loser } => {
                if !self.retreat_side(&battle, usize::from(loser), date) {
                    outcome = BattleOutcome::NoRetreat { loser };
                }
            }
            BattleOutcome::MutualRetreat => {
                let left = self.retreat_side(&battle, 0, date);
                let right = self.retreat_side(&battle, 1, date);
                outcome = match (left, right) {
                    (true, true) | (false, false) => BattleOutcome::MutualRetreat,
                    (false, true) => BattleOutcome::NoRetreat { loser: 0 },
                    (true, false) => BattleOutcome::NoRetreat { loser: 1 },
                };
            }
            BattleOutcome::Stackwipe { loser } | BattleOutcome::Overrun { loser } => {
                self.destroy_side(&battle, usize::from(loser), refund_wipe);
            }
            BattleOutcome::NoRetreat { loser } => {
                self.destroy_side(&battle, usize::from(loser), false);
            }
        }
        if let Some(loser) = outcome.loser() {
            let side = &mut snapshot[usize::from(loser)];
            if matches!(
                outcome,
                BattleOutcome::Stackwipe { .. }
                    | BattleOutcome::Overrun { .. }
                    | BattleOutcome::NoRetreat { .. }
            ) {
                side.lost += side.strength;
                side.strength = 0;
                side.morale = 0;
            }
        }
        let province = battle.province();
        self.military.battle_reports.insert(
            province,
            BattleReport {
                battle_id,
                province,
                on: date,
                outcome,
                sides: snapshot.clone(),
            },
        );
        self.record_finished_battle(province, date, &snapshot, outcome.loser());
        self.reengage_after(&battle, date);
    }

    fn apply_winner_floor(&mut self, battle_id: u32, winner: usize) {
        let max = self.max_morale();
        let loser_joined = i64::try_from(
            self.military.battles[&battle_id]
                .side(1 - winner)
                .joined_strength,
        )
        .unwrap_or(i64::MAX);
        let winner_joined = i64::try_from(
            self.military.battles[&battle_id]
                .side(winner)
                .joined_strength,
        )
        .unwrap_or(i64::MAX);
        let floor = winner_morale_floor(loser_joined, winner_joined, i64::from(max));
        let floor = u32::try_from(floor.max(0)).unwrap_or(0).min(max);
        let armies = self.military.battles[&battle_id]
            .side_armies(winner)
            .to_vec();
        for army_id in armies {
            let Some(army) = self.military.armies.get_mut(&army_id) else {
                continue;
            };
            for regiment in &mut army.regiments {
                let current = regiment.effective_morale(max);
                regiment.morale = current.max(floor).min(max);
            }
        }
    }

    fn reengage_after(&mut self, battle: &Battle, date: GameDate) {
        let province = battle.province();
        let mut order = battle.side_armies(1).to_vec();
        for id in battle.side_armies(0) {
            if !order.contains(id) {
                order.push(*id);
            }
        }
        let extras: Vec<u32> = self
            .military
            .armies
            .values()
            .filter(|army| army.province == province && !order.contains(&army.id()))
            .map(|army| army.id())
            .collect();
        order.extend(extras);
        for army_id in order {
            let Some(army) = self.military.armies.get(&army_id) else {
                continue;
            };
            if army.province != province
                || army.is_retreating()
                || army.battle_id().is_some()
                || army.strength() == 0
            {
                continue;
            }
            self.engage(province, date, Some(army_id), true);
        }
    }

    fn retreat_side(&mut self, battle: &Battle, side: usize, date: GameDate) -> bool {
        let mut ok = true;
        for army_id in battle.side_armies(side) {
            if self.armies().contains_key(army_id) && !self.begin_retreat(*army_id, date) {
                ok = false;
            }
        }
        ok
    }

    fn begin_retreat(&mut self, army_id: u32, date: GameDate) -> bool {
        match self.plan_retreat(army_id, date) {
            RetreatPlan::NoDestination => {
                self.destroy_army(army_id, false);
                false
            }
            RetreatPlan::RouteFailed => {
                // 目的地已经选出，但寻路失败：留下军队，清掉撤退标志，不返还人力。
                let army = self
                    .military
                    .armies
                    .get_mut(&army_id)
                    .expect("撤退军队存在");
                army.movement = None;
                army.retreating = false;
                army.retreat_shattered = Some(false);
                army.battle_id = None;
                true
            }
            RetreatPlan::Ready(movement) => {
                let army = self
                    .military
                    .armies
                    .get_mut(&army_id)
                    .expect("撤退军队存在");
                army.movement = Some(movement);
                army.retreating = true;
                army.retreat_shattered = Some(true);
                army.battle_id = None;
                true
            }
        }
    }

    /// 低士气先远端再相邻，否则只走相邻。这里不改军队，只决定路线。
    fn plan_retreat(&mut self, army_id: u32, date: GameDate) -> RetreatPlan {
        let max = self.max_morale();
        let army = &self.military.armies[&army_id];
        let country = army.country.clone();
        let from = army.province;
        let our_men = army.strength();
        let low = self.army_weighted_morale(army_id, max) < LOW_MORALE_THRESHOLD;
        let speed = army.march_speed(&self.military_rules)
            * (Money::ONE + self.military_rules.combat.shattered_retreat_speed);
        let destination = if low {
            self.far_retreat_destination(&country, from)
                .or_else(|| self.adjacent_retreat_destination(&country, from, our_men))
        } else {
            self.adjacent_retreat_destination(&country, from, our_men)
        };
        let Some(destination) = destination else {
            return RetreatPlan::NoDestination;
        };
        match self.route_between(&country, from, destination, speed, date) {
            Ok(movement) => RetreatPlan::Ready(movement),
            Err(_) => RetreatPlan::RouteFailed,
        }
    }

    /// 各军最大士气 × 退出兵力 / 本侧当前兵力 × 惩罚。计入待结算士气。
    fn apply_deliberate_penalty(&mut self, battle_id: u32, leaver: u32) {
        let Some(side) = (0..2).find(|&side| {
            self.military.battles[&battle_id]
                .side_armies(side)
                .contains(&leaver)
        }) else {
            return;
        };
        let armies = self.military.battles[&battle_id].side_armies(side).to_vec();
        let side_strength: u64 = armies
            .iter()
            .filter_map(|id| self.armies().get(id).map(crate::Army::strength))
            .sum();
        let leaver_strength = self.armies().get(&leaver).map_or(0, crate::Army::strength);
        let max = i64::from(self.max_morale());
        let share = trunc_div(
            i64::try_from(leaver_strength).unwrap_or(i64::MAX),
            i64::try_from(side_strength).unwrap_or(0),
        );
        let hit = trunc_mul(trunc_mul(max, share), DELIBERATE_RETREAT_PENALTY_MILLI);
        let hit = u32::try_from(hit.max(0)).unwrap_or(u32::MAX);
        if hit == 0 {
            return;
        }
        for army_id in armies {
            if army_id == leaver {
                continue;
            }
            let Some(army) = self.military.armies.get_mut(&army_id) else {
                continue;
            };
            for regiment in &mut army.regiments {
                regiment.morale_pending = regiment.morale_pending.saturating_add(hit);
            }
        }
    }

    /// 低士气先走远端。评分只看 1 到 10 步，而且每一步都要能进入。
    /// 要塞区域阻挡尚未实现；退路仍受现有通行规则限制，不绕过要隘。
    fn far_retreat_destination(&self, country: &str, from: u32) -> Option<u32> {
        let mut seen = BTreeSet::from([from]);
        let mut queue = VecDeque::from([(from, 0_u32)]);
        let mut best: Option<(u64, u32)> = None;
        while let Some((province, hops)) = queue.pop_front() {
            if hops == 10 {
                continue;
            }
            let Some(neighbors) = self.land_neighbors(province) else {
                continue;
            };
            for &next in neighbors.keys() {
                if !seen.insert(next) || !self.can_enter(country, next) {
                    continue;
                }
                let next_hops = hops + 1;
                queue.push_back((next, next_hops));
                if !self.far_endpoint_allowed(country, next) {
                    continue;
                }
                let development = self
                    .province(next)
                    .and_then(|province| province.development.total())
                    .unwrap_or(0);
                let owned = self
                    .province(next)
                    .is_some_and(|province| province.owner.as_deref() == Some(country));
                let score = far_retreat_score(development, next_hops, owned);
                if best.is_none_or(|(current, _)| score > current) {
                    best = Some((score, next));
                }
            }
        }
        best.map(|(_, province)| province)
    }

    fn far_endpoint_allowed(&self, country: &str, province: u32) -> bool {
        if self.hostile_present(country, province) {
            return false;
        }
        let Some(controller) = self
            .province(province)
            .and_then(|province| province.controller.clone())
        else {
            return false;
        };
        controller == country || self.same_war_side(country, &controller)
    }

    /// 双边战争里两个不同国家不会站在同一侧。
    pub(super) fn same_war_side(&self, left: &str, right: &str) -> bool {
        if left == right {
            return true;
        }
        self.war_state.active.values().any(|war| {
            (left == war.attacker() && right == war.attacker())
                || (left == war.defender() && right == war.defender())
        })
    }

    fn adjacent_retreat_destination(
        &mut self,
        country: &str,
        from: u32,
        our_men: u64,
    ) -> Option<u32> {
        let neighbors: Vec<u32> = self.land_neighbors(from)?.keys().copied().collect();
        let mut best: Option<(u64, u32)> = None;
        for next in neighbors {
            if next == from || !self.can_enter(country, next) {
                continue;
            }
            let enemy = self.enemy_men(country, next);
            if enemy_strength_blocks_retreat(our_men, enemy) {
                continue;
            }
            let base = self.retreat_base_score(country, next);
            let mut score = base;
            // 先按控制关系加分。高于 2000 再加 0–3000。无敌军时对这个分数乘 200，不限基础分。
            if base > 2_000 {
                score += u64::from(self.military.random.bonus_steps()) * 1_000;
            }
            if enemy == 0 {
                score = score.saturating_mul(200);
            }
            if best.is_none_or(|(current, _)| score > current) {
                best = Some((score, next));
            }
        }
        best.map(|(_, province)| province)
    }

    fn retreat_base_score(&self, country: &str, province: u32) -> u64 {
        let Some(province) = self.province(province) else {
            return 1_000;
        };
        if province.owner.is_none() {
            return 50;
        }
        let own = province.owner.as_deref() == Some(country);
        let control = province.controller.as_deref() == Some(country);
        if own && control {
            4_000
        } else if control {
            3_000
        } else if own {
            2_000
        } else {
            1_000
        }
    }

    fn enemy_men(&self, country: &str, province: u32) -> u64 {
        self.military
            .armies
            .values()
            .filter(|army| {
                army.province == province && !army.retreating && self.at_war(country, &army.country)
            })
            .map(|army| army.strength())
            .sum()
    }

    fn destroy_side(&mut self, battle: &Battle, side: usize, refund: bool) {
        for army_id in battle.side_armies(side) {
            self.destroy_army(*army_id, refund);
        }
    }

    fn destroy_army(&mut self, army_id: u32, refund: bool) {
        let Some(army) = self.military.armies.remove(&army_id) else {
            return;
        };
        if !refund {
            return;
        }
        let mut men = 0_i64;
        for regiment in &army.regiments {
            men += wipe_refund_men(
                i64::from(regiment.strength),
                i64::from(REGIMENT_STRENGTH),
                i64::try_from(self.military_rules.manpower_cost(regiment.kind)).unwrap_or(0),
            );
        }
        if men <= 0 {
            return;
        }
        if let Some(pool) = self
            .countries
            .get_mut(&army.country)
            .and_then(|country| country.manpower.as_mut())
        {
            pool.refund(u64::try_from(men).unwrap_or(0));
        }
    }

    fn prune_battle(&mut self, battle_id: u32) {
        let province = self.military.battles[&battle_id].province();
        let listed = self.battle_armies(battle_id);
        for army_id in listed {
            let keep = self.armies().get(&army_id).is_some_and(|army| {
                army.province == province
                    && army.battle_id() == Some(battle_id)
                    && !army.is_retreating()
            });
            if !keep {
                self.remove_from_battle(battle_id, army_id);
                if let Some(army) = self.military.armies.get_mut(&army_id)
                    && army.battle_id() == Some(battle_id)
                {
                    army.battle_id = None;
                }
            }
        }
    }

    fn remove_from_battle(&mut self, battle_id: u32, army_id: u32) {
        if let Some(battle) = self.military.battles.get_mut(&battle_id) {
            for side in 0..2 {
                battle.side_mut(side).armies.retain(|id| *id != army_id);
            }
        }
    }

    fn sides_still_hostile(&self, battle_id: u32) -> bool {
        let battle = &self.military.battles[&battle_id];
        battle.side_armies(0).iter().any(|left| {
            battle.side_armies(1).iter().any(|right| {
                let Some(left) = self.armies().get(left) else {
                    return false;
                };
                let Some(right) = self.armies().get(right) else {
                    return false;
                };
                self.at_war(&left.country, &right.country)
            })
        })
    }

    fn side_strength(&self, battle_id: u32, side: usize) -> u64 {
        self.military.battles[&battle_id]
            .side_armies(side)
            .iter()
            .filter_map(|id| self.armies().get(id))
            .map(crate::Army::strength)
            .sum()
    }

    fn front_strength(&self, battle_id: u32, side: usize) -> u64 {
        self.military.battles[&battle_id]
            .side(side)
            .front
            .iter()
            .filter_map(|slot| slot.as_ref())
            .filter_map(|unit| self.regiment_snapshot(unit))
            .map(|regiment| u64::from(regiment.strength))
            .sum()
    }

    fn side_fightable(&self, battle_id: u32, side: usize) -> bool {
        let max = self.max_morale();
        self.military.battles[&battle_id]
            .side_armies(side)
            .iter()
            .filter_map(|id| self.armies().get(id))
            .any(|army| {
                army.regiments
                    .iter()
                    .any(|regiment| regiment.strength > 0 && regiment.effective_morale(max) >= 10)
            })
    }

    fn side_morale(&self, battle_id: u32, side: usize, max: u32) -> u32 {
        let mut total = 0_u64;
        let mut count = 0_u64;
        for army_id in self.military.battles[&battle_id].side_armies(side) {
            let Some(army) = self.armies().get(army_id) else {
                continue;
            };
            for regiment in &army.regiments {
                if regiment.strength == 0 {
                    continue;
                }
                total += u64::from(regiment.effective_morale(max));
                count += 1;
            }
        }
        u32::try_from(total.checked_div(count).unwrap_or(0)).unwrap_or(u32::MAX)
    }

    fn snapshot(&self, battle_id: u32) -> [ReportedSide; 2] {
        let max = self.max_morale();
        let battle = &self.military.battles[&battle_id];
        [0, 1].map(|side| {
            let army_ids = battle.side_armies(side).to_vec();
            let mut countries = Vec::new();
            for id in &army_ids {
                if let Some(country) = self.armies().get(id).map(|army| army.country.clone())
                    && !countries.contains(&country)
                {
                    countries.push(country);
                }
            }
            ReportedSide {
                countries,
                strength: self.side_strength(battle_id, side),
                morale: self.side_morale(battle_id, side, max),
                lost: battle.strength_lost(side),
                army_ids,
            }
        })
    }

    pub(crate) fn disengage(&mut self, battle_id: u32) {
        let Some(battle) = self.military.battles.remove(&battle_id) else {
            return;
        };
        for side in 0..2 {
            for army_id in battle.side_armies(side) {
                if let Some(army) = self.military.armies.get_mut(army_id) {
                    army.battle_id = None;
                }
            }
        }
    }

    fn report_for_army(&self, army_id: u32) -> Option<&BattleReport> {
        self.military.battle_reports.values().find(|report| {
            report
                .sides
                .iter()
                .any(|side| side.army_ids.contains(&army_id))
        })
    }

    fn ongoing_text(&self, battle: &Battle) -> String {
        let max = self.max_morale();
        let phase = PhaseKind::from_battle_day(battle.elapsed_days().max(1));
        format!(
            "战斗 #{} · 第 {} 天 · {}\n{}",
            battle.id(),
            battle.elapsed_days().max(1),
            phase.label(),
            [0, 1]
                .map(|side| {
                    format!(
                        "{}：{} · {} 人 · 士气 {} · 伤亡 {}",
                        side_name(side),
                        self.side_countries(battle, side),
                        self.side_strength(battle.id(), side),
                        format_morale(self.side_morale(battle.id(), side, max), max),
                        battle.strength_lost(side)
                    )
                })
                .join("\n")
        )
    }

    fn side_countries(&self, battle: &Battle, side: usize) -> String {
        let mut countries = Vec::new();
        for id in battle.side_armies(side) {
            if let Some(country) = self.armies().get(id).map(|army| army.country.clone())
                && !countries.contains(&country)
            {
                countries.push(country);
            }
        }
        if countries.is_empty() {
            "无".to_owned()
        } else {
            countries.join("、")
        }
    }

    fn report_text(&self, report: &BattleReport) -> String {
        let max = self.max_morale();
        let headline = match report.outcome {
            BattleOutcome::Retreat { loser } => format!("{}撤退", side_name(loser as usize)),
            BattleOutcome::Stackwipe { loser } => format!("{}覆灭", side_name(loser as usize)),
            BattleOutcome::Overrun { loser } => {
                format!("兵力悬殊，{}被歼灭", side_name(loser as usize))
            }
            BattleOutcome::NoRetreat { loser } => {
                format!("{}没有退路，军队覆灭", side_name(loser as usize))
            }
            BattleOutcome::MutualRetreat => "双方撤退".to_owned(),
        };
        format!(
            "战斗 #{} 结束 · {}\n{}",
            report.battle_id,
            headline,
            report
                .sides
                .iter()
                .enumerate()
                .map(|(side, state)| {
                    format!(
                        "{}：{} · 剩余 {} 人 · 士气 {} · 伤亡 {}",
                        side_name(side),
                        if state.countries.is_empty() {
                            "无".to_owned()
                        } else {
                            state.countries.join("、")
                        },
                        state.strength,
                        format_morale(state.morale, max),
                        state.lost
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        )
    }

    fn battle_armies(&self, battle_id: u32) -> Vec<u32> {
        let Some(battle) = self.military.battles.get(&battle_id) else {
            return Vec::new();
        };
        battle
            .side_armies(0)
            .iter()
            .chain(battle.side_armies(1))
            .copied()
            .collect()
    }

    fn line_units(&self, battle_id: u32, side: usize) -> Vec<CombatantRef> {
        let Some(battle) = self.military.battles.get(&battle_id) else {
            return Vec::new();
        };
        battle
            .side(side)
            .front
            .iter()
            .flatten()
            .cloned()
            .chain(battle.side(side).reserve.iter().cloned())
            .collect()
    }

    fn listed_keys(&self, battle_id: u32, side: usize) -> BTreeSet<(u32, usize)> {
        self.line_units(battle_id, side)
            .into_iter()
            .map(|unit| (unit.army_id, unit.index))
            .collect()
    }

    fn valid_regiments(&self, battle_id: u32, side: usize) -> Vec<ListedRegiment> {
        let max = self.max_morale();
        let mut listed = Vec::new();
        let mut order = 0_u32;
        let armies = self.military.battles[&battle_id].side_armies(side).to_vec();
        let mut armies = armies;
        armies.sort_unstable();
        for army_id in armies {
            let Some(army) = self.armies().get(&army_id) else {
                continue;
            };
            for (index, regiment) in army.regiments.iter().enumerate() {
                let morale = regiment.effective_morale(max);
                if regiment.strength == 0 || morale < 10 {
                    continue;
                }
                order += 1;
                let unit_rules = self.military_rules.combat.unit(regiment.kind);
                listed.push(ListedRegiment {
                    unit: CombatantRef { army_id, index },
                    kind: regiment.kind,
                    score: regiment_sort_score(
                        unit_rules.maneuver,
                        unit_rules.pip_sum(),
                        morale,
                        regiment.strength,
                    ),
                    order,
                });
            }
        }
        listed
    }

    fn reserve_listed(&self, battle_id: u32, side: usize) -> Vec<ListedRegiment> {
        let keys: BTreeSet<_> = self.military.battles[&battle_id]
            .side(side)
            .reserve
            .iter()
            .map(|unit| (unit.army_id, unit.index))
            .collect();
        self.valid_regiments(battle_id, side)
            .into_iter()
            .filter(|regiment| keys.contains(&(regiment.unit.army_id, regiment.unit.index)))
            .collect()
    }

    fn front_kind_count(&self, battle_id: u32, side: usize, kind: UnitKind) -> usize {
        self.military.battles[&battle_id]
            .side(side)
            .front
            .iter()
            .filter_map(|slot| slot.as_ref())
            .filter_map(|unit| self.regiment_snapshot(unit))
            .filter(|regiment| regiment.kind == kind)
            .count()
    }

    fn regiment_on_line(
        &self,
        battle_id: u32,
        side: usize,
        province: u32,
        unit: &CombatantRef,
    ) -> bool {
        let max = self.max_morale();
        let Some(army) = self.armies().get(&unit.army_id) else {
            return false;
        };
        if army.province != province
            || army.battle_id() != Some(battle_id)
            || !self.military.battles[&battle_id]
                .side_armies(side)
                .contains(&unit.army_id)
        {
            return false;
        }
        army.regiments
            .get(unit.index)
            .is_some_and(|regiment| regiment.strength > 0 && regiment.effective_morale(max) >= 10)
    }

    fn regiment_fightable(&self, unit: &CombatantRef) -> bool {
        let max = self.max_morale();
        self.regiment_snapshot(unit).is_some_and(|regiment| {
            let morale = if regiment.morale == UNSET_MORALE {
                max
            } else {
                regiment.morale
            };
            regiment.strength > 0 && morale >= 10
        })
    }

    fn regiment_snapshot(&self, unit: &CombatantRef) -> Option<RegimentView> {
        let regiment = self
            .armies()
            .get(&unit.army_id)?
            .regiments
            .get(unit.index)?;
        Some(RegimentView {
            kind: regiment.kind,
            strength: regiment.strength,
            morale: regiment.morale,
        })
    }

    fn attack_target(
        &self,
        slot: usize,
        attacker: &CombatantRef,
        enemy: &[Option<CombatantRef>],
    ) -> Option<usize> {
        if enemy.is_empty() {
            return None;
        }
        let attacker = self.regiment_snapshot(attacker)?;
        let maneuver = self.military_rules.combat.unit(attacker.kind).maneuver;
        let range = flank_distance(maneuver, attacker.strength, REGIMENT_STRENGTH);
        let valid = |index: usize| {
            enemy
                .get(index)
                .and_then(|slot| slot.as_ref())
                .is_some_and(|unit| self.regiment_fightable(unit))
        };
        let occupied = |index: usize| enemy.get(index).and_then(|slot| slot.as_ref()).is_some();
        if valid(slot) {
            return Some(slot);
        }
        let last = enemy.len() - 1;
        let start = slot.saturating_sub(range);
        let end = slot.saturating_add(range).min(last);
        if let Some(index) = (start..=end).find(|&index| valid(index)) {
            return Some(index);
        }
        if occupied(slot) {
            return Some(slot);
        }
        (start..=end).find(|&index| occupied(index))
    }

    fn support_penalty(&self, battle_id: u32, side: usize) -> i64 {
        let mut infantry = 0_i64;
        let mut cavalry = 0_i64;
        for unit in self.military.battles[&battle_id]
            .side(side)
            .front
            .iter()
            .flatten()
        {
            let Some(regiment) = self.regiment_snapshot(unit) else {
                continue;
            };
            match regiment.kind {
                UnitKind::Infantry => infantry += i64::from(regiment.strength),
                UnitKind::Cavalry => cavalry += i64::from(regiment.strength),
            }
        }
        cavalry_support_penalty(
            infantry,
            cavalry,
            i64::from(self.military_rules.combat.allowed_cavalry_ratio),
        )
    }

    fn average_max_morale(&self, battle_id: u32) -> i64 {
        let max = i64::from(self.max_morale());
        let mut weighted = 0_i64;
        let mut count = 0_i64;
        for army_id in self.battle_armies(battle_id) {
            let Some(army) = self.armies().get(&army_id) else {
                continue;
            };
            let regiments = army
                .regiments
                .iter()
                .filter(|regiment| regiment.strength > 0)
                .count() as i64;
            weighted += regiments * max;
            count += regiments;
        }
        if count == 0 { max } else { weighted / count }
    }

    fn route_between(
        &self,
        country: &str,
        from: u32,
        destination: u32,
        speed: Money,
        started_on: GameDate,
    ) -> Result<ArmyMovement, MovementError> {
        self.land_route(country, from, destination, speed, started_on, None, false)
    }
}

struct PendingStrike<'a> {
    attacker: &'a CombatantRef,
    defender: &'a CombatantRef,
    phase: PhaseKind,
    die: i32,
    elapsed: i64,
    defender_penalty: i64,
    same_column_empty: bool,
}

struct RegimentView {
    kind: UnitKind,
    strength: u32,
    morale: u32,
}

fn side_name(side: usize) -> &'static str {
    if side == 0 { "防守方" } else { "进攻方" }
}

fn format_morale(current: u32, max: u32) -> String {
    format!(
        "{:.1}/{:.1}",
        f64::from(current) / 1_000.0,
        f64::from(max) / 1_000.0
    )
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::time::Duration;

    use crate::combat::{StrengthStrike, strength_damage};
    use crate::{
        Army, ArmyMovement, ArmyOrganizationError, BattleOutcome, CombatRules, Country,
        Development, GameDate, GameSpeed, LandConnection, ManpowerRules, MilitaryRules, Money,
        MovementError, MovementLeg, Province, ProvinceKind, Regiment, SavedGame, Simulation,
        UnitKind, World,
    };

    fn date() -> GameDate {
        GameDate::new(1444, 11, 11).unwrap()
    }

    fn rules() -> MilitaryRules {
        MilitaryRules {
            infantry_cost: Money::from(10),
            cavalry_cost: Money::from(25),
            infantry_days: 60,
            cavalry_days: 90,
            infantry_speed: Money::from(1),
            cavalry_speed: Money::from(1),
            recruitment_full_refund_progress: Money::new(1, 1),
            land_maintenance_factor: Money::ZERO,
            monthly_reinforce: Money::new(1, 1),
            extra_land_reinforce_cost: Money::from(2),
            supply_base: 6,
            base_land_forcelimit: Money::ZERO,
            development_land_forcelimit: Money::ZERO,
            land_forcelimit_extra_cost_factor: Money::from(2),
            goods_land_forcelimit: BTreeMap::new(),
            tax_recruitment_time: Money::ZERO,
            starting_army_size: Money::ZERO,
            starting_army_size_at_war: Money::ZERO,
            manpower: ManpowerRules {
                base_capacity: 10_000,
                per_development: 0,
                recovery_months: 10,
                minimum_monthly: 1,
                infantry_cost: 1_000,
                cavalry_cost: 1_000,
            },
            combat: CombatRules::baseline(),
        }
    }

    fn province(owner: &str, dev: u32) -> Province {
        let mut province = Province::new(ProvinceKind::Land);
        province.owner = Some(owner.to_owned());
        province.controller = Some(owner.to_owned());
        province.development = Development {
            tax: Some(dev),
            production: Some(dev),
            manpower: Some(dev),
        };
        province
    }

    fn link(from: u32, to: u32) -> LandConnection {
        LandConnection {
            from,
            to,
            distance: 1,
        }
    }

    fn world(provinces: Vec<(u32, Province)>, links: Vec<LandConnection>) -> World {
        let countries = ["AAA", "BBB", "CCC"].map(Country::new);
        let ids: Vec<_> = provinces.iter().map(|(id, _)| *id).collect();
        World::new(
            countries,
            provinces.into_iter().collect(),
            ids,
            [],
            rules(),
            links,
        )
        .unwrap()
    }

    fn station(world: &mut World, id: u32, country: &str, province: u32, regiments: usize) {
        world.military.armies.insert(
            id,
            Army {
                id,
                country: country.to_owned(),
                province,
                regiments: (0..regiments)
                    .map(|_| Regiment::fresh(UnitKind::Infantry, 1_000))
                    .collect(),
                movement: None,
                battle_id: None,
                retreating: false,
                retreat_shattered: Some(false),
                exiled: false,
                retreat_previous: None,
                retreat_return: None,
            },
        );
        world.military.next_army_id = world.military.next_army_id.max(id + 1);
    }

    fn strength(world: &World, id: u32) -> u64 {
        world.armies()[&id].strength()
    }

    fn set_morale(world: &mut World, id: u32, morale: u32) {
        for regiment in &mut world.military.armies.get_mut(&id).unwrap().regiments {
            regiment.morale = morale;
        }
    }

    fn set_first(world: &mut World, id: u32, men: u32, morale: u32) {
        for (index, regiment) in world
            .military
            .armies
            .get_mut(&id)
            .unwrap()
            .regiments
            .iter_mut()
            .enumerate()
        {
            regiment.strength = if index == 0 { men } else { 0 };
            regiment.morale = morale;
        }
    }

    /// 中性满编团在第 1 天、同列有敌时的兵力伤害。骰子来自战斗里保存的点数。
    fn men_lost(roll: u8) -> u32 {
        u32::try_from(strength_damage(StrengthStrike {
            die: i32::from(roll),
            margin: 0,
            strength: 1_000,
            discipline: 1_000,
            front_scalar: 1_000,
            defender_tactics: 500,
            tech_multiplier: 500,
            ability: 1_000,
            defender_received: 0,
            attacker_dealt: 0,
            elapsed_days: 1,
            same_column_empty: false,
            quarter: 4,
        }))
        .unwrap()
    }

    #[test]
    fn peaceful_armies_do_not_fight_and_war_changes_real_strength() {
        let mut world = world(vec![(1, province("AAA", 1))], vec![]);
        station(&mut world, 1, "AAA", 1, 1);
        station(&mut world, 2, "BBB", 1, 1);
        world.resolve_battles(date());
        assert!(world.battle_in_province(1).is_none());
        assert_eq!(strength(&world, 1), 1_000);

        world.declare_war("AAA", "BBB", date()).unwrap();
        world.resolve_battles(date());
        let battle = world.battle_in_province(1).unwrap();
        let attacker_loss = u64::from(men_lost(battle.rolls[0]));
        let defender_loss = u64::from(men_lost(battle.rolls[1]));
        assert_eq!(strength(&world, 1), 1_000 - attacker_loss);
        assert_eq!(strength(&world, 2), 1_000 - defender_loss);
        assert_eq!(
            world.armies()[&1].regiments[0].strength,
            u32::try_from(1_000 - attacker_loss).unwrap()
        );
        assert_eq!(world.military.random.count(), 2);
    }

    #[test]
    fn phase_dice_change_on_the_first_day_of_each_phase() {
        let mut world = world(vec![(1, province("AAA", 1))], vec![]);
        station(&mut world, 1, "AAA", 1, 1);
        station(&mut world, 2, "BBB", 1, 1);
        world.declare_war("AAA", "BBB", date()).unwrap();
        world.resolve_battles(date());
        assert_eq!(world.military.random.count(), 2);
        for day in 1..3 {
            world.resolve_battles(date().checked_add_days(day).unwrap());
            assert_eq!(world.military.random.count(), 2);
        }
        world.resolve_battles(date().checked_add_days(3).unwrap());
        assert_eq!(world.military.random.count(), 4);
    }

    #[test]
    fn pause_stops_resolution_and_speed_does_not_change_the_result() {
        let setup = || {
            let mut world = world(vec![(1, province("AAA", 1))], vec![]);
            station(&mut world, 1, "AAA", 1, 1);
            station(&mut world, 2, "BBB", 1, 1);
            world.declare_war("AAA", "BBB", date()).unwrap();
            world
        };
        let mut paused = setup();
        let mut clock = Simulation::new(date());
        assert_eq!(clock.advance(&mut paused, Duration::from_secs(8)), 0);
        assert_eq!(strength(&paused, 1), 1_000);
        assert!(paused.battle_in_province(1).is_none());

        let mut slow = setup();
        let mut fast = setup();
        let mut slow_clock = Simulation::new(date());
        let mut fast_clock = Simulation::new(date());
        slow_clock.set_paused(false);
        fast_clock.set_paused(false);
        fast_clock.set_speed(GameSpeed::Octuple);
        assert_eq!(slow_clock.advance(&mut slow, Duration::from_secs(8)), 8);
        assert_eq!(fast_clock.advance(&mut fast, Duration::from_secs(1)), 8);
        assert_eq!(strength(&slow, 1), strength(&fast, 1));
        assert_eq!(strength(&slow, 2), strength(&fast, 2));
        assert_eq!(
            slow.armies()[&1].regiments[0].morale,
            fast.armies()[&1].regiments[0].morale
        );
        assert!(strength(&slow, 1) < 1_000);
    }

    #[test]
    fn friendly_reinforcement_joins_without_resolving_the_same_day_twice() {
        let mut world = world(
            vec![(1, province("AAA", 1)), (2, province("BBB", 1))],
            vec![link(1, 2)],
        );
        station(&mut world, 1, "AAA", 2, 1);
        station(&mut world, 2, "BBB", 2, 1);
        station(&mut world, 3, "AAA", 1, 1);
        world.declare_war("AAA", "BBB", date()).unwrap();
        world.military.armies.get_mut(&3).unwrap().movement = Some(ArmyMovement {
            legs: vec![MovementLeg {
                from: 1,
                to: 2,
                started_on: date(),
                arrives_on: date(),
            }],
            leg_index: 0,
        });
        world.resolve_battles(date());
        assert!(
            !world
                .battle_in_province(2)
                .unwrap()
                .side_armies(1)
                .contains(&3)
        );
        let before = strength(&world, 1);
        world.complete_movements(date());
        world.resolve_battles(date());
        let battle = world.battle_in_province(2).unwrap();
        assert!(battle.side_armies(1).contains(&3));
        assert_eq!(strength(&world, 1), before);
        assert_eq!(world.armies()[&3].battle_id(), Some(battle.id()));
    }

    #[test]
    fn bystanders_hostile_to_neither_or_both_stay_out() {
        let mut world = world(vec![(1, province("AAA", 1))], vec![]);
        station(&mut world, 1, "AAA", 1, 1);
        station(&mut world, 2, "BBB", 1, 1);
        station(&mut world, 3, "CCC", 1, 1);
        world.declare_war("AAA", "BBB", date()).unwrap();
        world.resolve_battles(date());
        assert!(world.armies()[&3].battle_id().is_none());
        assert_eq!(strength(&world, 3), 1_000);

        world.declare_war("AAA", "CCC", date()).unwrap();
        world.declare_war("BBB", "CCC", date()).unwrap();
        world.resolve_battles(date());
        assert!(world.armies()[&3].battle_id().is_none());
        assert_eq!(strength(&world, 3), 1_000);
    }

    #[test]
    fn loser_retreats_along_a_controlled_route_and_does_not_refight() {
        let mut world = world(
            vec![
                (1, province("BBB", 1)),
                (2, province("AAA", 1)),
                (3, province("BBB", 100)),
            ],
            vec![link(2, 1), link(2, 3)],
        );
        station(&mut world, 1, "AAA", 2, 1);
        station(&mut world, 2, "BBB", 2, 1);
        set_first(&mut world, 1, 18, 2_000);
        set_first(&mut world, 2, 9, 9);
        world.declare_war("AAA", "BBB", date()).unwrap();
        world.resolve_battles(date());
        assert!(world.battle_in_province(2).is_none());
        let loser = &world.armies()[&2];
        assert!(loser.is_retreating());
        assert!(loser.is_shattered());
        assert_eq!(loser.movement.as_ref().unwrap().destination(), 3);
        assert_eq!(loser.province, 2);
        world.resolve_battles(date());
        assert!(world.battle_in_province(2).is_none());
        assert!(world.armies()[&2].is_retreating());
        assert_eq!(
            world.move_army("BBB", 2, 1, date()),
            Err(MovementError::Retreating)
        );

        let next = date().checked_add_days(1).unwrap();
        world.complete_movements(next);
        assert!(!world.armies()[&2].is_retreating());
        assert_eq!(world.armies()[&2].province, 3);
        world.resolve_battles(next);
        assert!(world.battle_in_province(3).is_none());
        assert!(world.battle_in_province(2).is_none());
    }

    #[test]
    fn no_exit_removes_the_army_without_a_refund() {
        let mut world = world(vec![(1, province("AAA", 1))], vec![]);
        station(&mut world, 1, "AAA", 1, 1);
        station(&mut world, 2, "BBB", 1, 1);
        set_first(&mut world, 1, 18, 2_000);
        set_first(&mut world, 2, 9, 9);
        world.declare_war("AAA", "BBB", date()).unwrap();
        let before = world.country_manpower("BBB").unwrap().available;
        world.resolve_battles(date());
        assert!(world.armies().get(&2).is_none());
        assert!(matches!(
            world.battle_report(1).unwrap().outcome,
            BattleOutcome::NoRetreat { loser: 0 }
        ));
        assert_eq!(world.country_manpower("BBB").unwrap().available, before);
    }

    #[test]
    fn ten_thousand_against_one_thousand_is_not_an_initial_overrun() {
        let mut world = world(vec![(1, province("AAA", 1))], vec![]);
        station(&mut world, 1, "BBB", 1, 1);
        station(&mut world, 2, "AAA", 1, 10);
        world.declare_war("AAA", "BBB", date()).unwrap();
        world.resolve_battles(date());
        // 恰好十倍不走开场歼灭。前排仍可能因为两倍比较在当天覆灭。
        // 编号小的军队先被遍历，所以弱方是进攻方。
        assert!(matches!(
            world.battle_report(1).unwrap().outcome,
            BattleOutcome::Stackwipe { loser: 1 }
        ));
        assert!(world.armies().get(&1).is_none());
        assert!(strength(&world, 2) > 0);
    }

    #[test]
    fn overrun_refunds_half_the_remaining_manpower() {
        let mut world = world(vec![(1, province("AAA", 1))], vec![]);
        station(&mut world, 1, "BBB", 1, 1);
        station(&mut world, 2, "AAA", 1, 11);
        world.declare_war("AAA", "BBB", date()).unwrap();
        let before = world.country_manpower("BBB").unwrap().available;
        world.resolve_battles(date());
        assert!(world.armies().get(&1).is_none());
        assert_eq!(strength(&world, 2), 11_000);
        assert_eq!(
            world.country_manpower("BBB").unwrap().available,
            before + 500
        );
        assert!(matches!(
            world.battle_report(1).unwrap().outcome,
            BattleOutcome::Overrun { loser: 1 }
        ));
    }

    #[test]
    fn early_strength_break_wipes_below_half_and_even_odds_wait_until_day_twelve() {
        let mut wiped = world(
            vec![(1, province("AAA", 1)), (2, province("BBB", 1))],
            vec![link(1, 2)],
        );
        station(&mut wiped, 1, "BBB", 1, 1);
        station(&mut wiped, 2, "AAA", 1, 1);
        // 胜方 20 人：9 小于一半，但还不小于十分之一，所以是当天覆灭而不是开场十倍。
        set_first(&mut wiped, 1, 9, 9);
        set_first(&mut wiped, 2, 20, 2_000);
        wiped.declare_war("AAA", "BBB", date()).unwrap();
        let before = wiped.country_manpower("BBB").unwrap().available;
        wiped.resolve_battles(date());
        assert!(wiped.armies().get(&1).is_none());
        assert!(matches!(
            wiped.battle_report(1).unwrap().outcome,
            BattleOutcome::Stackwipe { loser: 1 }
        ));
        assert_eq!(wiped.country_manpower("BBB").unwrap().available, before + 4);

        let mut even = world(
            vec![(1, province("AAA", 1)), (2, province("BBB", 1))],
            vec![link(1, 2)],
        );
        station(&mut even, 1, "AAA", 1, 1);
        station(&mut even, 2, "BBB", 1, 1);
        set_morale(&mut even, 1, 9);
        set_morale(&mut even, 2, 9);
        even.declare_war("AAA", "BBB", date()).unwrap();
        for day in 0..12 {
            even.resolve_battles(date().checked_add_days(day).unwrap());
        }
        assert!(matches!(
            even.battle_report(1).unwrap().outcome,
            BattleOutcome::Retreat { loser: 1 }
        ));
        assert!(even.armies()[&1].is_retreating());
        assert_eq!(
            even.armies()[&1].movement.as_ref().unwrap().destination(),
            2
        );
        assert!(even.armies().contains_key(&2));
        assert!(!even.armies()[&2].is_retreating());
    }

    #[test]
    fn battle_blocks_orders_and_zero_manpower_does_not_refill() {
        let mut world = world(
            vec![(1, province("AAA", 1)), (2, province("BBB", 1))],
            vec![link(1, 2)],
        );
        station(&mut world, 1, "AAA", 1, 1);
        station(&mut world, 2, "BBB", 1, 1);
        world.declare_war("AAA", "BBB", date()).unwrap();
        world.resolve_battles(date());
        assert_eq!(
            world.move_army("AAA", 1, 2, date()),
            Err(MovementError::InBattle)
        );
        assert_eq!(
            world.disband_army("AAA", 1),
            Err(ArmyOrganizationError::InBattle)
        );
        let damaged = strength(&world, 1);
        world
            .countries
            .get_mut("AAA")
            .unwrap()
            .finances
            .refund(Money::from(500));
        world
            .countries
            .get_mut("AAA")
            .unwrap()
            .manpower
            .as_mut()
            .unwrap()
            .available = 0;
        world.reinforce_armies();
        assert_eq!(strength(&world, 1), damaged);
    }

    #[test]
    fn save_keeps_the_battle_and_does_not_resolve_that_date_again() {
        let mut world = world(vec![(1, province("AAA", 1))], vec![]);
        station(&mut world, 1, "AAA", 1, 1);
        station(&mut world, 2, "BBB", 1, 1);
        world.declare_war("AAA", "BBB", date()).unwrap();
        world.resolve_battles(date());
        assert_eq!(world.military.random.count(), 2);
        let saved = SavedGame::capture(&world, &Simulation::new(date()), "AAA");
        let restored = serde_json::from_value::<SavedGame>(serde_json::to_value(&saved).unwrap())
            .unwrap()
            .restore(&world)
            .unwrap();
        assert_eq!(strength(&restored.world, 1), strength(&world, 1));
        assert_eq!(
            restored.world.armies()[&1].regiments[0].morale,
            world.armies()[&1].regiments[0].morale
        );
        assert_eq!(restored.world.military.random.count(), 2);
        let mut continued = restored.world;
        continued.resolve_battles(date());
        assert_eq!(strength(&continued, 1), strength(&world, 1));
        assert_eq!(continued.military.random.count(), 2);
        let after_first_day = strength(&world, 1);
        let next = date().checked_add_days(1).unwrap();
        continued.resolve_battles(next);
        world.resolve_battles(next);
        assert_eq!(strength(&continued, 1), strength(&world, 1));
        assert!(strength(&continued, 1) < after_first_day);
    }

    #[test]
    fn old_saves_without_morale_or_battles_restore_full_morale() {
        let mut world = world(vec![(1, province("AAA", 1))], vec![]);
        station(&mut world, 1, "AAA", 1, 1);
        let saved = SavedGame::capture(&world, &Simulation::new(date()), "AAA");
        let mut value = serde_json::to_value(&saved).unwrap();
        let regiment = value["military"]["armies"]["1"]["regiments"][0]
            .as_object_mut()
            .unwrap();
        regiment.remove("morale");
        regiment.remove("strength_pending");
        regiment.remove("morale_pending");
        let military = value["military"].as_object_mut().unwrap();
        military.remove("battles");
        military.remove("battle_reports");
        military.remove("next_battle_id");
        military.remove("random");
        let restored = serde_json::from_value::<SavedGame>(value)
            .unwrap()
            .restore(&world)
            .unwrap();
        let regiment = &restored.world.armies()[&1].regiments[0];
        assert_eq!(regiment.morale, 2_000);
        assert_eq!(regiment.strength_pending, 0);
        assert_eq!(regiment.morale_pending, 0);
        assert!(restored.world.battles().is_empty());
        assert_eq!(restored.world.military.random.count(), 0);

        let mut army = world.armies()[&1].clone();
        army.retreating = true;
        army.movement = None;
        let mut army_value = serde_json::to_value(&army).unwrap();
        army_value
            .as_object_mut()
            .unwrap()
            .remove("retreat_shattered");
        let loaded: Army = serde_json::from_value(army_value).unwrap();
        assert!(loaded.is_shattered());
        assert!(loaded.is_retreating());
    }

    #[test]
    fn empty_neighbor_beats_an_occupied_home_province() {
        let mut world = world(
            vec![
                (1, province("BBB", 1)),
                (2, province("BBB", 1)),
                (3, province("AAA", 1)),
            ],
            vec![link(1, 2), link(1, 3)],
        );
        station(&mut world, 1, "BBB", 1, 1);
        station(&mut world, 2, "AAA", 1, 1);
        station(&mut world, 4, "AAA", 2, 1);
        set_first(&mut world, 1, 9, 2_000);
        set_first(&mut world, 2, 18, 2_000);
        world.declare_war("AAA", "BBB", date()).unwrap();
        world.resolve_battles(date());
        let loser = &world.armies()[&1];
        assert!(loser.is_retreating());
        assert_eq!(loser.movement.as_ref().unwrap().destination(), 3);
    }

    #[test]
    fn same_day_reinforcement_joins_before_the_initial_overrun() {
        let mut world = world(
            vec![(1, province("AAA", 1)), (2, province("BBB", 1))],
            vec![link(1, 2)],
        );
        station(&mut world, 1, "BBB", 2, 11);
        station(&mut world, 2, "AAA", 1, 1);
        station(&mut world, 3, "AAA", 1, 11);
        world.declare_war("AAA", "BBB", date()).unwrap();
        for id in [2, 3] {
            world.military.armies.get_mut(&id).unwrap().movement = Some(ArmyMovement {
                legs: vec![MovementLeg {
                    from: 1,
                    to: 2,
                    started_on: date(),
                    arrives_on: date(),
                }],
                leg_index: 0,
            });
        }
        world.complete_movements(date());
        assert!(world.armies().contains_key(&2));
        world.resolve_battles(date());
        let battle = world.battle_in_province(2).unwrap();
        assert!(battle.side_armies(1).contains(&2));
        assert!(battle.side_armies(1).contains(&3));
        assert!(world.armies().contains_key(&2));
        assert!(world.battle_report(2).is_none());
    }

    #[test]
    fn follow_up_overrun_waits_until_the_next_day() {
        let mut world = world(vec![(1, province("AAA", 1))], vec![]);
        station(&mut world, 1, "BBB", 1, 1);
        station(&mut world, 2, "AAA", 1, 11);
        world.declare_war("AAA", "BBB", date()).unwrap();
        world.engage(1, date(), Some(1), true);
        assert_eq!(strength(&world, 1), 1_000);
        assert_eq!(
            world.battle_in_province(1).unwrap().last_resolved,
            Some(date())
        );
        world.resolve_battles(date());
        assert_eq!(strength(&world, 1), 1_000);
        let next = date().checked_add_days(1).unwrap();
        world.resolve_battles(next);
        assert!(world.armies().get(&1).is_none());
        assert!(matches!(
            world.battle_report(1).unwrap().outcome,
            BattleOutcome::Overrun { loser: 1 }
        ));
    }

    #[test]
    fn deliberate_retreat_after_twelve_days_penalizes_the_other_army() {
        let mut world = world(
            vec![(1, province("BBB", 1)), (2, province("AAA", 1))],
            vec![link(1, 2)],
        );
        station(&mut world, 1, "AAA", 1, 1);
        station(&mut world, 2, "BBB", 1, 1);
        station(&mut world, 3, "AAA", 1, 1);
        world.declare_war("AAA", "BBB", date()).unwrap();
        world.engage(1, date(), Some(1), true);
        assert_eq!(
            world.retreat_from_battle("AAA", 1, date()),
            Err(MovementError::RetreatLocked)
        );
        let battle_id = world.battle_in_province(1).unwrap().id();
        world
            .military
            .battles
            .get_mut(&battle_id)
            .unwrap()
            .elapsed_days = 12;
        world.retreat_from_battle("AAA", 1, date()).unwrap();
        let leaver = &world.armies()[&1];
        assert!(leaver.is_retreating());
        assert!(leaver.battle_id().is_none());
        assert_eq!(leaver.regiments[0].morale_pending, 0);
        assert_eq!(leaver.movement.as_ref().unwrap().destination(), 2);
        assert_eq!(world.armies()[&3].regiments[0].morale_pending, 1_000);
        assert_eq!(world.armies()[&3].battle_id(), Some(battle_id));

        let mut blocked = super::tests::world(vec![(1, province("BBB", 1))], vec![]);
        station(&mut blocked, 1, "AAA", 1, 1);
        station(&mut blocked, 2, "BBB", 1, 1);
        blocked.declare_war("AAA", "BBB", date()).unwrap();
        blocked.engage(1, date(), Some(1), true);
        let battle_id = blocked.battle_in_province(1).unwrap().id();
        blocked
            .military
            .battles
            .get_mut(&battle_id)
            .unwrap()
            .elapsed_days = 12;
        assert_eq!(
            blocked.retreat_from_battle("AAA", 1, date()),
            Err(MovementError::NoRetreatRoute)
        );
        assert_eq!(blocked.armies()[&1].battle_id(), Some(battle_id));
    }

    #[test]
    fn clearing_shattered_retreat_retimes_the_remaining_route() {
        let mut world = world(
            vec![(1, province("AAA", 1)), (2, province("AAA", 1))],
            vec![LandConnection {
                from: 1,
                to: 2,
                distance: 3,
            }],
        );
        station(&mut world, 1, "AAA", 1, 1);
        let shattered = Money::from(1) * (Money::ONE + Money::new(5, 1));
        let movement = world
            .land_route("AAA", 1, 2, shattered, date(), None, false)
            .unwrap();
        let fast = movement.arrives_on();
        world.military.armies.get_mut(&1).unwrap().movement = Some(movement);
        world.military.armies.get_mut(&1).unwrap().retreating = true;
        world.military.armies.get_mut(&1).unwrap().retreat_shattered = Some(true);
        world.maybe_clear_shattered(1);
        let army = &world.armies()[&1];
        assert!(!army.is_retreating());
        assert!(!army.is_shattered());
        let slowed = army.movement.as_ref().unwrap().arrives_on();
        assert!(slowed > fast);
        assert_eq!(date().days_until(slowed), 3);
    }
}
