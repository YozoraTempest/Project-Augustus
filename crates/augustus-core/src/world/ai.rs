// SPDX-License-Identifier: GPL-3.0-only

//! 非玩家国家在已经发生的战争中下达命令。
//!
//! 招募、行军和和谈都调用玩家使用的报价与执行接口，不直接改国库、人力或军队。
//! 战力用 1.37.5 的泰勒乘积。优先级、30 日增援、每天一次和主动撤回是项目取舍。
//! -25 战争分数也是项目取舍。条款能否成立只看已有的 `accepts_terms`。
//! 这里不选择宣战对象。
//!
//! [`World::direct_war_ai`] 返回当天的决定，供调试读取。
//! 这些记录不写入世界。跨日任务写在 `MilitaryState::war_plans` 里。

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::combat::{UnitPips, enemy_strength_blocks_retreat};
use crate::military::ArmyPlan;
use crate::{GameDate, Money, REGIMENT_STRENGTH, UnitKind};

use super::World;

/// 从本国视角低于这个战争分数时提出白和平。-25 点，单位是千分点。
const LOSING_WARSCORE_MILLI: i64 = -25_000;

/// 项目取舍：只把行军天数不超过这个值的军队派去增援。原版没有这个天数。
const REINFORCE_WITHIN_DAYS: u32 = 30;

const P_RELIEVE: u8 = 0;
const P_REINFORCE: u8 = 1;
const P_SIEGE: u8 = 2;
const P_ASSEMBLE: u8 = 3;
const P_OCCUPY: u8 = 4;
const P_RECOVER: u8 = 5;

/// `MORALE_STRENGTH_FACTOR_*`。原点 4，系数 6.233 / 2.057 / 0.082，千分定点。
const MORALE_ORIGIN_MILLI: i64 = 4_000;
const MORALE_POW0_MILLI: i64 = 6_233;
const MORALE_POW1_MILLI: i64 = 2_057;
const MORALE_POW2_MILLI: i64 = 82;

/// `COMBAT_PIP_IMPORTANCE` 1.04573，按十万分之一保存，连乘时向零截断。
const PIP_IMPORTANCE_NUM: u128 = 104_573;
const PIP_IMPORTANCE_DEN: u128 = 100_000;
const FIRE_PIP_MULT: u32 = 3;
const SHOCK_PIP_MULT: u32 = 3;
const MORALE_PIP_MULT: u32 = 2;

const ROLE_RELIEVE: &str = "relieve";
const ROLE_REINFORCE: &str = "reinforce";
const ROLE_SIEGE: &str = "siege";
const ROLE_ASSEMBLE: &str = "assemble";
const ROLE_OCCUPY: &str = "occupy";
const ROLE_RECOVER: &str = "recover";

/// 一个非玩家交战国在某个游戏日作出的和谈、招募和行军决定。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WarAiDecision {
    pub date: GameDate,
    pub country: String,
    pub peace: Vec<PeaceChoice>,
    pub recruit: RecruitChoice,
    pub armies: Vec<ArmyChoice>,
}

/// 对该敌人的和谈选择。`action` 为 `cede`、`white_peace`、`skip` 或 `failed`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PeaceChoice {
    pub enemy: String,
    pub action: String,
    pub provinces: Vec<u32>,
    pub reason: String,
}

/// 当天的招募选择。`action` 为 `order`、`skip` 或 `failed`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RecruitChoice {
    pub action: String,
    pub province: Option<u32>,
    pub reason: String,
}

/// 一支军队当天的去向。`action` 为 `move`、`stay` 或 `failed`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ArmyChoice {
    pub id: u32,
    pub action: String,
    pub destination: Option<u32>,
    pub rank: Option<u8>,
    /// `relieve`、`reinforce`、`siege`、`assemble`、`occupy`、`recover`。没有任务时为空。
    pub role: Option<String>,
    pub reason: String,
}

impl World {
    /// 每个新游戏日调用一次。`player` 为空、或该国不在战争中时什么都不做。
    pub(crate) fn direct_war_ai(
        &mut self,
        player: Option<&str>,
        date: GameDate,
    ) -> Vec<WarAiDecision> {
        let Some(player) = player else {
            return Vec::new();
        };
        if self.country(player).is_none() {
            return Vec::new();
        }
        let tags: Vec<String> = self
            .countries
            .keys()
            .filter(|tag| tag.as_str() != player)
            .cloned()
            .collect();
        let mut decisions = Vec::new();
        for tag in tags {
            if self.wars_of(&tag).next().is_none() {
                continue;
            }
            let peace = self.offer_peace(&tag, date);
            if self.wars_of(&tag).next().is_none() {
                decisions.push(WarAiDecision {
                    date,
                    country: tag,
                    peace,
                    recruit: RecruitChoice {
                        action: "skip".to_owned(),
                        province: None,
                        reason: "战争已经结束".to_owned(),
                    },
                    armies: Vec::new(),
                });
                continue;
            }
            let recruit = self.recruit_for_war(&tag, date);
            let armies = self.order_armies(&tag, player, date);
            decisions.push(WarAiDecision {
                date,
                country: tag,
                peace,
                recruit,
                armies,
            });
        }
        self.prune_war_plans(player);
        decisions
    }

    /// 先按接受判定割让已控制的省份；分数明显不利时才提出白和平。
    fn offer_peace(&mut self, country: &str, date: GameDate) -> Vec<PeaceChoice> {
        let enemies: Vec<String> = self
            .wars_of(country)
            .map(|war| {
                if war.attacker() == country {
                    war.defender().to_owned()
                } else {
                    war.attacker().to_owned()
                }
            })
            .collect();
        let mut choices = Vec::new();
        for enemy in enemies {
            if self.war_between(country, &enemy).is_none() {
                choices.push(PeaceChoice {
                    enemy,
                    action: "skip".to_owned(),
                    provinces: Vec::new(),
                    reason: "已经不在交战".to_owned(),
                });
                continue;
            }
            if let Some(provinces) = self.affordable_demands(country, &enemy, date)
                && !provinces.is_empty()
            {
                let choice = match self.make_peace(country, &enemy, &provinces, date) {
                    Ok(_) => PeaceChoice {
                        enemy,
                        action: "cede".to_owned(),
                        provinces,
                        reason: "已提出割让".to_owned(),
                    },
                    Err(error) => PeaceChoice {
                        enemy,
                        action: "failed".to_owned(),
                        provinces,
                        reason: error.to_string(),
                    },
                };
                choices.push(choice);
                continue;
            }
            let (offer, reason) = self.white_peace_choice(country, &enemy, date);
            if offer {
                choices.push(match self.make_peace(country, &enemy, &[], date) {
                    Ok(_) => PeaceChoice {
                        enemy,
                        action: "white_peace".to_owned(),
                        provinces: Vec::new(),
                        reason,
                    },
                    Err(error) => PeaceChoice {
                        enemy,
                        action: "failed".to_owned(),
                        provinces: Vec::new(),
                        reason: error.to_string(),
                    },
                });
            } else {
                choices.push(PeaceChoice {
                    enemy,
                    action: "skip".to_owned(),
                    provinces: Vec::new(),
                    reason,
                });
            }
        }
        choices
    }

    /// 只考虑提出方已经控制的省份，按费用从低到高装进当前战争分数。
    fn affordable_demands(&self, country: &str, enemy: &str, date: GameDate) -> Option<Vec<u32>> {
        let sheet = self.peace_sheet(country, enemy, date).ok()?;
        let mut demands = sheet.provinces;
        demands.retain(|demand| demand.controlled_by_offerer);
        demands.sort_by(|left, right| {
            left.cost_milli
                .cmp(&right.cost_milli)
                .then(left.province.cmp(&right.province))
        });
        let mut chosen = Vec::new();
        let mut cost = 0_i64;
        for demand in demands {
            let next = cost.saturating_add(demand.cost_milli);
            if next <= sheet.score.total_milli {
                cost = next;
                chosen.push(demand.province);
            }
        }
        if chosen.is_empty() {
            return Some(chosen);
        }
        self.quote_peace(country, enemy, &chosen, date)
            .ok()
            .filter(|quote| quote.accepted)
            .map(|_| chosen)
    }

    /// 与原先的白和平门槛相同，并带回提出或跳过的原因。
    fn white_peace_choice(&self, country: &str, enemy: &str, date: GameDate) -> (bool, String) {
        let quote = match self.quote_peace(country, enemy, &[], date) {
            Ok(quote) => quote,
            Err(error) => return (false, error.to_string()),
        };
        if quote.score_milli >= 0 {
            return (false, "战争分数不低于 0".to_owned());
        }
        if quote.score_milli <= LOSING_WARSCORE_MILLI {
            return (true, "战争分数不高于 -25".to_owned());
        }
        let fielded = self
            .armies()
            .values()
            .filter(|army| army.country == country)
            .map(crate::Army::strength)
            .sum::<u64>();
        let recruiting = self
            .recruitments()
            .values()
            .flatten()
            .any(|order| order.country == country);
        if fielded == 0 && !recruiting {
            return (true, "没有可战部队，也没有招募".to_owned());
        }
        (false, "仍有部队或正在招募，且战争分数高于 -25".to_owned())
    }

    /// 战争期间在陆军上限内最多保持一笔步兵招募。维护费已经超过税收时不再下单。
    fn recruit_for_war(&mut self, country: &str, date: GameDate) -> RecruitChoice {
        let Some(limit) = self.country_land_force_limit(country) else {
            return skip_recruit("没有陆军上限");
        };
        let queued = self
            .recruitments()
            .values()
            .flatten()
            .filter(|order| order.country == country)
            .count() as u64;
        if queued > 0 {
            return skip_recruit("已经有招募订单");
        }
        let fielded = self
            .armies()
            .values()
            .filter(|army| army.country == country)
            .map(|army| army.regiments.len() as u64)
            .sum::<u64>();
        if fielded >= limit {
            return skip_recruit("已经达到陆军上限");
        }
        let Some(budget) = self.country_monthly_budget(country) else {
            return skip_recruit("没有月度预算");
        };
        if budget.army_maintenance > budget.tax {
            return skip_recruit("维护费已经超过税收");
        }
        let extra = self.military_rules.annual_maintenance(UnitKind::Infantry) / Money::from(12);
        let treasury = self
            .country(country)
            .map(|state| state.finances.treasury())
            .unwrap_or(Money::ZERO);
        if budget.army_maintenance + extra > budget.tax && treasury < extra * Money::from(12) {
            return skip_recruit("新增维护超出税收，且国库不足一年的维护");
        }
        let mut provinces: Vec<u32> = self.active_provinces.iter().copied().collect();
        provinces.sort_unstable();
        for province in provinces {
            if self
                .quote_recruitment(country, province, UnitKind::Infantry, date)
                .is_err()
            {
                continue;
            }
            return match self.start_recruitment(country, province, UnitKind::Infantry, date) {
                Ok(()) => RecruitChoice {
                    action: "order".to_owned(),
                    province: Some(province),
                    reason: "已下令招募步兵".to_owned(),
                },
                Err(error) => RecruitChoice {
                    action: "failed".to_owned(),
                    province: Some(province),
                    reason: error.to_string(),
                },
            };
        }
        skip_recruit("没有省份能够招募步兵")
    }

