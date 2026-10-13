// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

//! Deterministic integer state and ordered plans, independent of input formats and UI.

#![forbid(unsafe_code)]

mod accruals;
mod administration;
mod affiliations;
mod attributes;
mod calendar;
mod completions;
mod control;
mod economy;
mod estate_actions;
mod estate_initialization;
mod estates;
mod event_choices;
mod event_decisions;
mod event_pictures;
mod governments;
mod ideas;
mod initial;
mod justifications;
mod markers;
mod metrics;
mod modifiers;
mod named_values;
mod objectives;
mod ownership;
mod plan;
mod politics;
mod resources;
mod scopes;
mod subjects;
mod taxation;
mod technology;
mod world;

pub use accruals::{
    AccrualAccount, AccrualRule, AccrualSettlement, MonthlyAccruals, MonthlyEconomy,
};
pub use administration::{
    Administration, AdministrationEvaluation, AdministrationRules, AdministrativeCategory,
    AreaStatus, CoreType, GeographicLevel, ProvinceAdministration,
};
pub use affiliations::{
    AffiliationAcceptance, AffiliationDimension, AffiliationRelation, Affiliations,
};
pub use attributes::{
    AffiliationEffect, AttributeAggregate, AttributeBase, AttributeChannel, AttributeContribution,
    AttributeEvaluation, AttributeFormula, AttributeStep, AttributeTerm, Attributes,
};
pub use calendar::Calendar;
pub use completions::Completions;
pub use control::CountryController;
pub use economy::{BuildingDefinition, Economy, ProvinceEconomy, TradeGoodDefinition, TradeNode};
pub use estate_actions::EstateMetric;
pub use estate_initialization::EstateLandRules;
pub use estates::{
    EstateContribution, EstateDefinition, EstateEffect, EstateEvaluation, EstateInfluenceBand,
    EstatePrivilege, EstateRules, EstateState, Estates,
};
pub use event_choices::{
    EventCatalog, EventChoice, EventDefinition, EventOption, PendingEvent, PendingEventId,
};
pub use event_decisions::{
    EmptyWeightChoice, EventDecisionSystem, EventOptionWeights, EventWeight, WeightModifier,
    WeightedEventDecisions,
};
pub use event_pictures::{EventPictures, PictureVariant};
pub use governments::{
    GovernmentContribution, GovernmentEvaluation, GovernmentForm, GovernmentMetric,
    GovernmentReform, GovernmentState, Governments,
};
pub use ideas::{
    IdeaContribution, IdeaDefinition, IdeaEffect, IdeaEvaluation, IdeaGroup, IdeaState, Ideas,
};
pub use initial::{CountryState, InitialWorld, ProvinceClass, ProvinceState, WorldDate};
pub use justifications::{GrantTerm, Justification, JustificationDefinition, Justifications};
pub use markers::{GlobalMarkers, Markers};
pub use metrics::{MetricContribution, MetricDefinition, MetricEffect, MetricEvaluation, Metrics};
pub use modifiers::{ModifierDefinition, ModifierInstance, ModifierRenewal, Modifiers};
pub use named_values::{NamedValues, NumericComparison, NumericOperation, NumericSlot};
pub use objectives::{ObjectiveGroup, Objectives};
pub use plan::{Action, Condition, Plan, Target};
pub use politics::{
    CountryPolitics, DiplomaticKind, DiplomaticRelation, NationalFocus, PersonState, Politics,
    RulerSkill, RulerSkillBounds,
};
pub use resources::CountryResources;
pub use scopes::{EntityRef, ScopeContext, ScopeSelector};
pub use subjects::Subjects;
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
