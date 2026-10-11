// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

//! Deterministic integer state and ordered plans, independent of input formats and UI.

#![forbid(unsafe_code)]

mod administration;
mod affiliations;
mod attributes;
mod calendar;
mod economy;
mod estates;
mod governments;
mod ideas;
mod initial;
mod modifiers;
mod plan;
mod politics;
mod resources;
mod taxation;
mod technology;
mod world;

pub use administration::{
    Administration, AdministrationEvaluation, AdministrationRules, AdministrativeCategory,
    AreaStatus, CoreType, ProvinceAdministration,
};
pub use affiliations::{
    AffiliationAcceptance, AffiliationDimension, AffiliationRelation, Affiliations,
};
pub use attributes::{
    AffiliationEffect, AttributeAggregate, AttributeBase, AttributeChannel, AttributeContribution,
    AttributeEvaluation, AttributeFormula, AttributeStep, AttributeTerm, Attributes,
};
pub use calendar::Calendar;
pub use economy::{BuildingDefinition, Economy, ProvinceEconomy, TradeGoodDefinition, TradeNode};
pub use estates::{
    EstateContribution, EstateDefinition, EstateEffect, EstateEvaluation, EstateInfluenceBand,
    EstatePrivilege, EstateRules, EstateState, Estates,
};
pub use governments::{
    GovernmentContribution, GovernmentEvaluation, GovernmentMetric, GovernmentReform,
    GovernmentState, Governments,
};
pub use ideas::{
    IdeaContribution, IdeaDefinition, IdeaEffect, IdeaEvaluation, IdeaGroup, IdeaState, Ideas,
};
pub use initial::{CountryState, InitialWorld, ProvinceClass, ProvinceState, WorldDate};
pub use modifiers::{ModifierDefinition, ModifierInstance, Modifiers};
pub use plan::{Action, Condition, Plan, Target};
pub use politics::{CountryPolitics, DiplomaticKind, DiplomaticRelation, PersonState, Politics};
pub use resources::CountryResources;
pub use taxation::{MonthlySystem, MonthlyTax, MonthlyTaxAccount, TaxSettlement, Taxation};
pub use technology::{Technology, TechnologyLevel};
pub use world::{Entity, EntityId, EntityKind, FieldId, RuleId, Schema, World};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid simulation state: {0}")]
    Invalid(String),
    #[error("integer overflow")]
    Overflow,
    #[error("save format: {0}")]
    Save(#[from] serde_json::Error),
}
