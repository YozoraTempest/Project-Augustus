// SPDX-License-Identifier: GPL-3.0-only

//! 陆战的基础规则与一场战斗的存档状态。
//!
//! 伤亡、阶段和士气采用欧陆风云 4 已公开的公式。将领、地形、科技等级、
//! 训练度和国家修正尚未实现，计算时保持中性，不另造替代项。
//! 火炮、后排和可手动选择的撤退不在本阶段。

use serde::{Deserialize, Serialize};

use crate::{GameDate, Money, UnitKind};

/// 旧档没有士气字段时的占位。读入后改成规则中的满士气，不能当成真实士气保存。
pub const UNSET_MORALE: u32 = u32::MAX;

/// 火力与冲击交替，每个阶段三天，先进入火力阶段。
pub const PHASE_DAYS: u32 = 3;
/// 一方在每个阶段开始时得到 0 到 9 的点数。
pub const DICE_SIDES: u32 = 10;
/// 士气伤害公式中的除数。
pub const MORALE_DIVISOR: u32 = 540;
/// 两个火力阶段和两个冲击阶段结束前，满足兵力条件时会覆灭而不是撤退。
pub const RETREAT_LOCK_DAYS: u32 = 12;
/// 锁定期内，胜方剩余兵力达到败方两倍才覆灭。
pub const STACKWIPE_RATIO: u64 = 2;
/// 接战时一方兵力达到另一方十倍，且较弱一方填不满战斗宽度，则直接覆灭。
pub const OVERRUN_RATIO: u64 = 10;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitCombat {
    pub offensive_fire: u32,
    pub defensive_fire: u32,
    pub offensive_shock: u32,
    pub defensive_shock: u32,
    pub offensive_morale: u32,
    pub defensive_morale: u32,
    pub maneuver: u32,
    /// 该兵种在当前阶段使用的火力伤害系数。未接入科技时取军事科技 0 级。
    pub fire_damage: Money,
    /// 冲击伤害系数，来源与火力相同。
    pub shock_damage: Money,
}

impl UnitCombat {
    fn non_negative(&self) -> bool {
        self.fire_damage >= Money::ZERO && self.shock_damage >= Money::ZERO
    }

    pub(crate) fn stats(&self, kind: PhaseKind, morale: bool) -> (i32, i32, Money) {
        let (offense, defense, damage) = match kind {
            PhaseKind::Fire => (self.offensive_fire, self.defensive_fire, self.fire_damage),
            PhaseKind::Shock => (
                self.offensive_shock,
                self.defensive_shock,
                self.shock_damage,
            ),
        };
        let (offense, defense) = if morale {
            (
                offense + self.offensive_morale,
                defense + self.defensive_morale,
            )
        } else {
            (offense, defense)
        };
        (offense as i32, defense as i32, damage)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PhaseKind {
    Fire,
    Shock,
}

impl PhaseKind {
    pub(crate) fn from_elapsed(elapsed_days: u32) -> Self {
        if (elapsed_days / PHASE_DAYS).is_multiple_of(2) {
            Self::Fire
        } else {
            Self::Shock
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Fire => "火力",
            Self::Shock => "冲击",
        }
    }
}

/// 全体国家共用的基础陆战规则。正式对局由内容文件填充，测试使用 [`CombatRules::baseline`]。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CombatRules {
    pub width: u32,
    pub max_morale: Money,
    /// 军事科技 0 级的战技。训练度尚未实现，不再乘额外系数。
    pub tactics: Money,
    pub recovery: Money,
    pub recovery_own_territory: Money,
    /// 溃退时加到行军速度上的比例，对应 `SHATTERED_RETREAT_SPEED_MODIFIER`。
    pub shattered_retreat_speed: Money,
    /// 战斗每多一天，伤害增加的比例，对应 `WAR_LENGTH_DAMAGE_MODIFIER`。
    pub battle_length_damage: Money,
    pub infantry: UnitCombat,
    pub cavalry: UnitCombat,
}

impl CombatRules {
    /// 宽度、士气、战技和恢复速度使用已核实的基础值。
    /// 兵种点数为 0，伤害系数与战技同为 0.5，只用于不读取游戏内容的测试。
    pub fn baseline() -> Self {
        let neutral = UnitCombat {
            offensive_fire: 0,
            defensive_fire: 0,
            offensive_shock: 0,
            defensive_shock: 0,
            offensive_morale: 0,
            defensive_morale: 0,
            maneuver: 0,
            fire_damage: Money::new(5, 1),
            shock_damage: Money::new(5, 1),
        };
        Self {
            width: 15,
            max_morale: Money::from(2),
            tactics: Money::new(5, 1),
            recovery: Money::new(15, 2),
            recovery_own_territory: Money::new(5, 2),
            shattered_retreat_speed: Money::new(5, 1),
            battle_length_damage: Money::new(1, 2),
            infantry: neutral.clone(),
            cavalry: neutral,
        }
    }