    fn order_armies(&mut self, country: &str, player: &str, date: GameDate) -> Vec<ArmyChoice> {
        let powers = self.army_powers();
        let snaps = self.own_snaps(country, &powers);
        let old = self.military.war_plans.clone();
        let picture = self.war_picture(country, player, &powers);
        let relieve = self.relieve_provinces(country, &picture);
        let relieve_at: BTreeSet<u32> = relieve.iter().copied().collect();
        let blocked = standing_hostiles(&picture);
        let mut assignments = BTreeMap::new();
        {
            let mut plan = Plan {
                country,
                snaps: &snaps,
                old: &old,
                picture: &picture,
                assignments: &mut assignments,
            };
            self.assign_relief(&mut plan, &relieve);
            let reserved = self.reserve_sieges(&plan);
            self.assign_reinforce(&mut plan, &powers, &reserved);
            let abandoned = self.assign_sieges(&mut plan);
            self.assign_assembly(&mut plan, &relieve_at, &blocked);
            self.assign_occupation(&mut plan);
            self.assign_recovery(&mut plan);
            self.assign_withdrawal(&mut plan, &abandoned);
        }
        self.apply_orders(country, date, &snaps, &assignments, &blocked)
    }

    /// 军队消失、属于玩家，或国家已经不在战争中时，计划不再保留。
    fn prune_war_plans(&mut self, player: &str) {
        let ids: Vec<u32> = self.military.war_plans.keys().copied().collect();
        let mut keep = BTreeSet::new();
        for id in ids {
            let Some(country) = self
                .military
                .armies
                .get(&id)
                .map(|army| army.country.clone())
            else {
                continue;
            };
            if country != player && self.wars_of(&country).next().is_some() {
                keep.insert(id);
            }
        }
        self.military.war_plans.retain(|id, _| keep.contains(id));
    }

    fn army_powers(&self) -> BTreeMap<u32, u64> {
        self.military
            .armies
            .iter()
            .map(|(&id, army)| (id, self.army_power(army)))
            .collect()
    }

    fn army_power(&self, army: &crate::Army) -> u64 {
        let max = self.country_morale_milli(&army.country);
        army.regiments.iter().fold(0, |total, regiment| {
            let pips = self.regiment_pips(army, regiment);
            total.saturating_add(regiment_power(
                regiment.strength,
                regiment.effective_morale(max),
                &pips,
            ))
        })
    }

    /// 团上没有点数时用国家当前兵种，再没有就是 0。
    fn regiment_pips(&self, army: &crate::Army, regiment: &crate::Regiment) -> UnitPips {
        if let Some(pips) = regiment.pips {
            return pips;
        }
        self.country_military(&army.country)
            .and_then(|military| military.unit(regiment.kind))
            .map(UnitPips::from_unit)
            .unwrap_or_else(zero_pips)
    }

    fn own_snaps(&self, country: &str, powers: &BTreeMap<u32, u64>) -> Vec<OwnArmy> {
        let max = self.country_morale_milli(country);
        let mut snaps = Vec::new();
        for (&id, army) in &self.military.armies {
            if army.country != country {
                continue;
            }
            let men = army.strength();
            let zero = men == 0 || army.regiments.is_empty();
            let speed = if army.regiments.is_empty() {
                Money::ZERO
            } else {
                army.march_speed(&self.military_rules)
            };
            // 流放和没有兵力的军队不会被派出去，省掉一次寻路。
            let travel = if army.is_exiled() || zero || speed <= Money::ZERO {
                BTreeMap::new()
            } else {
                self.land_travel_days(country, army.province, speed, false, &BTreeSet::new())
            };
            let morale_ready =
                !zero && u64::from(army.average_morale(max)).saturating_mul(2) >= u64::from(max);
            snaps.push(OwnArmy {
                id,
                province: army.province,
                dest: army.movement.as_ref().map(crate::ArmyMovement::destination),
                power: powers.get(&id).copied().unwrap_or(0),
                men,
                regiments: u32::try_from(army.regiments.len()).unwrap_or(u32::MAX),
                moving: army.movement.is_some(),
                in_battle: army.battle_id().is_some(),
                retreating: army.is_retreating(),
                exiled: army.is_exiled(),
                zero,
                morale_ready,
                travel,
            });
        }
        snaps
    }

    fn war_picture(&self, country: &str, player: &str, powers: &BTreeMap<u32, u64>) -> WarPicture {
        let mut picture = WarPicture {
            hostile_power: BTreeMap::new(),
            hostile_men: BTreeMap::new(),
            incoming_power: BTreeMap::new(),
            player_power: BTreeMap::new(),
            battles: self
                .military
                .battles
                .values()
                .map(crate::Battle::province)
                .collect(),
        };
        let player_friendly = !self.at_war(country, player);
        for (&id, army) in &self.military.armies {
            if army.is_retreating() || army.is_exiled() || army.strength() == 0 {
                continue;
            }
            let power = powers.get(&id).copied().unwrap_or(0);
            if player_friendly && army.country == player {
                *picture.player_power.entry(army.province).or_default() += power;
            }
            if army.country != country && self.at_war(country, &army.country) {
                *picture.hostile_power.entry(army.province).or_default() += power;
                *picture.hostile_men.entry(army.province).or_default() += army.strength();
                if let Some(movement) = &army.movement {
                    let dest = movement.destination();
                    if dest != army.province {
                        *picture.incoming_power.entry(dest).or_default() += power;
                    }
                }
            }
        }
        picture
    }

    /// 本国省份上的敌方围城，或敌军正停在我国要塞。没有敌军时不列任务。
    fn relieve_provinces(&self, country: &str, picture: &WarPicture) -> Vec<u32> {
        self.active_provinces
            .iter()
            .copied()
            .filter(|&id| {
                if value_at(&picture.hostile_power, id) == 0 {
                    return false;
                }
                let Some(state) = self.province(id) else {
                    return false;
                };
                if state.owner.as_deref() != Some(country) {
                    return false;
                }
                let enemy_siege = self.province_siege(id).is_some_and(|siege| {
                    siege.besieger != country && self.at_war(country, &siege.besieger)
                });
                enemy_siege || self.province_fort_level(id) > 0
            })
            .collect()
    }

    fn assign_relief(&self, plan: &mut Plan, targets: &[u32]) {
        let mut targets = targets.to_vec();
        order_targets(&mut targets, plan.snaps, plan.old, ROLE_RELIEVE);
        for target in targets {
            let enemy = value_at(&plan.picture.hostile_power, target);
            let base = present_battle_power(plan.snaps, target)
                + value_at(&plan.picture.player_power, target);
            let chosen = {
                let mut candidates = reachable(plan, P_RELIEVE, target, &BTreeSet::new());
                sort_by_travel(&mut candidates, plan.old, ROLE_RELIEVE, target);
                pick_until(&candidates, base, enemy)
            };
            for id in chosen {
                put(
                    plan.assignments,
                    id,
                    Assignment {
                        priority: P_RELIEVE,
                        role: ROLE_RELIEVE,
                        target,
                        muster: None,
                        destination: target,
                        avoid_hostile: false,
                        reason: format!("解围省份 {target}"),
                    },
                );
            }
        }
    }

    /// 已经在打的围城先占住刚好够人数的军队。凑不齐就不占，避免挡住增援。
    fn reserve_sieges(&self, plan: &Plan) -> BTreeSet<u32> {
        let mut reserved = BTreeSet::new();
        for target in self.siege_targets(plan.country) {
            if self.siege_committed(plan, target)
                && let Some(pick) = self.choose_siege(plan, target)
            {
                reserved.extend(pick.ids);
            }
        }
        reserved
    }

    fn assign_reinforce(
        &self,
        plan: &mut Plan,
        powers: &BTreeMap<u32, u64>,
        reserved: &BTreeSet<u32>,
    ) {
        let battles: Vec<(u32, [Vec<u32>; 2])> = self
            .military
            .battles
            .values()
            .map(|battle| {
                (
                    battle.province(),
                    [
                        battle.side_armies(0).to_vec(),
                        battle.side_armies(1).to_vec(),
                    ],
                )
            })
            .collect();
        let mut needs = Vec::new();
        for (province, sides) in &battles {
            let our = (0..2).find(|&side| {
                sides[side].iter().any(|id| {
                    self.military
                        .armies
                        .get(id)
                        .is_some_and(|army| army.country == plan.country)
                })
            });
            let Some(our) = our else {
                continue;
            };
            let our_power = side_power(&sides[our], powers);
            let enemy_power = side_power(&sides[1 - our], powers);
            if our_power > enemy_power {
                continue;
            }
            needs.push((*province, our_power, enemy_power));
        }
        let mut targets: Vec<u32> = needs.iter().map(|(province, _, _)| *province).collect();
        order_targets(&mut targets, plan.snaps, plan.old, ROLE_REINFORCE);
        for target in targets {
            let Some((_, our_power, enemy_power)) =
                needs.iter().find(|(province, _, _)| *province == target)
            else {
                continue;
            };
            let chosen = {
                let mut candidates: Vec<&OwnArmy> = reachable(plan, P_REINFORCE, target, reserved)
                    .into_iter()
                    .filter(|army| {
                        army.travel
                            .get(&target)
                            .is_some_and(|days| *days <= REINFORCE_WITHIN_DAYS)
                    })
                    .collect();
                sort_by_travel(&mut candidates, plan.old, ROLE_REINFORCE, target);
                pick_until(&candidates, *our_power, *enemy_power)
            };
            for id in chosen {
                put(
                    plan.assignments,
                    id,
                    Assignment {
                        priority: P_REINFORCE,
                        role: ROLE_REINFORCE,
                        target,
                        muster: None,
                        destination: target,
                        avoid_hostile: false,
                        reason: format!("增援省份 {target} 的战斗"),
                    },
                );
            }
        }
    }

