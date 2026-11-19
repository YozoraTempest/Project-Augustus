// SPDX-License-Identifier: GPL-3.0-only

//! 国家实体的标识与状态，由 World 统一持有。

use crate::{CountryFinances, CountryMilitary, ManpowerPool};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Country {
    // 标识不可在创建后修改，保持 World 索引与省份归属引用一致。
    tag: String,
    pub capital: Option<u32>,
    pub primary_culture: Option<String>,
    pub religion: Option<String>,
    /// 第二宗教。旧档没有这个字段。
    #[serde(default)]
    pub secondary_religion: Option<String>,
    /// 开局行政、外交、军事科技等级。旧档没有时先空着。
    #[serde(default)]
    pub adm_tech: Option<u32>,
    #[serde(default)]
    pub dip_tech: Option<u32>,
    #[serde(default)]
    pub mil_tech: Option<u32>,
    /// 开局在位君主。没有就不填。
    #[serde(default)]
    pub monarch: Option<CourtPerson>,
    /// 开局继承人。没有就空着，右栏不画。
    #[serde(default)]
    pub heir: Option<CourtPerson>,
    pub finances: CountryFinances,
    // 缺失表示尚未组成世界或来自旧版存档，由 World 根据领土初始化。
    #[serde(default)]
    pub(crate) manpower: Option<ManpowerPool>,
    /// 开局军事配置。旧档没有该字段时先空着，读入后改用当时的统一配置。
    #[serde(default)]
    pub military: Option<CountryMilitary>,
}

impl Country {
    pub fn new(tag: impl Into<String>) -> Self {
        Self {
            tag: tag.into(),
            capital: None,
            primary_culture: None,
            religion: None,
            secondary_religion: None,
            adm_tech: None,
            dip_tech: None,
            mil_tech: None,
            monarch: None,
            heir: None,
            finances: CountryFinances::default(),
            manpower: None,
            military: None,
        }
    }

    pub fn tag(&self) -> &str {
        &self.tag
    }
}

/// 君主或继承人。这一步只留名字和三项能力。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CourtPerson {
    pub name: String,
    pub adm: u32,
    pub dip: u32,
    pub mil: u32,
}
