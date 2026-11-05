// SPDX-License-Identifier: GPL-3.0-only

//! 国家实体的标识与状态，由 World 统一持有。

use crate::{CountryFinances, ManpowerPool};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Country {
    // 标识不可在创建后修改，保持 World 索引与省份归属引用一致。
    tag: String,
    pub capital: Option<u32>,
    pub primary_culture: Option<String>,
    pub religion: Option<String>,
    pub finances: CountryFinances,
    // 缺失表示尚未组成世界或来自旧版存档，由 World 根据领土初始化。
    #[serde(default)]
    pub(crate) manpower: Option<ManpowerPool>,
}

impl Country {
    pub fn new(tag: impl Into<String>) -> Self {
        Self {
            tag: tag.into(),
            capital: None,
            primary_culture: None,
            religion: None,
            finances: CountryFinances::default(),
            manpower: None,
        }
    }

    pub fn tag(&self) -> &str {
        &self.tag
    }
}
