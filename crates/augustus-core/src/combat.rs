// SPDX-License-Identifier: GPL-3.0-only

//! 陆战的基础规则与一场战斗的存档状态。
//!
//! 伤亡使用千分整数，逐步向零截断。将领、地形、渡河、训练度、作战能力和
//! 国家修正尚未实现，调用时传入中性值，不另造替代项。
//! 火炮后排、雇佣兵和要塞区域阻挡不在本阶段。

use serde::{Deserialize, Serialize};

use crate::{GameDate, Money, UnitKind};

/// 旧档没有士气字段时的占位。读入后改成规则中的满士气，不能当成真实士气保存。
pub const UNSET_MORALE: u32 = u32::MAX;

/// 火力与冲击交替，每个阶段三天，先进入火力阶段。
pub const PHASE_DAYS: u32 = 3;
/// 一方在每个阶段开始时得到 0 到 9 的点数。
pub const DICE_SIDES: u32 = 10;
/// 十二个战斗日结束前，普通士气崩溃不结束战斗。
pub const RETREAT_LOCK_DAYS: u32 = 12;
/// 士气链使用的除数。满士气 2.0 与基数 2.7 都按千分保存时为 2700。
pub(crate) const MORALE_DIVISOR_MILLI: i64 = 2_700;
/// 中性训练度。1000 表示 100%，乘进去不改变千分整数。
pub(crate) const NEUTRAL_DISCIPLINE: i64 = 1_000;
/// 中性作战能力。必须是 1000；写成 0 会把后续伤害全部截成 0。
pub(crate) const NEUTRAL_ABILITY: i64 = 1_000;
/// 普通前排的战线系数。
pub(crate) const FRONT_SCALAR: i64 = 1_000;
/// 低士气退路阈值，绝对 0.5 点，不是最大士气的一半。
pub(crate) const LOW_MORALE_THRESHOLD: i64 = 500;
/// 提前结束溃退所需的相对士气，990/1000。
pub(crate) const SHATTERED_CLEAR_RELATIVE: u64 = 990;
/// 主动撤退让同侧其他军队承担的士气比例，对应 1.0。
pub(crate) const DELIBERATE_RETREAT_PENALTY_MILLI: i64 = 1_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnitCombat {
    pub offensive_fire: u32,
    pub defensive_fire: u32,
    pub offensive_shock: u32,
    pub defensive_shock: u32,
    pub offensive_morale: u32,
    pub defensive_morale: u32,
    pub maneuver: u32,
    /// 该兵种在火力阶段使用的伤害系数，来自所属国家已累计的军事科技。
    pub fire_damage: Money,
    /// 冲击伤害系数，来源与火力相同。
    pub shock_damage: Money,
}

/// 团自己保存的点数。伤害系数仍读所属国家的当前科技，不随兵种身份一起冻结。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnitPips {
    pub offensive_fire: u32,
    pub defensive_fire: u32,
    pub offensive_shock: u32,
    pub defensive_shock: u32,
    pub offensive_morale: u32,
    pub defensive_morale: u32,
    pub maneuver: u32,
}

impl UnitPips {
    pub fn from_unit(unit: &UnitCombat) -> Self {
        Self {
            offensive_fire: unit.offensive_fire,
            defensive_fire: unit.defensive_fire,
            offensive_shock: unit.offensive_shock,
            defensive_shock: unit.defensive_shock,
            offensive_morale: unit.offensive_morale,
            defensive_morale: unit.defensive_morale,
            maneuver: unit.maneuver,
        }
    }

    pub fn apply(self, unit: &mut UnitCombat) {
        unit.offensive_fire = self.offensive_fire;
        unit.defensive_fire = self.defensive_fire;
        unit.offensive_shock = self.offensive_shock;
        unit.defensive_shock = self.defensive_shock;
        unit.offensive_morale = self.offensive_morale;
        unit.defensive_morale = self.defensive_morale;
        unit.maneuver = self.maneuver;
    }
}

