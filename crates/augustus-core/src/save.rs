// SPDX-License-Identifier: GPL-3.0-only

use std::{collections::BTreeMap, time::Duration};

use serde::{Deserialize, Serialize};

use crate::{
    Country, GameClock, GameDate, GameSpeed, MilitaryState, Province, Simulation, World, WorldError,
};

/// 只保存可变模拟状态；地图、建筑规则和本地化仍从游戏内容载入。
#[derive(Debug, Serialize, Deserialize)]
pub struct SavedGame {
    player_country: String,
    clock: SavedClock,
    countries: Vec<Country>,
    provinces: BTreeMap<u32, Province>,
    military: MilitaryState,
}

#[derive(Debug, Serialize, Deserialize)]
struct SavedClock {
    date: GameDate,
    speed: u32,
    // 一秒模拟进度等于一天，保留纳秒部分可避免读档丢失不足一天的进度。
    progress_nanos: u32,
}

pub struct RestoredGame {
    pub player_country: String,
    pub simulation: Simulation,
    pub world: World,
}

#[derive(Debug, thiserror::Error)]
pub enum SaveError {
    #[error("存档与当前游戏内容的国家或省份定义不一致")]
    IncompatibleContent,
    #[error("存档中的玩家国家不存在")]
    UnknownPlayer,
    #[error("存档中的游戏速度或日内进度无效")]
    InvalidClock,
    #[error("省份 {0} 的建设项目无效")]
    InvalidConstruction(u32),
    #[error("存档中的军队、行军、招募项目或军队编号无效")]
    InvalidMilitary,
    #[error("存档中的世界状态无效：{0}")]
    InvalidWorld(#[from] WorldError),
}

impl SavedGame {
    pub fn capture(world: &World, simulation: &Simulation, player_country: &str) -> Self {
        let clock = simulation.clock();
        Self {
            player_country: player_country.to_owned(),
            clock: SavedClock {
                date: clock.date(),
                speed: clock.speed().multiplier(),
                progress_nanos: clock.progress().subsec_nanos(),
            },
            countries: world.countries().values().cloned().collect(),
            provinces: world.provinces().clone(),
            military: world.military().clone(),
        }
    }

    /// 先构建完整结果，调用者只在全部成功后替换当前会话。
    pub fn restore(self, content: &World) -> Result<RestoredGame, SaveError> {
        let speed = GameSpeed::from_multiplier(self.clock.speed)
            .filter(|_| self.clock.progress_nanos < 1_000_000_000)
            .ok_or(SaveError::InvalidClock)?;
        let mut world = content.restore_state(self.countries, self.provinces, self.clock.date)?;
        world.restore_military(self.military, self.clock.date)?;
        if world.country(&self.player_country).is_none() {
            return Err(SaveError::UnknownPlayer);
        }
        Ok(RestoredGame {
            player_country: self.player_country,
            simulation: Simulation::from_clock(GameClock::restore(
                self.clock.date,
                speed,
                Duration::from_nanos(u64::from(self.clock.progress_nanos)),
            )),
            world,
        })
    }
}
