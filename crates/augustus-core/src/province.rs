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
}
