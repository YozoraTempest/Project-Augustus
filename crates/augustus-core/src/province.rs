// SPDX-License-Identifier: GPL-3.0-only

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use crate::{Construction, Money};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProvinceKind {
    Land,
    Sea,
    Lake,
    Wasteland,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Province {
    pub kind: ProvinceKind,
    /// 保存国家标签，通过 World::province_owner 查询对应实体；无主省份为 None。
    pub owner: Option<String>,
    pub controller: Option<String>,
    pub culture: Option<String>,
    pub religion: Option<String>,
    pub trade_good: Option<String>,
    pub development: Development,
    pub buildings: BTreeSet<String>,
    pub construction: Option<Construction>,
    /// 无要塞占领进度。旧档没有该字段时视为没有进行中的占领。
    #[serde(default)]
    pub occupation: Option<OccupationProgress>,
    /// 要塞驻军人数。无要塞省份保持空；旧档由世界按当前规则补齐。
    #[serde(default)]
    pub garrison: Option<u32>,
    /// 要塞停用后驻军耗尽，并且不再每月恢复。
    #[serde(default)]
    pub fort_mothballed: bool,
    /// 进行中的要塞围城。与无要塞占领分开保存。
    #[serde(default)]
    pub siege: Option<SiegeProgress>,
    /// 地形提供的补给加成来自地图内容，不随存档变化；读档时由当前内容重新填入。
    #[serde(skip)]
    pub terrain_supply: u32,
}

/// 无要塞占领的阶段长度，也是围城已累计天数的存档上限。
/// 围城阶段长度的公式在围城模块；防御、围城能力和战术差都是 0 时同样是 30。
pub const OCCUPATION_PHASE_DAYS: u32 = 30;

/// 无要塞占领已累计的天数。满一个阶段后立即改变控制者并清除，因此存档中的天数小于阶段长度。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OccupationProgress {
    pub occupier: String,
    pub days: u32,
}

/// 要塞围城。天数是本阶段已累计的天数，检定之后回到 0，要塞状态另行保留。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SiegeProgress {
    pub besieger: String,
    pub days: u32,
    /// 加到下一次检定上的要塞状态。
    pub status: u32,
    /// 已经打开的破口。达到 3 次后不再破墙。旧档没有该字段时是 0。
    #[serde(default)]
    pub breaches: u32,
    #[serde(default)]
    pub last_roll: Option<u8>,
    #[serde(default)]
    pub last_result: Option<SiegeRoll>,
}

/// 一次围城检定的结果。标识稳定，界面只负责显示名称。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SiegeRoll {
    StatusQuo,
    SuppliesShortage,
    FoodShortage,
    WaterShortage,
    DefendersDesert,
    Disease,
    Surrender,
    Breach,
}

impl SiegeRoll {
    pub fn label(self) -> &'static str {
        match self {
            Self::StatusQuo => "没有变化",
            Self::SuppliesShortage => "补给短缺",
            Self::FoodShortage => "食物短缺",
            Self::WaterShortage => "饮水短缺",
            Self::DefendersDesert => "守军逃亡",
            Self::Disease => "疾病爆发",
            Self::Surrender => "投降",
            Self::Breach => "破墙",
        }
    }
}

impl Province {
    pub fn new(kind: ProvinceKind) -> Self {
        Self {
            kind,
            owner: None,
            controller: None,
            culture: None,
            religion: None,
            trade_good: None,
            development: Development::default(),
            buildings: BTreeSet::new(),
            construction: None,
            occupation: None,
            garrison: None,
            fort_mothballed: false,
            siege: None,
            terrain_supply: 0,
        }
    }

    /// 基础年税额等于税收发展度；水域、荒地与无主土地不产生国家税收。
    pub fn annual_tax(&self) -> Option<Money> {
        if self.kind != ProvinceKind::Land || self.owner.is_none() {
            return Some(Money::ZERO);
        }
        self.development.tax.map(Money::from)
    }

    pub fn monthly_tax(&self) -> Option<Money> {
        self.annual_tax().map(|annual| annual / Money::from(12))
    }
}

/// 三项基础发展度均为整数；未提供的字段保留 None，不把缺失内容当成零。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Development {
    pub tax: Option<u32>,
    pub production: Option<u32>,
    pub manpower: Option<u32>,
}

impl Development {
    pub fn total(&self) -> Option<u64> {
        Some(u64::from(self.tax?) + u64::from(self.production?) + u64::from(self.manpower?))
    }
}
