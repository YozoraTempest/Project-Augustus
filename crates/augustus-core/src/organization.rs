// SPDX-License-Identifier: GPL-3.0-only

use crate::Money;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArmyDisbandQuote {
    pub regiment_count: usize,
    pub strength: u64,
    pub monthly_maintenance: Money,
    pub manpower_returned: u64,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ArmyOrganizationError {
    #[error("请选择有效的国家")]
    UnknownCountry,
    #[error("请选择有效的军队")]
    UnknownArmy,
    #[error("只能操作本国军队")]
    NotPlayerArmy,
    #[error("请先让军队驻扎")]
    ArmyMoving,
    #[error("军队正在交战，不能编组")]
    InBattle,
    #[error("只能合并驻扎在同一省份的军队")]
    DifferentProvince,
    #[error("请选择要合并的军队")]
    NothingToMerge,
    #[error("合并列表不能包含重复军队或当前军队")]
    DuplicateArmy,
    #[error("拆分数量必须至少为 1 团，且原军队至少保留 1 团")]
    InvalidRegimentCount,
    #[error("军队编号已用尽")]
    ArmyLimit,
}
