// SPDX-License-Identifier: GPL-3.0-only

//! 与内容格式和显示引擎无关的国家、省份及归属状态。

mod country;
mod province;
mod world;

pub use country::Country;
pub use province::{Development, Province, ProvinceKind};
pub use world::{World, WorldError};
