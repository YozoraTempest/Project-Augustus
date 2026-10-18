// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

//! Deterministic integer state and ordered plans, independent of input formats and UI.

#![forbid(unsafe_code)]

mod accruals;
mod administration;
mod advisors;
mod affiliations;
mod army;
mod attributes;
mod calendar;
mod cashflows;
mod completions;
mod control;
mod convergence;
mod daily;
mod economy;
mod estate_actions;
mod estate_initialization;
mod estates;
mod event_choices;
mod event_decisions;
mod event_locations;
mod event_pictures;
mod governments;
mod ideas;
mod initial;
mod justifications;
mod markers;
mod metrics;
mod modifiers;
mod named_values;
mod naval;
mod objectives;
mod ownership;
mod plan;
mod politics;
mod production;
mod province_cores;
mod resources;
mod rng;
mod school_opinions;
mod scopes;
mod stocks;
mod subjects;
mod taxation;
mod technology;
mod trade;
mod truces;
mod unit_selections;
mod unit_templates;
mod world;
mod yearly;

pub use accruals::{
    AccrualAccount, AccrualRule, AccrualSettlement, MonthlyAccruals, MonthlyEconomy,
};
pub use administration::{
    Administration, AdministrationEvaluation, AdministrationRules, AdministrativeCategory,
    AreaStatus, CoreType, GeographicLevel, ProvinceAdministration,
};
pub use advisors::{AdvisorState, Advisors};
pub use affiliations::{
    AffiliationAcceptance, AffiliationDimension, AffiliationRelation, Affiliations,
};
pub use army::{Army, ArmyId, Regiment, RegimentId};
pub use attributes::{
    AffiliationEffect, AttributeAggregate, AttributeBase, AttributeChannel, AttributeContribution,
    AttributeEvaluation, AttributeFormula, AttributeStep, AttributeTerm, Attributes,
    MONTHLY_AUTONOMY_DELTA_ATTRIBUTE,
};
pub use calendar::Calendar;
pub use cashflows::{CashflowAccount, CashflowSettlement, MonthlyExternalCashflows};
pub use completions::Completions;
pub use control::CountryController;
pub use convergence::{
    ConvergenceAccount, ConvergenceRule, ConvergenceSettlement, MonthlyConvergence,
};
pub use daily::{
    DailyEventCandidate, DailyEventCountry, DailyEventGroup, DailyEventGroupMode, DailyEventRules,
    DailySystem,
};
pub use economy::{
    BuildingDefinition, BuildingRole, Economy, ProvinceEconomy, TradeGoodDefinition, TradeNode,
    TradeProductionInput, TradeProductionResult, VanillaDerivedModifierInput,
    VanillaDerivedModifierResult, VanillaModifierAggregationInput,
    VanillaModifierAggregationResult, VanillaProductionBaseValueInput,
    VanillaProductionBaseValueResult, VanillaProductionCountryResult, VanillaProductionInput,
    VanillaProductionInputState, VanillaProductionProvinceResult, VanillaProductionResult,
    VanillaTradeGoodPriceInput, VanillaTradeGoodPriceResult, VanillaTradeGoodsSizeInput,
    VanillaTradeGoodsSizeResult, aggregate_vanilla_modifier_contributions,
    calculate_trade_production, calculate_vanilla_derived_modifier, calculate_vanilla_production,
    calculate_vanilla_production_base_value, calculate_vanilla_production_for_country,
    calculate_vanilla_trade_good_price, calculate_vanilla_trade_goods_size,
};
pub use estate_actions::EstateMetric;
pub use estate_initialization::EstateLandRules;
pub use estates::{
    EstateConditionalModifier, EstateConditionalScale, EstateContribution, EstateDefinition,
    EstateEffect, EstateEvaluation, EstateInfluenceBand, EstateLoyaltyDecay, EstatePrivilege,
    EstateRules, EstateState, Estates,
};
pub use event_choices::{
    EventCatalog, EventChoice, EventDefinition, EventOption, PendingEvent, PendingEventId,
};
pub use event_decisions::{
    EmptyWeightChoice, EventDecisionSystem, EventOptionWeights, EventWeight, WeightModifier,
    WeightedEventDecisions,
};
pub use event_locations::EventLocations;
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
pub use naval::{NavalUnit, NavalUnitId};
pub use objectives::{ObjectiveGroup, Objectives};
pub use plan::{Action, Condition, Plan, SchoolOpinionTarget, Target};
pub use politics::{
    CountryPolitics, DiplomaticKind, DiplomaticRelation, NationalFocus, PersonState, Politics,
    RulerSkill, RulerSkillBounds,
};
pub use production::{
    MonthlyProduction, ProductionCountryAccount, ProductionProvinceAccount, ProductionSettlement,
};
pub use resources::{CountryResource, CountryResources};
pub use rng::Eu4Random;
pub use school_opinions::SchoolOpinions;
pub use scopes::{EntityRef, ScopeContext, ScopeSelector};
pub use stocks::{MonthlyStocks, StockAccount, StockRule, StockSettlement};
pub use subjects::Subjects;
pub use taxation::{MonthlySystem, MonthlyTax, MonthlyTaxAccount, TaxSettlement, Taxation};
pub use technology::{Technology, TechnologyLevel, TechnologyProperty};
pub use trade::{
    VanillaTradeCandidate, VanillaTradeDistributionCandidate, VanillaTradeDistributionInput,
    VanillaTradeDistributionResult, VanillaTradeDistributionRouteInput,
    VanillaTradeDistributionRouteResult, VanillaTradePropagationInput,
    VanillaTradePropagationResult, VanillaTradeRetentionInput, VanillaTradeRetentionResult,
    VanillaTradeRouteWeightInput, VanillaTradeRouteWeightResult,
    calculate_vanilla_trade_distribution, calculate_vanilla_trade_propagation,
    calculate_vanilla_trade_retention, calculate_vanilla_trade_route_weights,
    sort_vanilla_trade_candidates,
};
pub use truces::{Truce, Truces};
pub use unit_templates::{UnitTemplate, UnitTemplateProperty, UnitTemplates};
pub use world::{Entity, EntityId, EntityKind, FieldId, RuleId, Schema, World};
pub use yearly::{YearlyEventRules, YearlySystem};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid simulation state: {0}")]
    Invalid(String),
    #[error("integer overflow")]
    Overflow,
    #[error("save format: {0}")]
    Save(#[from] serde_json::Error),
}
