// SPDX-License-Identifier: GPL-3.0-only

use serde::{Deserialize, Serialize};

use crate::GameDate;

/// 无向陆路及基础行军天数，由内容层根据地图与兵种规则生成。
#[derive(Debug, Clone, Copy)]
pub struct LandConnection {
    pub from: u32,
    pub to: u32,
    pub days: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArmyMovement {
    pub from: u32,
    pub to: u32,
    pub started_on: GameDate,
    pub arrives_on: GameDate,
}

impl ArmyMovement {
    pub fn days_remaining(&self, date: GameDate) -> u32 {
        date.days_until(self.arrives_on).max(0) as u32
    }

    pub fn progress(&self, date: GameDate) -> f64 {
        let total = self.started_on.days_until(self.arrives_on);
        f64::from(self.started_on.days_until(date)).clamp(0.0, f64::from(total)) / f64::from(total)
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
    #[error("该军队正在行军，请等待抵达")]
    AlreadyMoving,
    #[error("军队已在该省份")]
    SameProvince,
    #[error("只能移动到地图上的陆地省份")]
    InvalidDestination,
    #[error("目标省份必须由本国拥有并控制")]
    NotOwnedAndControlled,
    #[error("目标省份必须与当前省份陆地相邻")]
    NotAdjacent,
    #[error("抵达日期超出游戏历法范围")]
    DateOutOfRange,
}