    fn assign_sieges(&self, plan: &mut Plan) -> BTreeSet<u32> {
        let mut targets = self.siege_targets(plan.country);
        order_targets(&mut targets, plan.snaps, plan.old, ROLE_SIEGE);
        let mut abandoned = BTreeSet::new();
        for target in targets {
            let committed = self.siege_committed(plan, target);
            let Some(pick) = self.choose_siege(plan, target) else {
                if committed {
                    abandoned.insert(target);
                }
                continue;
            };
            let reason = if pick.threat_added {
                format!("围攻省份 {target}，补上一支以超过来援")
            } else if pick.short_supply {
                format!("围攻省份 {target}，补给不足，仍留下所需兵力")
            } else {
                format!("围攻省份 {target}")
            };
            for id in pick.ids {
                put(
                    plan.assignments,
                    id,
                    Assignment {
                        priority: P_SIEGE,
                        role: ROLE_SIEGE,
                        target,
                        muster: None,
                        destination: target,
                        avoid_hostile: false,
                        reason: reason.clone(),
                    },
                );
            }
        }
        abandoned
    }

    fn siege_targets(&self, country: &str) -> Vec<u32> {
        self.active_provinces
            .iter()
            .copied()
            .filter(|&id| {
                let Some(state) = self.province(id) else {
                    return false;
                };
                let Some(owner) = state.owner.as_deref() else {
                    return false;
                };
                owner != country
                    && self.at_war(country, owner)
                    && state.controller.as_deref() != Some(country)
                    && self.siege_required_men(id).is_some()
            })
            .collect()
    }

    /// 围城进度、已经站在要塞上，或昨天的围城计划，都算已经承诺。
    fn siege_committed(&self, plan: &Plan, target: u32) -> bool {
        let underway = self
            .province_siege(target)
            .is_some_and(|siege| siege.besieger == plan.country);
        let present = plan.snaps.iter().any(|army| {
            army.province == target
                && army.morale_ready
                && !army.in_battle
                && !army.retreating
                && !army.exiled
                && army.men > 0
        });
        let planned = plan.snaps.iter().any(|army| {
            plan.old
                .get(&army.id)
                .is_some_and(|saved| saved.role == ROLE_SIEGE && saved.target == target)
        });
        underway || present || planned
    }

    /// 先按补给挑最少的军队。放不进上限时仍留下满足人数的一组，并在原因里说明。
    /// 来援战力已经超过围城军时，只再补一支；补不上就放弃。相等时不撤。
    fn choose_siege(&self, plan: &Plan, target: u32) -> Option<SiegePick> {
        if value_at(&plan.picture.hostile_power, target) > 0 {
            return None;
        }
        let required = self.siege_required_men(target)?;
        if required == 0 {
            return None;
        }
        let supply = self.province_supply_limit(target);
        let mut candidates = reachable(plan, P_SIEGE, target, &BTreeSet::new());
        candidates.sort_by(|left, right| {
            let left_key = siege_key(left, plan.old, target);
            let right_key = siege_key(right, plan.old, target);
            left_key.cmp(&right_key)
        });
        let (mut ids, short_supply) =
            if let Some(ids) = gather(&candidates, required, supply, false) {
                (ids, false)
            } else {
                (gather(&candidates, required, supply, true)?, true)
            };
        let threat = value_at(&plan.picture.incoming_power, target);
        let stack = ids
            .iter()
            .map(|id| snap(plan.snaps, *id).power)
            .fold(0, u64::saturating_add);
        let mut threat_added = false;
        if threat > stack {
            let mut extras: Vec<&OwnArmy> = candidates
                .iter()
                .copied()
                .filter(|army| !ids.contains(&army.id))
                .collect();
            extras.sort_by_key(|army| (army.travel[&target], army.id));
            let extra = extras
                .into_iter()
                .find(|army| stack.saturating_add(army.power) > threat)?;
            ids.push(extra.id);
            threat_added = true;
        }
        Some(SiegePick {
            ids,
            short_supply,
            threat_added,
        })
    }

    fn assign_assembly(&self, plan: &mut Plan, relieve: &BTreeSet<u32>, blocked: &BTreeSet<u32>) {
        let mut targets: Vec<u32> = plan
            .picture
            .hostile_power
            .iter()
            .filter(|&(&province, &power)| {
                power > 0
                    && !relieve.contains(&province)
                    && !plan.picture.battles.contains(&province)
            })
            .map(|(&province, _)| province)
            .collect();
        order_targets(&mut targets, plan.snaps, plan.old, ROLE_ASSEMBLE);
        if targets.is_empty() {
            return;
        }
        // 敌军已经站着的本国省不能当集合点，否则先到的一军会单独接战。没有安全集合点就不出发。
        let musters: Vec<u32> = self
            .owned_provinces(plan.country)
            .into_iter()
            .filter(|&province| value_at(&plan.picture.hostile_power, province) == 0)
            .collect();
        // 到齐前的天数也要避开这些省。普通最短路会抄近路穿过敌军，先到的一军会单独接战。
        let safe = self.safe_assembly_days(plan.country, plan.snaps, blocked);
        for target in targets {
            let enemy = value_at(&plan.picture.hostile_power, target);
            let solo = {
                let candidates = reachable(plan, P_ASSEMBLE, target, &BTreeSet::new());
                if candidates.is_empty() {
                    None
                } else {
                    self.pick_solo(plan, target, &candidates, enemy)
                }
            };
            if let Some(id) = solo {
                put(
                    plan.assignments,
                    id,
                    Assignment {
                        priority: P_ASSEMBLE,
                        role: ROLE_ASSEMBLE,
                        target,
                        muster: None,
                        destination: target,
                        avoid_hostile: false,
                        reason: format!("单独超过敌军，进攻省份 {target}"),
                    },
                );
                continue;
            }
            let Some(group) = ({
                let candidates = reachable(plan, P_ASSEMBLE, target, &BTreeSet::new());
                self.pick_group(plan, target, &candidates, &musters, enemy, &safe)
            }) else {
                continue;
            };
            let reason = if group.destination == target {
                format!("集结完成，进入省份 {target}")
            } else {
                format!("在省份 {} 集结，准备进攻省份 {target}", group.muster)
            };
            // 全员到齐后目的地改成目标省，这时必须进入敌军所在省，不再绕开。
            let avoid_hostile = group.destination != target;
            for id in group.ids {
                put(
                    plan.assignments,
                    id,
                    Assignment {
                        priority: P_ASSEMBLE,
                        role: ROLE_ASSEMBLE,
                        target,
                        muster: Some(group.muster),
                        destination: group.destination,
                        avoid_hostile,
                        reason: reason.clone(),
                    },
                );
            }
        }
    }

    /// 每支还能出动的军队，到齐前不进入 `blocked` 的最短天数。起点本身仍算 0 天。
    fn safe_assembly_days(
        &self,
        country: &str,
        snaps: &[OwnArmy],
        blocked: &BTreeSet<u32>,
    ) -> BTreeMap<u32, BTreeMap<u32, u32>> {
        snaps
            .iter()
            .filter(|army| army.morale_ready && !army.locked() && army.men > 0)
            .filter_map(|army| {
                let unit = self.military.armies.get(&army.id)?;
                let speed = unit.march_speed(&self.military_rules);
                Some((
                    army.id,
                    self.land_travel_days(country, army.province, speed, false, blocked),
                ))
            })
            .collect()
    }

    fn pick_solo(
        &self,
        plan: &Plan,
        target: u32,
        candidates: &[&OwnArmy],
        enemy: u64,
    ) -> Option<u32> {
        let mut solos: Vec<&OwnArmy> = candidates
            .iter()
            .copied()
            .filter(|army| {
                army.power > enemy
                    && self.retreat_allows(plan.country, target, army.men, plan.picture)
            })
            .collect();
        solos.sort_by_key(|army| {
            let sticky = u8::from(!plan.old.get(&army.id).is_some_and(|saved| {
                saved.role == ROLE_ASSEMBLE && saved.target == target && saved.muster.is_none()
            }));
            (sticky, army.travel[&target], army.id)
        });
        solos.first().map(|army| army.id)
    }

    fn pick_group(
        &self,
        plan: &Plan,
        target: u32,
        candidates: &[&OwnArmy],
        musters: &[u32],
        enemy: u64,
        safe: &BTreeMap<u32, BTreeMap<u32, u32>>,
    ) -> Option<GroupPick> {
        let mut selected: Option<GroupAt> = None;
        if let Some(muster) = common_muster(candidates, plan.old, target)
            && musters.contains(&muster)
            && let Some(group) = self.group_at(plan, target, muster, candidates, enemy, safe)
        {
            selected = Some(group);
        }
        if selected.is_none() {
            for &muster in musters {
                let Some(group) = self.group_at(plan, target, muster, candidates, enemy, safe)
                else {
                    continue;
                };
                let better = selected.as_ref().is_none_or(|current| {
                    (group.max_days, group.muster) < (current.max_days, current.muster)
                });
                if better {
                    selected = Some(group);
                }
            }
        }
        let group = selected?;
        let destination = self.group_destination(plan, target, group.muster, &group.ids, group.men);
        Some(GroupPick {
            muster: group.muster,
            destination,
            ids: group.ids,
        })
    }

