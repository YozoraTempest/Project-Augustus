// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

//! Read-only EU4 content access. EU4-specific syntax and metadata stay in this crate.

#![forbid(unsafe_code)]

mod administration;
mod administration_validation;
mod annual_ledger_rollover_queries;
mod attributes;
mod autonomy;
mod bootstrap;
mod casus_belli;
mod content;
mod country_conditions;
mod country_markers;
mod country_metrics;
mod country_runtime_queries;
mod country_status;
mod country_tags;
mod defines;
pub use defines::{DefineDifficulty, DefineValue, Defines, EvaluatedDefine};
mod definitions;
mod descriptor;
mod diagnostic;
mod economy;
mod estate_loyalty_queries;
mod estates;
mod eu4_rng;
mod event_ai;
mod force_limit;
mod fort_maintenance_queries;
mod geography;
mod gold_stage_queries;
mod governments;
mod ideas;
mod imperial_membership;
mod land_force_limit_queries;
mod liberty_desire;
mod load;
mod localisation;
mod missions;
mod monarch_power;
mod monthly_accrual_queries;
mod monthly_autonomy_queries;
mod monthly_cashflow_queries;
mod monthly_convergence_queries;
mod monthly_event_queries;
mod monthly_ledger_queries;
mod monthly_production_queries;
mod monthly_settlement_queries;
mod monthly_tax_queries;
mod native_development;
mod native_migration_defines;
mod native_migration_execution;
mod native_migration_queries;
mod native_migration_state;
mod native_migration_state_queries;
mod native_migration_transfer;
mod pending_event_queries;
pub use native_migration_execution::{
    VanillaNativeMigrationExecutionReport, VanillaNativeMigrationExecutionRequest,
    VanillaNativeMigrationExecutionStatus,
};
pub use native_migration_state_queries::{
    VanillaNativeMigrationContext, VanillaNativeMigrationStatePricing,
    VanillaNativeMigrationStateQuery, VanillaNativeMigrationStateQueryReport,
};
pub use native_migration_transfer::{
    VanillaNativeMigrationTransferReport, VanillaNativeMigrationTransferRequest,
    VanillaNativeMigrationTransferStatus,
};
mod naval_construction_cost_defines;
mod naval_construction_cost_queries;
mod naval_construction_requests;
mod naval_construction_time_defines;
mod naval_construction_time_queries;
mod naval_constructions;
mod naval_force_limit_queries;
mod navy_tradition;
mod power_income;
mod prestige;
mod production_queries;
mod production_value_queries;
mod truces;
mod unit_templates;
mod yearly_event_queries;
pub use annual_ledger_rollover_queries::{
    VanillaAnnualLedgerRolloverQuery, VanillaAnnualLedgerRolloverQueryReport,
};
pub use estate_loyalty_queries::{
    VanillaEstateLoyaltyEntry, VanillaEstateLoyaltyEvaluation, VanillaEstateLoyaltyQuery,
    VanillaEstateLoyaltyQueryReport,
};
pub use fort_maintenance_queries::{
    VanillaFortMaintenanceBuildingsQuery, VanillaFortMaintenanceDirectQuery,
    VanillaFortMaintenanceQuery, VanillaFortMaintenanceQueryMode,
    VanillaFortMaintenanceQueryReport,
};
pub use gold_stage_queries::{
    VanillaGoldBaseQuery, VanillaGoldProductionQuery, VanillaGoldStageEvaluation,
    VanillaGoldStageQuery, VanillaGoldStageQueryMode, VanillaGoldStageQueryReport,
};
pub use missions::MissionReport;
pub use monthly_accrual_queries::{VanillaMonthlyAccrualQuery, VanillaMonthlyAccrualQueryReport};
pub use monthly_autonomy_queries::{
    VanillaMonthlyAutonomyQuery, VanillaMonthlyAutonomyQueryReport,
};
pub use monthly_cashflow_queries::{
    VanillaMonthlyCashflowQuery, VanillaMonthlyCashflowQueryReport,
};
pub use monthly_convergence_queries::{
    VanillaMonthlyConvergenceQuery, VanillaMonthlyConvergenceQueryReport,
};
pub use monthly_event_queries::{VanillaMonthlyEventQuery, VanillaMonthlyEventQueryReport};
pub use monthly_ledger_queries::{VanillaMonthlyLedgerQuery, VanillaMonthlyLedgerQueryReport};
pub use monthly_production_queries::{
    VanillaMonthlyProductionQuery, VanillaMonthlyProductionQueryReport,
};
pub use monthly_settlement_queries::{
    VanillaMonthlySettlementQuery, VanillaMonthlySettlementQueryReport,
};
pub use monthly_tax_queries::{VanillaMonthlyTaxQuery, VanillaMonthlyTaxQueryReport};
pub use naval_construction_cost_queries::{
    VanillaNavalConstructionCostQuery, VanillaNavalConstructionCostQueryReport,
};
pub use naval_construction_requests::VanillaNavalConstructionRequest;
pub use naval_construction_time_queries::{
    VanillaNavalConstructionTimeQuery, VanillaNavalConstructionTimeQueryReport,
};
pub use naval_constructions::{
    VanillaNavalConstructionImportReport, VanillaNavalConstructionOrderView,
    VanillaNavalConstructionRecord, VanillaNavalConstructionReport,
    VanillaNavalConstructionUnresolved, read_vanilla_naval_constructions,
    read_vanilla_naval_constructions_for_queue_import,
    read_vanilla_naval_constructions_for_queue_import_with_token_map,
    read_vanilla_naval_constructions_for_queue_import_with_token_resolver,
    read_vanilla_naval_constructions_with_token_map,
    read_vanilla_naval_constructions_with_token_resolver,
};
pub use naval_force_limit_queries::{
    VanillaNavalForceLimitQuery, VanillaNavalForceLimitQueryReport,
};
pub use pending_event_queries::{
    VanillaPendingEventQuery, VanillaPendingEventQueryReport, VanillaPendingEventReport,
};
pub use production_queries::{
    VanillaGoldProductionCountryQuery, VanillaGoldProductionCountryQueryReport,
    VanillaProductionCountryQuery, VanillaProductionCountryQueryReport,
};
pub use production_value_queries::{
    VanillaProvinceContentProductionGoodsSizeQuery,
    VanillaProvinceContentProductionGoodsSizeQueryReport, VanillaProvinceProductionBaseValueQuery,
    VanillaProvinceProductionBaseValueQueryReport,
};
pub use yearly_event_queries::{VanillaYearlyEventQuery, VanillaYearlyEventQueryReport};
mod politics;
mod province_map;
mod province_metrics;
mod religious_schools;
mod save_binary;
mod save_state;
mod script;
mod setup;
mod state_edicts;
mod state_maintenance_queries;
mod subject_modifiers;
mod terrain_map;
mod text;
mod trade_price_queries;

