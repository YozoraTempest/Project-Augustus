// SPDX-License-Identifier: GPL-3.0-only

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProvinceKind {
    Land,
    Sea,
    Lake,
    Wasteland,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Province {
    pub kind: ProvinceKind,
    /// 保存国家标签，通过 World::province_owner 查询对应实体；无主省份为 None。
    pub owner: Option<String>,
    pub controller: Option<String>,
    pub culture: Option<String>,
    pub religion: Option<String>,
    pub trade_good: Option<String>,
    pub development: Development,
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
        }
    }
}

/// 三项基础发展度均为整数；未提供的字段保留 None，不把缺失内容当成零。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
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