    fn group_at(
        &self,
        plan: &Plan,
        target: u32,
        muster: u32,
        candidates: &[&OwnArmy],
        enemy: u64,
        safe: &BTreeMap<u32, BTreeMap<u32, u32>>,
    ) -> Option<GroupAt> {
        let days_of = |army: &OwnArmy| safe_days(safe, army.id, muster);
        let mut pool: Vec<&OwnArmy> = candidates
            .iter()
            .copied()
            .filter(|army| days_of(army).is_some())
            .collect();
        pool.sort_by_key(|army| {
            let sticky = u8::from(!plan.old.get(&army.id).is_some_and(|saved| {
                saved.role == ROLE_ASSEMBLE
                    && saved.target == target
                    && saved.muster == Some(muster)
            }));
            (sticky, days_of(army).unwrap_or(u32::MAX), army.id)
        });
        let mut ids = Vec::new();
        let mut power = present_battle_power(plan.snaps, target);
        let mut men = 0_u64;
        let mut max_days = 0_u32;
        for army in pool {
            let ready = power > enemy
                && !ids.is_empty()
                && self.retreat_allows(plan.country, target, men, plan.picture);
            if ready {
                break;
            }
            ids.push(army.id);
            power = power.saturating_add(army.power);
            men = men.saturating_add(army.men);
            max_days = max_days.max(days_of(army).unwrap_or(u32::MAX));
        }
        if power > enemy && self.retreat_allows(plan.country, target, men, plan.picture) {
            Some(GroupAt {
                muster,
                max_days,
                ids,
                men,
            })
        } else {
            None
        }
    }

    /// 全员还没在集合省停稳时，目的地保持集合省。已经一起出发后不再拉回去。
    fn group_destination(
        &self,
        plan: &Plan,
        target: u32,
        muster: u32,
        ids: &[u32],
        men: u64,
    ) -> u32 {
        let ready = ids.iter().all(|id| {
            let army = snap(plan.snaps, *id);
            !army.moving && !army.in_battle && army.province == muster
        });
        let en_route = ids.iter().all(|id| {
            let army = snap(plan.snaps, *id);
            army.province == target || (army.moving && army.dest == Some(target))
        });
        if (ready || en_route) && self.retreat_allows(plan.country, target, men, plan.picture) {
            target
        } else {
            muster
        }
    }

    fn owned_provinces(&self, country: &str) -> Vec<u32> {
        self.active_provinces
            .iter()
            .copied()
            .filter(|&id| {
                self.province(id)
                    .is_some_and(|state| state.owner.as_deref() == Some(country))
                    && self.can_enter(country, id)
            })
            .collect()
    }

    /// 从拟定目标看相邻土地。能进入，且敌军人数不触发退路封锁，才可以进攻。
    fn retreat_allows(
        &self,
        country: &str,
        target: u32,
        our_men: u64,
        picture: &WarPicture,
    ) -> bool {
        self.land_neighbors(target).is_some_and(|neighbors| {
            neighbors.keys().any(|&next| {
                next != target
                    && self.can_enter(country, next)
                    && !enemy_strength_blocks_retreat(our_men, value_at(&picture.hostile_men, next))
            })
        })
    }

    fn assign_occupation(&self, plan: &mut Plan) {
        let mut targets: Vec<u32> = self
            .active_provinces
            .iter()
            .copied()
            .filter(|&id| self.can_occupy(plan.country, id, plan.picture))
            .collect();
        order_targets(&mut targets, plan.snaps, plan.old, ROLE_OCCUPY);
        for target in targets {
            let chosen = {
                let mut candidates: Vec<&OwnArmy> =
                    reachable(plan, P_OCCUPY, target, &BTreeSet::new())
                        .into_iter()
                        .filter(|army| army.men >= u64::from(REGIMENT_STRENGTH))
                        .collect();
                candidates.sort_by_key(|army| {
                    let present = u8::from(army.province != target);
                    let sticky = u8::from(!has_plan(plan.old, army.id, ROLE_OCCUPY, target));
                    (present, sticky, army.travel[&target], army.id)
                });
                candidates.first().map(|army| army.id)
            };
            if let Some(id) = chosen {
                put(
                    plan.assignments,
                    id,
                    Assignment {
                        priority: P_OCCUPY,
                        role: ROLE_OCCUPY,
                        target,
                        muster: None,
                        destination: target,
                        avoid_hostile: false,
                        reason: format!("占领无要塞省份 {target}"),
                    },
                );
            }
        }
    }

    fn can_occupy(&self, country: &str, province: u32, picture: &WarPicture) -> bool {
        if self.province_fort_level(province) > 0 || value_at(&picture.hostile_men, province) > 0 {
            return false;
        }
        let Some(state) = self.province(province) else {
            return false;
        };
        let Some(owner) = state.owner.as_deref() else {
            return false;
        };
        owner != country
            && self.at_war(country, owner)
            && state.controller.as_deref() != Some(country)
    }

    fn assign_recovery(&self, plan: &mut Plan) {
        for army in plan.snaps {
            if army.locked() || army.morale_ready || plan.assignments.contains_key(&army.id) {
                continue;
            }
            let saved = plan
                .old
                .get(&army.id)
                .filter(|saved| saved.role == ROLE_RECOVER)
                .map(|saved| saved.target)
                .filter(|&province| {
                    self.is_safe_home(plan.country, province) && army.travel.contains_key(&province)
                });
            let Some(destination) = saved.or_else(|| self.nearest_home(plan.country, &army.travel))
            else {
                continue;
            };
            put(
                plan.assignments,
                army.id,
                Assignment {
                    priority: P_RECOVER,
                    role: ROLE_RECOVER,
                    target: destination,
                    muster: None,
                    destination,
                    avoid_hostile: false,
                    reason: format!("士气不足，返回省份 {destination}"),
                },
            );
        }
    }

    /// 没有任务、却正走向打不赢或打平的省份，围城已经放弃，或定额之外仍停在围城、占领目标上时，改回本国控制的省份。
    fn assign_withdrawal(&self, plan: &mut Plan, abandoned: &BTreeSet<u32>) {
        let crowded_siege: BTreeSet<u32> = plan
            .assignments
            .values()
            .filter(|task| task.role == ROLE_SIEGE)
            .map(|task| task.target)
            .collect();
        let crowded_occupy: BTreeSet<u32> = plan
            .assignments
            .values()
            .filter(|task| task.role == ROLE_OCCUPY)
            .map(|task| task.target)
            .collect();
        for army in plan.snaps {
            if army.locked() || !army.morale_ready || plan.assignments.contains_key(&army.id) {
                continue;
            }
            let heading = if army.moving {
                army.dest
            } else {
                Some(army.province)
            };
            let Some(province) = heading else {
                continue;
            };
            // 持平不算超过。已经在走的单军不能继续进入。
            let into_lost =
                army.moving && value_at(&plan.picture.hostile_power, province) >= army.power;
            let leave_siege = abandoned.contains(&province);
            let spare_siege = crowded_siege.contains(&province);
            let spare_occupy = crowded_occupy.contains(&province);
            if !into_lost && !leave_siege && !spare_siege && !spare_occupy {
                continue;
            }
            let Some(home) = self.nearest_home(plan.country, &army.travel) else {
                continue;
            };
            if !army.moving && home == army.province {
                continue;
            }
            let reason = if spare_siege {
                format!("围攻省份 {province} 的兵力已经够，撤回")
            } else if spare_occupy {
                format!("占领省份 {province} 的兵力已经够，撤回")
            } else if leave_siege {
                format!("无法守住省份 {province} 的围城，撤回")
            } else {
                format!("兵力不足，从省份 {province} 撤回")
            };
            put(
                plan.assignments,
                army.id,
                Assignment {
                    priority: P_RECOVER,
                    role: ROLE_RECOVER,
                    target: home,
                    muster: None,
                    destination: home,
                    avoid_hostile: false,
                    reason,
                },
            );
        }
    }

    fn nearest_home(&self, country: &str, travel: &BTreeMap<u32, u32>) -> Option<u32> {
        let mut best: Option<(u32, u32)> = None;
        for (&province, &days) in travel {
            if !self.is_safe_home(country, province) {
                continue;
            }
            let candidate = (days, province);
            if best.is_none_or(|current| candidate < current) {
                best = Some(candidate);
            }
        }
        best.map(|(_, province)| province)
    }

    fn is_safe_home(&self, country: &str, province: u32) -> bool {
        self.province(province).is_some_and(|state| {
            state.owner.as_deref() == Some(country) && state.controller.as_deref() == Some(country)
        })
    }

    fn apply_orders(
        &mut self,
        country: &str,
        date: GameDate,
        snaps: &[OwnArmy],
        assignments: &BTreeMap<u32, Assignment>,
        blocked: &BTreeSet<u32>,
    ) -> Vec<ArmyChoice> {
        let mut choices = Vec::new();
        for army in snaps {
            if army.locked() {
                self.military.war_plans.remove(&army.id);
                let reason = if army.in_battle {
                    "正在交战"
                } else if army.retreating {
                    "正在撤退"
                } else if army.exiled {
                    "正在流放"
                } else {
                    "没有兵力"
                };
                choices.push(stay(army.id, reason));
                continue;
            }
            let Some(task) = assignments.get(&army.id) else {
                self.military.war_plans.remove(&army.id);
                let reason = if army.morale_ready {
                    "没有更合适的交战或占领目标"
                } else {
                    "士气不足，留在原地恢复"
                };
                choices.push(stay(army.id, reason));
                continue;
            };
            let choice = self.issue_order(country, army, task, date, blocked);
            self.store_plan(army.id, task, date);
            choices.push(choice);
        }
        choices
    }

    fn issue_order(
        &mut self,
        country: &str,
        army: &OwnArmy,
        task: &Assignment,
        date: GameDate,
        blocked: &BTreeSet<u32>,
    ) -> ArmyChoice {
        let same = if army.moving {
            army.dest == Some(task.destination)
        } else {
            army.province == task.destination
        };
        // 目的地没变、且剩下的路段不再进入敌军省时，不重复下令。
        // 已经在走的当前路段会保留；只重算后面的路。
        let crosses = task.avoid_hostile && self.remaining_legs_enter(army.id, blocked);
        if same && !crosses {
            return task_choice(army.id, "stay", task);
        }
        let moved = if task.avoid_hostile {
            self.move_army_avoiding(country, army.id, task.destination, date, blocked)
        } else {
            self.move_army(country, army.id, task.destination, date)
        };
        match moved {
            Ok(()) => task_choice(army.id, "move", task),
            Err(error) => ArmyChoice {
                id: army.id,
                action: "failed".to_owned(),
                destination: Some(task.destination),
                rank: Some(task.priority),
                role: Some(task.role.to_owned()),
                reason: error.to_string(),
            },
        }
    }

