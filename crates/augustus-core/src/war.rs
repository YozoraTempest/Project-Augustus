// SPDX-License-Identifier: GPL-3.0-only

//! 双边战争与停战。联盟、属国、单独媾和和赔款不在此对象内。

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

/// 一场已结束战斗对战争进攻方的分数。合计限制在查询时计算，不写回本条。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BattleScore {
    province: u32,
    on: GameDate,
    /// 战争进攻方视角的千分点。正数表示进攻方因这场战斗领先。
    attacker_milli: i64,
    loser: String,
    losses: u64,
}

impl BattleScore {
    pub(crate) fn new(
        province: u32,
        on: GameDate,
        attacker_milli: i64,
        loser: String,
        losses: u64,
    ) -> Self {
        Self {
            province,
            on,
            attacker_milli,
            loser,
            losses,
        }
    }

    pub fn province(&self) -> u32 {
        self.province
    }

    pub fn on(&self) -> GameDate {
        self.on
    }

    pub fn attacker_milli(&self) -> i64 {
        self.attacker_milli
    }

    pub fn loser(&self) -> &str {
        &self.loser
    }

    pub fn losses(&self) -> u64 {
        self.losses
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
    /// 旧档没有战斗账本时视为这场战争尚未产生战斗分数。
    #[serde(default)]
    battles: Vec<BattleScore>,
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

    pub fn battles(&self) -> &[BattleScore] {
        &self.battles
    }

    pub(crate) fn record_battle(&mut self, score: BattleScore) {
        self.battles.push(score);
    }
}

/// 双方共享的停战。`until` 当天已经可以再次宣战，停战在该日之前有效。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Truce {
    countries: [String; 2],
    until: GameDate,
}

impl Truce {
    pub(crate) fn new(countries: [String; 2], until: GameDate) -> Self {
        Self { countries, until }
    }

    pub fn countries(&self) -> &[String; 2] {
        &self.countries
    }

    pub fn until(&self) -> GameDate {
        self.until
    }

    pub fn binds(&self, left: &str, right: &str) -> bool {
        let [first, second] = &self.countries;
        (first == left && second == right) || (first == right && second == left)
    }

    pub fn active_on(&self, date: GameDate) -> bool {
        date < self.until
    }
}

/// 进行中的战争。编号只增不减，读档后新战争不会占用旧编号。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct WarState {
    #[serde(default)]
    pub(crate) active: BTreeMap<u32, War>,
    #[serde(default = "first_war_id")]
    pub(crate) next_id: u32,
    /// 旧档没有停战字段时视为没有停战。
    #[serde(default)]
    pub(crate) truces: Vec<Truce>,
}

fn first_war_id() -> u32 {
    1
}

impl Default for WarState {
    fn default() -> Self {
        Self {
            active: BTreeMap::new(),
            next_id: first_war_id(),
            truces: Vec::new(),
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
                battles: Vec::new(),
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
    #[error("停战在 {until} 之前有效，不能宣战")]
    Truce { until: GameDate },
}
