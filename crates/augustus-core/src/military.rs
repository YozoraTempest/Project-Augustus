// SPDX-License-Identifier: GPL-3.0-only

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{ArmyMovement, GameDate, Money};

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
}

#[derive(Debug, Clone)]
pub struct RecruitmentRules {
    pub infantry_cost: Money,
    pub infantry_days: u32,
}

#[derive(Debug)]
pub struct RecruitmentQuote {
    pub cost: Money,
    pub completes_on: GameDate,
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
    #[error("国库不足，需要 {cost} 金币，当前有 {available} 金币")]
    InsufficientFunds { cost: Money, available: Money },
    #[error("招募完成日期超出游戏历法范围")]
    DateOutOfRange,
    #[error("军队编号已用尽")]
    ArmyLimit,
}
