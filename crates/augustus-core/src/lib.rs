// SPDX-License-Identifier: GPL-3.0-only

//! 与内容格式和显示引擎无关的国家、省份、归属和游戏时间状态。

mod country;
mod province;
mod time;
mod world;

pub use country::Country;
pub use province::{Development, Province, ProvinceKind};
pub use time::{GameClock, GameDate, GameSpeed};
pub use world::{World, WorldError};
