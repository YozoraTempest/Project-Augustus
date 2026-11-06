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

#[derive(Debug, Clone)]
pub struct MilitaryRules {
    pub infantry_cost: Money,
    pub infantry_days: u32,
    /// 取消招募时仍可全退金币的进度上界，恰好等于上界也全退。
    pub recruitment_full_refund_progress: Money,
    pub land_maintenance_factor: Money,
    pub manpower: ManpowerRules,
}

impl MilitaryRules {
    /// 基础维护系数作用于招募费用，得到每团年费；月费在国家汇总后折算。
    pub fn annual_maintenance(&self, kind: UnitKind) -> Money {
        let cost = match kind {
            UnitKind::Infantry => self.infantry_cost,
        };
        cost * self.land_maintenance_factor
    }
}

#[derive(Debug)]
pub struct RecruitmentQuote {
    pub cost: Money,
    pub manpower: u64,
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
    pub(crate) recruitments: BTreeMap<u32, Recruitment>,
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
    #[error("该省份正在招募部队")]
    AlreadyRecruiting,
    #[error("该省份没有正在进行的招募")]
    NotRecruiting,
    #[error("只能取消本国的招募")]
    NotPlayerRecruitment,
    #[error("该招募已完成或尚未开始，无法取消")]
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