    /// 从当前路段起，是否还会进入已经有敌军的省。
    fn remaining_legs_enter(&self, id: u32, blocked: &BTreeSet<u32>) -> bool {
        self.military.armies.get(&id).is_some_and(|army| {
            army.movement.as_ref().is_some_and(|movement| {
                movement.legs()[movement.leg_index()..]
                    .iter()
                    .any(|leg| blocked.contains(&leg.to))
            })
        })
    }

    fn store_plan(&mut self, id: u32, task: &Assignment, date: GameDate) {
        let unchanged = self.military.war_plans.get(&id).is_some_and(|saved| {
            saved.role == task.role && saved.target == task.target && saved.muster == task.muster
        });
        if unchanged {
            return;
        }
        self.military.war_plans.insert(
            id,
            ArmyPlan {
                role: task.role.to_owned(),
                target: task.target,
                muster: task.muster,
                since: date,
            },
        );
    }
}

/// 训练度和伤害修正在仓库里还没有，因子保持 1，不乘进战力。
/// 士气多项式是 `6.233 + 2.057*(M-4) + 0.082*(M-4)^2`，千分定点，负值当作 0。
fn morale_factor_milli(morale_milli: u32) -> u64 {
    let delta = i64::from(morale_milli) - MORALE_ORIGIN_MILLI;
    let linear = MORALE_POW1_MILLI * delta / 1_000;
    let square = MORALE_POW2_MILLI * delta * delta / 1_000_000;
    let factor = MORALE_POW0_MILLI + linear + square;
    if factor > 0 {
        u64::try_from(factor).unwrap_or(0)
    } else {
        0
    }
}

/// `COMBAT_PIP_IMPORTANCE` 连乘。指数 0 是 1000，指数 1 截成 1045。
fn pip_factor_milli(exponent: u32) -> u64 {
    let mut factor: u128 = 1_000;
    for _ in 0..exponent {
        factor = factor * PIP_IMPORTANCE_NUM / PIP_IMPORTANCE_DEN;
    }
    u64::try_from(factor).unwrap_or(u64::MAX)
}

fn regiment_power(men: u32, morale_milli: u32, pips: &UnitPips) -> u64 {
    let exponent = FIRE_PIP_MULT * (pips.offensive_fire + pips.defensive_fire)
        + SHOCK_PIP_MULT * (pips.offensive_shock + pips.defensive_shock)
        + MORALE_PIP_MULT * (pips.offensive_morale + pips.defensive_morale);
    let mut power = u128::from(men);
    power = power * u128::from(morale_factor_milli(morale_milli)) / 1_000;
    power = power * u128::from(pip_factor_milli(exponent)) / 1_000;
    u64::try_from(power).unwrap_or(u64::MAX)
}

fn zero_pips() -> UnitPips {
    UnitPips {
        offensive_fire: 0,
        defensive_fire: 0,
        offensive_shock: 0,
        defensive_shock: 0,
        offensive_morale: 0,
        defensive_morale: 0,
        maneuver: 0,
    }
}

fn skip_recruit(reason: &str) -> RecruitChoice {
    RecruitChoice {
        action: "skip".to_owned(),
        province: None,
        reason: reason.to_owned(),
    }
}

fn stay(id: u32, reason: &str) -> ArmyChoice {
    ArmyChoice {
        id,
        action: "stay".to_owned(),
        destination: None,
        rank: None,
        role: None,
        reason: reason.to_owned(),
    }
}

fn task_choice(id: u32, action: &str, task: &Assignment) -> ArmyChoice {
    ArmyChoice {
        id,
        action: action.to_owned(),
        destination: Some(task.destination),
        rank: Some(task.priority),
        role: Some(task.role.to_owned()),
        reason: task.reason.clone(),
    }
}

fn value_at(map: &BTreeMap<u32, u64>, province: u32) -> u64 {
    map.get(&province).copied().unwrap_or(0)
}

fn standing_hostiles(picture: &WarPicture) -> BTreeSet<u32> {
    picture
        .hostile_power
        .iter()
        .filter(|&(_, &power)| power > 0)
        .map(|(&province, _)| province)
        .collect()
}

fn safe_days(safe: &BTreeMap<u32, BTreeMap<u32, u32>>, id: u32, province: u32) -> Option<u32> {
    safe.get(&id).and_then(|days| days.get(&province).copied())
}

fn present_battle_power(snaps: &[OwnArmy], province: u32) -> u64 {
    snaps
        .iter()
        .filter(|army| army.province == province && army.in_battle && army.men > 0)
        .map(|army| army.power)
        .sum()
}

fn side_power(ids: &[u32], powers: &BTreeMap<u32, u64>) -> u64 {
    ids.iter()
        .map(|id| powers.get(id).copied().unwrap_or(0))
        .sum()
}

fn snap(snaps: &[OwnArmy], id: u32) -> &OwnArmy {
    snaps
        .iter()
        .find(|army| army.id == id)
        .expect("分配只引用本国军队")
}

fn has_plan(old: &BTreeMap<u32, ArmyPlan>, id: u32, role: &str, target: u32) -> bool {
    old.get(&id)
        .is_some_and(|saved| saved.role == role && saved.target == target)
}

fn order_targets(
    targets: &mut [u32],
    snaps: &[OwnArmy],
    old: &BTreeMap<u32, ArmyPlan>,
    role: &str,
) {
    targets.sort_by_key(|target| {
        let sticky = snaps
            .iter()
            .any(|army| has_plan(old, army.id, role, *target));
        (u8::from(!sticky), *target)
    });
}

fn reachable<'a>(
    plan: &'a Plan,
    priority: u8,
    target: u32,
    reserved: &BTreeSet<u32>,
) -> Vec<&'a OwnArmy> {
    plan.snaps
        .iter()
        .filter(|army| {
            army.morale_ready
                && !army.locked()
                && army.men > 0
                && army.travel.contains_key(&target)
                && !reserved.contains(&army.id)
                && plan
                    .assignments
                    .get(&army.id)
                    .is_none_or(|task| task.priority > priority)
        })
        .collect()
}

fn sort_by_travel(
    candidates: &mut [&OwnArmy],
    old: &BTreeMap<u32, ArmyPlan>,
    role: &str,
    target: u32,
) {
    candidates.sort_by_key(|army| {
        let sticky = u8::from(!has_plan(old, army.id, role, target));
        (sticky, army.travel[&target], army.id)
    });
}

fn siege_key(
    army: &OwnArmy,
    old: &BTreeMap<u32, ArmyPlan>,
    target: u32,
) -> (u8, u8, u32, Reverse<u64>, u32) {
    let sticky = u8::from(!has_plan(old, army.id, ROLE_SIEGE, target));
    let present = u8::from(army.province != target);
    (
        sticky,
        present,
        army.travel[&target],
        Reverse(army.men),
        army.id,
    )
}

fn gather(
    candidates: &[&OwnArmy],
    required: u64,
    supply: Option<u32>,
    ignore_supply: bool,
) -> Option<Vec<u32>> {
    let mut ids = Vec::new();
    let mut men = 0_u64;
    let mut regiments = 0_u32;
    for army in candidates {
        if men >= required {
            break;
        }
        let next = regiments.saturating_add(army.regiments);
        if !ignore_supply && supply.is_some_and(|limit| next > limit) {
            continue;
        }
        ids.push(army.id);
        men = men.saturating_add(army.men);
        regiments = next;
    }
    (men >= required).then_some(ids)
}

/// 已经超过敌方就不再加人。加完仍然不超过时一个人都不派。
fn pick_until(candidates: &[&OwnArmy], base: u64, enemy: u64) -> Vec<u32> {
    if base > enemy {
        return Vec::new();
    }
    let mut total = base;
    let mut chosen = Vec::new();
    for army in candidates {
        if total > enemy {
            break;
        }
        chosen.push(army.id);
        total = total.saturating_add(army.power);
    }
    if total > enemy { chosen } else { Vec::new() }
}

fn put(assignments: &mut BTreeMap<u32, Assignment>, id: u32, task: Assignment) {
    if assignments
        .get(&id)
        .is_some_and(|current| current.priority <= task.priority)
    {
        return;
    }
    assignments.insert(id, task);
}

fn common_muster(
    candidates: &[&OwnArmy],
    old: &BTreeMap<u32, ArmyPlan>,
    target: u32,
) -> Option<u32> {
    let mut found: Option<u32> = None;
    for army in candidates {
        let Some(saved) = old.get(&army.id) else {
            continue;
        };
        if saved.role != ROLE_ASSEMBLE || saved.target != target {
            continue;
        }
        let Some(muster) = saved.muster else {
            continue;
        };
        match found {
            Some(current) if current != muster => return None,
            Some(_) => {}
            None => found = Some(muster),
        }
    }
    found
}

struct Plan<'a> {
    country: &'a str,
    snaps: &'a [OwnArmy],
    old: &'a BTreeMap<u32, ArmyPlan>,
    picture: &'a WarPicture,
    assignments: &'a mut BTreeMap<u32, Assignment>,
}

struct OwnArmy {
    id: u32,
    province: u32,
    dest: Option<u32>,
    power: u64,
    men: u64,
    regiments: u32,
    moving: bool,
    in_battle: bool,
    retreating: bool,
    exiled: bool,
    zero: bool,
    morale_ready: bool,
    travel: BTreeMap<u32, u32>,
}

impl OwnArmy {
    fn locked(&self) -> bool {
        self.in_battle || self.retreating || self.exiled || self.zero
    }
}

struct Assignment {
    priority: u8,
    role: &'static str,
    target: u32,
    muster: Option<u32>,
    destination: u32,
    /// 集结还没到齐时，去集合点的路不能进入已经有敌军的省。
    avoid_hostile: bool,
    reason: String,
}

struct WarPicture {
    hostile_power: BTreeMap<u32, u64>,
    hostile_men: BTreeMap<u32, u64>,
    incoming_power: BTreeMap<u32, u64>,
    player_power: BTreeMap<u32, u64>,
    battles: BTreeSet<u32>,
}

struct SiegePick {
    ids: Vec<u32>,
    short_supply: bool,
    threat_added: bool,
}

struct GroupAt {
    muster: u32,
    max_days: u32,
    ids: Vec<u32>,
    men: u64,
}

