// SPDX-License-Identifier: GPL-3.0-only

//! 双边战争。联盟、属国、宣战理由和议和都不在此对象内。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::GameDate;

/// 战争进行状态。首版只有进行中，结束后的记录不保存在这里。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WarStatus {
    Ongoing,
}

impl WarStatus {
    pub const fn id(self) -> &'static str {
        match self {
            Self::Ongoing => "ongoing",
        }
    }
}

/// 一场只有进攻方和防守方的战争。双方查询到的是同一条记录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct War {
    id: u32,
    attacker: String,
    defender: String,
    started_on: GameDate,
    status: WarStatus,
}

impl War {
    pub fn id(&self) -> u32 {
        self.id
    }

    pub fn attacker(&self) -> &str {
        &self.attacker
    }

    pub fn defender(&self) -> &str {
        &self.defender
    }

    pub fn started_on(&self) -> GameDate {
        self.started_on
    }

    pub fn status(&self) -> WarStatus {
        self.status
    }

    pub(crate) fn involves(&self, country: &str) -> bool {
        self.attacker == country || self.defender == country
    }

    pub(crate) fn binds(&self, left: &str, right: &str) -> bool {
        (self.attacker == left && self.defender == right)
            || (self.attacker == right && self.defender == left)
    }
}

/// 进行中的战争。编号只增不减，读档后新战争不会占用旧编号。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct WarState {
    #[serde(default)]
    pub(crate) active: BTreeMap<u32, War>,
    #[serde(default = "first_war_id")]
    pub(crate) next_id: u32,
}

fn first_war_id() -> u32 {
    1
}

impl Default for WarState {
    fn default() -> Self {
        Self {
            active: BTreeMap::new(),
            next_id: first_war_id(),
        }
    }
}

impl WarState {
    pub(crate) fn insert(&mut self, attacker: &str, defender: &str, started_on: GameDate) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        let replaced = self.active.insert(
            id,
            War {
                id,
                attacker: attacker.to_owned(),
                defender: defender.to_owned(),
                started_on,
                status: WarStatus::Ongoing,
            },
        );
        assert!(replaced.is_none(), "战争编号不可重复");
        id
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum WarError {
    #[error("请选择有效的国家")]
    UnknownCountry,
    #[error("不能向自己宣战")]
    SelfTarget,
    #[error("已与该国交战")]
    AlreadyAtWar,
    #[error("战争编号已用尽")]
    WarLimit,
}
