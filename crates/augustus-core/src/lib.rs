// SPDX-License-Identifier: GPL-3.0-only

//! 与内容格式和显示引擎无关的国家、省份、归属和游戏时间状态。

mod building;
mod country;
mod economy;
mod province;
mod simulation;
mod time;
mod world;

pub use building::{BuildingDefinition, Construction, ConstructionError, ConstructionQuote};
pub use country::Country;
pub use economy::{CountryFinances, MonthlyTaxSettlement};
pub use province::{Development, Province, ProvinceKind};
pub use rust_decimal::Decimal as Money;
pub use simulation::Simulation;
pub use time::{GameClock, GameDate, GameSpeed};
pub use world::{World, WorldError};