impl UnitCombat {
    fn non_negative(&self) -> bool {
        self.fire_damage >= Money::ZERO && self.shock_damage >= Money::ZERO
    }

    pub(crate) fn phase_pips(&self, kind: PhaseKind) -> (i32, i32) {
        match kind {
            PhaseKind::Fire => (self.offensive_fire as i32, self.defensive_fire as i32),
            PhaseKind::Shock => (self.offensive_shock as i32, self.defensive_shock as i32),
        }
    }

    pub(crate) fn morale_pips(&self) -> (i32, i32) {
        (self.offensive_morale as i32, self.defensive_morale as i32)
    }

    /// 六点攻防点数之和，列阵排序使用。机动另计。
    pub(crate) fn pip_sum(&self) -> u32 {
        self.offensive_fire
            + self.defensive_fire
            + self.offensive_shock
            + self.defensive_shock
            + self.offensive_morale
            + self.defensive_morale
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PhaseKind {
    Fire,
    Shock,
}

impl PhaseKind {
    /// `elapsed_days` 是已经加一后的战斗日。第 1 到 3 日为火力。
    pub(crate) fn from_battle_day(battle_day: u32) -> Self {
        let index = battle_day.saturating_sub(1) / PHASE_DAYS;
        if index.is_multiple_of(2) {
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

/// 各国共用的定义值，以及旧档还没有国家军事配置时沿用的统一兵种。
/// 新开局的等级、兵种和累计数值在 [`CountryMilitary`]，不读这里的兵种。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CombatRules {
    pub width: u32,
    pub max_morale: Money,
    /// 统一配置下的战技。分国之后，战斗改用防守方国家的战技。
    pub tactics: Money,
    pub recovery: Money,
    pub recovery_own_territory: Money,
    /// 溃退标志 204 加到行军速度上的比例，对应 `SHATTERED_RETREAT_SPEED_MODIFIER`。
    pub shattered_retreat_speed: Money,
    /// 战斗每多一天，伤害增加的比例，对应 `WAR_LENGTH_DAMAGE_MODIFIER`。
    pub battle_length_damage: Money,
    /// 前排骑兵允许比例，千分比。500 表示 50%。国家修正尚未接入。
    pub allowed_cavalry_ratio: u32,
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
            allowed_cavalry_ratio: 500,
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
            && self.allowed_cavalry_ratio <= 1_000
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

/// 一个国家当前的军事等级、招募兵种和已累计数值。
///
/// 新招募使用这里的兵种。已经存在的团各自保存兵种身份，不跟着这份选择改变。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CountryMilitary {
    /// 兵种组。历史写了 `unit_type` 时用它，否则用科技组。
    pub unit_group: String,
    /// 科技组。开局等级只读该组的 `start_level`。
    pub technology_group: String,
    /// 开局有效军事科技等级。
    pub level: u32,
    /// 当前步兵。空字符串表示还没有选中的步兵。
    pub infantry_unit: String,
    /// 当前骑兵。这一级没有可用骑兵时为空。
    pub cavalry_unit: String,
    pub infantry: UnitCombat,
    /// 这一级没有可用骑兵时为空。招募骑兵会报告国家和等级，不另选默认兵种。
    pub cavalry: Option<UnitCombat>,
    pub max_morale: Money,
    pub tactics: Money,
    pub width: u32,
}

impl CountryMilitary {
    /// 旧档没有分国配置时，全体国家沿用当时的统一兵种和科技 0 数值。
    pub fn from_shared(rules: &CombatRules) -> Self {
        Self {
            unit_group: String::new(),
            technology_group: String::new(),
            level: 0,
            infantry_unit: String::new(),
            cavalry_unit: String::new(),
            infantry: rules.infantry.clone(),
            cavalry: Some(rules.cavalry.clone()),
            max_morale: rules.max_morale,
            tactics: rules.tactics,
            width: rules.width,
        }
    }

    pub fn unit(&self, kind: UnitKind) -> Option<&UnitCombat> {
        match kind {
            UnitKind::Infantry => Some(&self.infantry),
            UnitKind::Cavalry => self.cavalry.as_ref(),
        }
    }

    pub fn unit_name(&self, kind: UnitKind) -> &str {
        match kind {
            UnitKind::Infantry => &self.infantry_unit,
            UnitKind::Cavalry => &self.cavalry_unit,
        }
    }

    pub fn morale_milli(&self) -> Option<u32> {
        let scaled = (self.max_morale * Money::from(1_000)).trunc();
        u32::try_from(scaled)
            .ok()
            .filter(|value| (1..UNSET_MORALE).contains(value))
    }

    pub fn is_playable(&self) -> bool {
        self.width >= 1
            && self.max_morale > Money::ZERO
            && self.tactics > Money::ZERO
            && self.morale_milli().is_some()
            && self.infantry.non_negative()
            && self.cavalry.as_ref().is_none_or(UnitCombat::non_negative)
    }

    /// 升级只更换国家当前招募的兵种。已经存在的团继续使用自己保存的点数。
    pub fn select_unit(&mut self, kind: UnitKind, name: impl Into<String>, unit: UnitCombat) {
        let name = name.into();
        match kind {
            UnitKind::Infantry => {
                self.infantry_unit = name;
                self.infantry = unit;
            }
            UnitKind::Cavalry => {
                self.cavalry_unit = name;
                self.cavalry = Some(unit);
            }
        }
    }
}

/// 向零截断的千分乘法。`F(a, b) = trunc(a * b / 1000)`。
pub(crate) fn trunc_mul(value: i64, factor: i64) -> i64 {
    value.saturating_mul(factor) / 1_000
}

/// 向零截断的千分除法。`D(a, b) = trunc(a * 1000 / b)`。除数为 0 时返回 0。
pub(crate) fn trunc_div(value: i64, divisor: i64) -> i64 {
    if divisor == 0 {
        0
    } else {
        value.saturating_mul(1_000) / divisor
    }
}

pub(crate) fn thousandths(value: Money) -> i64 {
    let scaled = (value * Money::from(1_000)).trunc();
    i64::try_from(scaled).unwrap_or(0)
}

/// `25 * (clamp(die + margin + 2, 0, 21) + 1)`。单独的骰子基数是这个值的五分之一。
pub(crate) fn dice_scale(die: i32, margin: i32) -> i64 {
    let clamped = (die + margin + 2).clamp(0, 21);
    25 * (i64::from(clamped) + 1)
}

#[derive(Clone, Copy)]
pub(crate) struct StrengthStrike {
    pub die: i32,
    /// 将领、地形、渡河与阶段攻防点数之差。骰子本身不要重复放进这里。
    pub margin: i32,
    pub strength: i64,
    pub discipline: i64,
    pub front_scalar: i64,
    pub defender_tactics: i64,
    pub tech_multiplier: i64,
    pub ability: i64,
    pub defender_received: i64,
    pub attacker_dealt: i64,
    pub elapsed_days: i64,
    pub same_column_empty: bool,
    /// 本团兵力四档，取 1 到 4。只在同列为空时乘进兵力伤害。
    pub quarter: i64,
}

pub(crate) fn strength_damage(strike: StrengthStrike) -> i64 {
    let mut damage = trunc_mul(strike.strength, dice_scale(strike.die, strike.margin));
    damage = trunc_mul(strike.discipline, damage);
    damage = trunc_mul(strike.front_scalar, damage);
    damage = trunc_div(damage, strike.defender_tactics);
    damage = trunc_mul(strike.tech_multiplier, damage);
    damage = trunc_mul(strike.ability, damage);
    damage = trunc_mul(1_000 + strike.defender_received.max(-950), damage);
    damage = trunc_mul(1_000 + strike.attacker_dealt, damage);
    damage = trunc_mul(damage, 200);
    if strike.same_column_empty {
        damage = trunc_mul(damage, strike.quarter.clamp(1, 4) * 250);
    }
    trunc_mul(damage, 1_000 + strike.elapsed_days * 10)
}

#[derive(Clone, Copy)]
pub(crate) struct MoraleStrike {
    pub die: i32,
    pub margin: i32,
    pub max_morale: i64,
    pub strength: i64,
    pub discipline: i64,
    pub front_scalar: i64,
    pub defender_tactics: i64,
    pub ability: i64,
    pub tech_multiplier: i64,
    pub elapsed_days: i64,
    pub defender_received: i64,
    pub attacker_dealt: i64,
}

/// 士气链先乘作战能力、再乘科技，不用兵力链的 0.2，也不用侧击四档。
pub(crate) fn morale_damage(strike: MoraleStrike) -> i64 {
    let mut damage = trunc_mul(
        trunc_div(strike.max_morale, MORALE_DIVISOR_MILLI),
        dice_scale(strike.die, strike.margin),
    );
    damage = trunc_mul(strike.strength, damage);
    damage = trunc_mul(strike.discipline, damage);
    damage = trunc_mul(strike.front_scalar, damage);
    damage = trunc_div(damage, strike.defender_tactics);
    damage = trunc_mul(strike.ability, damage);
    damage = trunc_mul(damage, strike.tech_multiplier);
    damage = trunc_mul(damage, 1_000 + strike.elapsed_days * 10);
    damage = trunc_mul(1_000 + strike.defender_received, damage);
    trunc_mul(1_000 + strike.attacker_dealt, damage)
}

/// 前排骑兵超过允许比例时，被打的骑兵团缩小防守方战技。不是固定 25% 惩罚。
pub(crate) fn cavalry_support_penalty(
    front_infantry: i64,
    front_cavalry: i64,
    allowed_ratio: i64,
) -> i64 {
    let total = front_infantry + front_cavalry;
    if total <= 0 {
        return 0;
    }
    let actual = trunc_div(front_cavalry, total);
    let allowed = allowed_ratio.clamp(0, 1_000);
    if actual > allowed {
        trunc_mul(-1_000, actual - allowed).clamp(-1_000, 0)
    } else {
        0
    }
}

pub(crate) fn effective_tactics(base: i64, target_is_cavalry: bool, penalty: i64) -> i64 {
    let tactics = if target_is_cavalry {
        trunc_mul(base, 1_000 + penalty)
    } else {
        base
    };
    tactics.max(1)
}

/// 剩余兵力严格分档。大于 75% 为 4，其后每降一档减 1。
pub(crate) fn strength_quarter(strength: u32, max_strength: u32) -> i64 {
    if max_strength == 0 {
        return 1;
    }
    let ratio = u64::from(strength) * 1_000 / u64::from(max_strength);
    if ratio <= 250 {
        1
    } else if ratio <= 500 {
        2
    } else if ratio <= 750 {
        3
    } else {
        4
    }
}

/// 侧翼距离按剩余兵力缩放后至少 1 格。科技和骑兵的额外机动尚未接入。
pub(crate) fn flank_distance(maneuver: u32, strength: u32, max_strength: u32) -> usize {
    if max_strength == 0 {
        return 1;
    }
    let ratio = u64::from(strength) * 1_000 / u64::from(max_strength);
    let scale = if ratio > 750 {
        1_000
    } else if ratio > 500 {
        500
    } else if ratio > 250 {
        250
    } else {
        0
    };
    let scaled = u64::from(maneuver) * scale / 1_000;
    usize::try_from(scaled).unwrap_or(0).max(1)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LinePlan {
    pub core: usize,
    pub cavalry: usize,
    pub extra_infantry: usize,
}

/// `E` 是对方有效的非炮兵团数。本阶段没有炮兵，因此等于对方有效团数。
pub(crate) fn line_plan(
    infantry: usize,
    cavalry: usize,
    enemy: usize,
    width: usize,
    infantry_flank: usize,
) -> LinePlan {
    let infantry = infantry as u32;
    let cavalry = cavalry as u32;
    let enemy = enemy as u32;
    let width = width as u32;
    let mut core = infantry.min(enemy);
    let mut flank = cavalry / 2;
    if cavalry > 0 {
        flank = flank.max(1);
    }
    if width < core.saturating_add(cavalry) {
        let inner = core.min(width / 2);
        flank = flank.min(width.saturating_sub(inner) / 2);
        core = core.min(width.saturating_sub(flank.saturating_mul(2)));
    }
    let expand = width.min(enemy.saturating_add((infantry_flank as u32).saturating_mul(2)));
    LinePlan {
        core: core as usize,
        cavalry: (flank.saturating_mul(2).min(cavalry)) as usize,
        extra_infantry: (expand as usize)
            .saturating_sub(core as usize)
            .min((infantry as usize).saturating_sub(core as usize)),
    }
}

/// 宽度 15 时中心下标是 7，随后先左后右。
pub(crate) fn center_out(width: usize) -> Vec<usize> {
    if width == 0 {
        return Vec::new();
    }
    let center = width / 2;
    let mut order = Vec::with_capacity(width);
    order.push(center);
    for distance in 1..width {
        if let Some(left) = center.checked_sub(distance) {
            order.push(left);
        }
        let right = center + distance;
        if right < width {
            order.push(right);
        }
    }
    order
}

pub(crate) fn regiment_sort_score(maneuver: u32, pip_sum: u32, morale: u32, strength: u32) -> u128 {
    let base = u128::from(maneuver.saturating_add(pip_sum)) * 1_000;
    base * u128::from(morale) / 1_000 * u128::from(strength) / 1_000
}

/// 每天额外士气。前排 1%，预备 2%。预备队组织度尚未实现，中性为 0，因此预备不打折。
pub(crate) fn daily_morale_loss(average_max: i64, reserve: bool) -> i64 {
    trunc_mul(average_max, if reserve { 20 } else { 10 })
}

pub(crate) fn recovered_morale(
    current: i64,
    max_morale: i64,
    regiment_multiplier: i64,
    rate_milli: i64,
) -> i64 {
    let target = trunc_mul(max_morale, regiment_multiplier);
    let gain = trunc_mul(rate_milli, target);
    current.saturating_add(gain).min(target)
}

/// 已判败之后的覆灭。恰好 2:1 和 10:1 都不覆灭。
pub(crate) fn wipes_after_defeat(elapsed: u32, loser: u64, winner: u64) -> bool {
    (elapsed < RETREAT_LOCK_DAYS && loser < winner / 2) || loser < winner / 10
}

/// 接战时的十倍测试。双方各自的千人团数都达到宽度时跳过。科技高至少 2 级且仍有可战团时保护弱方。
pub(crate) fn initial_overrun_loser(
    strength: [u64; 2],
    width: u64,
    tech: [i32; 2],
    fightable: [bool; 2],
) -> Option<usize> {
    if strength[0] / 1_000 >= width && strength[1] / 1_000 >= width {
        return None;
    }
    for loser in [0, 1] {
        let winner = 1 - loser;
        if strength[loser] < strength[winner] / 10 {
            let protected = fightable[loser] && tech[loser] >= tech[winner] + 2;
            if !protected {
                return Some(loser);
            }
        }
    }
    None
}

/// 十二日前的前排两倍测试。不套用“双方都填满宽度则跳过”。先看进攻方。
pub(crate) fn front_ratio_loser(
    front: [u64; 2],
    tech: [i32; 2],
    fightable: [bool; 2],
) -> Option<usize> {
    for loser in [1, 0] {
        let winner = 1 - loser;
        if front[loser] < front[winner] / 2 {
            let protected = fightable[loser] && tech[loser] >= tech[winner] + 2;
            if !protected {
                return Some(loser);
            }
        }
    }
    None
}

pub(crate) fn wipe_refund_men(remaining: i64, max_strength: i64, definition_manpower: i64) -> i64 {
    if definition_manpower <= 0 || max_strength <= 0 || remaining <= 0 {
        return 0;
    }
    let ratio = trunc_div(remaining, max_strength);
    let amount = trunc_mul(ratio, definition_manpower);
    trunc_mul(amount, 500).max(0)
}

pub(crate) fn winner_morale_floor(loser_joined: i64, winner_joined: i64, army_max: i64) -> i64 {
    if winner_joined <= 0 || army_max <= 0 {
        return 0;
    }
    let ratio = trunc_div(loser_joined.max(0), winner_joined);
    let fraction = trunc_mul(ratio, 1_250).min(750);
    trunc_mul(army_max, fraction)
}

/// 兵力加权的当前士气。先逐团做千分乘法，再按总兵力做千分除法。
pub(crate) fn strength_weighted_morale(regiments: &[(u32, u32)]) -> i64 {
    let mut weighted = 0_i64;
    let mut total = 0_i64;
    for &(strength, morale) in regiments {
        if strength == 0 {
            continue;
        }
        weighted += trunc_mul(i64::from(strength), i64::from(morale));
        total += i64::from(strength);
    }
    if total <= 0 {
        0
    } else {
        trunc_div(weighted, total)
    }
}

/// 相邻退路的敌军人头过滤。1000 对 9000 通过，1000 对 10000 拒绝，999 对 5000 拒绝。
pub(crate) fn enemy_strength_blocks_retreat(our_men: u64, enemy_men: u64) -> bool {
    let enemy = enemy_men / 1_000;
    let ours = our_men / 1_000;
    enemy != 0 && ours < enemy / 5
}

pub(crate) fn far_retreat_score(development: u64, hops: u32, owned: bool) -> u64 {
    let discounted = match hops {
        1 => development / 100,
        2 | 3 => development / 10,
        4..=8 => development,
        9 => development / 10,
        10 => development / 20,
        _ => 0,
    };
    if owned {
        discounted.saturating_mul(2)
    } else {
        discounted
    }
}

/// 可继续的随机序列。不是原版生成器，只保证同一存档续算时消费顺序不变。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RandomStream {
    #[serde(default = "default_random_state")]
    state: u64,
    #[serde(default)]
    count: u64,
}

fn default_random_state() -> u64 {
    0xA5A5_1444_8D9B_994B
}

impl Default for RandomStream {
    fn default() -> Self {
        Self {
            state: default_random_state(),
            count: 0,
        }
    }
}

impl RandomStream {
    pub(crate) fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut mixed = self.state;
        mixed = (mixed ^ (mixed >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        mixed = (mixed ^ (mixed >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        self.count = self.count.wrapping_add(1);
        mixed ^ (mixed >> 31)
    }

    pub(crate) fn dice(&mut self) -> u8 {
        (self.next_u64() % u64::from(DICE_SIDES)) as u8
    }

    /// 闭区间 1 到 `sides`。围城骰面数和野战的 10 面不是同一个定义。
    pub(crate) fn roll_to(&mut self, sides: u32) -> u32 {
        (self.next_u64() % u64::from(sides)) as u32 + 1
    }

    /// 相邻退路在基础分高于 2000 时追加的 0 到 3。
    pub(crate) fn bonus_steps(&mut self) -> u32 {
        (self.next_u64() % 4) as u32
    }

    /// 测试核对阶段换骰时的消费次数。正式结算只向前取数，不读取这个计数。
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn count(&self) -> u64 {
        self.count
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct CombatantRef {
    pub army_id: u32,
    pub index: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BattleSide {
    pub(crate) armies: Vec<u32>,
    pub(crate) strength_lost: u64,
    /// 累计加入兵力，含增援。胜方士气下限用它，不用战后剩余。
    #[serde(default)]
    pub(crate) joined_strength: u64,
    /// 与战斗宽度对齐。尚未列阵时为空，和“已经列阵但格子是空的”用 `deployed` 区分。
    #[serde(default)]
    pub(crate) front: Vec<Option<CombatantRef>>,
    #[serde(default)]
    pub(crate) reserve: Vec<CombatantRef>,
    #[serde(default)]
    pub(crate) deployed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Battle {
    id: u32,
    province: u32,
    started_on: GameDate,
    /// 已结算的最后日期。同一日期再次推进不会重复扣兵。
    /// 当天结束战斗后新开的续战也会写成这一天，留到下一日才首结算。
    pub(crate) last_resolved: Option<GameDate>,
    pub(crate) elapsed_days: u32,
    pub(crate) rolls: [u8; 2],
    sides: [BattleSide; 2],
}

fn empty_side() -> BattleSide {
    BattleSide {
        armies: Vec::new(),
        strength_lost: 0,
        joined_strength: 0,
        front: Vec::new(),
        reserve: Vec::new(),
        deployed: false,
    }
}

impl Battle {
    pub(crate) fn new(id: u32, province: u32, started_on: GameDate) -> Self {
        Self {
            id,
            province,
            started_on,
            last_resolved: None,
            elapsed_days: 0,
            rolls: [0, 0],
            sides: [empty_side(), empty_side()],
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

    pub(crate) fn side(&self, side: usize) -> &BattleSide {
        &self.sides[side]
    }

    pub(crate) fn side_mut(&mut self, side: usize) -> &mut BattleSide {
        &mut self.sides[side]
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
    /// 败方没有可进入的省份，军队被删除且不返还人力。
    NoRetreat {
        loser: u8,
    },
    /// 旧档可能留下这个结果。新的结算不再产生它：同日双方都失败时进攻方败、防守方胜。
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
    use super::{
        LinePlan, MoraleStrike, StrengthStrike, cavalry_support_penalty, dice_scale,
        effective_tactics, enemy_strength_blocks_retreat, far_retreat_score, flank_distance,
        front_ratio_loser, initial_overrun_loser, line_plan, morale_damage, strength_damage,
        strength_quarter, thousandths, winner_morale_floor, wipe_refund_men, wipes_after_defeat,
    };
    use crate::Money;

    fn neutral_strength(die: i32, tech: i64, tactics: i64, elapsed: i64) -> i64 {
        strength_damage(StrengthStrike {
            die,
            margin: 0,
            strength: 1_000,
            discipline: 1_000,
            front_scalar: 1_000,
            defender_tactics: tactics,
            tech_multiplier: tech,
            ability: 1_000,
            defender_received: 0,
            attacker_dealt: 0,
            elapsed_days: elapsed,
            same_column_empty: false,
            quarter: 4,
        })
    }

    #[test]
    fn dice_scale_uses_the_clamped_margin() {
        assert_eq!(dice_scale(0, 0) / 5, 15);
        assert_eq!(dice_scale(0, -1) / 5, 10);
        assert_eq!(dice_scale(0, -2) / 5, 5);
        assert_eq!(dice_scale(0, 19) / 5, 110);
        assert_eq!(dice_scale(0, 40) / 5, 110);
    }

    #[test]
    fn full_regiment_truncates_strength_and_morale_independently() {
        assert_eq!(thousandths(Money::new(5, 1)), 500);
        assert_eq!(neutral_strength(0, 500, 500, 1), 15);
        assert_eq!(neutral_strength(0, 250, 500, 1), 7);
        assert_eq!(neutral_strength(9, 500, 500, 1), 60);
        let morale = morale_damage(MoraleStrike {
            die: 0,
            margin: 0,
            max_morale: 2_000,
            strength: 1_000,
            discipline: 1_000,
            front_scalar: 1_000,
            defender_tactics: 500,
            ability: 1_000,
            tech_multiplier: 500,
            elapsed_days: 1,
            defender_received: 0,
            attacker_dealt: 0,
        });
        assert_eq!(morale, 55);
    }

    #[test]
    fn damaged_regiment_loses_more_when_the_column_is_empty() {
        let full = neutral_strength(5, 500, 500, 1);
        let flanking = strength_damage(StrengthStrike {
            die: 5,
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
            same_column_empty: true,
            quarter: strength_quarter(200, 1_000),
        });
        assert_eq!(strength_quarter(200, 1_000), 1);
        assert!(flanking < full);
        assert_eq!(flank_distance(2, 1_000, 1_000), 2);
        assert_eq!(flank_distance(2, 100, 1_000), 1);
    }

    #[test]
    fn unsupported_cavalry_shrinks_only_the_cavalry_target() {
        let penalty = cavalry_support_penalty(400, 600, 500);
        assert_eq!(penalty, -100);
        assert_eq!(effective_tactics(500, true, penalty), 450);
        assert_eq!(effective_tactics(500, false, penalty), 500);
        assert_eq!(cavalry_support_penalty(500, 500, 500), 0);
    }

    #[test]
    fn line_plan_puts_a_small_engagement_in_the_center() {
        assert_eq!(
            line_plan(1, 0, 1, 15, 1),
            LinePlan {
                core: 1,
                cavalry: 0,
                extra_infantry: 0,
            }
        );
        assert_eq!(
            line_plan(10, 0, 1, 15, 1),
            LinePlan {
                core: 1,
                cavalry: 0,
                extra_infantry: 2,
            }
        );
        assert_eq!(
            line_plan(4, 2, 4, 15, 1),
            LinePlan {
                core: 4,
                cavalry: 2,
                extra_infantry: 0,
            }
        );
    }

    #[test]
    fn wipe_boundaries_are_strict_after_truncation() {
        assert!(!wipes_after_defeat(11, 1_000, 2_000));
        assert!(!wipes_after_defeat(11, 1_000, 2_001));
        assert!(wipes_after_defeat(11, 999, 2_000));
        assert!(!wipes_after_defeat(12, 1_000, 10_000));
        assert!(wipes_after_defeat(12, 999, 10_000));
        assert_eq!(
            initial_overrun_loser([1_000, 10_000], 15, [0, 0], [true, true]),
            None
        );
        assert_eq!(
            initial_overrun_loser([1_000, 11_000], 15, [0, 0], [true, true]),
            Some(0)
        );
        assert_eq!(
            initial_overrun_loser([15_000, 150_000], 15, [0, 0], [true, true]),
            None
        );
        assert_eq!(
            initial_overrun_loser([1_000, 11_000], 15, [2, 0], [true, true]),
            None
        );
        assert_eq!(front_ratio_loser([0, 0], [0, 0], [false, false]), None);
        assert_eq!(front_ratio_loser([9, 18], [0, 0], [true, true]), None);
        assert_eq!(
            front_ratio_loser([0, 1_000], [0, 0], [false, true]),
            Some(0)
        );
    }

    #[test]
    fn refund_and_winner_floor_keep_both_truncations() {
        assert_eq!(wipe_refund_men(1_000, 1_000, 1_000), 500);
        assert_eq!(wipe_refund_men(9, 1_000, 1_000), 4);
        assert_eq!(wipe_refund_men(1_000, 1_000, 0), 0);
        assert_eq!(winner_morale_floor(1_000, 10_000, 2_000), 250);
        // 100% 投入时 F(1000, 1250) 超过 750，下限停在最大士气的 75%。
        assert_eq!(winner_morale_floor(10_000, 10_000, 2_000), 1_500);
    }

    #[test]
    fn retreat_integer_boundaries_do_not_round_the_ratio() {
        assert!(!enemy_strength_blocks_retreat(1_000, 9_000));
        assert!(enemy_strength_blocks_retreat(1_000, 10_000));
        assert!(enemy_strength_blocks_retreat(999, 5_000));
        assert_eq!(far_retreat_score(300, 1, true), 6);
        assert_eq!(far_retreat_score(300, 10, false), 15);
        assert_eq!(far_retreat_score(300, 11, true), 0);
    }
}
