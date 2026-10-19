// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

//! Read-only EU4 content access. EU4-specific syntax and metadata stay in this crate.

#![forbid(unsafe_code)]

mod administration;
mod administration_validation;
mod attributes;
mod autonomy;
mod bootstrap;
mod casus_belli;
mod content;
mod country_conditions;
mod country_markers;
mod country_metrics;
mod country_status;
mod defines;
pub use defines::{DefineDifficulty, DefineValue, Defines, EvaluatedDefine};
mod definitions;
mod descriptor;
mod diagnostic;
mod economy;
mod estates;
mod eu4_rng;
mod event_ai;
mod geography;
mod governments;
mod ideas;
mod imperial_membership;
mod liberty_desire;
mod load;
mod missions;
mod monarch_power;
mod power_income;
mod prestige;
mod truces;
mod unit_templates;
pub use missions::MissionReport;
mod politics;
mod province_map;
mod province_metrics;
mod religious_schools;
mod save_state;
mod script;
mod setup;
mod text;

pub use administration::AdministrationReport;
pub use administration_validation::{
    AdministrationCheckCount, AdministrationComparison, AdministrationDifference,
    SavedImperialBaselineReport, SavedImperialMembershipReport, SavedOfficePreferenceReport,
    apply_saved_hre_membership, apply_saved_imperial_baseline, apply_saved_office_preferences,
    compare_administration_save,
};
pub use attributes::AttributeReport;
pub use augustus_core::{
    COUNTRY_LEDGER_CATEGORY_COUNT, COUNTRY_LEDGER_EXPENSE_CATEGORY_COUNT,
    COUNTRY_LEDGER_EXPENSE_KEYS, COUNTRY_LEDGER_INCOME_GOLD_CATEGORY, COUNTRY_LEDGER_INCOME_KEYS,
    COUNTRY_LEDGER_INCOME_PRODUCTION_CATEGORY, COUNTRY_LEDGER_INCOME_TAX_CATEGORY,
    COUNTRY_LEDGER_INCOME_TRADE_CATEGORY, COUNTRY_LEDGER_NATIVE_FILTERED_EXPENSE_CATEGORIES,
    COUNTRY_LEDGER_SEND_OFFICERS_CATEGORY, COUNTRY_LEDGER_SUPPORT_LOYALISTS_CATEGORY,
    CashflowBreakdown, CountryLedger, CountryLedgerCategory, VanillaDerivedModifierInput,
    VanillaDerivedModifierResult, VanillaGoldBaseInput, VanillaGoldBaseResult,
    VanillaGoldProductionCountryResult, VanillaGoldProductionInput,
    VanillaGoldProductionInputState, VanillaGoldProductionProvinceResult,
    VanillaGoldProductionResult, VanillaModifierAggregationInput, VanillaModifierAggregationResult,
    VanillaProductionCountryResult, VanillaProductionInputState, VanillaProductionProvinceResult,
    VanillaTradeCandidate, VanillaTradeCaravanInput, VanillaTradeCaravanResult,
    VanillaTradeDistributionCandidate, VanillaTradeDistributionInput,
    VanillaTradeDistributionResult, VanillaTradeDistributionRouteInput,
    VanillaTradeDistributionRouteResult, VanillaTradeGoodPriceInput, VanillaTradeGoodPriceResult,
    VanillaTradeGoodPriceTableInput, VanillaTradeGoodPriceTableResult, VanillaTradeGoodsSizeInput,
    VanillaTradeGoodsSizeResult, VanillaTradePowerAmountInput, VanillaTradePowerAmountResult,
    VanillaTradePowerModifierInput, VanillaTradePowerModifierResult, VanillaTradePropagationInput,
    VanillaTradePropagationResult, VanillaTradeRetentionInput, VanillaTradeRetentionResult,
    VanillaTradeRouteWeightInput, VanillaTradeRouteWeightResult,
    aggregate_vanilla_modifier_contributions, calculate_vanilla_derived_modifier,
    calculate_vanilla_gold_base, calculate_vanilla_gold_production,
    calculate_vanilla_gold_production_for_country, calculate_vanilla_production_for_country,
    calculate_vanilla_trade_caravan, calculate_vanilla_trade_distribution,
    calculate_vanilla_trade_good_price, calculate_vanilla_trade_good_price_table,
    calculate_vanilla_trade_goods_size, calculate_vanilla_trade_power_amount,
    calculate_vanilla_trade_power_modifier, calculate_vanilla_trade_propagation,
    calculate_vanilla_trade_retention, calculate_vanilla_trade_route_weights,
    country_ledger_expense_key, country_ledger_income_key, sort_vanilla_trade_candidates,
};
pub use bootstrap::{
    BootstrapReport, CountryView, LoadedSource, MapEntry, ProvinceView,
    VanillaContentProductionGoodsSizeInput, VanillaContentProductionInput,
    VanillaContentProductionValueInput, VanillaExternalAdvisor, VanillaExternalArmy,
    VanillaExternalCountryMarkers, VanillaExternalCountryStatuses, VanillaExternalEstateState,
    VanillaExternalEstateStates, VanillaExternalNavalUnit, VanillaExternalProductionInput,
    VanillaExternalState, VanillaProvinceProductionBaseValueInput, VanillaWorld,
};
pub use casus_belli::{CasusBelliReport, CasusBelliType, CasusBelliTypes};
pub use content::{ContentRoot, ContentView, FileEntry};
pub use definitions::{DefinitionIndex, DefinitionKind, DefinitionOrigin};
pub use descriptor::ModDescriptor;
pub use diagnostic::{Error, SourceLocation};
pub use economy::EconomyReport;
pub use estates::EstateReport;
pub use eu4_rng::Eu4Random;
pub use event_ai::{evaluate_ai_event_weight, select_ai_event_option};
pub use governments::GovernmentReport;
pub use ideas::IdeaReport;
pub use load::{ContentSet, ContentSource, FileCandidate, FileOrigin, FileTrace, Suppression};
pub use politics::{RelationSource, RelationView};
pub use religious_schools::ReligiousSchoolReport;
pub use save_state::{VanillaSaveState, read_vanilla_save_state};
pub use script::{CompiledEvents, CompiledScript, ScriptCompiler, ScriptLibrary};
pub use setup::SetupReport;
pub use text::{FieldSummary, ParsedText, TextDocument, TextEncoding};

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