struct GroupPick {
    muster: u32,
    destination: u32,
    ids: Vec<u32>,
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, time::Duration};

    use crate::{
        Army, CombatRules, Country, Development, GameDate, GameSpeed, LandConnection,
        ManpowerRules, MilitaryRules, Money, Province, ProvinceKind, Regiment, SavedGame,
        Simulation, UnitKind, military::ArmyPlan,
    };

    use super::World;

    fn date() -> GameDate {
        GameDate::new(1444, 11, 11).unwrap()
    }

    fn province(owner: &str) -> Province {
        let mut province = Province::new(ProvinceKind::Land);
        province.owner = Some(owner.to_owned());
        province.controller = Some(owner.to_owned());
        province.development = Development {
            tax: Some(3),
            production: Some(3),
            manpower: Some(3),
        };
        province
    }

    fn rules(forcelimit: bool, maintenance: bool) -> MilitaryRules {
        MilitaryRules {
            infantry_cost: Money::from(10),
            cavalry_cost: Money::from(25),
            infantry_days: 60,
            cavalry_days: 90,
            infantry_speed: Money::from(1),
            cavalry_speed: Money::from(1),
            recruitment_full_refund_progress: Money::new(1, 1),
            land_maintenance_factor: if maintenance {
                Money::from(100)
            } else {
                Money::ZERO
            },
            monthly_reinforce: Money::ZERO,
            extra_land_reinforce_cost: Money::ZERO,
            supply_base: 6,
            base_land_forcelimit: Money::ZERO,
            development_land_forcelimit: if forcelimit { Money::ONE } else { Money::ZERO },
            land_forcelimit_extra_cost_factor: Money::from(2),
            goods_land_forcelimit: BTreeMap::new(),
            tax_recruitment_time: Money::ZERO,
            starting_army_size: Money::ZERO,
            starting_army_size_at_war: Money::ZERO,
            manpower: ManpowerRules {
                base_capacity: 20_000,
                per_development: 0,
                recovery_months: 10,
                minimum_monthly: 1,
                infantry_cost: 1_000,
                cavalry_cost: 1_000,
            },
            combat: CombatRules::baseline(),
        }
    }

    fn link(from: u32, to: u32, distance: u32) -> LandConnection {
        LandConnection { from, to, distance }
    }

    fn sample(maintenance: bool, links: Vec<LandConnection>) -> World {
        let mut world = World::new(
            [
                Country::new("AAA"),
                Country::new("BBB"),
                Country::new("CCC"),
            ],
            BTreeMap::from([
                (1, province("AAA")),
                (2, province("BBB")),
                (3, province("CCC")),
            ]),
            [1, 2, 3],
            Vec::<crate::BuildingDefinition>::new(),
            rules(true, maintenance),
            links,
        )
        .unwrap();
        for tag in ["AAA", "BBB", "CCC"] {
            world
                .countries
                .get_mut(tag)
                .unwrap()
                .finances
                .refund(Money::from(1_000));
        }
        world
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

    fn advance(world: &mut World, days: u32, speed: GameSpeed) -> Simulation {
        let mut simulation = Simulation::new(date());
        simulation.set_paused(false);
        simulation.set_speed(speed);
        let elapsed = Duration::from_secs(u64::from(days)) / speed.multiplier();
        assert_eq!(simulation.advance_for(world, elapsed, Some("AAA")), days);
        simulation
    }

    #[test]
    fn neighbor_orders_its_own_army_and_leaves_the_player_alone() {
        let mut world = sample(false, vec![link(1, 2, 10), link(2, 3, 1), link(3, 1, 30)]);
        world.declare_war("AAA", "BBB", date()).unwrap();
        station(&mut world, 1, "AAA", 1, 1);
        station(&mut world, 2, "BBB", 2, 2);
        station(&mut world, 3, "CCC", 3, 1);
        advance(&mut world, 1, GameSpeed::Normal);

        assert!(world.armies()[&1].movement.is_none());
        assert!(world.armies()[&3].movement.is_none());
        let movement = world.armies()[&2].movement.as_ref().unwrap();
        assert_eq!(movement.destination(), 1);
        assert!(
            movement
                .legs()
                .iter()
                .all(|leg| leg.from != 3 && leg.to != 3)
        );
        let ongoing = world.armies()[&2].movement.clone();
        advance(&mut world, 2, GameSpeed::Normal);
        assert_eq!(world.armies()[&2].province, 2);
        assert_eq!(world.armies()[&2].movement, ongoing);
    }

    #[test]
    fn closed_neutral_border_is_not_crossed_and_weak_or_tired_armies_stay() {
        let mut blocked = sample(false, vec![link(2, 3, 1), link(3, 1, 1)]);
        blocked.declare_war("AAA", "BBB", date()).unwrap();
        station(&mut blocked, 1, "AAA", 1, 1);
        station(&mut blocked, 2, "BBB", 2, 2);
        advance(&mut blocked, 1, GameSpeed::Normal);
        assert!(blocked.armies()[&2].movement.is_none());
        assert_eq!(blocked.armies()[&2].province, 2);

        let mut weaker = sample(false, vec![link(1, 2, 1)]);
        weaker.declare_war("AAA", "BBB", date()).unwrap();
        station(&mut weaker, 1, "AAA", 1, 3);
        station(&mut weaker, 2, "BBB", 2, 1);
        advance(&mut weaker, 1, GameSpeed::Normal);
        assert!(weaker.armies()[&2].movement.is_none());

        let mut tired = sample(false, vec![link(1, 2, 1)]);
        tired.declare_war("AAA", "BBB", date()).unwrap();
        station(&mut tired, 1, "AAA", 1, 1);
        station(&mut tired, 2, "BBB", 2, 2);
        tired.military.armies.get_mut(&2).unwrap().regiments[0].morale = 100;
        tired.military.armies.get_mut(&2).unwrap().retreating = true;
        tired.military.armies.get_mut(&2).unwrap().retreat_shattered = Some(true);
        advance(&mut tired, 1, GameSpeed::Normal);
        assert!(tired.armies()[&2].movement.is_none());
        assert!(tired.armies()[&2].is_retreating());
    }

    #[test]
    fn low_morale_does_not_reenter_the_enemy_province() {
        let mut world = sample(false, vec![link(1, 2, 1)]);
        world.declare_war("AAA", "BBB", date()).unwrap();
        station(&mut world, 2, "BBB", 1, 2);
        for regiment in &mut world.military.armies.get_mut(&2).unwrap().regiments {
            regiment.morale = 100;
        }
        advance(&mut world, 1, GameSpeed::Normal);
        let movement = world.armies()[&2].movement.as_ref().unwrap();
        assert_eq!(movement.destination(), 2);
        assert!(
            movement
                .legs()
                .iter()
                .all(|leg| leg.to != 1 || leg.from == 1)
        );
    }

    #[test]
    fn recruitment_uses_the_same_limits_and_is_not_repeated() {
        let mut world = sample(false, vec![link(1, 2, 1)]);
        world.declare_war("AAA", "BBB", date()).unwrap();
        let before = world.country("BBB").unwrap().finances.treasury();
        let simulation = advance(&mut world, 1, GameSpeed::Normal);
        assert_eq!(world.recruitment_queue(2).len(), 1);
        assert_eq!(world.recruitment_queue(2)[0].country, "BBB");
        assert_eq!(
            world.country("BBB").unwrap().finances.treasury(),
            before - Money::from(10)
        );
        let spent = world.country("BBB").unwrap().finances.treasury();
        world.direct_war_ai(Some("AAA"), simulation.clock().date());
        assert_eq!(world.recruitment_queue(2).len(), 1);
        assert_eq!(world.country("BBB").unwrap().finances.treasury(), spent);

        let mut paused = Simulation::new(simulation.clock().date());
        paused.set_speed(GameSpeed::Octuple);
        assert_eq!(
            paused.advance_for(&mut world, Duration::from_secs(3), Some("AAA")),
            0
        );
        assert_eq!(world.country("BBB").unwrap().finances.treasury(), spent);

        let mut broke = sample(false, vec![]);
        broke.declare_war("AAA", "BBB", date()).unwrap();
        broke.countries.get_mut("BBB").unwrap().finances = Default::default();
        advance(&mut broke, 1, GameSpeed::Normal);
        assert!(broke.recruitment_queue(2).is_empty());
        assert_eq!(
            broke.country("BBB").unwrap().finances.treasury(),
            Money::ZERO
        );

        let mut no_men = sample(false, vec![]);
        no_men.declare_war("AAA", "BBB", date()).unwrap();
        no_men
            .countries
            .get_mut("BBB")
            .unwrap()
            .manpower
            .as_mut()
            .unwrap()
            .available = 0;
        let rich = no_men.country("BBB").unwrap().finances.treasury();
        advance(&mut no_men, 1, GameSpeed::Normal);
        assert!(no_men.recruitment_queue(2).is_empty());
        assert_eq!(no_men.country("BBB").unwrap().finances.treasury(), rich);

        let mut over_budget = sample(true, vec![]);
        over_budget.declare_war("AAA", "BBB", date()).unwrap();
        station(&mut over_budget, 2, "BBB", 2, 1);
        let rich = over_budget.country("BBB").unwrap().finances.treasury();
        advance(&mut over_budget, 1, GameSpeed::Normal);
        assert!(over_budget.recruitment_queue(2).is_empty());
        assert_eq!(
            over_budget.country("BBB").unwrap().finances.treasury(),
            rich
        );
    }

    #[test]
    fn speed_and_save_do_not_repeat_the_day() {
        let links = vec![link(1, 2, 10)];
        let mut slow = sample(false, links.clone());
        slow.declare_war("AAA", "BBB", date()).unwrap();
        station(&mut slow, 2, "BBB", 2, 2);
        advance(&mut slow, 1, GameSpeed::Normal);

        let mut fast = sample(false, links);
        fast.declare_war("AAA", "BBB", date()).unwrap();
        station(&mut fast, 2, "BBB", 2, 2);
        advance(&mut fast, 1, GameSpeed::Octuple);

        assert_eq!(
            slow.country("BBB").unwrap().finances.treasury(),
            fast.country("BBB").unwrap().finances.treasury()
        );
        assert_eq!(slow.armies()[&2].movement, fast.armies()[&2].movement);
        assert_eq!(
            slow.recruitment_queue(2).len(),
            fast.recruitment_queue(2).len()
        );

        let saved = SavedGame::capture(
            &slow,
            &Simulation::new(date().checked_add_days(1).unwrap()),
            "AAA",
        );
        let restored = saved.restore(&slow).unwrap();
        assert_eq!(
            restored.world.country("BBB").unwrap().finances.treasury(),
            slow.country("BBB").unwrap().finances.treasury()
        );
        assert_eq!(
            restored.world.armies()[&2].movement,
            slow.armies()[&2].movement
        );
        assert_eq!(restored.world.recruitment_queue(2).len(), 1);
    }

    #[test]
    fn favorable_cession_and_losing_white_peace_follow_the_shared_rule() {
        let mut winning = sample(false, vec![link(1, 2, 1)]);
        winning.declare_war("AAA", "BBB", date()).unwrap();
        let aaa = winning.provinces.get_mut(&1).unwrap();
        aaa.controller = Some("BBB".to_owned());
        advance(&mut winning, 1, GameSpeed::Normal);
        assert!(!winning.at_war("AAA", "BBB"));
        assert_eq!(winning.province(1).unwrap().owner.as_deref(), Some("BBB"));
        assert!(winning.recruitment_queue(2).is_empty());

        let mut losing = sample(false, vec![link(1, 2, 1)]);
        losing.declare_war("AAA", "BBB", date()).unwrap();
        losing.provinces.get_mut(&2).unwrap().controller = Some("AAA".to_owned());
        station(&mut losing, 2, "BBB", 2, 2);
        advance(&mut losing, 1, GameSpeed::Normal);
        assert!(!losing.at_war("AAA", "BBB"));
        assert_eq!(losing.province(2).unwrap().owner.as_deref(), Some("BBB"));
        assert_eq!(
            losing.province(2).unwrap().controller.as_deref(),
            Some("BBB")
        );
    }

    #[test]
    fn calendar_advance_ignores_pause_and_records_the_order() {
        let links = vec![link(1, 2, 10), link(2, 3, 1), link(3, 1, 30)];
        let mut paused = sample(false, links.clone());
        paused.declare_war("AAA", "BBB", date()).unwrap();
        station(&mut paused, 2, "BBB", 2, 2);
        let mut simulation = Simulation::new(date());
        assert!(simulation.clock().is_paused());
        let step = simulation.advance_days(&mut paused, 1, Some("AAA"));
        assert!(simulation.clock().is_paused());
        assert_eq!(simulation.clock().speed(), GameSpeed::Normal);
        assert_eq!(step.days, 1);
        assert_eq!(
            simulation.clock().date(),
            date().checked_add_days(1).unwrap()
        );
        assert_eq!(step.decisions.len(), 1);
        let decision = &step.decisions[0];
        assert_eq!(decision.country, "BBB");
        assert_eq!(decision.recruit.action, "order");
        assert_eq!(decision.recruit.province, Some(2));
        assert_eq!(decision.armies.len(), 1);
        assert_eq!(decision.armies[0].action, "move");
        assert_eq!(decision.armies[0].destination, Some(1));
        let again = simulation.advance_days(&mut paused, 0, Some("AAA"));
        assert_eq!(again.days, 0);
        assert!(again.decisions.is_empty());

        let mut running = sample(false, links);
        running.declare_war("AAA", "BBB", date()).unwrap();
        station(&mut running, 2, "BBB", 2, 2);
        advance(&mut running, 1, GameSpeed::Normal);
        assert_eq!(paused.armies()[&2].movement, running.armies()[&2].movement);
        assert_eq!(
            paused.country("BBB").unwrap().finances.treasury(),
            running.country("BBB").unwrap().finances.treasury()
        );
    }

    fn build(owners: &[(u32, &str)], links: Vec<LandConnection>) -> World {
        let mut provinces = BTreeMap::new();
        let mut ids = Vec::new();
        for &(id, owner) in owners {
            provinces.insert(id, province(owner));
            ids.push(id);
        }
        let mut world = World::new(
            [
                Country::new("AAA"),
                Country::new("BBB"),
                Country::new("CCC"),
            ],
            provinces,
            ids,
            Vec::<crate::BuildingDefinition>::new(),
            rules(true, false),
            links,
        )
        .unwrap();
        for tag in ["AAA", "BBB", "CCC"] {
            world
                .countries
                .get_mut(tag)
                .unwrap()
                .finances
                .refund(Money::from(1_000));
        }
        world
    }

    fn open(world: &mut World) -> Simulation {
        let mut simulation = Simulation::new(date());
        simulation.set_paused(false);
        simulation.advance_days(world, 1, Some("AAA"));
        simulation
    }

    #[test]
    fn taylor_power_uses_the_morale_polynomial_and_pip_exponent() {
        assert_eq!(super::morale_factor_milli(2_000), 2_447);
        assert_eq!(super::pip_factor_milli(0), 1_000);
        assert_eq!(super::pip_factor_milli(1), 1_045);
        assert_eq!(
            super::regiment_power(1_000, 2_000, &super::zero_pips()),
            2_447
        );
        assert_eq!(super::regiment_power(1_000, 0, &super::zero_pips()), 0);
        // 指数 1 的点子因子是 1045，对应战力 1000×2447/1000×1045/1000 = 2557。
        // 一个火枪点子的权重是 3，因子为 1141，战力是 2792。
        assert_eq!(super::pip_factor_milli(3), 1_141);
        let mut pips = super::zero_pips();
        pips.offensive_fire = 1;
        assert_eq!(super::regiment_power(1_000, 2_000, &pips), 2_792);
    }

    #[test]
    fn two_armies_muster_then_depart_together() {
        let mut world = build(
            &[(1, "AAA"), (2, "BBB"), (4, "BBB")],
            vec![link(1, 2, 5), link(2, 4, 5)],
        );
        world.declare_war("AAA", "BBB", date()).unwrap();
        station(&mut world, 1, "AAA", 1, 3);
        station(&mut world, 10, "BBB", 2, 2);
        station(&mut world, 11, "BBB", 4, 2);
        let mut simulation = open(&mut world);
        assert!(world.armies()[&10].movement.is_none());
        assert_eq!(world.armies()[&10].province, 2);
        assert_eq!(
            world.armies()[&11].movement.as_ref().unwrap().destination(),
            2
        );
        assert_eq!(world.military.war_plans[&10].muster, Some(2));
        assert_eq!(world.military.war_plans[&11].role, "assemble");
        for _ in 0..12 {
            let waiting = world.armies()[&11]
                .movement
                .as_ref()
                .is_some_and(|movement| movement.destination() == 2);
            if !waiting {
                break;
            }
            assert_ne!(
                world.armies()[&10]
                    .movement
                    .as_ref()
                    .map(crate::ArmyMovement::destination),
                Some(1)
            );
            simulation.advance_days(&mut world, 1, Some("AAA"));
        }
        assert_eq!(world.armies()[&10].province, 2);
        assert_eq!(world.armies()[&11].province, 2);
        assert_eq!(
            world.armies()[&10].movement.as_ref().unwrap().destination(),
            1
        );
        assert_eq!(
            world.armies()[&11].movement.as_ref().unwrap().destination(),
            1
        );
    }

    #[test]
    fn nearby_army_reinforces_within_thirty_days_and_far_army_does_not() {
        let mut world = build(
            &[(1, "AAA"), (2, "BBB"), (4, "BBB")],
            vec![link(1, 2, 1), link(1, 4, 40)],
        );
        world.declare_war("AAA", "BBB", date()).unwrap();
        station(&mut world, 1, "AAA", 1, 3);
        station(&mut world, 2, "BBB", 1, 2);
        station(&mut world, 3, "BBB", 2, 2);
        station(&mut world, 4, "BBB", 4, 2);
        open(&mut world);
        assert!(world.armies()[&2].battle_id().is_some());
        assert_eq!(
            world.armies()[&3].movement.as_ref().unwrap().destination(),
            1
        );
        assert!(world.armies()[&4].movement.is_none());
        assert_eq!(world.military.war_plans[&3].role, "reinforce");
        assert!(!world.military.war_plans.contains_key(&4));
    }

    #[test]
    fn relief_is_not_pulled_into_a_battle() {
        let mut world = build(
            &[(1, "AAA"), (2, "BBB"), (4, "BBB")],
            vec![link(1, 4, 1), link(2, 4, 1)],
        );
        world.countries.get_mut("BBB").unwrap().capital = Some(2);
        world.provinces.get_mut(&2).unwrap().garrison = Some(1_000);
        world.declare_war("AAA", "BBB", date()).unwrap();
        station(&mut world, 1, "AAA", 1, 2);
        station(&mut world, 2, "BBB", 1, 2);
        station(&mut world, 8, "AAA", 2, 1);
        station(&mut world, 9, "BBB", 4, 2);
        open(&mut world);
        assert!(world.armies()[&2].battle_id().is_some());
        assert_eq!(
            world.armies()[&9].movement.as_ref().unwrap().destination(),
            2
        );
        assert_eq!(world.military.war_plans[&9].role, "relieve");
    }

    #[test]
    fn short_siege_is_not_stripped_for_a_nearby_battle() {
        let mut world = build(
            &[(1, "AAA"), (2, "BBB"), (3, "AAA"), (4, "BBB"), (5, "BBB")],
            vec![link(1, 2, 1), link(1, 3, 5), link(3, 4, 1), link(1, 5, 40)],
        );
        world.countries.get_mut("AAA").unwrap().capital = Some(3);
        world.provinces.get_mut(&3).unwrap().garrison = Some(1_000);
        assert_eq!(world.siege_required_men(3), Some(3_000));
        world.declare_war("AAA", "BBB", date()).unwrap();
        station(&mut world, 1, "AAA", 1, 3);
        station(&mut world, 2, "BBB", 1, 2);
        station(&mut world, 3, "BBB", 2, 2);
        station(&mut world, 4, "BBB", 3, 1);
        station(&mut world, 5, "BBB", 4, 2);
        station(&mut world, 6, "BBB", 5, 2);
        open(&mut world);
        assert_eq!(
            world.armies()[&3].movement.as_ref().unwrap().destination(),
            1
        );
        assert_eq!(world.military.war_plans[&3].role, "reinforce");
        assert!(world.armies()[&4].movement.is_none());
        assert_eq!(world.armies()[&4].province, 3);
        assert_eq!(world.military.war_plans[&4].role, "siege");
        assert_eq!(
            world.armies()[&5].movement.as_ref().unwrap().destination(),
            3
        );
        assert_eq!(world.military.war_plans[&5].role, "siege");
        assert!(world.armies()[&6].movement.is_none());
    }

    #[test]
    fn siege_sends_only_the_armies_required_and_names_a_supply_shortfall() {
        let mut world = build(&[(1, "AAA"), (2, "BBB")], vec![link(1, 2, 1)]);
        world.countries.get_mut("AAA").unwrap().capital = Some(1);
        world.provinces.get_mut(&1).unwrap().garrison = Some(1_000);
        assert_eq!(world.siege_required_men(1), Some(3_000));
        world.declare_war("AAA", "BBB", date()).unwrap();
        station(&mut world, 2, "BBB", 2, 3);
        station(&mut world, 4, "BBB", 2, 1);
        let choices = world.direct_war_ai(Some("AAA"), date());
        let decision = choices
            .iter()
            .find(|choice| choice.country == "BBB")
            .unwrap();
        let main = decision.armies.iter().find(|army| army.id == 2).unwrap();
        assert_eq!(main.action, "move");
        assert_eq!(main.destination, Some(1));
        assert_eq!(main.role.as_deref(), Some("siege"));
        let spare = decision.armies.iter().find(|army| army.id == 4).unwrap();
        assert_eq!(spare.action, "stay");
        assert!(world.armies()[&4].movement.is_none());

        world.military_rules.supply_base = 0;
        world.military.war_plans.clear();
        world.military.armies.get_mut(&2).unwrap().movement = None;
        let choices = world.direct_war_ai(Some("AAA"), date());
        let decision = choices
            .iter()
            .find(|choice| choice.country == "BBB")
            .unwrap();
        let main = decision.armies.iter().find(|army| army.id == 2).unwrap();
        assert_eq!(main.destination, Some(1));
        assert!(main.reason.contains("补给"));
        let spare = decision.armies.iter().find(|army| army.id == 4).unwrap();
        assert_eq!(spare.action, "stay");
    }

    #[test]
    fn occupy_target_does_not_shuttle_for_many_days() {
        let mut world = build(
            &[(1, "BBB"), (2, "AAA"), (3, "AAA")],
            vec![link(1, 2, 1), link(2, 3, 1), link(1, 3, 1)],
        );
        world.declare_war("AAA", "BBB", date()).unwrap();
        station(&mut world, 2, "BBB", 2, 1);
        let mut simulation = Simulation::new(date());
        simulation.set_paused(false);
        for _ in 0..15 {
            simulation.advance_days(&mut world, 1, Some("AAA"));
            assert!(world.armies()[&2].movement.is_none());
            assert_eq!(world.armies()[&2].province, 2);
        }
    }

    #[test]
    fn save_load_mid_plan_does_not_reissue_the_move_or_recruit() {
        let mut world = build(&[(1, "AAA"), (2, "BBB")], vec![link(1, 2, 10)]);
        world.declare_war("AAA", "BBB", date()).unwrap();
        station(&mut world, 2, "BBB", 2, 2);
        let simulation = open(&mut world);
        let movement = world.armies()[&2].movement.clone();
        let treasury = world.country("BBB").unwrap().finances.treasury();
        let plans = world.military.war_plans.clone();
        assert!(plans.contains_key(&2));
        let saved = SavedGame::capture(&world, &simulation, "AAA");
        let restored = saved.restore(&world).unwrap();
        assert_eq!(restored.world.military.war_plans, plans);
        let mut continued = Simulation::new(simulation.clock().date());
        continued.set_paused(false);
        let mut world = restored.world;
        continued.advance_days(&mut world, 1, Some("AAA"));
        assert_eq!(world.armies()[&2].movement, movement);
        assert_eq!(world.country("BBB").unwrap().finances.treasury(), treasury);
        assert_eq!(world.military.war_plans[&2].since, plans[&2].since);
    }

    #[test]
    fn direct_war_ai_does_not_draw_combat_or_siege_dice() {
        let mut world = build(&[(1, "AAA"), (2, "BBB")], vec![link(1, 2, 1)]);
        world.declare_war("AAA", "BBB", date()).unwrap();
        station(&mut world, 1, "AAA", 1, 1);
        station(&mut world, 2, "BBB", 2, 2);
        let random = world.military.random.count();
        let siege = world.military.siege_random.count();
        world.direct_war_ai(Some("AAA"), date());
        world.direct_war_ai(Some("AAA"), date());
        assert_eq!(world.military.random.count(), random);
        assert_eq!(world.military.siege_random.count(), siege);
    }

    #[test]
    fn assembly_does_not_muster_where_an_enemy_already_stands() {
        // 省 1 有敌军。经省 1 到省 6 只有 2 天，直达要 3 天。到齐前不能抄这条近路。
        let mut world = build(
            &[(1, "BBB"), (2, "BBB"), (4, "BBB"), (6, "BBB")],
            vec![
                link(2, 1, 1),
                link(4, 1, 1),
                link(2, 6, 3),
                link(4, 6, 3),
                link(1, 6, 1),
            ],
        );
        world.declare_war("AAA", "BBB", date()).unwrap();
        station(&mut world, 1, "AAA", 1, 3);
        station(&mut world, 10, "BBB", 2, 2);
        station(&mut world, 11, "BBB", 4, 2);
        let stuck = ArmyPlan {
            role: "assemble".to_owned(),
            target: 1,
            muster: Some(1),
            since: date(),
        };
        world.military.war_plans.insert(10, stuck.clone());
        world.military.war_plans.insert(11, stuck);
        world.direct_war_ai(Some("AAA"), date());
        for id in [10, 11] {
            let movement = world.armies()[&id].movement.as_ref().unwrap();
            assert_eq!(movement.destination(), 6);
            assert!(
                movement
                    .legs()
                    .iter()
                    .all(|leg| leg.to != 1 && leg.from != 1)
            );
            assert_eq!(world.military.war_plans[&id].muster, Some(6));
            assert_eq!(world.military.war_plans[&id].target, 1);
        }

        let mut nowhere = build(&[(1, "BBB"), (2, "BBB")], vec![link(1, 2, 1)]);
        nowhere.declare_war("AAA", "BBB", date()).unwrap();
        station(&mut nowhere, 1, "AAA", 1, 3);
        station(&mut nowhere, 8, "AAA", 2, 3);
        station(&mut nowhere, 10, "BBB", 2, 2);
        station(&mut nowhere, 11, "BBB", 2, 2);
        nowhere.direct_war_ai(Some("AAA"), date());
        assert!(nowhere.armies()[&10].movement.is_none());
        assert!(nowhere.armies()[&11].movement.is_none());
        assert!(nowhere.military.war_plans.is_empty());
    }

    #[test]
    fn spare_armies_leave_a_filled_siege_or_occupation() {
        let mut siege = build(&[(1, "AAA"), (2, "BBB")], vec![link(1, 2, 1)]);
        siege.countries.get_mut("AAA").unwrap().capital = Some(1);
        siege.provinces.get_mut(&1).unwrap().garrison = Some(1_000);
        siege.declare_war("AAA", "BBB", date()).unwrap();
        station(&mut siege, 2, "BBB", 1, 3);
        station(&mut siege, 4, "BBB", 1, 1);
        siege.direct_war_ai(Some("AAA"), date());
        assert!(siege.armies()[&2].movement.is_none());
        assert_eq!(siege.military.war_plans[&2].role, "siege");
        assert_eq!(
            siege.armies()[&4].movement.as_ref().unwrap().destination(),
            2
        );
        assert_eq!(siege.military.war_plans[&4].role, "recover");

        let mut occupation = build(&[(1, "AAA"), (2, "BBB")], vec![link(1, 2, 1)]);
        occupation.declare_war("AAA", "BBB", date()).unwrap();
        station(&mut occupation, 2, "BBB", 1, 1);
        station(&mut occupation, 4, "BBB", 1, 1);
        occupation.direct_war_ai(Some("AAA"), date());
        assert!(occupation.armies()[&2].movement.is_none());
        assert_eq!(occupation.military.war_plans[&2].role, "occupy");
        assert_eq!(
            occupation.armies()[&4]
                .movement
                .as_ref()
                .unwrap()
                .destination(),
            2
        );
        assert_eq!(occupation.military.war_plans[&4].role, "recover");
    }

    #[test]
    fn equal_power_does_not_abandon_a_siege_or_keep_marching_in() {
        let mut world = build(
            &[(1, "AAA"), (2, "BBB"), (3, "AAA")],
            vec![link(1, 2, 1), link(1, 3, 5)],
        );
        world.countries.get_mut("AAA").unwrap().capital = Some(1);
        world.provinces.get_mut(&1).unwrap().garrison = Some(1_000);
        world.declare_war("AAA", "BBB", date()).unwrap();
        station(&mut world, 2, "BBB", 1, 3);
        station(&mut world, 4, "BBB", 2, 1);
        station(&mut world, 8, "AAA", 3, 3);
        world.move_army("AAA", 8, 1, date()).unwrap();
        world.direct_war_ai(Some("AAA"), date());
        assert!(world.armies()[&2].movement.is_none());
        assert_eq!(world.military.war_plans[&2].role, "siege");
        assert!(world.armies()[&4].movement.is_none());

        world
            .military
            .armies
            .get_mut(&8)
            .unwrap()
            .regiments
            .push(Regiment::fresh(UnitKind::Infantry, 1_000));
        world.direct_war_ai(Some("AAA"), date());
        assert_eq!(
            world.armies()[&2].movement.as_ref().unwrap().destination(),
            2
        );
        assert_eq!(world.military.war_plans[&2].role, "recover");

        let mut marching = build(
            &[(1, "AAA"), (2, "BBB"), (4, "BBB")],
            vec![link(2, 4, 1), link(4, 1, 5)],
        );
        marching.declare_war("AAA", "BBB", date()).unwrap();
        station(&mut marching, 1, "AAA", 1, 2);
        station(&mut marching, 2, "BBB", 2, 2);
        marching.move_army("BBB", 2, 1, date()).unwrap();
        assert_eq!(
            marching.armies()[&2]
                .movement
                .as_ref()
                .unwrap()
                .destination(),
            1
        );
        marching.direct_war_ai(Some("AAA"), date());
        assert_eq!(
            marching.armies()[&2]
                .movement
                .as_ref()
                .unwrap()
                .destination(),
            2
        );
        assert_eq!(marching.military.war_plans[&2].role, "recover");
    }
}
