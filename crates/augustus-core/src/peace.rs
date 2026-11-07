// SPDX-License-Identifier: GPL-3.0-only

//! 双边战争分数、省份割让费用、接受判定和停战长度。
//!
//! 已核对的原版常数：`MAX_WARSCORE` 100、`WARSCORE_MAX_FROM_BATTLES` 40、
//! 省份费用乘数 0.8 / 未控制 1.1 / 首都 1.2、`TRUCE_YEARS` 5、`SCALED_TRUCE_YEARS` 10。
//! 陆战分数沿用损失与陆军上限的整数路径，单场下限 0.1，战斗小计限制在 ±40。
//! 本阶段没有战争目标，因此不计 ticking warscore；无要塞占领也不使用尚未确认的要塞权重。
//! 省份费用只按发展度乘上述乘数，不采用未在反编译中出现的加法底数。

use crate::GameDate;

pub const MAX_WARSCORE: i64 = 100;
pub const MAX_BATTLE_WARSCORE: i64 = 40;
const SCALE: i64 = 1_000;
pub const MAX_WARSCORE_MILLI: i64 = MAX_WARSCORE * SCALE;
pub const MAX_BATTLE_WARSCORE_MILLI: i64 = MAX_BATTLE_WARSCORE * SCALE;

const TRUCE_YEARS: i64 = 5;
const SCALED_TRUCE_YEARS: i64 = 10;

/// 把千分点格式化为三位小数。战斗下限 0.1 不能被整点截断丢掉。
pub fn format_warscore_milli(milli: i64) -> String {
    let sign = if milli < 0 { "-" } else { "" };
    let value = milli.unsigned_abs();
    format!("{sign}{}.{:03}", value / 1_000, value % 1_000)
}

/// 一场已结束陆战对胜方的千分点。
///
/// `losses` 是败方损失的兵力。`force_limit` 是败方陆军上限与战争进攻方陆军上限的较小值，单位为整团。
/// 分母在战斗结束时取定，之后不随领土变化重算。
pub fn land_battle_milli(losses: u64, force_limit: u64) -> i64 {
    let losses_thousands = i64::try_from(losses / 1_000).unwrap_or(i64::MAX);
    let force_limit = i64::try_from(force_limit).unwrap_or(i64::MAX);
    let doubled_limit = force_limit.saturating_mul(1_000).saturating_mul(2);
    let loss_floor = losses_thousands.saturating_mul(2_000);
    let denominator = doubled_limit.max(loss_floor);
    let ratio = if denominator == 0 {
        -1
    } else {
        losses_thousands.saturating_mul(1_000_000) / denominator
    };
    ratio.saturating_mul(MAX_BATTLE_WARSCORE / 2).max(100)
}

/// 占领贡献：`100 * 已占发展度 / 对方总发展度`，先截断为整点再换成千分点。总发展度为 0 时没有贡献。
pub fn occupation_share_milli(occupied_development: u64, total_development: u64) -> i64 {
    if total_development == 0 {
        return 0;
    }
    let points = (100 * occupied_development) / total_development;
    i64::try_from(points).unwrap_or(i64::MAX).saturating_mul(SCALE)
}

/// 发展度千分点依次乘首都 1.2、未由提出方控制 1.1、省份要求 0.8。每步向零截断。
pub fn province_cost_milli(development: u64, capital: bool, controlled_by_offerer: bool) -> i64 {
    let mut cost = i64::try_from(development).unwrap_or(i64::MAX).saturating_mul(SCALE);
    if capital {
        cost = cost.saturating_mul(12) / 10;
    }
    if !controlled_by_offerer {
        cost = cost.saturating_mul(11) / 10;
    }
    cost.saturating_mul(8) / 10
}

/// 把战斗条目合计限制在 ±40 点。
pub fn clamp_battle_milli(sum: i64) -> i64 {
    sum.clamp(-MAX_BATTLE_WARSCORE_MILLI, MAX_BATTLE_WARSCORE_MILLI)
}

/// 占领与战斗相加后限制在 ±100 点。
pub fn clamp_warscore_milli(sum: i64) -> i64 {
    sum.clamp(-MAX_WARSCORE_MILLI, MAX_WARSCORE_MILLI)
}

/// 停战月数。`cost_points` 是和约花费的整点战争分数，不是尚未使用的分数。
/// 0 点为 61 个月，100 点为 181 个月：基础 5 年，按百分比线性加到 10 年，再加 1 个月。
pub fn truce_months(cost_points: i64) -> u32 {
    let spent = cost_points.clamp(0, MAX_WARSCORE);
    let scaled = (TRUCE_YEARS * SCALE + spent * SCALED_TRUCE_YEARS * 10) * 12 + SCALE;
    u32::try_from(scaled / SCALE).unwrap_or(u32::MAX)
}

/// 到期日当天停战已经结束。生效区间是签订日到该日期的前一天。
/// 先按月数推进，再把日对齐到 1 日并多加一天，与脚本停战的月末观察一致。
pub fn truce_expiry(signed_on: GameDate, cost_points: i64) -> Option<GameDate> {
    let arrived = signed_on.checked_add_months(truce_months(cost_points))?;
    GameDate::new(arrived.year(), arrived.month(), 1)?.checked_add_days(1)
}

/// 接受非白和平条款时，提出方的战争分数必须不少于条款费用。白和平费用为 0，始终可接受。
pub fn accepts_terms(score_milli: i64, cost_milli: i64, white_peace: bool) -> bool {
    white_peace || score_milli >= cost_milli
}

