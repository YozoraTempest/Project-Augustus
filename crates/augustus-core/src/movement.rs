// SPDX-License-Identifier: GPL-3.0-only

use serde::{Deserialize, Serialize};

use crate::GameDate;

/// 无向陆路距离。行军天数按具体军队的最慢兵种速度计算。
#[derive(Debug, Clone, Copy)]
pub struct LandConnection {
    pub from: u32,
    pub to: u32,
    pub distance: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MovementLeg {
    pub from: u32,
    pub to: u32,
    pub started_on: GameDate,
    pub arrives_on: GameDate,
}

impl MovementLeg {
    pub fn progress(&self, date: GameDate) -> f64 {
        let total = self.started_on.days_until(self.arrives_on);
        f64::from(self.started_on.days_until(date)).clamp(0.0, f64::from(total)) / f64::from(total)
    }
}

/// 保留整条路线和当前路段；已走过的路段仍用于显示完整行程。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArmyMovement {
    pub(crate) legs: Vec<MovementLeg>,
    pub(crate) leg_index: usize,
}

impl ArmyMovement {
    pub fn legs(&self) -> &[MovementLeg] {
        &self.legs
    }

    pub fn leg_index(&self) -> usize {
        self.leg_index
    }

    pub fn current_leg(&self) -> &MovementLeg {
        &self.legs[self.leg_index]
    }

    pub fn destination(&self) -> u32 {
        self.legs.last().expect("行军路线非空").to
    }

    pub fn started_on(&self) -> GameDate {
        self.legs[0].started_on
    }

    pub fn arrives_on(&self) -> GameDate {
        self.legs.last().expect("行军路线非空").arrives_on
    }

    pub fn days_remaining(&self, date: GameDate) -> u32 {
        date.days_until(self.arrives_on()).max(0) as u32
    }

    pub fn progress(&self, date: GameDate) -> f64 {
        let total = self.started_on().days_until(self.arrives_on());
        f64::from(self.started_on().days_until(date)).clamp(0.0, f64::from(total))
            / f64::from(total)
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum MovementError {
    #[error("请选择有效的国家")]
    UnknownCountry,
    #[error("请选择有效的军队")]
    UnknownArmy,
    #[error("只能指挥本国军队")]
    NotPlayerArmy,
    #[error("该军队未在行军")]
    NotMoving,
    #[error("军队已在该省份")]
    SameProvince,
    #[error("只能移动到地图上的陆地省份")]
    InvalidDestination,
    #[error("只能进入本国控制的省份或交战国领土")]
    NoAccess,
    #[error("没有通往目标省份的可通行陆路")]
    NoRoute,
    #[error("抵达日期超出游戏历法范围")]
    DateOutOfRange,
}
