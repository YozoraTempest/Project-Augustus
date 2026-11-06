// SPDX-License-Identifier: GPL-3.0-only

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Regiment {
    pub kind: UnitKind,
    pub strength: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Army {
    pub(crate) id: u32,
    pub country: String,
    pub province: u32,
    pub regiments: Vec<Regiment>,
    pub movement: Option<ArmyMovement>,
}

impl Army {
    pub fn id(&self) -> u32 {
        self.id
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
    /// 省份补给上限的基础值，对应 defines 的 SUPPLYLIMIT_BASE_MULTIPLIER。
    pub supply_base: u32,
    pub manpower: ManpowerRules,
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

    /// 基础维护系数作用于招募费用，得到每团年费；月费在国家汇总后折算。
    pub fn annual_maintenance(&self, kind: UnitKind) -> Money {
        self.cost(kind) * self.land_maintenance_factor
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MilitaryState {
    pub(crate) armies: BTreeMap<u32, Army>,
    // 每个省份按完成顺序保存订单。第 9 版之前每个省份只有一个对象，读取时视为仅含队首的队列。
    #[serde(deserialize_with = "deserialize_recruitments")]
    pub(crate) recruitments: BTreeMap<u32, Vec<Recruitment>>,
    pub(crate) next_army_id: u32,
}

impl Default for MilitaryState {
    fn default() -> Self {
        Self {
            armies: BTreeMap::new(),
            recruitments: BTreeMap::new(),
            next_army_id: 1,
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