/// 从查询国视角看到的战争分数，以及可向界面说明的来源。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WarScoreView {
    pub total_milli: i64,
    pub battle_milli: i64,
    pub battle_unclamped_milli: i64,
    pub occupation_milli: i64,
    pub battles: Vec<ViewerBattle>,
    pub occupied_enemy_development: u64,
    pub enemy_development: u64,
    pub enemy_occupied_development: u64,
    pub own_development: u64,
}

/// 一场战斗在查询国视角下的贡献。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewerBattle {
    pub province: u32,
    pub on: GameDate,
    pub loser: String,
    pub losses: u64,
    pub milli: i64,
}

impl WarScoreView {
    pub fn summary_text(&self) -> String {
        format!(
            "战争分数 {}（上限 ±{}）",
            format_warscore_milli(self.total_milli),
            MAX_WARSCORE
        )
    }

    pub fn battle_text(&self) -> String {
        let mut lines = vec![format!(
            "战斗 {}（合计 {}，上限 ±{}）",
            format_warscore_milli(self.battle_milli),
            format_warscore_milli(self.battle_unclamped_milli),
            MAX_BATTLE_WARSCORE
        )];
        if self.battles.is_empty() {
            lines.push("尚无已结束的战斗".to_owned());
        }
        for battle in &self.battles {
            lines.push(format!(
                "{} 省份 {}：{} 损失 {} 人，{}",
                battle.on,
                battle.province,
                battle.loser,
                battle.losses,
                format_warscore_milli(battle.milli)
            ));
        }
        lines.join("\n")
    }

    pub fn occupation_text(&self) -> String {
        format!(
            "占领 {}：已占对方发展度 {}/{}，对方占我方 {}/{}",
            format_warscore_milli(self.occupation_milli),
            self.occupied_enemy_development,
            self.enemy_development,
            self.enemy_occupied_development,
            self.own_development
        )
    }
}

/// 一个可割让省份的费用。费用已经包含本阶段使用的乘数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProvinceCost {
    pub province: u32,
    pub development: u64,
    pub cost_milli: i64,
    pub capital: bool,
    pub controlled_by_offerer: bool,
}

impl ProvinceCost {
    pub fn cost_text(&self) -> String {
        let mut notes = Vec::new();
        if self.capital {
            notes.push("首都");
        }
        notes.push(if self.controlled_by_offerer {
            "已控制"
        } else {
            "未控制"
        });
        format!(
            "发展度 {} · 费用 {} · {}",
            self.development,
            format_warscore_milli(self.cost_milli),
            notes.join("，")
        )
    }
}

/// 条款是否合法、能否接受，以及接受后的停战到期日。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeaceQuote {
    pub score_milli: i64,
    pub cost_milli: i64,
    pub accepted: bool,
    pub reason: String,
    pub demands: Vec<ProvinceCost>,
    pub truce_until: GameDate,
}

/// 和谈界面一次取回的分数、可割让省份和白和平。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeaceSheet {
    pub score: WarScoreView,
    pub provinces: Vec<ProvinceCost>,
    pub white_peace: PeaceQuote,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PeaceError {
    #[error("请选择有效的国家")]
    UnknownCountry,
    #[error("双方没有进行中的战争")]
    NotAtWar,
    #[error("只能割让对方拥有的陆地省份")]
    NotCession,
    #[error("战争分数不足：现有 {score}，条款需要 {cost}")]
    InsufficientScore { score: String, cost: String },
    #[error("停战日期超出游戏历法范围")]
    DateOutOfRange,
}

#[cfg(test)]
mod tests {
    use super::{
        accepts_terms, clamp_battle_milli, format_warscore_milli, land_battle_milli,
        occupation_share_milli, province_cost_milli, truce_expiry, truce_months,
    };
    use crate::GameDate;

    #[test]
    fn land_battle_matches_the_force_limit_example_and_the_floor() {
        // 损失 10000、陆军上限 11：分母取双倍上限，结果 9.080。
        assert_eq!(land_battle_milli(10_000, 11), 9_080);
        // 伤亡超过上限对应的兵力后，单场正好是 10 点，随后才做 ±40 合计限制。
        assert_eq!(land_battle_milli(20_000, 11), 10_000);
        assert_eq!(land_battle_milli(0, 0), 100);
        assert_eq!(format_warscore_milli(9_080), "9.080");
        assert_eq!(format_warscore_milli(-100), "-0.100");
    }

    #[test]
    fn occupation_and_province_cost_use_only_the_confirmed_factors() {
        assert_eq!(occupation_share_milli(9, 18), 50_000);
        assert_eq!(occupation_share_milli(1, 0), 0);
        // 发展度 10，首都且未控制：10000 → 12000 → 13200 → 10560。
        assert_eq!(province_cost_milli(10, true, false), 10_560);
        assert_eq!(province_cost_milli(10, false, true), 8_000);
        assert_eq!(clamp_battle_milli(50_000), 40_000);
        assert!(!accepts_terms(7_999, 8_000, false));
        assert!(accepts_terms(0, 0, true));
    }

    #[test]
    fn truce_length_follows_the_spent_warscore() {
        assert_eq!(truce_months(0), 61);
        assert_eq!(truce_months(50), 121);
        assert_eq!(truce_months(100), 181);
        let signed = GameDate::new(1444, 11, 11).unwrap();
        assert_eq!(
            truce_expiry(signed, 0),
            GameDate::new(1449, 12, 2)
        );
        assert_eq!(
            truce_expiry(signed, 100),
            GameDate::new(1459, 12, 2)
        );
    }
}