pub use administration::AdministrationReport;
pub use administration_validation::{
    AdministrationCheckCount, AdministrationComparison, AdministrationDifference,
    SavedImperialBaselineReport, SavedImperialMembershipReport, SavedOfficePreferenceReport,
    VanillaSaveAdministration, apply_saved_hre_membership, apply_saved_imperial_baseline,
    apply_saved_office_preferences, compare_administration_save,
};
pub use attributes::AttributeReport;
pub use augustus_core::{
    COUNTRY_LEDGER_CATEGORY_COUNT, COUNTRY_LEDGER_EXPENSE_CATEGORY_COUNT,
    COUNTRY_LEDGER_EXPENSE_FORT_MAINTENANCE_CATEGORY, COUNTRY_LEDGER_EXPENSE_KEYS,
    COUNTRY_LEDGER_EXPENSE_STATE_MAINTENANCE_CATEGORY, COUNTRY_LEDGER_INCOME_GOLD_CATEGORY,
    COUNTRY_LEDGER_INCOME_KEYS, COUNTRY_LEDGER_INCOME_PRODUCTION_CATEGORY,
    COUNTRY_LEDGER_INCOME_TAX_CATEGORY, COUNTRY_LEDGER_INCOME_TRADE_CATEGORY,
    COUNTRY_LEDGER_NATIVE_FILTERED_EXPENSE_CATEGORIES,
    COUNTRY_LEDGER_NATIVE_FILTERED_INCOME_CATEGORIES, COUNTRY_LEDGER_SEND_OFFICERS_CATEGORY,
    COUNTRY_LEDGER_SUPPORT_LOYALISTS_CATEGORY, CashflowBreakdown, CountryLedger,
    CountryLedgerCategory, CountryValueIndexEntry, CountryValueTable, OwnerAreaAdjacencyCounts,
    OwnerAreaAdjacencyEntry, VanillaCompanyCompositionDefinition, VanillaCompanyCompositionInput,
    VanillaCompanyCompositionResult, VanillaConstructionMoraleInput,
    VanillaConstructionMoraleResult, VanillaDerivedModifierInput, VanillaDerivedModifierResult,
    VanillaEstateInfluenceScaleInput, VanillaEstateInfluenceScaleResult, VanillaForceLimitInput,
    VanillaForceLimitResult, VanillaFortMaintenanceInput, VanillaFortMaintenanceResult,
    VanillaGoldBaseInput, VanillaGoldBaseResult, VanillaGoldProductionCountryResult,
    VanillaGoldProductionInput, VanillaGoldProductionInputState,
    VanillaGoldProductionProvinceResult, VanillaGoldProductionResult,
    VanillaModifierAggregationInput, VanillaModifierAggregationResult,
    VanillaNavalConstructionTimeInput, VanillaNavalConstructionTimeResult,
    VanillaProductionCountryResult, VanillaProductionInputState, VanillaProductionProvinceResult,
    VanillaStateMaintenanceDistanceInput, VanillaStateMaintenanceInput,
    VanillaStateMaintenanceProvinceInputState, VanillaStateMaintenanceResult,
    VanillaTradeAttachedModifierGroup, VanillaTradeCandidate, VanillaTradeCaravanInput,
    VanillaTradeCaravanResult, VanillaTradeCountryFlags, VanillaTradeCountryPowerShareInput,
    VanillaTradeDistributionCandidate, VanillaTradeDistributionCandidateInput,
    VanillaTradeDistributionInput, VanillaTradeDistributionResult,
    VanillaTradeDistributionRouteInput, VanillaTradeDistributionRouteResult,
    VanillaTradeGoodPriceInput, VanillaTradeGoodPriceResult, VanillaTradeGoodPriceTableInput,
    VanillaTradeGoodPriceTableResult, VanillaTradeGoodsSizeInput, VanillaTradeGoodsSizeResult,
    VanillaTradeIncomeContextInput, VanillaTradeIncomeInput, VanillaTradeIncomePassInput,
    VanillaTradeIncomePassResult, VanillaTradeIncomeResult, VanillaTradeNodeCountryValueShareInput,
    VanillaTradeNodeCountryValueShareResult, VanillaTradeNodeIncomePassInput,
    VanillaTradeNodeIncomePassResult, VanillaTradeNodeIncomeRowInput,
    VanillaTradeNodeIncomeRowResult, VanillaTradeNodePassFromRetentionSourcesInput,
    VanillaTradeNodePassFromRetentionSourcesResult, VanillaTradeNodePassInput,
    VanillaTradeNodePassResult, VanillaTradeNodeRouteInput, VanillaTradeOutflowInput,
    VanillaTradeOutflowResult, VanillaTradePowerAmountInput, VanillaTradePowerAmountResult,
    VanillaTradePowerModifierInput, VanillaTradePowerModifierResult, VanillaTradePropagationInput,
    VanillaTradePropagationResult, VanillaTradeRetentionContributionRowInput,
    VanillaTradeRetentionInput, VanillaTradeRetentionParticipantInput,
    VanillaTradeRetentionPassInput, VanillaTradeRetentionPassResult,
    VanillaTradeRetentionPoolsInput, VanillaTradeRetentionResult,
    VanillaTradeRetentionSequenceEntryInput, VanillaTradeRetentionSourcesInput,
    VanillaTradeRetentionSourcesResult, VanillaTradeRouteWeightContributionInput,
    VanillaTradeRouteWeightInput, VanillaTradeRouteWeightPassFromRowsInput,
    VanillaTradeRouteWeightPassInput, VanillaTradeRouteWeightPassResult,
    VanillaTradeRouteWeightResult, VanillaTradeRouteWeightSourceRowInput,
    VanillaTradeSteeringEligibilityInput, VanillaTradeSteeringModifierInput,
    VanillaTradeSteeringModifierResult, VanillaUnitProductionQuantityInput,
    VanillaUnitProductionQuantityResult, aggregate_vanilla_modifier_contributions,
    calculate_vanilla_company_composition, calculate_vanilla_construction_morale,
    calculate_vanilla_derived_modifier, calculate_vanilla_estate_influence_scale,
    calculate_vanilla_force_limit, calculate_vanilla_fort_maintenance, calculate_vanilla_gold_base,
    calculate_vanilla_gold_production, calculate_vanilla_gold_production_for_country,
    calculate_vanilla_naval_construction_time, calculate_vanilla_production_for_country,
    calculate_vanilla_state_maintenance, calculate_vanilla_state_maintenance_for_country,
    calculate_vanilla_trade_caravan, calculate_vanilla_trade_country_power_share,
    calculate_vanilla_trade_distribution, calculate_vanilla_trade_good_price,
    calculate_vanilla_trade_good_price_table, calculate_vanilla_trade_goods_size,
    calculate_vanilla_trade_income, calculate_vanilla_trade_income_pass,
    calculate_vanilla_trade_node_country_value_share, calculate_vanilla_trade_node_income_pass,
    calculate_vanilla_trade_node_pass, calculate_vanilla_trade_node_pass_from_retention_sources,
    calculate_vanilla_trade_outflow, calculate_vanilla_trade_power_amount,
    calculate_vanilla_trade_power_modifier, calculate_vanilla_trade_propagation,
    calculate_vanilla_trade_retention, calculate_vanilla_trade_retention_from_pools,
    calculate_vanilla_trade_retention_from_sources, calculate_vanilla_trade_retention_pass,
    calculate_vanilla_trade_route_weight_pass, calculate_vanilla_trade_route_weight_pass_from_rows,
    calculate_vanilla_trade_route_weights, calculate_vanilla_trade_steering_modifier,
    calculate_vanilla_unit_production_quantity, country_ledger_expense_key,
    country_ledger_income_key, is_vanilla_trade_collector, is_vanilla_trade_steering_eligible,
    select_vanilla_trade_distribution_candidates, sort_vanilla_trade_candidates,
};
pub use augustus_core::{
    VanillaNativeMigrationGateInput, VanillaNativeMigrationGateResult,
    calculate_vanilla_native_migration_gate,
};
pub use bootstrap::{
    BootstrapReport, CountryView, LoadedSource, MapEntry, ProvinceView,
    VanillaContentProductionGoodsSizeInput, VanillaContentProductionInput,
    VanillaContentProductionValueInput, VanillaContentStateMaintenanceInput,
    VanillaExternalAdvisor, VanillaExternalArmy, VanillaExternalCountryMarkers,
    VanillaExternalCountryPersons, VanillaExternalCountryStatuses, VanillaExternalEstateState,
    VanillaExternalEstateStates, VanillaExternalEu4RandomState, VanillaExternalFleet,
    VanillaExternalNavalUnit, VanillaExternalPerson, VanillaExternalProductionInput,
    VanillaExternalProvinceDevelopment, VanillaExternalRegimentState, VanillaExternalShipState,
    VanillaExternalState, VanillaFortMaintenanceContextInput,
    VanillaProvinceProductionBaseValueInput, VanillaProvinceStateMaintenanceInput, VanillaWorld,
};
pub use casus_belli::{CasusBelliReport, CasusBelliType, CasusBelliTypes};
pub use content::{ContentRoot, ContentView, FileEntry};
pub use country_runtime_queries::{VanillaCountryRuntimeQuery, VanillaCountryRuntimeQueryReport};
pub use country_tags::{CountryTagRegistry, CountryTagSlot, CountryTagSlotKind};
pub use definitions::{DefinitionIndex, DefinitionKind, DefinitionOrigin};
pub use descriptor::ModDescriptor;
pub use diagnostic::{Error, SourceLocation};
pub use economy::EconomyReport;
pub use estates::EstateReport;
pub use eu4_rng::Eu4Random;
pub use event_ai::{evaluate_ai_event_weight, select_ai_event_option};
pub use governments::GovernmentReport;
pub use ideas::IdeaReport;
pub use jomini::binary::TokenResolver as Eu4TokenResolver;
pub use land_force_limit_queries::{VanillaLandForceLimitQuery, VanillaLandForceLimitQueryReport};
pub use load::{ContentSet, ContentSource, FileCandidate, FileOrigin, FileTrace, Suppression};
pub use localisation::EventText;
pub use native_migration_queries::{
    VanillaNativeMigrationContentPricing, VanillaNativeMigrationContentQuery,
    VanillaNativeMigrationPricing, VanillaNativeMigrationQuery, VanillaNativeMigrationQueryReport,
};
pub use native_migration_state::{
    VanillaExternalNativeMigrationState, VanillaNativeMigrationImport,
};
pub use politics::{RelationSource, RelationView};
pub use religious_schools::ReligiousSchoolReport;
pub use save_state::{
    VanillaSaveState, VanillaSaveStateWithOrderedTradeNodeRuntimeStates,
    VanillaSaveStateWithStateEdicts, VanillaSaveStateWithStateEdictsAndAdministration,
    VanillaSaveStateWithTradeNodeRuntimeStates, VanillaSaveStateWithTradeNodes,
    read_vanilla_native_migration_states, read_vanilla_native_migration_states_with_token_map,
    read_vanilla_native_migration_states_with_token_resolver, read_vanilla_save_state,
    read_vanilla_save_state_and_ordered_trade_node_runtime_states,
    read_vanilla_save_state_and_ordered_trade_node_runtime_states_with_token_map,
    read_vanilla_save_state_and_ordered_trade_node_runtime_states_with_token_resolver,
    read_vanilla_save_state_and_trade_node_runtime_states,
    read_vanilla_save_state_and_trade_node_runtime_states_with_token_map,
    read_vanilla_save_state_and_trade_node_runtime_states_with_token_resolver,
    read_vanilla_save_state_and_trade_nodes,
    read_vanilla_save_state_and_trade_nodes_with_token_map,
    read_vanilla_save_state_and_trade_nodes_with_token_resolver,
    read_vanilla_save_state_with_state_edicts,
    read_vanilla_save_state_with_state_edicts_and_administration,
    read_vanilla_save_state_with_state_edicts_with_token_map,
    read_vanilla_save_state_with_state_edicts_with_token_resolver,
    read_vanilla_save_state_with_token_map, read_vanilla_save_state_with_token_resolver,
};
pub use script::{CompiledEvents, CompiledScript, ScriptCompiler, ScriptLibrary};
pub use setup::SetupReport;
pub use state_edicts::{VanillaExternalStateEdict, VanillaExternalStateEdicts};
pub use state_maintenance_queries::{
    VanillaStateMaintenanceContentQuery, VanillaStateMaintenanceDirectQuery,
    VanillaStateMaintenanceQuery, VanillaStateMaintenanceQueryMode,
    VanillaStateMaintenanceQueryReport,
};
pub use text::{FieldSummary, ParsedText, TextDocument, TextEncoding};
pub use trade_price_queries::{
    VanillaTradeGoodPriceTableQuery, VanillaTradeGoodPriceTableQueryReport,
};

pub(crate) const EXPANSIONS: [&str; 22] = [
    "Conquest of Paradise",
    "Wealth of Nations",
    "Res Publica",
    "Art of War",
    "El Dorado",
    "Common Sense",
    "The Cossacks",
    "Mare Nostrum",
    "Rights of Man",
    "Mandate of Heaven",
    "Third Rome",
    "Cradle of Civilization",
    "Rule Britannia",
    "Dharma",
    "Golden Century",
    "Emperor",
    "Leviathan",
    "Origins",
    "Lions of the North",
    "Domination",
    "King of Kings",
    "Winds of Change",
];

/// Recognized DLC queries under the fixed final-edition, all-DLC ruleset.
/// Mechanism support remains separate from content availability.
pub(crate) fn is_known_dlc(name: &str) -> bool {
    EXPANSIONS.contains(&name)
        || [
            "American Dream",
            "Purple Phoenix",
            "Star and Crescent",
            "Women in History",
            "Dharma - Hindu Unit Pack",
            "Dharma - Indian Sultanate Pack",
        ]
        .contains(&name)
}
