// SPDX-License-Identifier: GPL-3.0-only

//! 每日交战、增援、士气崩溃和溃退。
//!
//! 查询进行中的战斗供后续占领使用。本模块不处理围攻或占领。

use std::collections::{BTreeMap, BTreeSet};

use crate::combat::{OVERRUN_RATIO, RETREAT_LOCK_DAYS, STACKWIPE_RATIO};
use crate::{
    ArmyMovement, Battle, BattleOutcome, BattleReport, GameDate, Money, MovementError, PhaseKind,
    ReportedSide, SaveError, UNSET_MORALE, UnitKind, base_casualties, casualty_amount,
    morale_milli_from, passive_morale, whole_men,
};

use super::World;

#[derive(Clone, Copy)]
struct Fighter {
    army_id: u32,
    index: usize,
    kind: UnitKind,
    strength: u32,
    maneuver: u32,
}

impl World {
    pub fn battles(&self) -> &BTreeMap<u32, Battle> {
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
        let max = self
            .military_rules
            .combat
            .morale_milli()
            .expect("陆战规则已检查");
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
        let max = self
            .military_rules
            .combat
            .morale_milli()
            .expect("陆战规则已检查");
        let rules = self.military_rules.combat.clone();
        let ids: Vec<u32> = self.military.armies.keys().copied().collect();
        for id in ids {
            let army = &self.military.armies[&id];
            if army.battle_id().is_some() {
                continue;
            }
            let mut rate = rules.recovery;
            if self.province(army.province).is_some_and(|province| {
                province.controller.as_deref() == Some(army.country.as_str())
            }) {
                rate += rules.recovery_own_territory;
            }
            let gain = whole_men(Money::from(max) * rate);
            let army = self
                .military
                .armies
                .get_mut(&id)
                .expect("恢复士气的军队存在");
            for regiment in &mut army.regiments {
                let current = regiment.effective_morale(max);
                regiment.morale = current.saturating_add(gain).min(max);
            }
        }
    }

    pub(crate) fn resolve_battles(&mut self, date: GameDate) {
        let provinces: BTreeSet<u32> = self
            .military
            .armies
            .values()
            .map(|army| army.province)
            .collect();
        let ongoing: Vec<u32> = self.military.battles.keys().copied().collect();
        for id in ongoing {
            self.resolve_existing(id, date);
        }
        for province in provinces {
            if self.battle_in_province(province).is_none() {
                self.try_start_battle(province, date);
            }
        }
    }