    pub(crate) fn is_playable(&self) -> bool {
        self.width >= 1
            && self.max_morale > Money::ZERO
            && self.tactics > Money::ZERO
            && self.recovery >= Money::ZERO
            && self.recovery_own_territory >= Money::ZERO
            && self.shattered_retreat_speed >= Money::ZERO
            && self.battle_length_damage >= Money::ZERO
            && self.morale_milli().is_some()
            && self.infantry.non_negative()
            && self.cavalry.non_negative()
    }

    /// 士气以千分之一点保存，避免每天结算引入二进制小数。
    pub fn morale_milli(&self) -> Option<u32> {
        let scaled = (self.max_morale * Money::from(1_000)).trunc();
        u32::try_from(scaled)
            .ok()
            .filter(|value| (1..UNSET_MORALE).contains(value))
    }

    pub(crate) fn unit(&self, kind: UnitKind) -> &UnitCombat {
        match kind {
            UnitKind::Infantry => &self.infantry,
            UnitKind::Cavalry => &self.cavalry,
        }
    }
}

/// 攻击方基数。将领、地形和额外骰子修正为 0。结果不低于 15。
pub(crate) fn base_casualties(roll: u32, offensive_pips: i32, defensive_pips: i32) -> u32 {
    let margin = roll as i32 + offensive_pips - defensive_pips;
    15 + 5 * margin.max(0) as u32
}

/// 伤害乘数中的共同部分，不含基数。战斗能力、训练度加成为 0。
pub(crate) fn casualty_amount(
    base: u32,
    strength: u32,
    damage: Money,
    tactics: Money,
    length_damage: Money,
    battle_day: u32,
) -> Money {
    Money::from(base) * Money::from(strength) / Money::from(1_000) * damage / tactics
        * (Money::ONE + Money::from(battle_day) * length_damage)
}

pub(crate) fn whole_men(amount: Money) -> u32 {
    u32::try_from(amount.trunc()).unwrap_or(u32::MAX)
}

pub(crate) fn morale_milli_from(amount: Money, max_morale: Money) -> u32 {
    let scaled = amount * max_morale / Money::from(MORALE_DIVISOR) * Money::from(1_000);
    whole_men(scaled)
}

/// 前排每天承受对方平均最大士气的 1%，预备队为 2%。
pub(crate) fn passive_morale(max_milli: u32, reserve: bool) -> u32 {
    max_milli * if reserve { 2 } else { 1 } / 100
}

/// 阶段骰子由战斗编号、阶段和阵营决定，范围仍是 0 到 9。
/// 不使用全局随机流，因此同一游戏日在不同速度和读档后续算时结果一致。
pub(crate) fn phase_roll(battle_id: u32, phase: u32, side: u32) -> u32 {
    let mixed = battle_id.wrapping_mul(0x9E37_79B1)
        ^ phase.wrapping_mul(0x85EB_CA6B)
        ^ side.wrapping_mul(0xC2B2_AE35);
    let mixed = mixed ^ (mixed >> 16);
    mixed % DICE_SIDES
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BattleSide {
    pub(crate) armies: Vec<u32>,
    pub(crate) strength_lost: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Battle {
    id: u32,
    province: u32,
    started_on: GameDate,
    /// 已结算的最后日期。同一日期再次推进不会重复扣兵。
    pub(crate) last_resolved: Option<GameDate>,
    pub(crate) elapsed_days: u32,
    pub(crate) rolls: [u8; 2],
    sides: [BattleSide; 2],
}

impl Battle {
    pub(crate) fn new(id: u32, province: u32, started_on: GameDate) -> Self {
        let phase = 0;
        Self {
            id,
            province,
            started_on,
            last_resolved: None,
            elapsed_days: 0,
            rolls: [
                phase_roll(id, phase, 0) as u8,
                phase_roll(id, phase, 1) as u8,
            ],
            sides: [
                BattleSide {
                    armies: Vec::new(),
                    strength_lost: 0,
                },
                BattleSide {
                    armies: Vec::new(),
                    strength_lost: 0,
                },
            ],
        }
    }

    pub fn id(&self) -> u32 {
        self.id
    }

    pub fn province(&self) -> u32 {
        self.province
    }

    pub fn started_on(&self) -> GameDate {
        self.started_on
    }

    pub fn elapsed_days(&self) -> u32 {
        self.elapsed_days
    }

    pub fn side_armies(&self, side: usize) -> &[u32] {
        &self.sides[side].armies
    }

    pub fn strength_lost(&self, side: usize) -> u64 {
        self.sides[side].strength_lost
    }

    pub(crate) fn side_mut(&mut self, side: usize) -> &mut BattleSide {
        &mut self.sides[side]
    }

    pub(crate) fn refresh_roll(&mut self) {
        let phase = self.elapsed_days / PHASE_DAYS;
        if self.elapsed_days.is_multiple_of(PHASE_DAYS) {
            self.rolls = [
                phase_roll(self.id, phase, 0) as u8,
                phase_roll(self.id, phase, 1) as u8,
            ];
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BattleOutcome {
    Retreat {
        loser: u8,
    },
    Stackwipe {
        loser: u8,
    },
    Overrun {
        loser: u8,
    },
    /// 败方没有可进入的省份，不能按规则撤出一省。
    NoRetreat {
        loser: u8,
    },
    MutualRetreat,
}

impl BattleOutcome {
    pub(crate) fn loser(self) -> Option<u8> {
        match self {
            Self::Retreat { loser }
            | Self::Stackwipe { loser }
            | Self::Overrun { loser }
            | Self::NoRetreat { loser } => Some(loser),
            Self::MutualRetreat => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportedSide {
    pub(crate) countries: Vec<String>,
    pub(crate) army_ids: Vec<u32>,
    pub(crate) strength: u64,
    pub(crate) morale: u32,
    pub(crate) lost: u64,
}

/// 一个省份最近一场已经结束的战斗。进行中的战斗另存在 [`Battle`]。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BattleReport {
    pub(crate) battle_id: u32,
    pub(crate) province: u32,
    pub(crate) on: GameDate,
    pub(crate) outcome: BattleOutcome,
    pub(crate) sides: [ReportedSide; 2],
}

#[cfg(test)]
mod tests {
    use super::{base_casualties, casualty_amount, morale_milli_from, phase_roll, whole_men};
    use crate::Money;

    #[test]
    fn base_casualties_never_drop_below_fifteen() {
        assert_eq!(base_casualties(0, 0, 0), 15);
        assert_eq!(base_casualties(0, 0, 4), 15);
        assert_eq!(base_casualties(9, 1, 0), 65);
    }

    #[test]
    fn full_regiment_loses_the_truncated_men_and_matching_morale() {
        let amount = casualty_amount(
            15,
            1_000,
            Money::new(5, 1),
            Money::new(5, 1),
            Money::new(1, 2),
            1,
        );
        assert_eq!(amount, Money::new(1515, 2));
        assert_eq!(whole_men(amount), 15);
        assert_eq!(morale_milli_from(amount, Money::from(2)), 56);
    }

    #[test]
    fn phase_rolls_stay_inside_one_decade_and_do_not_change() {
        let first = phase_roll(3, 1, 0);
        assert!(first < 10);
        assert_eq!(first, phase_roll(3, 1, 0));
        assert_ne!(first, phase_roll(3, 1, 1));
    }
}
