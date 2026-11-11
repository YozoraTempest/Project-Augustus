// SPDX-License-Identifier: GPL-3.0-only

//! 与内容格式和显示引擎无关的国家、省份、归属和游戏时间状态。

mod building;
mod combat;
mod country;
mod economy;
mod manpower;
mod military;
mod movement;
mod organization;
mod peace;
mod province;
mod save;
mod simulation;
mod time;
mod war;
mod world;

pub use building::{BuildingDefinition, Construction, ConstructionError, ConstructionQuote};
pub(crate) use combat::ReportedSide;
pub use combat::{
    Battle, BattleOutcome, BattleReport, CombatRules, CountryMilitary, UNSET_MORALE, UnitCombat,
    UnitPips,
};
pub use country::Country;
pub use economy::{CountryFinances, MonthlyBudget, MonthlySettlement};
pub use manpower::{ManpowerPool, ManpowerRules, ManpowerSummary};
pub use military::{
    Army, MilitaryRules, MilitaryState, REGIMENT_STRENGTH, Recruitment, RecruitmentError,
    RecruitmentQuote, RecruitmentRefund, Regiment, UnitKind,
};
pub use movement::{ArmyMovement, LandConnection, MovementError, MovementLeg};
pub use organization::{ArmyDisbandQuote, ArmyOrganizationError};
pub use peace::{
    PeaceError, PeaceQuote, PeaceSheet, ProvinceCost, WarScoreView, format_warscore_milli,
};
pub use province::{
    Development, OCCUPATION_PHASE_DAYS, OccupationProgress, Province, ProvinceKind, SiegeProgress,
    SiegeRoll,
};
pub use rust_decimal::Decimal as Money;
pub use save::{RestoredGame, SaveError, SavedGame};
pub use simulation::{DayAdvance, Simulation};
pub use time::{GameClock, GameDate, GameSpeed};
pub(crate) use war::WarState;
pub use war::{BattleScore, Truce, War, WarError, WarStatus};
pub use world::{
    ArmyChoice, PeaceChoice, RecruitChoice, SupplyForecast, WarAiDecision, World, WorldError,
};