    pub(super) fn validate_battles(&self, date: GameDate) -> Result<(), SaveError> {
        let max = self
            .military_rules
            .combat
            .morale_milli()
            .expect("陆战规则已检查");
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

    fn resolve_existing(&mut self, battle_id: u32, date: GameDate) {
        self.prune_battle(battle_id);
        if !self.military.battles.contains_key(&battle_id) {
            return;
        }
        self.admit_reinforcements(battle_id);
        let battle = &self.military.battles[&battle_id];
        if battle.last_resolved == Some(date) {
            return;
        }
        if !self.sides_still_hostile(battle_id) {
            self.disengage(battle_id);
            return;
        }
        if (0..2).any(|side| {
            self.military.battles[&battle_id]
                .side_armies(side)
                .is_empty()
        }) {
            self.finish_empty_side(battle_id, date);
            return;
        }
        self.fight_day(battle_id, date);
    }

    fn try_start_battle(&mut self, province: u32, date: GameDate) {
        let Some((defender, attacker)) = self.opening_pair(province) else {
            return;
        };
        if self.military.next_battle_id == u32::MAX {
            return;
        }
        let id = self.military.next_battle_id;
        self.military.next_battle_id += 1;
        let mut battle = Battle::new(id, province, date);
        battle.side_mut(0).armies = defender;
        battle.side_mut(1).armies = attacker;
        for army_id in battle.side_armies(0).iter().chain(battle.side_armies(1)) {
            self.military
                .armies
                .get_mut(army_id)
                .expect("参战军队存在")
                .battle_id = Some(id);
        }
        self.military.battles.insert(id, battle);
        self.admit_reinforcements(id);
        if self.overrun(id, date) {
            return;
        }
        self.fight_day(id, date);
    }

    fn opening_pair(&self, province: u32) -> Option<(Vec<u32>, Vec<u32>)> {
        let present = self.present_armies(province);
        for war in self.war_state.active.values() {
            let defender: Vec<u32> = present
                .iter()
                .copied()
                .filter(|&id| self.armies()[&id].country == war.defender())
                .collect();
            let attacker: Vec<u32> = present
                .iter()
                .copied()
                .filter(|&id| self.armies()[&id].country == war.attacker())
                .collect();
            if !defender.is_empty() && !attacker.is_empty() {
                return Some((defender, attacker));
            }
        }
        None
    }

    fn present_armies(&self, province: u32) -> Vec<u32> {
        self.military
            .armies
            .values()
            .filter(|army| {
                army.province == province
                    && army.movement.is_none()
                    && !army.is_retreating()
                    && army.battle_id().is_none()
                    && army.strength() > 0
            })
            .map(|army| army.id())
            .collect()
    }

    fn admit_reinforcements(&mut self, battle_id: u32) {
        let province = self.military.battles[&battle_id].province();
        let candidates: Vec<u32> = self
            .military
            .armies
            .values()
            .filter(|army| {
                army.province == province
                    && army.movement.is_none()
                    && !army.is_retreating()
                    && army.battle_id().is_none()
            })
            .map(|army| army.id())
            .collect();
        for army_id in candidates {
            let country = self.armies()[&army_id].country.clone();
            let side = self.reinforcement_side(battle_id, &country);
            if let Some(side) = side {
                self.military
                    .battles
                    .get_mut(&battle_id)
                    .expect("战斗存在")
                    .side_mut(side)
                    .armies
                    .push(army_id);
                self.military
                    .armies
                    .get_mut(&army_id)
                    .expect("增援军队存在")
                    .battle_id = Some(battle_id);
            }
        }
    }

    fn reinforcement_side(&self, battle_id: u32, country: &str) -> Option<usize> {
        let battle = &self.military.battles[&battle_id];
        let on_side = |side: usize| {
            battle.side_armies(side).iter().any(|id| {
                self.armies()
                    .get(id)
                    .is_some_and(|army| army.country == country)
            })
        };
        if on_side(0) {
            return Some(0);
        }
        if on_side(1) {
            return Some(1);
        }
        let hostile = |side: usize| {
            battle.side_armies(side).iter().any(|id| {
                self.armies()
                    .get(id)
                    .is_some_and(|army| self.at_war(country, &army.country))
            })
        };
        match (hostile(0), hostile(1)) {
            (true, false) => Some(1),
            (false, true) => Some(0),
            _ => None,
        }
    }

    fn overrun(&mut self, battle_id: u32, date: GameDate) -> bool {
        let width = u64::from(self.military_rules.combat.width);
        let strengths = [
            self.side_strength(battle_id, 0),
            self.side_strength(battle_id, 1),
        ];
        let regiments = [
            self.side_regiments(battle_id, 0),
            self.side_regiments(battle_id, 1),
        ];
        let loser = if strengths[1] >= strengths[0].saturating_mul(OVERRUN_RATIO)
            && regiments[0] < width
        {
            Some(0)
        } else if strengths[0] >= strengths[1].saturating_mul(OVERRUN_RATIO) && regiments[1] < width
        {
            Some(1)
        } else {
            None
        };
        let Some(loser) = loser else {
            return false;
        };
        self.finish(battle_id, BattleOutcome::Overrun { loser }, date, false);
        true
    }

    fn fight_day(&mut self, battle_id: u32, date: GameDate) {
        let battle = self.military.battles.get_mut(&battle_id).expect("战斗存在");
        battle.refresh_roll();
        let phase = PhaseKind::from_elapsed(battle.elapsed_days);
        let battle_day = battle.elapsed_days + 1;
        let rolls = battle.rolls;
        let sides = [
            battle.side_armies(0).to_vec(),
            battle.side_armies(1).to_vec(),
        ];
        let max = self
            .military_rules
            .combat
            .morale_milli()
            .expect("陆战规则已检查");
        let deployed = [self.deploy(&sides[0], max), self.deploy(&sides[1], max)];
        let mut damage: BTreeMap<(u32, usize), (u32, u32)> = BTreeMap::new();
        for (attacker_side, attackers) in deployed.iter().enumerate() {
            let defender_side = 1 - attacker_side;
            let defenders = &deployed[defender_side];
            for (slot, fighter) in attackers.front.iter().enumerate() {
                let target = target_slot(slot, fighter.maneuver, &defenders.front);
                let Some(target) = target else {
                    continue;
                };
                let dealt = self.strike(
                    fighter,
                    &defenders.front[target],
                    phase,
                    rolls[attacker_side],
                    battle_day,
                );
                let entry = damage
                    .entry((
                        defenders.front[target].army_id,
                        defenders.front[target].index,
                    ))
                    .or_insert((0, 0));
                entry.0 = entry.0.saturating_add(dealt.0);
                entry.1 = entry.1.saturating_add(dealt.1);
            }
            for fighter in &attackers.front {
                let entry = damage
                    .entry((fighter.army_id, fighter.index))
                    .or_insert((0, 0));
                entry.1 = entry.1.saturating_add(passive_morale(max, false));
            }
            for fighter in &attackers.reserve {
                let entry = damage
                    .entry((fighter.army_id, fighter.index))
                    .or_insert((0, 0));
                entry.1 = entry.1.saturating_add(passive_morale(max, true));
            }
        }
        self.apply_damage(battle_id, &damage, max);
        let battle = self.military.battles.get_mut(&battle_id).expect("战斗存在");
        battle.elapsed_days = battle_day;
        battle.last_resolved = Some(date);
        self.collapse_or_continue(battle_id, date);
    }

    fn strike(
        &self,
        attacker: &Fighter,
        defender: &Fighter,
        phase: PhaseKind,
        roll: u8,
        battle_day: u32,
    ) -> (u32, u32) {
        let rules = &self.military_rules.combat;
        let attacker_unit = rules.unit(attacker.kind);
        let defender_unit = rules.unit(defender.kind);
        let (offense, _, damage) = attacker_unit.stats(phase, false);
        let (_, defense, _) = defender_unit.stats(phase, false);
        let (morale_offense, _, _) = attacker_unit.stats(phase, true);
        let (_, morale_defense, _) = defender_unit.stats(phase, true);
        let strength_base = base_casualties(u32::from(roll), offense, defense);
        let morale_base = base_casualties(u32::from(roll), morale_offense, morale_defense);
        let men = casualty_amount(
            strength_base,
            attacker.strength,
            damage,
            rules.tactics,
            rules.battle_length_damage,
            battle_day,
        );
        let morale = casualty_amount(
            morale_base,
            attacker.strength,
            damage,
            rules.tactics,
            rules.battle_length_damage,
            battle_day,
        );
        (whole_men(men), morale_milli_from(morale, rules.max_morale))
    }

    /// 原版没有公开列阵规则。这里按军队编号和团序号，先步兵后骑兵，从左侧填满宽度。
    /// 超出宽度的部队只攻击机动范围内最近的敌军；机动为 0 时只打正前方。
    fn deploy(&self, armies: &[u32], max: u32) -> Deployment {
        let mut infantry = Vec::new();
        let mut cavalry = Vec::new();
        let mut ordered = armies.to_vec();
        ordered.sort_unstable();
        for &army_id in &ordered {
            let Some(army) = self.armies().get(&army_id) else {
                continue;
            };
            for (index, regiment) in army.regiments.iter().enumerate() {
                if regiment.strength == 0 || regiment.effective_morale(max) == 0 {
                    continue;
                }
                let fighter = Fighter {
                    army_id,
                    index,
                    kind: regiment.kind,
                    strength: regiment.strength,
                    maneuver: self.military_rules.combat.unit(regiment.kind).maneuver,
                };
                match regiment.kind {
                    UnitKind::Infantry => infantry.push(fighter),
                    UnitKind::Cavalry => cavalry.push(fighter),
                }
            }
        }
        infantry.append(&mut cavalry);
        let width = self.military_rules.combat.width as usize;
        let split = infantry.len().min(width);
        let reserve = infantry.split_off(split);
        Deployment {
            front: infantry,
            reserve,
        }
    }

    fn apply_damage(
        &mut self,
        battle_id: u32,
        damage: &BTreeMap<(u32, usize), (u32, u32)>,
        max: u32,
    ) {
        for (&(army_id, index), &(men, morale)) in damage {
            let lost = {
                let Some(army) = self.military.armies.get_mut(&army_id) else {
                    continue;
                };
                let Some(regiment) = army.regiments.get_mut(index) else {
                    continue;
                };
                let before = regiment.strength;
                regiment.strength = before.saturating_sub(men);
                let current = regiment.effective_morale(max);
                regiment.morale = current.saturating_sub(morale);
                u64::from(before - regiment.strength)
            };
            if lost > 0 {
                let battle = self.military.battles.get_mut(&battle_id).expect("战斗存在");
                for side in 0..2 {
                    if battle.side_armies(side).contains(&army_id) {
                        battle.side_mut(side).strength_lost += lost;
                    }
                }
            }
        }
        let armies: Vec<u32> = self.military.battles[&battle_id]
            .side_armies(0)
            .iter()
            .chain(self.military.battles[&battle_id].side_armies(1))
            .copied()
            .collect();
        for army_id in armies {
            let Some(army) = self.military.armies.get_mut(&army_id) else {
                continue;
            };
            army.regiments.retain(|regiment| regiment.strength > 0);
            if army.regiments.is_empty() {
                self.remove_from_battle(battle_id, army_id);
                self.military.armies.remove(&army_id);
            }
        }
    }

    fn collapse_or_continue(&mut self, battle_id: u32, date: GameDate) {
        let max = self
            .military_rules
            .combat
            .morale_milli()
            .expect("陆战规则已检查");
        let broken = [
            self.side_broken(battle_id, 0, max),
            self.side_broken(battle_id, 1, max),
        ];
        if broken == [false, false] {
            return;
        }
        if broken == [true, true] {
            self.finish(battle_id, BattleOutcome::MutualRetreat, date, true);
            return;
        }
        let loser = if broken[0] { 0_u8 } else { 1 };
        let winner = usize::from(1 - loser);
        let elapsed = self.military.battles[&battle_id].elapsed_days;
        let loser_strength = self.side_strength(battle_id, usize::from(loser));
        let winner_strength = self.side_strength(battle_id, winner);
        // 兵力已经归零，或十二天锁定期内敌方仍有两倍兵力，都按全歼处理。
        let wiped = loser_strength == 0
            || (elapsed < RETREAT_LOCK_DAYS
                && winner_strength >= loser_strength.saturating_mul(STACKWIPE_RATIO));
        let outcome = if wiped {
            BattleOutcome::Stackwipe { loser }
        } else {
            BattleOutcome::Retreat { loser }
        };
        let refund = matches!(outcome, BattleOutcome::Stackwipe { .. });
        self.finish(battle_id, outcome, date, refund);
    }

    fn finish_empty_side(&mut self, battle_id: u32, date: GameDate) {
        let empty = (0..2).find(|side| {
            self.military.battles[&battle_id]
                .side_armies(*side)
                .is_empty()
        });
        if let Some(loser) = empty {
            if self.military.battles[&battle_id]
                .side_armies(1 - loser)
                .is_empty()
            {
                self.military.battles.remove(&battle_id);
                return;
            }
            self.finish(
                battle_id,
                BattleOutcome::Retreat {
                    loser: u8::try_from(loser).expect("阵营只有两方"),
                },
                date,
                false,
            );
        }
    }

    fn finish(
        &mut self,
        battle_id: u32,
        outcome: BattleOutcome,
        date: GameDate,
        refund_wipe: bool,
    ) {
        let mut snapshot = self.snapshot(battle_id);
        let battle = self
            .military
            .battles
            .remove(&battle_id)
            .expect("结束的战斗存在");
        for side in 0..2 {
            for army_id in battle.side_armies(side).to_vec() {
                if let Some(army) = self.military.armies.get_mut(&army_id) {
                    army.battle_id = None;
                }
            }
        }
        let mut outcome = outcome;
        match outcome {
            BattleOutcome::Retreat { loser } => {
                // 走不通的军队已在撤退时解散并返还半数人力，这里只改结果。
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
            BattleOutcome::Stackwipe { loser } | BattleOutcome::NoRetreat { loser } => {
                self.destroy_side(&battle, usize::from(loser), refund_wipe);
            }
            BattleOutcome::Overrun { loser } => {
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
        self.military.battle_reports.insert(
            battle.province(),
            BattleReport {
                battle_id,
                province: battle.province(),
                on: date,
                outcome,
                sides: snapshot,
            },
        );
    }

    /// 一侧全部军队都开始撤退时返回真。没有退路的军队在这里解散并返还半数剩余人力。
    fn retreat_side(&mut self, battle: &Battle, side: usize, date: GameDate) -> bool {
        let mut ok = true;
        for army_id in battle.side_armies(side).to_vec() {
            if self.armies().contains_key(&army_id) && !self.begin_retreat(army_id, date) {
                ok = false;
            }
        }
        ok
    }

    fn begin_retreat(&mut self, army_id: u32, date: GameDate) -> bool {
        let army = &self.military.armies[&army_id];
        let country = army.country.clone();
        let from = army.province;
        let speed = army.march_speed(&self.military_rules)
            * (Money::ONE + self.military_rules.combat.shattered_retreat_speed);
        let Some(destination) = self.retreat_destination(&country, from) else {
            self.destroy_army(army_id, true);
            return false;
        };
        match self.route_between(&country, from, destination, speed, date) {
            Ok(movement) => {
                let army = self
                    .military
                    .armies
                    .get_mut(&army_id)
                    .expect("撤退军队存在");
                army.movement = Some(movement);
                army.retreating = true;
                army.battle_id = None;
                true
            }
            Err(_) => {
                self.destroy_army(army_id, true);
                false
            }
        }
    }

    /// 优先撤往本国控制、附近无敌军且发展度较高的省份。
    /// 要塞和距离上限没有已核实的数值，这里不另造。没有控制区时只退入一个可进入的邻省。
    fn retreat_destination(&self, country: &str, from: u32) -> Option<u32> {
        let controlled = self.reachable(country, from);
        let mut best: Option<(bool, u64, u32)> = None;
        for province in controlled {
            let development = self
                .province(province)
                .and_then(|province| province.development.total())
                .unwrap_or(0);
            let threatened = self.hostile_present(country, province)
                || self
                    .land_neighbors(province)
                    .into_iter()
                    .flat_map(|neighbors| neighbors.iter())
                    .any(|(&next, _)| self.hostile_present(country, next));
            let rank = (!threatened, development, u32::MAX - province);
            if best.is_none_or(|current| rank > current) {
                best = Some(rank);
            }
        }
        if let Some((_, _, encoded)) = best {
            return Some(u32::MAX - encoded);
        }
        self.land_neighbors(from)
            .into_iter()
            .flat_map(|neighbors| neighbors.iter())
            .find_map(|(&next, _)| self.can_enter(country, next).then_some(next))
    }

    fn reachable(&self, country: &str, from: u32) -> Vec<u32> {
        let mut seen = BTreeSet::from([from]);
        let mut queue = vec![from];
        let mut found = Vec::new();
        while let Some(province) = queue.pop() {
            let Some(neighbors) = self.land_neighbors(province) else {
                continue;
            };
            for &next in neighbors.keys() {
                if !seen.insert(next) || !self.can_enter(country, next) {
                    continue;
                }
                queue.push(next);
                if self
                    .province(next)
                    .is_some_and(|state| state.controller.as_deref() == Some(country))
                {
                    found.push(next);
                }
            }
        }
        found
    }

    fn destroy_side(&mut self, battle: &Battle, side: usize, refund: bool) {
        for army_id in battle.side_armies(side).to_vec() {
            self.destroy_army(army_id, refund);
        }
    }

    fn destroy_army(&mut self, army_id: u32, refund: bool) {
        let Some(army) = self.military.armies.remove(&army_id) else {
            return;
        };
        if refund {
            let men = army.strength() / 2;
            if let Some(pool) = self
                .countries
                .get_mut(&army.country)
                .and_then(|country| country.manpower.as_mut())
            {
                pool.refund(men);
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

    fn prune_battle(&mut self, battle_id: u32) {
        let province = self.military.battles[&battle_id].province();
        let listed: Vec<u32> = self.military.battles[&battle_id]
            .side_armies(0)
            .iter()
            .chain(self.military.battles[&battle_id].side_armies(1))
            .copied()
            .collect();
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

    fn side_regiments(&self, battle_id: u32, side: usize) -> u64 {
        self.military.battles[&battle_id]
            .side_armies(side)
            .iter()
            .filter_map(|id| self.armies().get(id))
            .map(|army| army.regiments.len() as u64)
            .sum()
    }

    fn side_broken(&self, battle_id: u32, side: usize, max: u32) -> bool {
        for army_id in self.military.battles[&battle_id].side_armies(side) {
            let Some(army) = self.armies().get(army_id) else {
                continue;
            };
            if army
                .regiments
                .iter()
                .any(|regiment| regiment.strength > 0 && regiment.effective_morale(max) > 0)
            {
                return false;
            }
        }
        true
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
        total.checked_div(count).unwrap_or(0) as u32
    }

    fn snapshot(&self, battle_id: u32) -> [ReportedSide; 2] {
        let max = self
            .military_rules
            .combat
            .morale_milli()
            .expect("陆战规则已检查");
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

    fn disengage(&mut self, battle_id: u32) {
        let battle = self.military.battles.remove(&battle_id).expect("战斗存在");
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
        let max = self
            .military_rules
            .combat
            .morale_milli()
            .expect("陆战规则已检查");
        let phase = PhaseKind::from_elapsed(battle.elapsed_days.saturating_sub(1));
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
        let max = self
            .military_rules
            .combat
            .morale_milli()
            .expect("陆战规则已检查");
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

    pub(crate) fn fill_unset_morale(&mut self) {
        let max = self
            .military_rules
            .combat
            .morale_milli()
            .expect("陆战规则已检查");
        for army in self.military.armies.values_mut() {
            for regiment in &mut army.regiments {
                if regiment.morale == UNSET_MORALE {
                    regiment.morale = max;
                }
            }
        }
    }
}

struct Deployment {
    front: Vec<Fighter>,
    reserve: Vec<Fighter>,
}

fn target_slot(slot: usize, maneuver: u32, front: &[Fighter]) -> Option<usize> {
    if front.is_empty() {
        return None;
    }
    if slot < front.len() {
        return Some(slot);
    }
    let distance = slot - (front.len() - 1);
    (distance <= maneuver as usize).then_some(front.len() - 1)
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

impl World {
    fn route_between(
        &self,
        country: &str,
        from: u32,
        destination: u32,
        speed: Money,
        started_on: GameDate,
    ) -> Result<ArmyMovement, MovementError> {
        self.land_route(country, from, destination, speed, started_on, None)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::time::Duration;

    use crate::{
        Army, ArmyMovement, ArmyOrganizationError, BattleOutcome, CombatRules, Country,
        Development, GameDate, GameSpeed, LandConnection, ManpowerRules, MilitaryRules, Money,
        MovementError, MovementLeg, Province, ProvinceKind, Regiment, SavedGame, Simulation,
        UnitKind, World, base_casualties, casualty_amount, whole_men,
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

    /// 测试兵种点数为 0，伤害系数和战技都是 0.5，因此只由阶段骰子和战斗天数决定减员。
    fn men_lost(roll: u8, day: u32) -> u32 {
        whole_men(casualty_amount(
            base_casualties(u32::from(roll), 0, 0),
            1_000,
            Money::new(5, 1),
            Money::new(5, 1),
            Money::new(1, 2),
            day,
        ))
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
        let attacker_loss = u64::from(men_lost(battle.rolls[0], 1));
        let defender_loss = u64::from(men_lost(battle.rolls[1], 1));
        assert_eq!(strength(&world, 1), 1_000 - attacker_loss);
        assert_eq!(strength(&world, 2), 1_000 - defender_loss);
        assert_eq!(
            world.armies()[&1].regiments[0].strength,
            u32::try_from(1_000 - attacker_loss).unwrap()
        );
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
                (3, province("BBB", 3)),
            ],
            vec![link(2, 1), link(2, 3)],
        );
        station(&mut world, 1, "AAA", 2, 1);
        station(&mut world, 2, "BBB", 2, 1);
        set_morale(&mut world, 2, 1);
        world.declare_war("AAA", "BBB", date()).unwrap();
        world.resolve_battles(date());
        assert!(world.battle_in_province(2).is_none());
        let loser = &world.armies()[&2];
        assert!(loser.is_retreating());
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
    fn no_exit_removes_the_army_and_refunds_half_the_survivors() {
        let mut world = world(vec![(1, province("AAA", 1))], vec![]);
        station(&mut world, 1, "AAA", 1, 1);
        station(&mut world, 2, "BBB", 1, 1);
        set_morale(&mut world, 2, 1);
        world.declare_war("AAA", "BBB", date()).unwrap();
        let before = world.country_manpower("BBB").unwrap().available;
        let lost = u64::from(men_lost(crate::combat::phase_roll(1, 0, 1) as u8, 1));
        world.resolve_battles(date());
        assert!(world.armies().get(&2).is_none());
        assert!(matches!(
            world.battle_report(1).unwrap().outcome,
            BattleOutcome::NoRetreat { loser: 0 }
        ));
        assert_eq!(
            world.country_manpower("BBB").unwrap().available,
            before + (1_000 - lost) / 2
        );
    }

    #[test]
    fn overrun_destroys_the_smaller_side_without_a_refund_or_winner_losses() {
        let mut world = world(vec![(1, province("AAA", 1))], vec![]);
        station(&mut world, 1, "BBB", 1, 1);
        station(&mut world, 2, "AAA", 1, 10);
        world.declare_war("AAA", "BBB", date()).unwrap();
        let before = world.country_manpower("BBB").unwrap().available;
        world.resolve_battles(date());
        assert!(world.armies().get(&1).is_none());
        assert_eq!(strength(&world, 2), 10_000);
        assert_eq!(world.country_manpower("BBB").unwrap().available, before);
        assert!(matches!(
            world.battle_report(1).unwrap().outcome,
            BattleOutcome::Overrun { loser: 0 }
        ));
    }

    #[test]
    fn early_two_to_one_break_is_a_stackwipe_and_even_odds_retreat() {
        let mut wiped = world(
            vec![(1, province("AAA", 1)), (2, province("BBB", 1))],
            vec![link(1, 2)],
        );
        station(&mut wiped, 1, "BBB", 1, 1);
        station(&mut wiped, 2, "AAA", 1, 2);
        set_morale(&mut wiped, 1, 1);
        wiped.declare_war("AAA", "BBB", date()).unwrap();
        let before = wiped.country_manpower("BBB").unwrap().available;
        // 败方是守方，减员来自攻方这一阶段的骰子。
        let lost = u64::from(men_lost(crate::combat::phase_roll(1, 0, 1) as u8, 1));
        wiped.resolve_battles(date());
        assert!(wiped.armies().get(&1).is_none());
        assert!(matches!(
            wiped.battle_report(1).unwrap().outcome,
            BattleOutcome::Stackwipe { loser: 0 }
        ));
        assert_eq!(
            wiped.country_manpower("BBB").unwrap().available,
            before + (1_000 - lost) / 2
        );

        let mut even = world(
            vec![(1, province("AAA", 1)), (2, province("BBB", 1))],
            vec![link(1, 2)],
        );
        station(&mut even, 1, "AAA", 1, 1);
        station(&mut even, 2, "BBB", 1, 1);
        set_morale(&mut even, 2, 1);
        even.declare_war("AAA", "BBB", date()).unwrap();
        even.resolve_battles(date());
        assert!(even.armies().get(&2).is_some());
        assert!(matches!(
            even.battle_report(1).unwrap().outcome,
            BattleOutcome::Retreat { loser: 0 }
        ));
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
        let mut continued = restored.world;
        continued.resolve_battles(date());
        assert_eq!(strength(&continued, 1), strength(&world, 1));
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
        value["military"]["armies"]["1"]["regiments"][0]
            .as_object_mut()
            .unwrap()
            .remove("morale");
        let military = value["military"].as_object_mut().unwrap();
        military.remove("battles");
        military.remove("battle_reports");
        military.remove("next_battle_id");
        let restored = serde_json::from_value::<SavedGame>(value)
            .unwrap()
            .restore(&world)
            .unwrap();
        assert_eq!(restored.world.armies()[&1].regiments[0].morale, 2_000);
        assert!(restored.world.battles().is_empty());
    }
}
