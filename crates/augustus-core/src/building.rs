// SPDX-License-Identifier: GPL-3.0-only

use crate::{GameDate, Money};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct BuildingDefinition {
    pub id: String,
    pub cost: Money,
    pub months: u32,
    pub tax_modifier: Money,
    /// 建成后提供的陆军上限，来自建筑 `modifier.land_forcelimit`。
    pub land_forcelimit: Money,
    /// 建成后加入该省招募费用修正。
    pub local_regiment_cost: Money,
    /// 建成后加入该省招募时间修正。负值缩短工期。
    pub regiment_recruit_speed: Money,
    pub replaces: Option<String>,
    /// 当前内容规则允许新建的建筑；其他定义仍用于读取开局已有建筑。
    pub constructible: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Construction {
    pub building: String,
    pub country: String,
    pub started_on: GameDate,
    pub completes_on: GameDate,
}

#[derive(Debug)]
pub struct ConstructionQuote {
    pub cost: Money,
    pub completes_on: GameDate,
    pub monthly_tax_before: Money,
    pub monthly_tax_after: Money,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ConstructionError {
    #[error("请选择有效的国家")]
    UnknownCountry,
    #[error("只能在地图上的陆地省份建设")]
    InvalidProvince,
    #[error("只能在本国拥有并控制的省份建设")]
    NotOwnedAndControlled,
    #[error("该建筑当前不可新建")]
    UnavailableBuilding,
    #[error("该建筑或其升级建筑已经建成")]
    AlreadyBuilt,
    #[error("该省份已有建设项目")]
    AlreadyConstructing,
    #[error("国库不足，需要 {cost} 金币，当前有 {available} 金币")]
    InsufficientFunds { cost: Money, available: Money },
    #[error("完工日期超出游戏历法范围")]
    DateOutOfRange,
}
