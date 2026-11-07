// SPDX-License-Identifier: GPL-3.0-only

use std::collections::BTreeMap;

use rust_decimal::RoundingStrategy;
use serde::{Deserialize, Serialize};

use crate::combat::{CombatRules, UNSET_MORALE};
use crate::{ArmyMovement, GameDate, ManpowerRules, Money};

/// 一个满编团的兵力；军队可以持有多个团，编组与兵种分别保存。
pub const REGIMENT_STRENGTH: u32 = 1_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnitKind {
    Infantry,
    Cavalry,
}

impl UnitKind {
    pub const ALL: [Self; 2] = [Self::Infantry, Self::Cavalry];

    /// 与存档和界面命令使用的标识一致。
    pub const fn id(self) -> &'static str {
        match self {
            Self::Infantry => "infantry",
            Self::Cavalry => "cavalry",
        }
    }
}

fn unset_morale() -> u32 {
    UNSET_MORALE
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Regiment {
    pub kind: UnitKind,
    pub strength: u32,
    /// 以千分之一点保存。旧档没有该字段时先占位，读入后改成满士气。
    #[serde(default = "unset_morale")]
    pub morale: u32,
}

impl Regiment {
    /// 测试和直接驻军使用满士气占位。招募完成时写入规则中的实际满士气。
    pub fn fresh(kind: UnitKind, strength: u32) -> Self {
        Self {
            kind,
            strength,
            morale: UNSET_MORALE,
        }
    }

    pub(crate) fn effective_morale(&self, max: u32) -> u32 {
        if self.morale == UNSET_MORALE {
            max
        } else {
            self.morale
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Army {
    pub(crate) id: u32,
    pub country: String,
    pub province: u32,
    pub regiments: Vec<Regiment>,
    pub movement: Option<ArmyMovement>,
    /// 进行中的战斗。一支军队同时只属于一场战斗。
    #[serde(default)]
    pub(crate) battle_id: Option<u32>,
    /// 败退途中不能接敌，也不能接受调动或编组。
    #[serde(default)]
    pub(crate) retreating: bool,
}

impl Army {
    pub fn id(&self) -> u32 {
        self.id
    }

    pub fn battle_id(&self) -> Option<u32> {
        self.battle_id
    }

    pub fn is_retreating(&self) -> bool {
        self.retreating
    }

    /// 仍有兵力的团的平均士气。没有活人时为零。
    pub fn average_morale(&self, max: u32) -> u32 {
        let mut total = 0_u64;
        let mut count = 0_u64;
        for regiment in &self.regiments {
            if regiment.strength == 0 {
                continue;
            }
            total += u64::from(regiment.effective_morale(max));
            count += 1;
        }
        total.checked_div(count).unwrap_or(0) as u32
    }

    pub fn strength(&self) -> u64 {
        self.regiments
            .iter()
            .map(|regiment| u64::from(regiment.strength))
            .sum()
    }

    /// 返回该兵种的团数和兵力合计。
    pub fn composition(&self, kind: UnitKind) -> (usize, u64) {
        self.regiments
            .iter()
            .filter(|regiment| regiment.kind == kind)
            .fold((0, 0), |(count, strength), regiment| {
                (count + 1, strength + u64::from(regiment.strength))
            })
    }

    /// 混编军队的行军速度由最慢的兵种决定。
    pub fn march_speed(&self, rules: &MilitaryRules) -> Money {
        self.regiments
            .iter()
            .map(|regiment| rules.speed(regiment.kind))
            .min()
            .expect("军队至少有一个团")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recruitment {
    // 发起招募时预留编号，读取存档或多省份同时完成都不会重复分配。
    pub(crate) army_id: u32,
    pub country: String,
    pub kind: UnitKind,
    pub started_on: GameDate,
    pub completes_on: GameDate,
    // 旧存档缺少实付记录，由 World 恢复时按当时已实现的招募规则补齐。
    #[serde(default)]
    pub(crate) paid_cost: Option<Money>,
    #[serde(default)]
    pub(crate) paid_manpower: Option<u64>,
}

impl Recruitment {
    /// 订单编号，同时是这支部队完成时可能使用的军队编号。
    pub fn id(&self) -> u32 {
        self.army_id
    }

    /// 尚未到开始日的订单进度为零；进行中的订单按已过天数占本单工期的比例。
    pub fn progress(&self, date: GameDate) -> f64 {
        if date < self.started_on {
            return 0.0;
        }
        let total = self.started_on.days_until(self.completes_on);
        if total <= 0 {
            return 0.0;
        }
        let elapsed = total - date.days_until(self.completes_on).max(0);
        f64::from(elapsed) / f64::from(total) * 100.0
    }
}

#[derive(Debug, Clone)]
pub struct MilitaryRules {
    pub infantry_cost: Money,
    pub cavalry_cost: Money,
    pub infantry_days: u32,
    pub cavalry_days: u32,
    pub infantry_speed: Money,
    pub cavalry_speed: Money,
    /// 取消招募时仍可全退金币的进度上界，恰好等于上界也全退。
    pub recruitment_full_refund_progress: Money,
    pub land_maintenance_factor: Money,
    /// 每团每月基础补员比例，来自 `MONTHLY_REINFORCE`。维护费滑条尚未接入，系数按满额计算。
    pub monthly_reinforce: Money,
    /// 补员费相对基础维护的倍数，来自 `EXTRA_LAND_REINFORCE_COST`。不乘超额维护。
    pub extra_land_reinforce_cost: Money,
    /// 省份补给上限的基础值，对应 defines 的 SUPPLYLIMIT_BASE_MULTIPLIER。
    pub supply_base: u32,
    /// 有陆地领土的国家获得一次，来自 static modifier `all_nations`。
    pub base_land_forcelimit: Money,
    /// 本国每点发展度提供的陆军上限，来自 static modifier `development`，不按自治度折减。
    pub development_land_forcelimit: Money,
    /// 超编时作用于全部基础军费的系数，来自 `LAND_FORCELIMIT_EXTRA_COST_FACTOR`。
    pub land_forcelimit_extra_cost_factor: Money,
    /// 贸易商品对本省提供的陆军上限，读取商品定义中的 `province.land_forcelimit`。
    pub goods_land_forcelimit: BTreeMap<String, Money>,
    /// 每点税收发展度加入的省份招募时间修正。
    pub tax_recruitment_time: Money,
    /// 和平开局陆军占已取整陆军上限的比例，来自 `STARTING_ARMY_SIZE`。0.75 表示 75%。
    pub starting_army_size: Money,
    /// 已经处于战争时使用的开局比例，来自 `STARTING_ARMY_SIZE_AT_WAR`。
    pub starting_army_size_at_war: Money,
    pub manpower: ManpowerRules,
    /// 基础陆战。放在结构末尾，避免和开局军队规模字段抢同一位置。
    pub combat: CombatRules,
}

impl MilitaryRules {
    pub fn cost(&self, kind: UnitKind) -> Money {
        match kind {
            UnitKind::Infantry => self.infantry_cost,
            UnitKind::Cavalry => self.cavalry_cost,
        }
    }

    pub fn recruitment_days(&self, kind: UnitKind) -> u32 {
        match kind {
            UnitKind::Infantry => self.infantry_days,
            UnitKind::Cavalry => self.cavalry_days,
        }
    }

    /// 费用修正先相加，系数不低于 20%，再乘基础费用，按千分之一金币向零截断。
    pub fn modified_cost(&self, kind: UnitKind, modifier: Money) -> Money {
        let factor = (Money::ONE + modifier).max(Money::new(2, 1));
        (self.cost(kind) * factor).trunc_with_scale(3)
    }

    /// 时间修正先相加后乘基础天数，向上取整，至少 1 天。
    pub fn modified_days(&self, kind: UnitKind, modifier: Money) -> u32 {
        let duration = Money::from(self.recruitment_days(kind)) * (Money::ONE + modifier);
        if duration <= Money::ZERO {
            return 1;
        }
        u32::try_from(duration.ceil())
            .expect("招募天数在支持范围内")
            .max(1)
    }

    pub fn speed(&self, kind: UnitKind) -> Money {
        match kind {
            UnitKind::Infantry => self.infantry_speed,
            UnitKind::Cavalry => self.cavalry_speed,
        }
    }

    pub fn manpower_cost(&self, kind: UnitKind) -> u64 {
        match kind {
            UnitKind::Infantry => self.manpower.infantry_cost,
            UnitKind::Cavalry => self.manpower.cavalry_cost,
        }
    }

    /// 地图距离除以兵种速度，向上取整为完整游戏日。
    pub fn travel_days(distance: u32, speed: Money) -> u32 {
        u32::try_from((Money::from(distance) / speed).ceil()).expect("行军天数在支持范围内")
    }

    /// 年维护费使用兵种基础费用，不乘招募报价的修正。
    pub fn annual_maintenance(&self, kind: UnitKind) -> Money {
        self.cost(kind) * self.land_maintenance_factor
    }

    /// 实际补上的人数一次性扣款。精确到千分之一金币，尾数舍去，不并入月度维护。
    pub fn reinforcement_cost(&self, kind: UnitKind, men: u64) -> Money {
        if men == 0 {
            return Money::ZERO;
        }
        let amount =
            self.extra_land_reinforce_cost * self.land_maintenance_factor * Money::from(men)
                / Money::from(REGIMENT_STRENGTH)
                * self.cost(kind);
        amount.round_dp_with_strategy(3, RoundingStrategy::ToZero)
    }

    /// 超编超额年费加在全部基础军费上。取整上限为 0 时原公式不能相除，超额为零。
    pub fn excess_annual_maintenance(
        &self,
        base_annual: Money,
        regiments: u64,
        limit: u64,
    ) -> Money {
        if limit == 0 || regiments <= limit {
            return Money::ZERO;
        }
        let over = Money::from(regiments - limit);
        base_annual * over * self.land_forcelimit_extra_cost_factor / Money::from(limit)
    }
}

#[derive(Debug)]
pub struct RecruitmentQuote {
    pub cost: Money,
    pub manpower: u64,
    pub started_on: GameDate,
    pub completes_on: GameDate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecruitmentRefund {
    pub cost: Money,
    pub manpower: u64,
}

fn default_next_battle_id() -> u32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MilitaryState {
    pub(crate) armies: BTreeMap<u32, Army>,
    // 每个省份按完成顺序保存订单。第 9 版之前每个省份只有一个对象，读取时视为仅含队首的队列。
    #[serde(deserialize_with = "deserialize_recruitments")]
    pub(crate) recruitments: BTreeMap<u32, Vec<Recruitment>>,
    pub(crate) next_army_id: u32,
    /// 进行中的战斗。旧档没有战斗，读入后为空。
    #[serde(default)]
    pub(crate) battles: BTreeMap<u32, crate::Battle>,
    /// 每个省份保留最近一场已结束的战斗，供界面和读档后续算查询。
    #[serde(default)]
    pub(crate) battle_reports: BTreeMap<u32, crate::BattleReport>,
    #[serde(default = "default_next_battle_id")]
    pub(crate) next_battle_id: u32,
}

impl Default for MilitaryState {
    fn default() -> Self {
        Self {
            armies: BTreeMap::new(),
            recruitments: BTreeMap::new(),
            next_army_id: 1,
            battles: BTreeMap::new(),
            battle_reports: BTreeMap::new(),
            next_battle_id: default_next_battle_id(),
        }
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RecruitmentError {
    #[error("请选择有效的国家")]
    UnknownCountry,
    #[error("只能在地图上的陆地省份招募")]
    InvalidProvince,
    #[error("只能在本国拥有并控制的省份招募")]
    NotOwnedAndControlled,
    #[error("该省份已有其他国家的招募")]
    ForeignQueue,
    #[error("该省份没有正在进行的招募")]
    NotRecruiting,
    #[error("找不到该招募订单")]
    UnknownOrder,
    #[error("只能取消本国的招募")]
    NotPlayerRecruitment,
    #[error("该招募已完成，无法取消")]
    RecruitmentNotActive,
    #[error("国库不足，需要 {cost} 金币，当前有 {available} 金币")]
    InsufficientFunds { cost: Money, available: Money },
    #[error("人力不足，需要 {cost} 人，当前可用 {available} 人")]
    InsufficientManpower { cost: u64, available: u64 },
    #[error("招募完成日期超出游戏历法范围")]
    DateOutOfRange,
    #[error("军队编号已用尽")]
    ArmyLimit,
}

fn deserialize_recruitments<'de, D>(
    deserializer: D,
) -> Result<BTreeMap<u32, Vec<Recruitment>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Slot {
        Orders(Vec<Recruitment>),
        Single(Recruitment),
    }

    Ok(BTreeMap::<u32, Slot>::deserialize(deserializer)?
        .into_iter()
        .map(|(province, slot)| {
            (
                province,
                match slot {
                    Slot::Orders(orders) => orders,
                    Slot::Single(order) => vec![order],
                },
            )
        })
        .collect())
}
