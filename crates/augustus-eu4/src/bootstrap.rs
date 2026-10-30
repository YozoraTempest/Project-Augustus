// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    path::{Path, PathBuf},
};

use augustus_core::{
    Action, Calendar, CashflowBreakdown, CountryLedger, CountryResources, CountryState, EntityKind,
    InitialWorld, ModifierDefinition, ModifierInstance, MonthlyEconomy, MonthlyEventRules,
    MonthlyExternalCashflows, MonthlyPipeline, MonthlyProduction, MonthlySystem, MonthlyTax,
    NumericOperation, NumericSlot, ProvinceClass, ProvinceState, Target, Taxation,
    TechnologyProperty, VanillaGoldBaseInput, VanillaGoldBaseResult,
    VanillaGoldProductionCountryResult, VanillaGoldProductionInput,
    VanillaGoldProductionInputState, VanillaGoldProductionResult,
    VanillaNativeGrowthApplicationInput, VanillaNativeGrowthApplicationResult,
    VanillaNativeGrowthCacheInput, VanillaNativeGrowthCacheResult, VanillaNativeMigrationCostInput,
    VanillaNativeMigrationCostResult, VanillaNativeMigrationGateInput,
    VanillaNativeMigrationGateResult, VanillaProductionBaseValueInput,
    VanillaProductionBaseValueResult, VanillaProductionInput, VanillaStateMaintenanceDistanceInput,
    VanillaStateMaintenanceInput, VanillaStateMaintenanceResult, VanillaTradeGoodPriceInput,
    VanillaTradeGoodPriceTableInput, VanillaTradeGoodsSizeInput, WorldDate, YearlyEventRules,
    calculate_vanilla_gold_base, calculate_vanilla_gold_production,
    calculate_vanilla_gold_production_for_country, calculate_vanilla_native_growth_application,
    calculate_vanilla_native_growth_cache, calculate_vanilla_native_migration_cost,
    calculate_vanilla_native_migration_gate, calculate_vanilla_production_base_value,
    calculate_vanilla_state_maintenance, calculate_vanilla_trade_good_price,
    calculate_vanilla_trade_good_price_table, calculate_vanilla_trade_goods_size,
};
use jomini::{
    Encoding,
    common::{Date, PdsDate},
    text::{ObjectReader, Operator},
};
use serde::{Deserialize, Serialize, Serializer};
use sha2::{Digest, Sha256};

use crate::administration::{AdministrationDraft, AdministrationReport};
use crate::attributes::{ATTRIBUTE_NAMES, AttributeDraft, AttributeReport};
use crate::country_status::CountryStatusDraft;
use crate::economy::{EconomyDraft, EconomyReport};
use crate::estates::{EstateDraft, EstateReport};
use crate::governments::{GovernmentDraft, GovernmentReport};
use crate::ideas::{IdeaDraft, IdeaReport};
use crate::naval_constructions::{
    VanillaNavalConstructionImportReport, VanillaNavalConstructionReport,
};
use crate::politics::{self, CountryDraft, RelationSource, RelationView};
use crate::setup::{SetupDraft, SetupReport};
use crate::state_edicts::VanillaExternalStateEdicts;
use crate::{
    ContentRoot, ContentSet, ContentView, Error, ParsedText, SourceLocation, TextEncoding,
};

const ENCODING: TextEncoding = TextEncoding::Windows1252;
const EU4_1_37_5_FORT_BASE_COST_MILLI: i32 = 500;
const EU4_1_37_5_CURRENCY_CONVERSION_FACTOR_MICROS: i32 = 1_000_000;

#[derive(Debug, Clone, Serialize)]
pub struct LoadedSource {
    pub path: PathBuf,
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone)]
struct HistoricalPrestigeDelta {
    country: String,
    value_milli: i64,
    source: LoadedSource,
}

#[derive(Debug, Serialize)]
pub struct BootstrapReport {
    pub ruleset: &'static str,
    pub game_version: String,
    pub start_date: WorldDate,
    pub map_width: u32,
    pub map_height: u32,
    pub countries: usize,
    pub country_tag_slots: usize,
    pub generated_country_tag_slots: usize,
    pub active_countries: usize,
    pub provinces: usize,
    pub country_history_files: usize,
    pub province_history_files: usize,
    pub diplomacy_history_files: usize,
    pub people: usize,
    pub diplomatic_relations: usize,
    pub duplicate_relation_records: usize,
    pub person_sources: BTreeMap<String, BTreeMap<String, LoadedSource>>,
    pub relation_sources: Vec<RelationSource>,
    pub office_sources: BTreeMap<String, LoadedSource>,
    pub expired_people: Vec<String>,
    pub not_yet_born_people: Vec<String>,
    pub unapplied_diplomacy_fields: BTreeMap<String, usize>,
    pub missing_province_history: Vec<u32>,
    pub province_discovery_states: usize,
    pub unresolved_discovery_identifiers: BTreeMap<String, usize>,
    pub unmapped_definition_ids: Vec<u32>,
    pub unknown_map_colors: Vec<[u8; 3]>,
    pub unbound_active_capitals: Vec<String>,
    pub ambiguous_country_histories: BTreeMap<String, Vec<PathBuf>>,
    pub unapplied_country_fields: BTreeMap<String, usize>,
    pub unapplied_province_fields: BTreeMap<String, usize>,
    pub unapplied_examples: BTreeMap<String, LoadedSource>,
    pub economy: EconomyReport,
    pub setup: SetupReport,
    pub attributes: AttributeReport,
    pub administration: AdministrationReport,
    pub governments: GovernmentReport,
    pub ideas: IdeaReport,
    pub estates: EstateReport,
    pub religious_schools: crate::ReligiousSchoolReport,
    pub casus_belli: crate::CasusBelliReport,
    pub missions: crate::MissionReport,
}

#[derive(Debug, Serialize)]
pub struct ProvinceView<'a> {
    #[serde(flatten)]
    pub state: &'a ProvinceState,
    /// Persisted previous-controller history. The outer `None` is unknown;
    /// `Some(None)` is an explicit known absence.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_controller: Option<Option<&'a str>>,
    /// Persisted tribal-owner relation. The outer `None` is unknown;
    /// `Some(None)` is an explicit known absence.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tribal_owner: Option<Option<&'a str>>,
    pub terrain: Option<&'a str>,
    pub metrics: Option<&'a BTreeMap<String, Option<i64>>>,
    pub buildings: Option<Vec<&'a str>>,
    pub trade_node: Option<&'a str>,
    pub trade_node_outgoing: Option<Vec<&'a str>>,
    pub trade_good_definition: Option<&'a augustus_core::TradeGoodDefinition>,
    pub trade_good_price_milli: Option<i64>,
    pub modifiers: Vec<&'a str>,
    pub modifier_attachments_known: bool,
    pub declared_modifier_totals_micros: BTreeMap<String, i64>,
    pub attributes: BTreeMap<String, augustus_core::AttributeEvaluation>,
    pub administration: Option<augustus_core::AdministrationEvaluation>,
}

fn serialize_capital_history<S>(
    history: &Option<BTreeMap<WorldDate, Option<u32>>>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let Some(history) = history else {
        return serializer.serialize_none();
    };
    let values = history
        .iter()
        .map(|(date, capital)| {
            (
                format!("{:04}.{:02}.{:02}", date.year, date.month, date.day),
                *capital,
            )
        })
        .collect::<BTreeMap<_, _>>();
    values.serialize(serializer)
}

#[derive(Debug, Serialize)]
pub struct CountryView<'a> {
    pub metrics: Option<&'a BTreeMap<String, Option<i64>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metric_ceilings: Option<&'a BTreeMap<String, Option<i64>>>,
    /// Current advisors, when the compatibility layer has authoritative data.
    /// An empty list is meaningful: it means the country is known to have no
    /// active advisors. `advisors_known` distinguishes that from missing data.
    pub advisors: Option<&'a Vec<augustus_core::AdvisorState>>,
    pub advisors_known: bool,
    /// Category slots that are known even when the full advisor roster is not.
    /// Missing categories are unknown; `None` values are known-empty slots.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub advisor_slots:
        Option<BTreeMap<augustus_core::RulerSkill, Option<augustus_core::AdvisorState>>>,
    pub flags: Option<&'a BTreeMap<String, bool>>,
    pub flag_namespace_known: bool,
    pub estates: Option<&'a BTreeMap<String, augustus_core::EstateState>>,
    pub declared_estate_effects: augustus_core::EstateEvaluation,
    pub ideas: Option<&'a augustus_core::IdeaState>,
    pub declared_idea_effects: augustus_core::IdeaEvaluation,
    pub last_monthly_tax: Option<&'a augustus_core::MonthlyTaxAccount>,
    pub last_monthly_production: Option<&'a augustus_core::ProductionCountryAccount>,
    pub last_monthly_cashflow: Option<&'a augustus_core::CashflowAccount>,
    pub government: Option<&'a augustus_core::GovernmentState>,
    pub declared_government_effects: augustus_core::GovernmentEvaluation,
    pub state: &'a CountryState,
    pub owned_provinces: Vec<u32>,
    pub bound_capital: Option<u32>,
    /// Persisted EU4 original-capital history. Unknown is omitted, explicit
    /// absence is serialized as null.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_capital: Option<Option<u32>>,
    /// Dated capital events imported from the save history block.
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "serialize_capital_history"
    )]
    pub capital_history: Option<BTreeMap<WorldDate, Option<u32>>>,
    pub politics: Option<&'a augustus_core::CountryPolitics>,
    pub diplomacy: Vec<RelationView<'a>>,
    pub political_offices: Vec<&'a str>,
    pub technology: Option<&'a BTreeMap<String, u16>>,
    pub resources: Option<&'a augustus_core::CountryResources>,
    pub modifiers: Vec<&'a str>,
    pub modifier_attachments_known: bool,
    pub declared_modifier_totals_micros: BTreeMap<String, i64>,
    pub province_attribute_totals: BTreeMap<String, augustus_core::AttributeAggregate>,
    pub administrative_areas: Option<&'a BTreeMap<String, augustus_core::AreaStatus>>,
}

#[derive(Debug, Serialize)]
pub struct MapEntry {
    pub province: u32,
    pub color: [u8; 3],
    pub display_color: [u8; 3],
}

/// Inputs for the ordinary production value stage after quantity has already
/// been resolved by a compatibility provider. The trade-good base price is
/// read from the loaded EU4 content rather than supplied by the caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaProvinceProductionBaseValueInput {
    pub goods_size_milli: i32,
    pub flat_modifier_milli: i32,
    pub external_flat_modifier_milli: i32,
    pub rate_modifier_milli: i32,
    pub external_rate_modifier_milli: i32,
}

/// Resolved context for one province's state-maintenance calculation.
///
/// Tax, production, and manpower development are read from the current
/// province state. Distance and modifier sources remain explicit because
/// their upstream rules have not been fully translated yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaProvinceStateMaintenanceInput {
    pub development_scale_milli: i32,
    pub special_development_scale_milli: Option<i32>,
    pub distance: Option<VanillaStateMaintenanceDistanceInput>,
    pub region_difference_modifier_milli: i32,
    pub owner_relation_modifier_milli: i32,
    pub country_key61_modifier_milli: i32,
    pub province_key62_modifier_milli: i32,
    pub caller_extra_modifier_milli: i32,
}

/// Fort-maintenance context that is not yet resolved from loaded world state.
/// The 1.37.5 base cost and currency conversion are supplied by the adapter;
/// the local fort level is read from the loaded province building.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaFortMaintenanceContextInput {
    pub country_modifier_milli: i32,
    pub province_modifier_milli: i32,
    pub relationship_modifier_milli: i32,
    pub apply_relationship_modifier: bool,
    pub halve_for_special_case: bool,
}

/// Resolved state-maintenance context using the development factor loaded from EU4 content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaContentStateMaintenanceInput {
    pub special_development_scale_milli: Option<i32>,
    pub distance: Option<VanillaStateMaintenanceDistanceInput>,
    pub region_difference_modifier_milli: i32,
    pub owner_relation_modifier_milli: i32,
    pub country_key61_modifier_milli: i32,
    pub province_key62_modifier_milli: i32,
    pub caller_extra_modifier_milli: i32,
}

/// Fully explicit production-stage inputs supplied by an external provider.
/// The compatibility layer fills the price from the province's EU4 content,
/// then keeps the downstream monthly-production modifiers explicit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaExternalProductionInput {
    pub goods_size_milli: i32,
    pub flat_modifier_milli: i32,
    pub external_flat_modifier_milli: i32,
    pub rate_modifier_milli: i32,
    pub external_rate_modifier_milli: i32,
    pub country_modifier_milli: i32,
    pub legacy_province_modifier_milli: i32,
    pub queried_province_modifier_milli: i32,
}

/// Production inputs whose country-side efficiency is resolved from the
/// current administrative technology instead of being repeated by the caller.
/// The remaining province and value stages stay explicit until their native
/// producers are mapped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaContentProductionInput {
    pub base_value_milli: i32,
    pub legacy_province_modifier_milli: i32,
    pub queried_province_modifier_milli: i32,
}

/// Production value inputs whose trade-good price and country efficiency are
/// resolved from loaded EU4 content. Goods quantity and province modifier
/// slots remain explicit until their native producers are mapped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaContentProductionValueInput {
    pub goods_size_milli: i32,
    pub flat_modifier_milli: i32,
    pub external_flat_modifier_milli: i32,
    pub rate_modifier_milli: i32,
    pub external_rate_modifier_milli: i32,
    pub legacy_province_modifier_milli: i32,
    pub queried_province_modifier_milli: i32,
}

/// Production value inputs whose goods quantity is derived from the six
/// source terms resolved by the compatibility layer. The resulting quantity
/// then follows the existing content-price and country-efficiency path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaContentProductionGoodsSizeInput {
    pub goods_size_input: VanillaTradeGoodsSizeInput,
    pub flat_modifier_milli: i32,
    pub external_flat_modifier_milli: i32,
    pub rate_modifier_milli: i32,
    pub external_rate_modifier_milli: i32,
    pub legacy_province_modifier_milli: i32,
    pub queried_province_modifier_milli: i32,
}

/// A starting land stack translated by an external simulator or save reader.
/// The EU4 compatibility boundary resolves the country tag and province ID;
/// the shared core stores regiment types and any supplied current state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaExternalArmy {
    pub owner: String,
    pub location: u32,
    #[serde(default)]
    pub name: String,
    pub regiments: Vec<String>,
    /// Optional current state aligned with `regiments` by index. An empty
    /// vector preserves compatibility with older external army snapshots.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub regiment_states: Vec<VanillaExternalRegimentState>,
}

/// Current state translated for a regiment from a save reader or simulator.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaExternalRegimentState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strength_milli: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub morale_milli: Option<i64>,
    /// EU4 province ID; the compatibility layer resolves it to a core entity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub home_province: Option<u32>,
}

/// A starting naval unit translated by an external simulator or save reader.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaExternalNavalUnit {
    pub owner: String,
    pub location: u32,
    pub unit_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<VanillaExternalShipState>,
}

/// A fleet translated from a save or another compatibility source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaExternalFleet {
    pub owner: String,
    pub location: u32,
    #[serde(default)]
    pub name: Option<String>,
    pub ship_types: Vec<String>,
    /// Optional current state aligned with `ship_types` by index.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ship_states: Vec<VanillaExternalShipState>,
}

/// Current state translated for a ship from a save reader or simulator.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaExternalShipState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub morale_milli: Option<i64>,
    /// EU4 province ID; the compatibility layer resolves it to a core entity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub home_province: Option<u32>,
}

/// A current advisor translated from a save or another compatibility source.
/// The EU4 advisor type is resolved to a monarch-power category during binding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaExternalAdvisor {
    pub type_name: String,
    pub skill: u8,
    #[serde(default)]
    pub id: Option<u64>,
}

/// Estate privilege sets supplied by a compatibility provider, keyed by EU4
/// country tag and estate identifier. `Some(empty)` means the estate is known
/// to have no privileges; `None` remains unresolved.
pub type VanillaExternalEstatePrivileges =
    BTreeMap<String, BTreeMap<String, Option<BTreeSet<String>>>>;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaExternalEstateState {
    /// Current loyalty in EU4 thousandths of a percentage point.
    #[serde(default)]
    pub loyalty_milli: Option<i64>,
    /// Current land share as a millionth fraction of the country's land.
    #[serde(default)]
    pub land_share_micros: Option<i64>,
    /// Native active influence modifier identifiers from an EU4 save. The
    /// compatibility layer resolves zero-based identifiers against the estate's
    /// declaration order; identifiers without a matching definition remain
    /// unresolved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_influences: Option<BTreeSet<u32>>,
}

pub type VanillaExternalEstateStates =
    BTreeMap<String, BTreeMap<String, VanillaExternalEstateState>>;

/// Saved government mechanic resource state, keyed by country, mechanic, and
/// power identifier. Values stay separate from monarch power stocks.
pub type VanillaExternalGovernmentMechanicPowers = BTreeMap<
    String,
    BTreeMap<String, BTreeMap<String, augustus_core::GovernmentMechanicResourceState>>,
>;

/// Country flag namespaces supplied by a compatibility provider.
///
/// `Some(set)` means the namespace is complete and only the listed flags are
/// present. `Some(empty)` is therefore a known empty namespace; `None` keeps
/// the namespace unresolved.
pub type VanillaExternalCountryMarkers = BTreeMap<String, Option<BTreeSet<String>>>;

/// Current country boolean statuses supplied by a compatibility provider.
/// Missing status entries are left as they were loaded; an explicit `None`
/// keeps one status unresolved.
pub type VanillaExternalCountryStatuses = BTreeMap<String, BTreeMap<String, Option<bool>>>;

/// Current bilateral attitude and trust values supplied by a save reader or
/// another compatibility provider, keyed by owner and target country tags.
pub type VanillaExternalCountryRelationStates =
    BTreeMap<String, BTreeMap<String, augustus_core::CountryRelationState>>;

/// Current province development values translated from an EU4 save or another
/// compatibility provider, in thousandths. Omitted values remain
/// content-derived.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaExternalProvinceDevelopment {
    #[serde(default)]
    pub base_tax_milli: Option<i64>,
    #[serde(default)]
    pub base_production_milli: Option<i64>,
    #[serde(default)]
    pub base_manpower_milli: Option<i64>,
}

#[derive(Debug)]
pub struct VanillaWorld {
    pub initial: InitialWorld,
    pub report: BootstrapReport,
    pub country_tag_registry: crate::CountryTagRegistry,
    pub province_bitmap: PathBuf,
    pub country_sources: BTreeMap<String, Vec<LoadedSource>>,
    pub province_sources: BTreeMap<u32, Vec<LoadedSource>>,
    province_mothball_status: BTreeMap<u32, u8>,
    state_maintenance_development_scale_milli: Option<i32>,
    event_texts: BTreeMap<String, crate::EventText>,
    monthly_event_rules: Option<MonthlyEventRules>,
    monthly_event_system_id: Option<String>,
    monthly_event_base: Option<MonthlyEventBase>,
    yearly_event_rules: Option<augustus_core::YearlyEventRules>,
    naval_construction_import_report: VanillaNavalConstructionImportReport,
    content: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MonthlyEventBase {
    Tax,
    Economy,
    EventsOnly,
}

impl MonthlyEventBase {
    fn system_id(self) -> Option<&'static str> {
        match self {
            Self::Tax => Some("monthly-tax:1"),
            Self::Economy => Some("monthly-economy:1"),
            Self::EventsOnly => None,
        }
    }
}

fn monthly_event_base(initial: &InitialWorld) -> Result<MonthlyEventBase, augustus_core::Error> {
    match initial.monthly_system_id() {
        Some(id) if id == MonthlyTax.id() => Ok(MonthlyEventBase::Tax),
        Some(id) if id == MonthlyEconomy.id() => Ok(MonthlyEventBase::Economy),
        None => Ok(MonthlyEventBase::EventsOnly),
        Some(id) => Err(augustus_core::Error::Invalid(format!(
            "cannot compose monthly pulse events with custom monthly system {id}"
        ))),
    }
}

fn monthly_event_pipeline_id(
    base: MonthlyEventBase,
    rules: &MonthlyEventRules,
) -> Result<String, Error> {
    let identity = serde_json::to_vec(&(base.system_id(), rules.id()))
        .map_err(|error| Error::caused_by("on_monthly_pulse", error))?;
    let fingerprint: String = Sha256::digest(identity)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    Ok(format!("eu4-monthly-pulse-pipeline:1:{fingerprint}"))
}

fn monthly_event_pipeline<'a>(
    id: impl Into<String>,
    base: MonthlyEventBase,
    rules: &'a MonthlyEventRules,
) -> Result<MonthlyPipeline<'a>, augustus_core::Error> {
    let world_phase: Option<&dyn MonthlySystem> = match base {
        MonthlyEventBase::Tax => Some(&MonthlyTax),
        MonthlyEventBase::Economy => Some(&MonthlyEconomy),
        MonthlyEventBase::EventsOnly => None,
    };
    let event_phase: &dyn MonthlySystem = rules;
    MonthlyPipeline::new(id, world_phase, None, Some(event_phase))
}

/// State supplied by a second simulation or an external save reader.
///
/// EU4 tags and province identifiers stay at the compatibility boundary. The
/// shared core receives only the already translated numeric state and keeps
/// omitted values unresolved.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaExternalPerson {
    pub state: augustus_core::PersonState,
    /// EU4 country tag for a person's origin, resolved to an entity when bound.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub country_of_origin: Option<String>,
}

pub type VanillaExternalCountryPersons =
    BTreeMap<String, BTreeMap<String, Option<VanillaExternalPerson>>>;

/// Current EU4 random stream position translated from a save or provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaExternalEu4RandomState {
    pub seed: u32,
    pub count: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaExternalState {
    #[serde(default)]
    pub country_metric_values: BTreeMap<String, BTreeMap<String, Option<i64>>>,
    /// Current national-focus selection supplied by a save reader or another
    /// compatibility provider. A missing country remains unknown; a focus
    /// with no skill records an explicitly cleared focus.
    #[serde(default)]
    pub country_national_focus: BTreeMap<String, augustus_core::NationalFocus>,
    /// Current country capital supplied by a save reader. `None` explicitly
    /// records a country with no capital; an omitted country keeps history.
    #[serde(default)]
    pub country_capitals: BTreeMap<String, Option<u32>>,
    /// Original capital supplied by a save reader. Omitted countries remain
    /// unknown; `None` records explicit absence.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub country_original_capitals: BTreeMap<String, Option<u32>>,
    /// Dated country capital events supplied by a save reader.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub country_capital_history: BTreeMap<String, BTreeMap<WorldDate, Option<u32>>>,
    /// Current ruler, heir, or consort supplied by a save reader. Missing
    /// roles keep their content-derived state; a present `None` clears a role.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub country_persons: VanillaExternalCountryPersons,
    /// Saved EU4 seed and draw count, restored before startup effects execute.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eu4_random_state: Option<VanillaExternalEu4RandomState>,
    /// Current administrative, diplomatic, and military levels supplied by a
    /// save reader. Omitted branches keep the content-derived level.
    #[serde(default)]
    pub country_technology_levels: BTreeMap<String, BTreeMap<String, u16>>,
    /// Current country technology group and unit type supplied by a save
    /// reader. Omitted countries keep their content-derived values.
    #[serde(default)]
    pub country_technology_groups: BTreeMap<String, Option<String>>,
    #[serde(default)]
    pub country_unit_types: BTreeMap<String, Option<String>>,
    /// Current government rank supplied by a save reader. An omitted country
    /// keeps its content-derived rank; `None` explicitly leaves it unknown.
    #[serde(default)]
    pub country_government_ranks: BTreeMap<String, Option<u8>>,
    /// Current government kind and selected reforms supplied by a save
    /// reader. A present `None` marks that component unknown.
    #[serde(default)]
    pub country_government_kinds: BTreeMap<String, Option<String>>,
    #[serde(default)]
    pub country_government_reforms: BTreeMap<String, Option<BTreeSet<String>>>,
    /// Current active or inactive government metric values supplied by a save
    /// reader. The compatibility loader filters these against government
    /// metric definitions, separately from generic country attributes.
    #[serde(default)]
    pub country_government_metric_values: BTreeMap<String, BTreeMap<String, Option<i64>>>,
    /// Current government-specific resource values and freeze flags from a save.
    #[serde(default)]
    pub country_government_mechanic_powers: VanillaExternalGovernmentMechanicPowers,
    /// Current country culture and religion supplied by a save reader. A
    /// missing country keeps its content-derived values; `None` clears one.
    #[serde(default)]
    pub country_primary_cultures: BTreeMap<String, Option<String>>,
    #[serde(default)]
    pub country_religions: BTreeMap<String, Option<String>>,
    /// Current active idea groups and unlocked idea counts supplied by a save
    /// reader. A missing country keeps the content-derived idea state.
    #[serde(default)]
    pub country_ideas: BTreeMap<String, augustus_core::IdeaState>,
    /// Completed mission identifiers supplied by a save reader. A present
    /// empty set means no missions are complete; an omitted country is unknown.
    #[serde(default)]
    pub country_mission_completions: BTreeMap<String, BTreeSet<String>>,
    /// Current country variables from a save, stored in thousandths. A
    /// present empty map means the country's variable namespace is empty.
    #[serde(default)]
    pub country_variables: BTreeMap<String, BTreeMap<String, i64>>,
    /// Current per-country growth ceilings for metrics such as monarch power.
    /// A missing country or metric remains unresolved; `Some(value)` is an
    /// authoritative ceiling supplied by the compatibility provider.
    #[serde(default)]
    pub country_metric_ceilings: BTreeMap<String, BTreeMap<String, Option<i64>>>,
    #[serde(default)]
    pub country_resources: BTreeMap<String, CountryResources>,
    #[serde(default)]
    pub production_inputs: BTreeMap<u32, augustus_core::VanillaProductionInputState>,
    /// Per-province gold production inputs already resolved by a compatibility
    /// provider. The core gold calculator filters these by current ownership.
    #[serde(default)]
    pub gold_production_inputs: BTreeMap<u32, augustus_core::VanillaGoldProductionInputState>,
    #[serde(default)]
    pub production_value_inputs: BTreeMap<u32, VanillaExternalProductionInput>,
    /// Runtime price-impact totals keyed by the native trade-good table index.
    /// An omitted index means an empty impact chain, matching the vanilla
    /// zero-impact state rather than inventing a new price.
    #[serde(default)]
    pub trade_good_price_impacts_milli: BTreeMap<u32, i32>,
    /// Ordered runtime price-impact entries keyed by the native trade-good
    /// table index. When present, these entries are accumulated with the
    /// native 32-bit wrapping rule before the effective price is rebuilt.
    #[serde(default)]
    pub trade_good_price_impact_entries_milli: BTreeMap<u32, Vec<i32>>,
    /// Current runtime prices keyed by the EU4 trade-good identifier. An
    /// explicit `None` keeps a known trade good unresolved.
    #[serde(default)]
    pub trade_good_prices_milli: BTreeMap<String, Option<i64>>,
    /// Persistent price effects recovered from EU4 saves, grouped by good and key.
    #[serde(default)]
    pub trade_good_keyed_price_impacts_milli: BTreeMap<String, BTreeMap<String, i32>>,
    /// Save effects whose expiry or value is not supported by the current loader.
    #[serde(default)]
    pub unresolved_trade_good_price_impacts_milli: BTreeMap<String, BTreeSet<String>>,
    /// Verified annual country-tax sources keyed by EU4 country tag. A value
    /// overrides the content-derived country attribute for that country;
    /// `None` keeps its tax unresolved while retaining known province tax.
    #[serde(default)]
    pub country_annual_tax_milli: BTreeMap<String, Option<i64>>,
    /// Verified annual province-tax sources keyed by EU4 province ID. A value
    /// replaces the content-derived annual tax for that province; `None`
    /// keeps that province unresolved while other provinces remain usable.
    #[serde(default)]
    pub province_annual_tax_milli: BTreeMap<u32, Option<i64>>,
    /// Current province owner supplied by a save reader. An omitted province
    /// remains content-derived; `None` explicitly records an unowned province.
    #[serde(default)]
    pub province_owners: BTreeMap<u32, Option<String>>,
    /// Current completed province buildings supplied by a save reader. An
    /// omitted province keeps its content-derived set; a save reader supplies
    /// an empty set when its province block omits the building subobject.
    #[serde(default)]
    pub province_buildings: BTreeMap<u32, BTreeSet<String>>,
    /// Current province core tags supplied by a save reader, in save order.
    /// A save reader supplies an empty list when a province omits the field;
    /// the supported field does not identify full versus territorial core.
    #[serde(default)]
    pub province_cores: BTreeMap<u32, Vec<String>>,
    /// Current countries that have discovered each province. An omitted
    /// province keeps its history-derived state; a present empty set is known
    /// to be undiscovered by every country in the loaded content.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub province_discovered_by: BTreeMap<u32, BTreeSet<String>>,
    /// Current province trade goods supplied by a save reader. An omitted
    /// province keeps its content-derived good; `None` explicitly clears it.
    #[serde(default)]
    pub province_trade_goods: BTreeMap<u32, Option<String>>,
    /// Current center-of-trade levels supplied by a save reader. An omitted
    /// province keeps its history-derived level; `None` leaves it unresolved.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub province_center_of_trade_levels: BTreeMap<u32, Option<u8>>,
    /// Current province culture and religion supplied by a save reader. An
    /// omitted province keeps its content-derived values; `None` clears one.
    #[serde(default)]
    pub province_cultures: BTreeMap<u32, Option<String>>,
    #[serde(default)]
    pub province_religions: BTreeMap<u32, Option<String>>,
    /// Current province controller supplied by a save reader. An omitted
    /// province remains content-derived; `None` keeps its controller unknown.
    #[serde(default)]
    pub province_controllers: BTreeMap<u32, Option<String>>,
    /// Previous province controller supplied by a save reader. An omitted
    /// province remains history-unknown; `None` records known absence.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub province_previous_controllers: BTreeMap<u32, Option<String>>,
    /// Current province tribal owner supplied by a save reader. An omitted
    /// province remains history-unknown; `None` records known absence.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub province_tribal_owners: BTreeMap<u32, Option<String>>,
    /// Current base tax, production, and manpower values supplied by a save
    /// reader. An omitted province or value remains content-derived.
    #[serde(default)]
    pub province_development: BTreeMap<u32, VanillaExternalProvinceDevelopment>,
    /// Current province metric values supplied by a save reader. Omitted
    /// provinces and metric names keep their content-derived state.
    #[serde(default)]
    pub province_metric_values: BTreeMap<u32, BTreeMap<String, Option<i64>>>,
    /// Current city status supplied by a save reader. An omitted province
    /// remains unknown; `Some(false)` preserves an explicit non-city state.
    #[serde(default)]
    pub province_is_city: BTreeMap<u32, Option<bool>>,
    /// Current HRE membership supplied by a save reader. An omitted province
    /// remains unknown; `Some(false)` records explicit absence.
    #[serde(default)]
    pub province_in_empire: BTreeMap<u32, Option<bool>>,
    /// Raw EU4 province `mothball_status` bytes. New provinces default to 1;
    /// EU4 saves omit that default and retain other values verbatim.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub province_mothball_status: BTreeMap<u32, u8>,
    /// Current raw province autonomy supplied by a save reader, in EU4's
    /// thousandths. The core stores the same value in millionths and applies
    /// monthly autonomy rules from that state on the next month boundary.
    #[serde(default)]
    pub province_raw_autonomy_milli: BTreeMap<u32, Option<i64>>,
    /// Current estate privilege sets supplied by a save reader. `Some(empty)`
    /// means the estate is known to have no privileges; `None` remains unknown.
    #[serde(default)]
    pub estate_privileges: VanillaExternalEstatePrivileges,
    /// Current estate loyalty, land share, and active influence identifiers
    /// supplied by a save reader or another compatibility provider. Omitted
    /// metrics remain unresolved.
    #[serde(default)]
    pub estate_states: VanillaExternalEstateStates,
    /// Current country flag namespaces supplied by a save reader. A complete
    /// set replaces the historical namespace for that country.
    #[serde(default)]
    pub country_markers: VanillaExternalCountryMarkers,
    /// Current country boolean statuses such as `at_war` supplied by a save
    /// reader. Omitted statuses remain at their content-derived state.
    #[serde(default)]
    pub country_statuses: VanillaExternalCountryStatuses,
    /// Current political office holders supplied by a save reader. An omitted
    /// office keeps its content-derived holder; `None` explicitly clears it.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub political_office_holders: BTreeMap<String, Option<String>>,
    /// Current bilateral relation inputs imported from a save. Missing
    /// targets remain unresolved; this does not recalculate AI attitudes.
    #[serde(default)]
    pub country_relation_states: VanillaExternalCountryRelationStates,
    #[serde(default)]
    pub estate_loyalty_targets: BTreeMap<String, BTreeMap<String, Option<i64>>>,
    #[serde(default)]
    pub country_monthly_cashflows_milli: BTreeMap<String, Option<i64>>,
    /// Monthly ledger categories translated by a compatibility provider.
    /// Trade records may be calculated by the core; gold is a receipt and
    /// expenses are a positive cost.
    #[serde(default)]
    pub country_monthly_cashflow_breakdowns: BTreeMap<String, CashflowBreakdown>,
    /// Verified monthly monarch-power income supplied by a compatibility
    /// provider. Keys are EU4 metric names (`adm_power`, `dip_power`, and
    /// `mil_power`); values use the metric's integer units.
    #[serde(default)]
    pub country_monthly_power_income: BTreeMap<String, BTreeMap<String, Option<i64>>>,
    /// Starting land and naval units supplied by another simulation. These
    /// are bound before startup effects so scripts can query the same units.
    #[serde(default)]
    pub armies: Vec<VanillaExternalArmy>,
    #[serde(default)]
    pub naval_units: Vec<VanillaExternalNavalUnit>,
    /// Preserves fleet grouping when the source provides it. Older external
    /// bundles that contain only `naval_units` import each ship as a fleet.
    #[serde(default)]
    pub naval_fleets: Vec<VanillaExternalFleet>,
    #[serde(default)]
    pub advisors: BTreeMap<String, Vec<VanillaExternalAdvisor>>,
}

impl VanillaExternalState {
    /// Applies supplemental translated inputs on top of this state bundle.
    /// Nested country maps and province development merge by field; non-empty
    /// unit rosters replace the existing roster so an external provider can
    /// supply an authoritative snapshot.
    pub fn apply_overlay(&mut self, overlay: &Self) {
        overlay_external_state(self, overlay);
    }
}

#[derive(Clone, Copy, Default)]
struct LoadOverrides<'a> {
    country_metric_values: Option<&'a BTreeMap<String, BTreeMap<String, Option<i64>>>>,
    country_national_focus: Option<&'a BTreeMap<String, augustus_core::NationalFocus>>,
    country_capitals: Option<&'a BTreeMap<String, Option<u32>>>,
    country_original_capitals: Option<&'a BTreeMap<String, Option<u32>>>,
    country_capital_history: Option<&'a BTreeMap<String, BTreeMap<WorldDate, Option<u32>>>>,
    country_persons: Option<&'a VanillaExternalCountryPersons>,
    eu4_random_state: Option<VanillaExternalEu4RandomState>,
    country_technology_levels: Option<&'a BTreeMap<String, BTreeMap<String, u16>>>,
    country_technology_groups: Option<&'a BTreeMap<String, Option<String>>>,
    country_unit_types: Option<&'a BTreeMap<String, Option<String>>>,
    country_government_ranks: Option<&'a BTreeMap<String, Option<u8>>>,
    country_government_kinds: Option<&'a BTreeMap<String, Option<String>>>,
    country_government_reforms: Option<&'a BTreeMap<String, Option<BTreeSet<String>>>>,
    country_government_metric_values: Option<&'a BTreeMap<String, BTreeMap<String, Option<i64>>>>,
    country_government_mechanic_powers: Option<&'a VanillaExternalGovernmentMechanicPowers>,
    country_primary_cultures: Option<&'a BTreeMap<String, Option<String>>>,
    country_religions: Option<&'a BTreeMap<String, Option<String>>>,
    country_ideas: Option<&'a BTreeMap<String, augustus_core::IdeaState>>,
    country_mission_completions: Option<&'a BTreeMap<String, BTreeSet<String>>>,
    country_variables: Option<&'a BTreeMap<String, BTreeMap<String, i64>>>,
    country_metric_ceilings: Option<&'a BTreeMap<String, BTreeMap<String, Option<i64>>>>,
    country_resources: Option<&'a BTreeMap<String, CountryResources>>,
    country_ledgers: Option<&'a BTreeMap<String, CountryLedger>>,
    production_inputs: Option<&'a BTreeMap<u32, augustus_core::VanillaProductionInputState>>,
    gold_production_inputs:
        Option<&'a BTreeMap<u32, augustus_core::VanillaGoldProductionInputState>>,
    production_value_inputs: Option<&'a BTreeMap<u32, VanillaExternalProductionInput>>,
    trade_good_price_impacts_milli: Option<&'a BTreeMap<u32, i32>>,
    trade_good_price_impact_entries_milli: Option<&'a BTreeMap<u32, Vec<i32>>>,
    trade_good_prices_milli: Option<&'a BTreeMap<String, Option<i64>>>,
    trade_good_keyed_price_impacts_milli: Option<&'a BTreeMap<String, BTreeMap<String, i32>>>,
    unresolved_trade_good_price_impacts_milli: Option<&'a BTreeMap<String, BTreeSet<String>>>,
    country_annual_tax_milli: Option<&'a BTreeMap<String, Option<i64>>>,
    province_annual_tax_milli: Option<&'a BTreeMap<u32, Option<i64>>>,
    province_owners: Option<&'a BTreeMap<u32, Option<String>>>,
    province_buildings: Option<&'a BTreeMap<u32, BTreeSet<String>>>,
    province_cores: Option<&'a BTreeMap<u32, Vec<String>>>,
    province_discovered_by: Option<&'a BTreeMap<u32, BTreeSet<String>>>,
    province_trade_goods: Option<&'a BTreeMap<u32, Option<String>>>,
    province_center_of_trade_levels: Option<&'a BTreeMap<u32, Option<u8>>>,
    province_cultures: Option<&'a BTreeMap<u32, Option<String>>>,
    province_religions: Option<&'a BTreeMap<u32, Option<String>>>,
    province_controllers: Option<&'a BTreeMap<u32, Option<String>>>,
    province_previous_controllers: Option<&'a BTreeMap<u32, Option<String>>>,
    province_tribal_owners: Option<&'a BTreeMap<u32, Option<String>>>,
    province_development: Option<&'a BTreeMap<u32, VanillaExternalProvinceDevelopment>>,
    province_metric_values: Option<&'a BTreeMap<u32, BTreeMap<String, Option<i64>>>>,
    state_edicts: Option<&'a VanillaExternalStateEdicts>,
    naval_construction_report: Option<&'a VanillaNavalConstructionReport>,
    province_is_city: Option<&'a BTreeMap<u32, Option<bool>>>,
    province_in_empire: Option<&'a BTreeMap<u32, Option<bool>>>,
    province_mothball_status: Option<&'a BTreeMap<u32, u8>>,
    province_raw_autonomy_milli: Option<&'a BTreeMap<u32, Option<i64>>>,
    estate_privileges: Option<&'a VanillaExternalEstatePrivileges>,
    estate_states: Option<&'a VanillaExternalEstateStates>,
    country_markers: Option<&'a VanillaExternalCountryMarkers>,
    country_statuses: Option<&'a VanillaExternalCountryStatuses>,
    political_office_holders: Option<&'a BTreeMap<String, Option<String>>>,
    country_relation_states: Option<&'a VanillaExternalCountryRelationStates>,
    estate_loyalty_targets: Option<&'a BTreeMap<String, BTreeMap<String, Option<i64>>>>,
    country_monthly_cashflows_milli: Option<&'a BTreeMap<String, Option<i64>>>,
    country_monthly_cashflow_breakdowns: Option<&'a BTreeMap<String, CashflowBreakdown>>,
    country_monthly_power_income: Option<&'a BTreeMap<String, BTreeMap<String, Option<i64>>>>,
    armies: Option<&'a [VanillaExternalArmy]>,
    naval_units: Option<&'a [VanillaExternalNavalUnit]>,
    naval_fleets: Option<&'a [VanillaExternalFleet]>,
    advisors: Option<&'a BTreeMap<String, Vec<VanillaExternalAdvisor>>>,
}

impl<'a> LoadOverrides<'a> {
    fn from_external_state(state: &'a VanillaExternalState) -> Self {
        Self {
            country_metric_values: (!state.country_metric_values.is_empty())
                .then_some(&state.country_metric_values),
            naval_construction_report: None,
            country_national_focus: (!state.country_national_focus.is_empty())
                .then_some(&state.country_national_focus),
            country_capitals: (!state.country_capitals.is_empty())
                .then_some(&state.country_capitals),
            country_original_capitals: (!state.country_original_capitals.is_empty())
                .then_some(&state.country_original_capitals),
            country_capital_history: (!state.country_capital_history.is_empty())
                .then_some(&state.country_capital_history),
            country_persons: (!state.country_persons.is_empty()).then_some(&state.country_persons),
            eu4_random_state: state.eu4_random_state,
            country_technology_levels: (!state.country_technology_levels.is_empty())
                .then_some(&state.country_technology_levels),
            country_technology_groups: (!state.country_technology_groups.is_empty())
                .then_some(&state.country_technology_groups),
            country_unit_types: (!state.country_unit_types.is_empty())
                .then_some(&state.country_unit_types),
            country_government_ranks: (!state.country_government_ranks.is_empty())
                .then_some(&state.country_government_ranks),
            country_government_kinds: (!state.country_government_kinds.is_empty())
                .then_some(&state.country_government_kinds),
            country_government_reforms: (!state.country_government_reforms.is_empty())
                .then_some(&state.country_government_reforms),
            country_government_metric_values: (!state.country_government_metric_values.is_empty())
                .then_some(&state.country_government_metric_values),
            country_government_mechanic_powers: (!state
                .country_government_mechanic_powers
                .is_empty())
            .then_some(&state.country_government_mechanic_powers),
            country_primary_cultures: (!state.country_primary_cultures.is_empty())
                .then_some(&state.country_primary_cultures),
            country_religions: (!state.country_religions.is_empty())
                .then_some(&state.country_religions),
            country_ideas: (!state.country_ideas.is_empty()).then_some(&state.country_ideas),
            country_mission_completions: (!state.country_mission_completions.is_empty())
                .then_some(&state.country_mission_completions),
            country_variables: (!state.country_variables.is_empty())
                .then_some(&state.country_variables),
            country_metric_ceilings: (!state.country_metric_ceilings.is_empty())
                .then_some(&state.country_metric_ceilings),
            country_resources: (!state.country_resources.is_empty())
                .then_some(&state.country_resources),
            country_ledgers: None,
            production_inputs: (!state.production_inputs.is_empty())
                .then_some(&state.production_inputs),
            gold_production_inputs: (!state.gold_production_inputs.is_empty())
                .then_some(&state.gold_production_inputs),
            production_value_inputs: (!state.production_value_inputs.is_empty())
                .then_some(&state.production_value_inputs),
            trade_good_price_impacts_milli: (!state.trade_good_price_impacts_milli.is_empty())
                .then_some(&state.trade_good_price_impacts_milli),
            trade_good_price_impact_entries_milli: (!state
                .trade_good_price_impact_entries_milli
                .is_empty())
            .then_some(&state.trade_good_price_impact_entries_milli),
            trade_good_prices_milli: (!state.trade_good_prices_milli.is_empty())
                .then_some(&state.trade_good_prices_milli),
            trade_good_keyed_price_impacts_milli: (!state
                .trade_good_keyed_price_impacts_milli
                .is_empty())
            .then_some(&state.trade_good_keyed_price_impacts_milli),
            unresolved_trade_good_price_impacts_milli: (!state
                .unresolved_trade_good_price_impacts_milli
                .is_empty())
            .then_some(&state.unresolved_trade_good_price_impacts_milli),
            country_annual_tax_milli: (!state.country_annual_tax_milli.is_empty())
                .then_some(&state.country_annual_tax_milli),
            province_annual_tax_milli: (!state.province_annual_tax_milli.is_empty())
                .then_some(&state.province_annual_tax_milli),
            province_owners: (!state.province_owners.is_empty()).then_some(&state.province_owners),
            province_buildings: (!state.province_buildings.is_empty())
                .then_some(&state.province_buildings),
            province_cores: (!state.province_cores.is_empty()).then_some(&state.province_cores),
            province_discovered_by: (!state.province_discovered_by.is_empty())
                .then_some(&state.province_discovered_by),
            province_trade_goods: (!state.province_trade_goods.is_empty())
                .then_some(&state.province_trade_goods),
            province_center_of_trade_levels: (!state.province_center_of_trade_levels.is_empty())
                .then_some(&state.province_center_of_trade_levels),
            province_cultures: (!state.province_cultures.is_empty())
                .then_some(&state.province_cultures),
            province_religions: (!state.province_religions.is_empty())
                .then_some(&state.province_religions),
            province_controllers: (!state.province_controllers.is_empty())
                .then_some(&state.province_controllers),
            province_previous_controllers: (!state.province_previous_controllers.is_empty())
                .then_some(&state.province_previous_controllers),
            province_tribal_owners: (!state.province_tribal_owners.is_empty())
                .then_some(&state.province_tribal_owners),
            province_development: (!state.province_development.is_empty())
                .then_some(&state.province_development),
            province_metric_values: (!state.province_metric_values.is_empty())
                .then_some(&state.province_metric_values),
            state_edicts: None,
            province_is_city: (!state.province_is_city.is_empty())
                .then_some(&state.province_is_city),
            province_in_empire: (!state.province_in_empire.is_empty())
                .then_some(&state.province_in_empire),
            province_mothball_status: (!state.province_mothball_status.is_empty())
                .then_some(&state.province_mothball_status),
            province_raw_autonomy_milli: (!state.province_raw_autonomy_milli.is_empty())
                .then_some(&state.province_raw_autonomy_milli),
            estate_privileges: (!state.estate_privileges.is_empty())
                .then_some(&state.estate_privileges),
            estate_states: (!state.estate_states.is_empty()).then_some(&state.estate_states),
            country_markers: (!state.country_markers.is_empty()).then_some(&state.country_markers),
            country_statuses: (!state.country_statuses.is_empty())
                .then_some(&state.country_statuses),
            political_office_holders: (!state.political_office_holders.is_empty())
                .then_some(&state.political_office_holders),
            country_relation_states: (!state.country_relation_states.is_empty())
                .then_some(&state.country_relation_states),
            estate_loyalty_targets: (!state.estate_loyalty_targets.is_empty())
                .then_some(&state.estate_loyalty_targets),
            country_monthly_cashflows_milli: (!state.country_monthly_cashflows_milli.is_empty())
                .then_some(&state.country_monthly_cashflows_milli),
            country_monthly_cashflow_breakdowns: (!state
                .country_monthly_cashflow_breakdowns
                .is_empty())
            .then_some(&state.country_monthly_cashflow_breakdowns),
            country_monthly_power_income: (!state.country_monthly_power_income.is_empty())
                .then_some(&state.country_monthly_power_income),
            armies: (!state.armies.is_empty()).then_some(state.armies.as_slice()),
            naval_units: (state.naval_fleets.is_empty() && !state.naval_units.is_empty())
                .then_some(state.naval_units.as_slice()),
            naval_fleets: (!state.naval_fleets.is_empty()).then_some(state.naval_fleets.as_slice()),
            advisors: (!state.advisors.is_empty()).then_some(&state.advisors),
        }
    }
}

fn overlay_nested_map<K1, K2, V>(
    base: &mut BTreeMap<K1, BTreeMap<K2, V>>,
    overlay: &BTreeMap<K1, BTreeMap<K2, V>>,
) where
    K1: Ord + Clone,
    K2: Ord + Clone,
    V: Clone,
{
    for (key, values) in overlay {
        base.entry(key.clone()).or_default().extend(values.clone());
    }
}

fn overlay_estate_states(
    base: &mut VanillaExternalEstateStates,
    overlay: &VanillaExternalEstateStates,
) {
    for (country, estates) in overlay {
        let destination = base.entry(country.clone()).or_default();
        for (estate, values) in estates {
            let current = destination.entry(estate.clone()).or_default();
            if values.loyalty_milli.is_some() {
                current.loyalty_milli = values.loyalty_milli;
            }
            if values.land_share_micros.is_some() {
                current.land_share_micros = values.land_share_micros;
            }
            if values.active_influences.is_some() {
                current.active_influences = values.active_influences.clone();
            }
        }
    }
}

fn overlay_country_relation_states(
    base: &mut VanillaExternalCountryRelationStates,
    overlay: &VanillaExternalCountryRelationStates,
) {
    for (owner, relations) in overlay {
        let destination = base.entry(owner.clone()).or_default();
        for (target, values) in relations {
            let current = destination.entry(target.clone()).or_default();
            if values.attitude.is_some() {
                current.attitude.clone_from(&values.attitude);
            }
            if values.trust_value.is_some() {
                current.trust_value = values.trust_value;
            }
        }
    }
}

fn overlay_government_mechanic_powers(
    base: &mut VanillaExternalGovernmentMechanicPowers,
    overlay: &VanillaExternalGovernmentMechanicPowers,
) {
    for (country, mechanics) in overlay {
        let destination = base.entry(country.clone()).or_default();
        for (mechanic, powers) in mechanics {
            destination
                .entry(mechanic.clone())
                .or_default()
                .extend(powers.clone());
        }
    }
}

fn overlay_province_development(
    base: &mut BTreeMap<u32, VanillaExternalProvinceDevelopment>,
    overlay: &BTreeMap<u32, VanillaExternalProvinceDevelopment>,
) {
    for (province, values) in overlay {
        let current = base.entry(*province).or_default();
        if values.base_tax_milli.is_some() {
            current.base_tax_milli = values.base_tax_milli;
        }
        if values.base_production_milli.is_some() {
            current.base_production_milli = values.base_production_milli;
        }
        if values.base_manpower_milli.is_some() {
            current.base_manpower_milli = values.base_manpower_milli;
        }
    }
}

fn overlay_external_state(base: &mut VanillaExternalState, overlay: &VanillaExternalState) {
    overlay_nested_map(
        &mut base.country_metric_values,
        &overlay.country_metric_values,
    );
    base.country_national_focus
        .extend(overlay.country_national_focus.clone());
    base.country_capitals
        .extend(overlay.country_capitals.clone());
    base.country_original_capitals
        .extend(overlay.country_original_capitals.clone());
    for (country, history) in &overlay.country_capital_history {
        base.country_capital_history
            .entry(country.clone())
            .or_default()
            .extend(history.clone());
    }
    overlay_nested_map(&mut base.country_persons, &overlay.country_persons);
    if overlay.eu4_random_state.is_some() {
        base.eu4_random_state = overlay.eu4_random_state;
    }
    overlay_nested_map(
        &mut base.country_technology_levels,
        &overlay.country_technology_levels,
    );
    base.country_technology_groups
        .extend(overlay.country_technology_groups.clone());
    base.country_unit_types
        .extend(overlay.country_unit_types.clone());
    base.country_government_ranks
        .extend(overlay.country_government_ranks.clone());
    base.country_government_kinds
        .extend(overlay.country_government_kinds.clone());
    base.country_government_reforms
        .extend(overlay.country_government_reforms.clone());
    overlay_nested_map(
        &mut base.country_government_metric_values,
        &overlay.country_government_metric_values,
    );
    overlay_government_mechanic_powers(
        &mut base.country_government_mechanic_powers,
        &overlay.country_government_mechanic_powers,
    );
    base.country_primary_cultures
        .extend(overlay.country_primary_cultures.clone());
    base.country_religions
        .extend(overlay.country_religions.clone());
    base.country_ideas.extend(overlay.country_ideas.clone());
    base.country_mission_completions
        .extend(overlay.country_mission_completions.clone());
    base.country_variables
        .extend(overlay.country_variables.clone());
    overlay_nested_map(
        &mut base.country_metric_ceilings,
        &overlay.country_metric_ceilings,
    );
    base.country_resources
        .extend(overlay.country_resources.clone());
    base.production_inputs
        .extend(overlay.production_inputs.clone());
    base.gold_production_inputs
        .extend(overlay.gold_production_inputs.clone());
    base.production_value_inputs
        .extend(overlay.production_value_inputs.clone());
    base.trade_good_price_impacts_milli
        .extend(overlay.trade_good_price_impacts_milli.clone());
    base.trade_good_price_impact_entries_milli
        .extend(overlay.trade_good_price_impact_entries_milli.clone());
    base.trade_good_prices_milli
        .extend(overlay.trade_good_prices_milli.clone());
    overlay_nested_map(
        &mut base.trade_good_keyed_price_impacts_milli,
        &overlay.trade_good_keyed_price_impacts_milli,
    );
    for (trade_good, keys) in &overlay.unresolved_trade_good_price_impacts_milli {
        base.unresolved_trade_good_price_impacts_milli
            .entry(trade_good.clone())
            .or_default()
            .extend(keys.clone());
    }
    base.country_annual_tax_milli
        .extend(overlay.country_annual_tax_milli.clone());
    base.province_annual_tax_milli
        .extend(overlay.province_annual_tax_milli.clone());
    base.province_owners.extend(overlay.province_owners.clone());
    base.province_buildings
        .extend(overlay.province_buildings.clone());
    base.province_cores.extend(overlay.province_cores.clone());
    base.province_discovered_by
        .extend(overlay.province_discovered_by.clone());
    base.province_trade_goods
        .extend(overlay.province_trade_goods.clone());
    base.province_center_of_trade_levels
        .extend(overlay.province_center_of_trade_levels.clone());
    base.province_cultures
        .extend(overlay.province_cultures.clone());
    base.province_religions
        .extend(overlay.province_religions.clone());
    base.province_controllers
        .extend(overlay.province_controllers.clone());
    base.province_previous_controllers
        .extend(overlay.province_previous_controllers.clone());
    base.province_tribal_owners
        .extend(overlay.province_tribal_owners.clone());
    overlay_province_development(
        &mut base.province_development,
        &overlay.province_development,
    );
    overlay_nested_map(
        &mut base.province_metric_values,
        &overlay.province_metric_values,
    );
    base.province_is_city
        .extend(overlay.province_is_city.clone());
    base.province_in_empire
        .extend(overlay.province_in_empire.clone());
    base.province_mothball_status
        .extend(overlay.province_mothball_status.clone());
    base.province_raw_autonomy_milli
        .extend(overlay.province_raw_autonomy_milli.clone());
    overlay_nested_map(&mut base.estate_privileges, &overlay.estate_privileges);
    overlay_estate_states(&mut base.estate_states, &overlay.estate_states);
    base.country_markers.extend(overlay.country_markers.clone());
    overlay_nested_map(&mut base.country_statuses, &overlay.country_statuses);
    base.political_office_holders
        .extend(overlay.political_office_holders.clone());
    overlay_country_relation_states(
        &mut base.country_relation_states,
        &overlay.country_relation_states,
    );
    overlay_nested_map(
        &mut base.estate_loyalty_targets,
        &overlay.estate_loyalty_targets,
    );
    base.country_monthly_cashflows_milli
        .extend(overlay.country_monthly_cashflows_milli.clone());
    base.country_monthly_cashflow_breakdowns
        .extend(overlay.country_monthly_cashflow_breakdowns.clone());
    overlay_nested_map(
        &mut base.country_monthly_power_income,
        &overlay.country_monthly_power_income,
    );
    base.advisors.extend(overlay.advisors.clone());
    if !overlay.armies.is_empty() {
        base.armies = overlay.armies.clone();
    }
    if !overlay.naval_fleets.is_empty() {
        base.naval_fleets = overlay.naval_fleets.clone();
        base.naval_units = overlay.naval_units.clone();
    } else if !overlay.naval_units.is_empty() {
        base.naval_fleets.clear();
        base.naval_units = overlay.naval_units.clone();
    }
}

fn bind_external_units(
    initial: &mut InitialWorld,
    armies: Option<&[VanillaExternalArmy]>,
    naval_units: Option<&[VanillaExternalNavalUnit]>,
    naval_fleets: Option<&[VanillaExternalFleet]>,
) -> Result<(), Error> {
    if let Some(armies) = armies {
        for army in armies {
            let owner = initial.country_entity(&army.owner).ok_or_else(|| {
                Error::message(
                    "external armies",
                    format!("unknown army owner {}", army.owner),
                )
            })?;
            let location = initial.province_entity(army.location).ok_or_else(|| {
                Error::message(
                    "external armies",
                    format!("unknown army location {}", army.location),
                )
            })?;
            if !army.regiment_states.is_empty()
                && army.regiment_states.len() != army.regiments.len()
            {
                return Err(Error::message(
                    "external armies",
                    "army regiment state count does not match regiment count",
                ));
            }
            let states = army
                .regiment_states
                .iter()
                .map(|state| {
                    let home_province = state
                        .home_province
                        .map(|province| {
                            initial.province_entity(province).ok_or_else(|| {
                                Error::message(
                                    "external armies",
                                    format!("unknown regiment home province {province}"),
                                )
                            })
                        })
                        .transpose()?;
                    Ok(augustus_core::RegimentState {
                        name: state.name.clone(),
                        strength_milli: state.strength_milli,
                        morale_milli: state.morale_milli,
                        home_province,
                    })
                })
                .collect::<Result<Vec<_>, Error>>()?;
            initial
                .create_army_with_regiment_states(
                    owner,
                    location,
                    &army.name,
                    &army.regiments,
                    &states,
                )
                .map_err(|error| Error::caused_by("external armies", error))?;
        }
    }
    if let Some(naval_fleets) = naval_fleets {
        for fleet in naval_fleets {
            let owner = initial.country_entity(&fleet.owner).ok_or_else(|| {
                Error::message(
                    "external fleets",
                    format!("unknown fleet owner {}", fleet.owner),
                )
            })?;
            let location = initial.province_entity(fleet.location).ok_or_else(|| {
                Error::message(
                    "external fleets",
                    format!("unknown fleet location {}", fleet.location),
                )
            })?;
            if !fleet.ship_states.is_empty() && fleet.ship_states.len() != fleet.ship_types.len() {
                return Err(Error::message(
                    "external fleets",
                    "fleet ship state count does not match ship count",
                ));
            }
            if fleet.ship_states.is_empty() {
                initial.create_fleet(owner, location, fleet.name.clone(), &fleet.ship_types)
            } else {
                let states = fleet
                    .ship_states
                    .iter()
                    .map(|state| translated_ship_state(initial, state, "external fleets"))
                    .collect::<Result<Vec<_>, _>>()?;
                initial.create_fleet_with_ship_states(
                    owner,
                    location,
                    fleet.name.clone(),
                    &fleet.ship_types,
                    &states,
                )
            }
            .map_err(|error| Error::caused_by("external fleets", error))?;
        }
    } else if let Some(naval_units) = naval_units {
        for unit in naval_units {
            let owner = initial.country_entity(&unit.owner).ok_or_else(|| {
                Error::message(
                    "external naval units",
                    format!("unknown naval unit owner {}", unit.owner),
                )
            })?;
            let location = initial.province_entity(unit.location).ok_or_else(|| {
                Error::message(
                    "external naval units",
                    format!("unknown naval unit location {}", unit.location),
                )
            })?;
            if let Some(state) = &unit.state {
                let state = translated_ship_state(initial, state, "external naval units")?;
                initial.create_fleet_with_ship_states(
                    owner,
                    location,
                    None,
                    std::slice::from_ref(&unit.unit_type),
                    std::slice::from_ref(&state),
                )
            } else {
                initial.create_fleet(owner, location, None, std::slice::from_ref(&unit.unit_type))
            }
            .map_err(|error| Error::caused_by("external naval units", error))?;
        }
    }
    Ok(())
}

fn translated_ship_state(
    initial: &InitialWorld,
    state: &VanillaExternalShipState,
    source: &str,
) -> Result<augustus_core::NavalUnitState, Error> {
    let home_province = state
        .home_province
        .map(|province| {
            initial.province_entity(province).ok_or_else(|| {
                Error::message(source, format!("unknown ship home province {province}"))
            })
        })
        .transpose()?;
    Ok(augustus_core::NavalUnitState {
        name: state.name.clone(),
        morale_milli: state.morale_milli,
        home_province,
    })
}

fn bind_external_advisors(
    initial: &mut InitialWorld,
    values: Option<&BTreeMap<String, Vec<VanillaExternalAdvisor>>>,
    definitions: &BTreeMap<String, augustus_core::RulerSkill>,
) -> Result<(), Error> {
    let Some(values) = values else {
        return Ok(());
    };
    let mut advisors = initial.world().advisors().clone();
    for (country, entries) in values {
        let entity = initial.country_entity(country).ok_or_else(|| {
            Error::message(
                "external advisors",
                format!("unknown advisor country {country}"),
            )
        })?;
        let translated = entries
            .iter()
            .map(|advisor| {
                let category = definitions.get(&advisor.type_name).ok_or_else(|| {
                    Error::message(
                        "external advisors",
                        format!("unknown advisor type {}", advisor.type_name),
                    )
                })?;
                Ok(augustus_core::AdvisorState {
                    type_name: advisor.type_name.clone(),
                    category: *category,
                    skill: advisor.skill,
                    cost_multiplier_milli: None,
                })
            })
            .collect::<Result<Vec<_>, Error>>()?;
        advisors.countries.insert(entity, Some(translated));
    }
    initial
        .set_advisors(advisors)
        .map_err(|e| Error::caused_by("external advisors", e))
}

fn bind_external_estate_privileges(
    initial: &mut InitialWorld,
    values: Option<&VanillaExternalEstatePrivileges>,
) -> Result<(), Error> {
    let Some(values) = values else {
        return Ok(());
    };
    let mut estates = initial.world().estates().clone();
    for (country, estate_values) in values {
        let entity = initial.country_entity(country).ok_or_else(|| {
            Error::message(
                "external estate privileges",
                format!("unknown EU4 estate country {country}"),
            )
        })?;
        let states = estates.countries.get_mut(&entity).ok_or_else(|| {
            Error::message(
                "external estate privileges",
                format!("country {country} has no estate state"),
            )
        })?;
        for (estate, privileges) in estate_values {
            let state = states.get_mut(estate).ok_or_else(|| {
                Error::message(
                    "external estate privileges",
                    format!("country {country} has no estate {estate}"),
                )
            })?;
            state.privileges = privileges.clone();
        }
    }
    initial
        .set_estates(estates)
        .map_err(|e| Error::caused_by("external estate privileges", e))
}

fn bind_external_estate_states(
    initial: &mut InitialWorld,
    values: Option<&VanillaExternalEstateStates>,
) -> Result<(), Error> {
    let Some(values) = values else {
        return Ok(());
    };
    let mut estates = initial.world().estates().clone();
    for (country, estate_values) in values {
        let entity = initial.country_entity(country).ok_or_else(|| {
            Error::message(
                "external estate state",
                format!("unknown EU4 estate country {country}"),
            )
        })?;
        let states = estates.countries.get_mut(&entity).ok_or_else(|| {
            Error::message(
                "external estate state",
                format!("country {country} has no estate state"),
            )
        })?;
        for (estate, values) in estate_values {
            let state = states.get_mut(estate).ok_or_else(|| {
                Error::message(
                    "external estate state",
                    format!("country {country} has no estate {estate}"),
                )
            })?;
            if let Some(loyalty) = values.loyalty_milli {
                state.loyalty_milli = Some(loyalty);
            }
            if let Some(land_share) = values.land_share_micros {
                state.land_share_micros = Some(land_share);
            }
        }
    }
    initial
        .set_estates(estates)
        .map_err(|e| Error::caused_by("external estate state", e))
}

fn bind_external_country_markers(
    initial: &mut InitialWorld,
    values: Option<&VanillaExternalCountryMarkers>,
) -> Result<(), Error> {
    let Some(values) = values else {
        return Ok(());
    };
    let mut markers = initial.world().markers().clone();
    for (country, names) in values {
        let entity = initial.country_entity(country).ok_or_else(|| {
            Error::message(
                "external country markers",
                format!("unknown EU4 marker country {country}"),
            )
        })?;
        match names {
            Some(names) => {
                markers.entities.insert(
                    entity,
                    names.iter().map(|name| (name.clone(), true)).collect(),
                );
                markers.unknown_entities.remove(&entity);
            }
            None => markers.mark_unknown(entity),
        }
    }
    initial
        .set_markers(markers)
        .map_err(|e| Error::caused_by("external country markers", e))
}

fn bind_external_province_in_empire(
    initial: &mut InitialWorld,
    values: Option<&BTreeMap<u32, Option<bool>>>,
) -> Result<(), Error> {
    let Some(values) = values else {
        return Ok(());
    };
    let mut affiliations = initial.world().affiliations().clone();
    let dimension = affiliations
        .0
        .entry(crate::imperial_membership::DIMENSION.into())
        .or_default();
    dimension.groups.entry("hre".into()).or_insert(None);
    for (province, value) in values {
        let entity = initial.province_entity(*province).ok_or_else(|| {
            Error::message(
                "external HRE membership",
                format!("unknown EU4 HRE province {province}"),
            )
        })?;
        dimension.members.remove(&entity);
        dimension.absent_members.remove(&entity);
        match value {
            Some(true) => {
                dimension.members.insert(entity, Some("hre".into()));
            }
            Some(false) => {
                dimension.absent_members.insert(entity);
            }
            None => {}
        }
    }
    initial
        .set_affiliations(affiliations)
        .map_err(|e| Error::caused_by("external HRE membership", e))
}

fn bind_external_country_metrics(
    initial: &mut InitialWorld,
    values: Option<&BTreeMap<String, BTreeMap<String, Option<i64>>>>,
    ceilings: Option<&BTreeMap<String, BTreeMap<String, Option<i64>>>>,
) -> Result<(), Error> {
    if let Some(values) = values {
        let bound = values
            .iter()
            .map(|(country, metrics)| {
                let entity = initial.country_entity(country).ok_or_else(|| {
                    Error::message(
                        "external country metrics",
                        format!("unknown EU4 metric country {country}"),
                    )
                })?;
                Ok((entity, metrics.clone()))
            })
            .collect::<Result<BTreeMap<_, _>, Error>>()?;
        initial
            .set_metric_values(bound)
            .map_err(|e| Error::caused_by("external country metrics", e))?;
    }
    if let Some(ceilings) = ceilings {
        let bound = ceilings
            .iter()
            .map(|(country, metrics)| {
                let entity = initial.country_entity(country).ok_or_else(|| {
                    Error::message(
                        "external country metric ceilings",
                        format!("unknown EU4 metric ceiling country {country}"),
                    )
                })?;
                Ok((entity, metrics.clone()))
            })
            .collect::<Result<BTreeMap<_, _>, Error>>()?;
        initial
            .set_metric_ceilings(bound)
            .map_err(|e| Error::caused_by("external country metric ceilings", e))?;
    }
    Ok(())
}

fn bind_external_province_metrics(
    initial: &mut InitialWorld,
    values: Option<&BTreeMap<u32, BTreeMap<String, Option<i64>>>>,
) -> Result<(), Error> {
    let Some(values) = values else {
        return Ok(());
    };
    let bound = values
        .iter()
        .map(|(province, metrics)| {
            let entity = initial.province_entity(*province).ok_or_else(|| {
                Error::message(
                    "external province metrics",
                    format!("unknown EU4 metric province {province}"),
                )
            })?;
            Ok((entity, metrics.clone()))
        })
        .collect::<Result<BTreeMap<_, _>, Error>>()?;
    initial
        .set_metric_values(bound)
        .map_err(|e| Error::caused_by("external province metrics", e))
}

fn bind_external_state_edicts(
    initial: &mut InitialWorld,
    values: Option<&VanillaExternalStateEdicts>,
    report: &mut SetupReport,
) -> Result<(), Error> {
    let Some(values) = values else {
        return Ok(());
    };
    let mut modifiers = initial.world().modifiers().clone();
    for (country, areas) in values {
        let Some(owner) = initial.country_entity(country) else {
            for area in areas.keys() {
                report
                    .unapplied_definition_fields
                    .entry(format!("state_edict:{country}:{area}:unknown_country"))
                    .and_modify(|count| *count += 1)
                    .or_insert(1);
            }
            continue;
        };
        for (area, edict) in areas {
            let mut provinces = Vec::new();
            for province in initial.provinces() {
                let Some(entity) = initial.province_entity(province.id) else {
                    continue;
                };
                if initial
                    .world()
                    .entity(entity)
                    .map_err(|error| Error::caused_by("external state edicts", error))?
                    .owner
                    != Some(owner)
                {
                    continue;
                }
                let province_area = initial
                    .world()
                    .administration()
                    .provinces
                    .get(&entity)
                    .and_then(|state| state.area.as_deref());
                if province_area == Some(area.as_str()) {
                    provinces.push(entity);
                }
            }
            if provinces.is_empty() {
                report
                    .unapplied_definition_fields
                    .entry(format!("state_edict:{country}:{area}:no_owned_provinces"))
                    .and_modify(|count| *count += 1)
                    .or_insert(1);
                continue;
            }

            let modifier_name = crate::state_edicts::modifier_identifier(&edict.identifier);
            if !modifiers.definitions.contains_key(&modifier_name) {
                modifiers
                    .definitions
                    .insert(modifier_name.clone(), ModifierDefinition::default());
                for province in provinces {
                    modifiers
                        .unresolved
                        .entry(province)
                        .or_default()
                        .insert(modifier_name.clone());
                }
                report
                    .unapplied_definition_fields
                    .entry(format!(
                        "state_edict:{}:unknown_active_definition",
                        edict.identifier
                    ))
                    .and_modify(|count| *count += 1)
                    .or_insert(1);
                continue;
            }

            for province in provinces {
                modifiers
                    .active
                    .entry(province)
                    .or_default()
                    .insert(modifier_name.clone(), ModifierInstance::default());
            }
        }
    }
    initial
        .set_modifiers(modifiers)
        .map_err(|e| Error::caused_by("external state edicts", e))
}

fn bind_province_discoveries(
    initial: &InitialWorld,
    history: &BTreeMap<u32, BTreeSet<String>>,
    saved: Option<&BTreeMap<u32, BTreeSet<String>>>,
    technology_groups: &BTreeSet<String>,
    report: &mut BootstrapReport,
) -> Result<augustus_core::MapKnowledge, Error> {
    let countries = initial
        .countries()
        .iter()
        .map(|country| {
            initial
                .country_entity(&country.id)
                .expect("registered country")
        })
        .collect::<BTreeSet<_>>();
    let mut countries_by_group = BTreeMap::<String, BTreeSet<augustus_core::EntityId>>::new();
    for country in initial.countries() {
        if let Some(group) = &country.technology_group {
            countries_by_group.entry(group.clone()).or_default().insert(
                initial
                    .country_entity(&country.id)
                    .expect("registered country"),
            );
        }
    }

    let province_ids = history
        .keys()
        .chain(saved.into_iter().flat_map(BTreeMap::keys))
        .copied()
        .collect::<BTreeSet<_>>();
    let mut map_knowledge = augustus_core::MapKnowledge::default();
    for id in province_ids {
        let province = initial.province_entity(id).ok_or_else(|| {
            Error::message(
                "province map knowledge",
                format!("unknown EU4 discovery province {id}"),
            )
        })?;
        let saved_discoveries = saved.and_then(|values| values.get(&id));
        let identifiers = saved_discoveries.or_else(|| history.get(&id));
        let mut discovery = augustus_core::ProvinceDiscovery::default();
        let mut unresolved_identifier = false;
        if let Some(identifiers) = identifiers {
            for identifier in identifiers {
                if saved_discoveries.is_some() {
                    if let Some(country) = initial.country_entity(identifier) {
                        discovery.discovered_by.insert(country);
                    } else {
                        unresolved_identifier = true;
                        *report
                            .unresolved_discovery_identifiers
                            .entry(identifier.clone())
                            .or_default() += 1;
                    }
                } else if let Some(country) = initial.country_entity(identifier) {
                    discovery.discovered_by.insert(country);
                } else if technology_groups.contains(identifier) {
                    if let Some(group_countries) = countries_by_group.get(identifier) {
                        discovery
                            .discovered_by
                            .extend(group_countries.iter().copied());
                    }
                } else {
                    unresolved_identifier = true;
                    *report
                        .unresolved_discovery_identifiers
                        .entry(identifier.clone())
                        .or_default() += 1;
                }
            }
        }
        if unresolved_identifier {
            discovery
                .unresolved_countries
                .extend(countries.difference(&discovery.discovered_by).copied());
        }
        map_knowledge.provinces.insert(province, discovery);
    }
    report.province_discovery_states = map_knowledge.provinces.len();
    Ok(map_knowledge)
}

fn bind_external_country_statuses(
    initial: &mut InitialWorld,
    values: Option<&VanillaExternalCountryStatuses>,
) -> Result<(), Error> {
    let Some(values) = values else {
        return Ok(());
    };
    let mut politics = initial.world().politics().clone();
    for (country, statuses) in values {
        let entity = initial.country_entity(country).ok_or_else(|| {
            Error::message(
                "external country statuses",
                format!("unknown EU4 status country {country}"),
            )
        })?;
        let state = politics.countries.entry(entity).or_default();
        for (name, value) in statuses {
            if name.is_empty() {
                return Err(Error::message(
                    "external country statuses",
                    format!("country {country} has an empty status name"),
                ));
            }
            state.statuses.insert(name.clone(), *value);
        }
    }
    initial
        .set_politics(politics)
        .map_err(|e| Error::caused_by("external country statuses", e))
}

fn bind_external_political_office_holders(
    initial: &mut InitialWorld,
    values: Option<&BTreeMap<String, Option<String>>>,
) -> Result<(), Error> {
    let Some(values) = values else {
        return Ok(());
    };
    let mut politics = initial.world().politics().clone();
    for (office, holder) in values {
        if office.is_empty() {
            return Err(Error::message(
                "external political offices",
                "political office name is empty",
            ));
        }
        if let Some(holder) = holder {
            let entity = initial.country_entity(holder).ok_or_else(|| {
                Error::message(
                    "external political offices",
                    format!("unknown holder {holder} for office {office}"),
                )
            })?;
            politics.offices.insert(office.clone(), entity);
        } else {
            politics.offices.remove(office);
        }
    }
    initial
        .set_politics(politics)
        .map_err(|error| Error::caused_by("external political offices", error))
}

fn bind_external_country_relation_states(
    initial: &mut InitialWorld,
    values: Option<&VanillaExternalCountryRelationStates>,
) -> Result<(), Error> {
    let Some(values) = values else {
        return Ok(());
    };
    let mut states: BTreeMap<
        augustus_core::EntityId,
        BTreeMap<augustus_core::EntityId, augustus_core::CountryRelationState>,
    > = BTreeMap::new();
    for (owner, relations) in values {
        let owner_entity = initial.country_entity(owner).ok_or_else(|| {
            Error::message(
                "external country relations",
                format!("unknown relation owner {owner}"),
            )
        })?;
        for (target, values) in relations {
            let target_entity = initial.country_entity(target).ok_or_else(|| {
                Error::message(
                    "external country relations",
                    format!("unknown relation target {target}"),
                )
            })?;
            states
                .entry(owner_entity)
                .or_default()
                .entry(target_entity)
                .or_insert_with(|| values.clone());
        }
    }
    initial
        .set_country_relation_states(states)
        .map_err(|e| Error::caused_by("external country relations", e))
}

fn bind_external_country_variables(
    initial: &mut InitialWorld,
    values: Option<&BTreeMap<String, BTreeMap<String, i64>>>,
) -> Result<(), Error> {
    let Some(values) = values else {
        return Ok(());
    };
    let mut variables = initial.world().named_values().clone();
    for (country, state) in values {
        let entity = initial.country_entity(country).ok_or_else(|| {
            Error::message(
                "external country variables",
                format!("unknown country {country}"),
            )
        })?;
        variables.entities.insert(entity, state.clone());
        variables.unknown_entities.remove(&entity);
    }
    initial
        .set_named_values(variables)
        .map_err(|e| Error::caused_by("external country variables", e))
}

fn bind_external_national_focus(
    initial: &mut InitialWorld,
    values: Option<&BTreeMap<String, augustus_core::NationalFocus>>,
) -> Result<(), Error> {
    let Some(values) = values else {
        return Ok(());
    };
    let mut politics = initial.world().politics().clone();
    for (country, focus) in values {
        let entity = initial.country_entity(country).ok_or_else(|| {
            Error::message(
                "external national focus",
                format!("unknown EU4 focus country {country}"),
            )
        })?;
        politics.countries.entry(entity).or_default().national_focus = Some(*focus);
    }
    initial
        .set_politics(politics)
        .map_err(|e| Error::caused_by("external national focus", e))
}

fn bind_external_country_persons(
    initial: &mut InitialWorld,
    values: Option<&VanillaExternalCountryPersons>,
) -> Result<(), Error> {
    let Some(values) = values else {
        return Ok(());
    };
    let mut politics = initial.world().politics().clone();
    for (country, persons) in values {
        let entity = initial.country_entity(country).ok_or_else(|| {
            Error::message(
                "external country persons",
                format!("unknown country {country}"),
            )
        })?;
        let state = politics.countries.entry(entity).or_default();
        for (role, person) in persons {
            let destination = match role.as_str() {
                "ruler" => &mut state.ruler,
                "heir" => &mut state.heir,
                "consort" => &mut state.consort,
                _ => {
                    return Err(Error::message(
                        "external country persons",
                        format!("unknown person role {role} for {country}"),
                    ));
                }
            };
            *destination = person
                .as_ref()
                .map(|external_person| {
                    let mut person = external_person.state.clone();
                    if let Some(origin) = &external_person.country_of_origin {
                        let origin_entity = initial.country_entity(origin).ok_or_else(|| {
                            Error::message(
                                "external country persons",
                                format!("unknown origin country {origin} for {country} {role}"),
                            )
                        })?;
                        person.country_of_origin = Some(origin_entity);
                    }
                    Ok(person)
                })
                .transpose()?;
        }
    }
    initial
        .set_politics(politics)
        .map_err(|error| Error::caused_by("external country persons", error))
}

fn bind_external_trade_good_prices(
    initial: &mut InitialWorld,
    values: Option<&BTreeMap<String, Option<i64>>>,
) -> Result<(), Error> {
    let Some(values) = values else {
        return Ok(());
    };
    let mut economy = initial.world().economy().clone();
    for (name, price) in values {
        if !economy.trade_goods.contains_key(name) {
            return Err(Error::message(
                "external trade-good prices",
                format!("unknown EU4 trade good {name}"),
            ));
        }
        economy
            .current_trade_good_prices_milli
            .insert(name.clone(), *price);
    }
    initial
        .set_economy(economy)
        .map_err(|e| Error::caused_by("external trade-good prices", e))
}

fn bind_external_trade_good_price_impacts(
    initial: &mut InitialWorld,
    totals: Option<&BTreeMap<u32, i32>>,
    entries: Option<&BTreeMap<u32, Vec<i32>>>,
    keyed: Option<&BTreeMap<String, BTreeMap<String, i32>>>,
    unresolved: Option<&BTreeMap<String, BTreeSet<String>>>,
) -> Result<(), Error> {
    if totals.is_none() && entries.is_none() && keyed.is_none() && unresolved.is_none() {
        return Ok(());
    }
    let mut economy = initial.world().economy().clone();
    if let Some(entries) = entries {
        for (&index, values) in entries {
            let Some((name, _)) = economy.trade_good_by_runtime_index(index) else {
                return Err(Error::message(
                    "external trade-good price impacts",
                    format!("unknown EU4 trade-good index {index}"),
                ));
            };
            economy
                .external_trade_good_price_impacts_milli
                .insert(name.into(), values.clone());
        }
    }
    if let Some(totals) = totals {
        for (&index, value) in totals {
            if entries.is_some_and(|entries| entries.contains_key(&index)) {
                continue;
            }
            let Some((name, _)) = economy.trade_good_by_runtime_index(index) else {
                return Err(Error::message(
                    "external trade-good price impacts",
                    format!("unknown EU4 trade-good index {index}"),
                ));
            };
            economy
                .external_trade_good_price_impacts_milli
                .insert(name.into(), vec![*value]);
        }
    }
    if let Some(keyed) = keyed {
        for (name, values) in keyed {
            if !economy.trade_goods.contains_key(name) {
                return Err(Error::message(
                    "external trade-good price impacts",
                    format!("unknown EU4 trade good {name}"),
                ));
            }
            economy
                .keyed_trade_good_price_impacts_milli
                .entry(name.clone())
                .or_default()
                .extend(values.clone());
        }
    }
    if let Some(unresolved) = unresolved {
        for (name, keys) in unresolved {
            if !economy.trade_goods.contains_key(name) {
                return Err(Error::message(
                    "external trade-good price impacts",
                    format!("unknown EU4 trade good {name}"),
                ));
            }
            economy
                .unresolved_trade_good_price_impacts_milli
                .entry(name.clone())
                .or_default()
                .extend(keys.clone());
        }
    }
    initial
        .set_economy(economy)
        .map_err(|error| Error::caused_by("external trade-good price impacts", error))
}

fn bind_external_province_buildings(
    initial: &mut InitialWorld,
    values: Option<&BTreeMap<u32, BTreeSet<String>>>,
) -> Result<(), Error> {
    let Some(values) = values else {
        return Ok(());
    };
    let mut economy = initial.world().economy().clone();
    for (province, buildings) in values {
        let entity = initial.province_entity(*province).ok_or_else(|| {
            Error::message(
                "external province buildings",
                format!("unknown province {province}"),
            )
        })?;
        for building in buildings {
            if !economy.buildings.contains_key(building) {
                return Err(Error::message(
                    "external province buildings",
                    format!("province {province} references unknown building {building}"),
                ));
            }
        }
        economy
            .provinces
            .entry(entity)
            .or_default()
            .buildings
            .clone_from(buildings);
    }
    initial
        .set_economy(economy)
        .map_err(|error| Error::caused_by("external province buildings", error))
}

fn bind_external_province_cores(
    initial: &InitialWorld,
    administration: &mut augustus_core::Administration,
    values: Option<&BTreeMap<u32, Vec<String>>>,
) -> Result<(), Error> {
    let Some(values) = values else {
        return Ok(());
    };
    for (province, tags) in values {
        let entity = initial.province_entity(*province).ok_or_else(|| {
            Error::message(
                "external province cores",
                format!("unknown province {province}"),
            )
        })?;
        let Some(state) = administration.provinces.get_mut(&entity) else {
            if tags.is_empty() {
                continue;
            }
            return Err(Error::message(
                "external province cores",
                format!("province {province} has no administration state"),
            ));
        };
        let mut cores = BTreeMap::new();
        for tag in tags {
            let country = initial.country_entity(tag).ok_or_else(|| {
                Error::message(
                    "external province cores",
                    format!("province {province} references unknown core country {tag}"),
                )
            })?;
            cores.insert(country, augustus_core::CoreType::Unspecified);
        }
        state.cores = cores;
        state.cores_known = true;
    }
    Ok(())
}

struct ExternalProductionValueBindings {
    states: BTreeMap<augustus_core::EntityId, augustus_core::VanillaProductionInputState>,
    price_inputs: BTreeMap<augustus_core::EntityId, VanillaProductionBaseValueInput>,
}

fn bind_external_production_value_inputs(
    initial: &InitialWorld,
    inputs: &BTreeMap<u32, VanillaExternalProductionInput>,
    price_impacts: Option<&BTreeMap<u32, i32>>,
    price_impact_entries: Option<&BTreeMap<u32, Vec<i32>>>,
) -> Result<ExternalProductionValueBindings, Error> {
    let economy = initial.world().economy();
    let mut states = BTreeMap::new();
    let mut price_inputs = BTreeMap::new();
    for (province, source) in inputs {
        let entity = initial.province_entity(*province).ok_or_else(|| {
            Error::message(
                "external production value inputs",
                format!("unknown EU4 production province {province}"),
            )
        })?;
        let trade_good = economy
            .provinces
            .get(&entity)
            .and_then(|state| state.trade_good.as_deref());
        let definition = trade_good.and_then(|name| economy.trade_goods.get(name));
        let price = if let Some(current) =
            trade_good.and_then(|name| economy.current_trade_good_prices_milli.get(name))
        {
            current
                    .as_ref()
                    .map(|price| {
                        i32::try_from(*price).map_err(|_| {
                            Error::message(
                                "external production value inputs",
                                format!(
                                    "trade-good current price for province {province} is outside the production range"
                                ),
                            )
                        })
                    })
                    .transpose()?
        } else if let Some(base_price) =
            definition.and_then(|definition| definition.base_price_milli)
        {
            let base_price = i32::try_from(base_price).map_err(|_| {
                    Error::message(
                        "external production value inputs",
                        format!(
                            "trade-good base price for province {province} is outside the production range"
                        ),
                    )
                })?;
            let runtime_index = definition.and_then(|definition| definition.runtime_index);
            let price = runtime_index
                .and_then(|index| {
                    price_impact_entries
                        .and_then(|entries| entries.get(&index))
                        .map(|entries| {
                            calculate_vanilla_trade_good_price_table(
                                VanillaTradeGoodPriceTableInput {
                                    base_price_milli: base_price,
                                    price_impacts_milli: entries.clone(),
                                },
                            )
                            .effective_price_milli
                        })
                        .or_else(|| {
                            price_impacts
                                .and_then(|impacts| impacts.get(&index))
                                .copied()
                                .map(|price_impact_milli| {
                                    calculate_vanilla_trade_good_price(VanillaTradeGoodPriceInput {
                                        base_price_milli: base_price,
                                        price_impact_milli,
                                    })
                                    .effective_price_milli
                                })
                        })
                })
                .unwrap_or_else(|| {
                    calculate_vanilla_trade_good_price(VanillaTradeGoodPriceInput {
                        base_price_milli: base_price,
                        price_impact_milli: 0,
                    })
                    .effective_price_milli
                });
            Some(price)
        } else {
            None
        };
        let state = match price {
            Some(price) => {
                let price_input = VanillaProductionBaseValueInput {
                    goods_size_milli: source.goods_size_milli,
                    trade_good_price_milli: price,
                    flat_modifier_milli: source.flat_modifier_milli,
                    external_flat_modifier_milli: source.external_flat_modifier_milli,
                    rate_modifier_milli: source.rate_modifier_milli,
                    external_rate_modifier_milli: source.external_rate_modifier_milli,
                };
                let value = calculate_vanilla_production_base_value(price_input);
                price_inputs.insert(entity, price_input);
                augustus_core::VanillaProductionInputState {
                    input: Some(VanillaProductionInput {
                        base_value_milli: value.value_milli,
                        country_modifier_milli: source.country_modifier_milli,
                        legacy_province_modifier_milli: source.legacy_province_modifier_milli,
                        queried_province_modifier_milli: source.queried_province_modifier_milli,
                    }),
                    unresolved: Default::default(),
                }
            }
            None => augustus_core::VanillaProductionInputState {
                input: None,
                unresolved: ["missing_trade_good_base_price".into()].into(),
            },
        };
        states.insert(entity, state);
    }
    Ok(ExternalProductionValueBindings {
        states,
        price_inputs,
    })
}

fn bind_vanilla_gold_production_inputs(
    initial: &InitialWorld,
    inputs: &BTreeMap<u32, VanillaGoldProductionInputState>,
) -> Result<BTreeMap<augustus_core::EntityId, VanillaGoldProductionInputState>, Error> {
    for &province in inputs.keys() {
        let entity = initial.province_entity(province).ok_or_else(|| {
            Error::message(
                "external gold production inputs",
                format!("unknown EU4 gold province {province}"),
            )
        })?;
        let good = initial
            .world()
            .economy()
            .provinces
            .get(&entity)
            .and_then(|state| state.trade_good.as_ref());
        let is_gold = good
            .and_then(|name| initial.world().economy().trade_goods.get(name))
            .is_some_and(|definition| definition.gold_type == Some(true));
        if !is_gold {
            return Err(Error::message(
                "external gold production inputs",
                format!("EU4 province {province} is not a gold province"),
            ));
        }
    }

    let mut bound = BTreeMap::new();
    for province in initial.provinces() {
        let Some(entity) = initial.province_entity(province.id) else {
            continue;
        };
        let good = initial
            .world()
            .economy()
            .provinces
            .get(&entity)
            .and_then(|state| state.trade_good.as_ref());
        let is_gold = good
            .and_then(|name| initial.world().economy().trade_goods.get(name))
            .is_some_and(|definition| definition.gold_type == Some(true));
        if is_gold {
            bound.insert(
                entity,
                inputs
                    .get(&province.id)
                    .cloned()
                    .unwrap_or(VanillaGoldProductionInputState {
                        input: None,
                        unresolved: BTreeSet::new(),
                    }),
            );
        }
    }
    Ok(bound)
}

impl VanillaWorld {
    /// Loads the supported initial-state fields and selects calendar/tax rules.
    /// Final-edition rules always target all expansion mechanics, independently of launcher switches.
    pub fn load(path: impl AsRef<Path>, date: &str) -> Result<Self, Error> {
        Self::load_session(path, date, None, 0, None, LoadOverrides::default())
    }

    pub fn load_with_seed(path: impl AsRef<Path>, date: &str, seed: u32) -> Result<Self, Error> {
        Self::load_session(path, date, None, seed, Some(seed), LoadOverrides::default())
    }

    /// Loads a world with externally supplied country metric values bound before
    /// startup events execute.
    pub fn load_with_country_metric_values(
        path: impl AsRef<Path>,
        date: &str,
        values: &BTreeMap<String, BTreeMap<String, Option<i64>>>,
    ) -> Result<Self, Error> {
        Self::load_session(
            path,
            date,
            None,
            0,
            None,
            LoadOverrides {
                country_metric_values: Some(values),
                ..Default::default()
            },
        )
    }

    /// Loads a world with externally supplied country resources bound before
    /// startup events execute.
    pub fn load_with_country_resources(
        path: impl AsRef<Path>,
        date: &str,
        resources: &BTreeMap<String, CountryResources>,
    ) -> Result<Self, Error> {
        Self::load_session(
            path,
            date,
            None,
            0,
            None,
            LoadOverrides {
                country_resources: Some(resources),
                ..Default::default()
            },
        )
    }

    /// Loads a world with translated country-ledger state bound before
    /// startup effects execute. Ledger category meanings remain owned by the
    /// compatibility provider; this method only resolves country identifiers.
    pub fn load_with_country_ledgers(
        path: impl AsRef<Path>,
        date: &str,
        ledgers: &BTreeMap<String, CountryLedger>,
    ) -> Result<Self, Error> {
        Self::load_session(
            path,
            date,
            None,
            0,
            None,
            LoadOverrides {
                country_ledgers: Some(ledgers),
                ..Default::default()
            },
        )
    }

    /// Loads a world with state translated by an external simulation or save
    /// reader before startup events execute.
    pub fn load_with_external_state(
        path: impl AsRef<Path>,
        date: &str,
        state: &VanillaExternalState,
    ) -> Result<Self, Error> {
        Self::load_session(
            path,
            date,
            None,
            0,
            None,
            LoadOverrides::from_external_state(state),
        )
    }

    /// A supplied roster defines human countries; the remaining countries use AI control.
    /// An empty roster explicitly requests an AI-only session.
    pub fn load_with_players(
        path: impl AsRef<Path>,
        date: &str,
        players: &std::collections::BTreeSet<String>,
    ) -> Result<Self, Error> {
        Self::load_session(path, date, Some(players), 0, None, LoadOverrides::default())
    }

    pub fn load_with_players_and_seed(
        path: impl AsRef<Path>,
        date: &str,
        players: &std::collections::BTreeSet<String>,
        seed: u32,
    ) -> Result<Self, Error> {
        Self::load_session(
            path,
            date,
            Some(players),
            seed,
            Some(seed),
            LoadOverrides::default(),
        )
    }

    /// Loads an initial world through the resolved EU4 virtual content order.
    pub fn load_from_content(content: &ContentSet, date: &str) -> Result<Self, Error> {
        Self::load_from_view(
            content,
            content.base_root(),
            date,
            None,
            0,
            None,
            LoadOverrides::default(),
        )
    }

    pub fn load_from_content_with_seed(
        content: &ContentSet,
        date: &str,
        seed: u32,
    ) -> Result<Self, Error> {
        Self::load_from_view(
            content,
            content.base_root(),
            date,
            None,
            seed,
            Some(seed),
            LoadOverrides::default(),
        )
    }

    /// Loads resolved content with externally supplied country metric values
    /// bound before startup events execute.
    pub fn load_from_content_with_country_metric_values(
        content: &ContentSet,
        date: &str,
        values: &BTreeMap<String, BTreeMap<String, Option<i64>>>,
    ) -> Result<Self, Error> {
        Self::load_from_view(
            content,
            content.base_root(),
            date,
            None,
            0,
            None,
            LoadOverrides {
                country_metric_values: Some(values),
                ..Default::default()
            },
        )
    }

    /// Loads resolved content with externally supplied country resources bound
    /// before startup events execute.
    pub fn load_from_content_with_country_resources(
        content: &ContentSet,
        date: &str,
        resources: &BTreeMap<String, CountryResources>,
    ) -> Result<Self, Error> {
        Self::load_from_view(
            content,
            content.base_root(),
            date,
            None,
            0,
            None,
            LoadOverrides {
                country_resources: Some(resources),
                ..Default::default()
            },
        )
    }

    /// Content-set variant of [`VanillaWorld::load_with_country_ledgers`].
    pub fn load_from_content_with_country_ledgers(
        content: &ContentSet,
        date: &str,
        ledgers: &BTreeMap<String, CountryLedger>,
    ) -> Result<Self, Error> {
        Self::load_from_view(
            content,
            content.base_root(),
            date,
            None,
            0,
            None,
            LoadOverrides {
                country_ledgers: Some(ledgers),
                ..Default::default()
            },
        )
    }

    /// Loads resolved content with state translated by an external simulation
    /// or save reader before startup events execute.
    pub fn load_from_content_with_external_state(
        content: &ContentSet,
        date: &str,
        state: &VanillaExternalState,
    ) -> Result<Self, Error> {
        Self::load_from_view(
            content,
            content.base_root(),
            date,
            None,
            0,
            None,
            LoadOverrides::from_external_state(state),
        )
    }

    /// Loads a plaintext EU4 save, translates its supported state fields, and
    /// binds them before startup effects execute. The save itself remains an
    /// input artifact; no EU4 save format is stored in the core snapshot.
    pub fn load_from_content_with_save(
        content: &ContentSet,
        path: impl AsRef<Path>,
    ) -> Result<Self, Error> {
        let path = path.as_ref();
        let save = crate::read_vanilla_save_state_with_state_edicts(path)?;
        let constructions = crate::read_vanilla_naval_constructions(path)?;
        Self::load_from_content_with_state_edicts_and_naval_constructions(
            content,
            &save,
            None,
            Some(&constructions),
        )
    }

    /// Loads a plaintext EU4 save and overlays supplemental translated state
    /// before startup effects execute. The save remains the baseline; fields
    /// present in `overlay` replace or extend the corresponding translated
    /// inputs, which lets an external provider supply systems not represented
    /// by the EU4 save reader.
    pub fn load_from_content_with_save_and_external_state(
        content: &ContentSet,
        path: impl AsRef<Path>,
        overlay: &VanillaExternalState,
    ) -> Result<Self, Error> {
        let path = path.as_ref();
        let save = crate::read_vanilla_save_state_with_state_edicts(path)?;
        let constructions = crate::read_vanilla_naval_constructions(path)?;
        Self::load_from_content_with_state_edicts_and_naval_constructions(
            content,
            &save,
            Some(overlay),
            Some(&constructions),
        )
    }

    /// Loads a save together with its current state edicts before startup effects execute.
    pub fn load_from_content_with_state_edicts(
        content: &ContentSet,
        save: &crate::VanillaSaveStateWithStateEdicts,
    ) -> Result<Self, Error> {
        Self::load_from_content_with_state_edicts_and_external_state(content, save, None)
    }

    /// Loads a save with its state edicts and overlays supplemental translated state.
    pub fn load_from_content_with_state_edicts_and_external_state(
        content: &ContentSet,
        save: &crate::VanillaSaveStateWithStateEdicts,
        overlay: Option<&VanillaExternalState>,
    ) -> Result<Self, Error> {
        Self::load_from_content_with_state_edicts_and_naval_constructions(
            content, save, overlay, None,
        )
    }

    fn load_from_content_with_state_edicts_and_naval_constructions(
        content: &ContentSet,
        save: &crate::VanillaSaveStateWithStateEdicts,
        overlay: Option<&VanillaExternalState>,
        constructions: Option<&VanillaNavalConstructionReport>,
    ) -> Result<Self, Error> {
        let save = save.clone();
        let overlay = overlay.cloned();
        let constructions = constructions.cloned();
        let root = content.base_root();
        std::thread::scope(|scope| {
            let handle = std::thread::Builder::new()
                .name("augustus-eu4-save-loader".into())
                .stack_size(32 * 1024 * 1024)
                .spawn_scoped(scope, || {
                    Self::load_from_content_with_save_state_inner(
                        content,
                        &save.save,
                        overlay.as_ref(),
                        Some(&save.state_edicts),
                        constructions.as_ref(),
                    )
                })
                .map_err(|error| Error::caused_by(root.path(), error))?;
            handle
                .join()
                .map_err(|_| Error::message(root.path(), "EU4 save loader thread panicked"))?
        })
    }

    /// Variant of [`VanillaWorld::load_from_content_with_save`] for callers
    /// that already translated the save in a compatibility-owned boundary.
    pub fn load_from_content_with_save_state(
        content: &ContentSet,
        save: &crate::VanillaSaveState,
    ) -> Result<Self, Error> {
        Self::load_from_content_with_save_state_and_external_state(content, save, None)
    }

    /// Variant of [`VanillaWorld::load_from_content_with_save_and_external_state`]
    /// for callers that already translated the save in a compatibility-owned
    /// boundary.
    pub fn load_from_content_with_save_state_and_external_state(
        content: &ContentSet,
        save: &crate::VanillaSaveState,
        overlay: Option<&VanillaExternalState>,
    ) -> Result<Self, Error> {
        // Save import builds a baseline world before rebuilding it with the
        // translated state. Keep that nested compatibility path off the small
        // caller stack used by desktop frontends and command-line tools.
        let save = save.clone();
        let overlay = overlay.cloned();
        let root = content.base_root();
        std::thread::scope(|scope| {
            let handle = std::thread::Builder::new()
                .name("augustus-eu4-save-loader".into())
                .stack_size(32 * 1024 * 1024)
                .spawn_scoped(scope, || {
                    Self::load_from_content_with_save_state_inner(
                        content,
                        &save,
                        overlay.as_ref(),
                        None,
                        None,
                    )
                })
                .map_err(|error| Error::caused_by(root.path(), error))?;
            handle
                .join()
                .map_err(|_| Error::message(root.path(), "EU4 save loader thread panicked"))?
        })
    }

    /// Binds independently translated naval orders with a save state before
    /// startup effects execute. The report date must match `save.date`.
    pub fn load_from_content_with_save_state_and_naval_constructions(
        content: &ContentSet,
        save: &crate::VanillaSaveState,
        constructions: &VanillaNavalConstructionReport,
        overlay: Option<&VanillaExternalState>,
    ) -> Result<Self, Error> {
        let save = save.clone();
        let constructions = constructions.clone();
        let overlay = overlay.cloned();
        let root = content.base_root();
        std::thread::scope(|scope| {
            let handle = std::thread::Builder::new()
                .name("augustus-eu4-save-loader".into())
                .stack_size(32 * 1024 * 1024)
                .spawn_scoped(scope, || {
                    Self::load_from_content_with_save_state_inner(
                        content,
                        &save,
                        overlay.as_ref(),
                        None,
                        Some(&constructions),
                    )
                })
                .map_err(|error| Error::caused_by(root.path(), error))?;
            handle
                .join()
                .map_err(|_| Error::message(root.path(), "EU4 save loader thread panicked"))?
        })
    }

    fn load_from_content_with_save_state_inner(
        content: &ContentSet,
        save: &crate::VanillaSaveState,
        overlay: Option<&VanillaExternalState>,
        state_edicts: Option<&VanillaExternalStateEdicts>,
        naval_constructions: Option<&VanillaNavalConstructionReport>,
    ) -> Result<Self, Error> {
        let baseline = Self::load_from_content(content, &save.date)?;
        let (players, mut state) = save.for_world(&baseline)?;
        if let Some(overlay) = overlay {
            state.apply_overlay(overlay);
        }
        let mut overrides = LoadOverrides::from_external_state(&state);
        overrides.state_edicts = state_edicts;
        overrides.naval_construction_report = naval_constructions;
        Self::load_from_view(
            content,
            content.base_root(),
            &save.date,
            Some(&players),
            0,
            None,
            overrides,
        )
    }

    pub fn load_from_content_with_seed_and_external_state(
        content: &ContentSet,
        date: &str,
        seed: u32,
        state: &VanillaExternalState,
    ) -> Result<Self, Error> {
        Self::load_from_view(
            content,
            content.base_root(),
            date,
            None,
            seed,
            Some(seed),
            LoadOverrides::from_external_state(state),
        )
    }

    pub fn load_from_content_with_players_and_external_state(
        content: &ContentSet,
        date: &str,
        players: &std::collections::BTreeSet<String>,
        state: &VanillaExternalState,
    ) -> Result<Self, Error> {
        Self::load_from_view(
            content,
            content.base_root(),
            date,
            Some(players),
            0,
            None,
            LoadOverrides::from_external_state(state),
        )
    }

    /// Loads an initial world from resolved content with an explicit human roster.
    pub fn load_from_content_with_players(
        content: &ContentSet,
        date: &str,
        players: &std::collections::BTreeSet<String>,
    ) -> Result<Self, Error> {
        Self::load_from_view(
            content,
            content.base_root(),
            date,
            Some(players),
            0,
            None,
            LoadOverrides::default(),
        )
    }

    pub fn load_from_content_with_players_and_seed(
        content: &ContentSet,
        date: &str,
        players: &std::collections::BTreeSet<String>,
        seed: u32,
    ) -> Result<Self, Error> {
        Self::load_from_view(
            content,
            content.base_root(),
            date,
            Some(players),
            seed,
            Some(seed),
            LoadOverrides::default(),
        )
    }

    pub fn load_from_content_with_players_and_seed_and_external_state(
        content: &ContentSet,
        date: &str,
        players: &std::collections::BTreeSet<String>,
        seed: u32,
        state: &VanillaExternalState,
    ) -> Result<Self, Error> {
        Self::load_from_view(
            content,
            content.base_root(),
            date,
            Some(players),
            seed,
            Some(seed),
            LoadOverrides::from_external_state(state),
        )
    }

    fn load_session(
        path: impl AsRef<Path>,
        date: &str,
        players: Option<&std::collections::BTreeSet<String>>,
        seed: u32,
        fixed_seed: Option<u32>,
        overrides: LoadOverrides<'_>,
    ) -> Result<Self, Error> {
        let content = ContentSet::from_installation(path, &[], None, None, TextEncoding::Utf8)?;
        let root = content.base_root();
        Self::load_from_view(&content, root, date, players, seed, fixed_seed, overrides)
    }

    fn load_from_view(
        root: &impl ContentView,
        metadata_root: &ContentRoot,
        date: &str,
        players: Option<&std::collections::BTreeSet<String>>,
        seed: u32,
        fixed_seed: Option<u32>,
        overrides: LoadOverrides<'_>,
    ) -> Result<Self, Error> {
        let clock_origin = std::time::Instant::now();
        let date = Date::parse(date).map_err(|e| Error::caused_by(root.path(), e))?;
        let metadata_path = metadata_root.path().join("launcher-settings.json");
        let metadata = metadata_root.read_text("launcher-settings.json", TextEncoding::Utf8)?;
        let metadata: serde_json::Value = serde_json::from_slice(metadata.bytes())
            .map_err(|e| Error::caused_by(&metadata_path, e))?;
        let version = metadata
            .get("rawVersion")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::message(&metadata_path, "missing rawVersion"))?;
        if !matches!(version, "v1.37.5.0" | "1.37.5.0" | "1.37.5") {
            return Err(Error::message(
                &metadata_path,
                "world loader targets EU4 1.37.5 final edition",
            ));
        }
        let map_document = root.read_text("map/default.map", ENCODING)?;
        let map_parsed = map_document.parse()?;
        let map = map_parsed.reader();
        let width = required_u32(&map_parsed, map.clone(), "width")?;
        let height = required_u32(&map_parsed, map.clone(), "height")?;
        let max = required_u32(&map_parsed, map.clone(), "max_provinces")?;
        if width == 0 || height == 0 || max < 2 {
            return Err(Error::message(
                map_document.source(),
                "invalid map dimensions or province limit",
            ));
        }
        let definition_file = map_file(&map_parsed, map.clone(), "definitions")?;
        let bitmap_file = map_file(&map_parsed, map.clone(), "provinces")?;
        let terrain_bitmap_file = optional_map_file(&map_parsed, map.clone(), "terrain")?;
        let rivers_bitmap_file = optional_map_file(&map_parsed, map.clone(), "rivers")?;
        let terrain_definition_file =
            optional_map_file(&map_parsed, map.clone(), "terrain_definition")?;
        let tree_bitmap_file = optional_map_file(&map_parsed, map.clone(), "tree_definition")?;
        let automatic_tree_indices = optional_ids(&map_parsed, map.clone(), "tree")?
            .into_iter()
            .map(|id| {
                u8::try_from(id).map_err(|_| {
                    Error::message(
                        map_document.source(),
                        "tree palette index is outside 0..255",
                    )
                })
            })
            .collect::<Result<BTreeSet<_>, _>>()?;
        // Resolve through the same traversal checks used for text inputs.
        let bitmap = root.read_text(&bitmap_file, ENCODING)?;
        let pixels = image::ImageReader::new(std::io::Cursor::new(bitmap.bytes()))
            .with_guessed_format()
            .map_err(|e| Error::caused_by(bitmap.source(), e))?
            .decode()
            .map_err(|e| Error::caused_by(bitmap.source(), e))?
            .into_rgb8();
        if pixels.width() != width || pixels.height() != height {
            return Err(Error::message(
                bitmap.source(),
                "province bitmap dimensions do not match default.map",
            ));
        }
        let used_colors: HashSet<[u8; 3]> = pixels.pixels().map(|p| p.0).collect();
        let sea = ids(&map_parsed, map.clone(), "sea_starts")?;
        let lakes = ids(&map_parsed, map.clone(), "lakes")?;
        let unused = ids(&map_parsed, map.clone(), "only_used_for_random")?;
        let climate_file = map_file(&map_parsed, map.clone(), "climate")?;
        let climate = root.read_text(climate_file, ENCODING)?;
        let climate_parsed = climate.parse()?;
        let wasteland = ids(&climate_parsed, climate_parsed.reader(), "impassable")?;
        let mut report = BootstrapReport {
            ruleset: "eu4-final-all-dlc",
            game_version: version.into(),
            start_date: WorldDate {
                year: date.year(),
                month: date.month(),
                day: date.day(),
            },
            map_width: width,
            map_height: height,
            countries: 0,
            country_tag_slots: 0,
            generated_country_tag_slots: 0,
            active_countries: 0,
            provinces: 0,
            country_history_files: 0,
            province_history_files: 0,
            diplomacy_history_files: 0,
            people: 0,
            diplomatic_relations: 0,
            duplicate_relation_records: 0,
            person_sources: BTreeMap::new(),
            relation_sources: Vec::new(),
            office_sources: BTreeMap::new(),
            expired_people: Vec::new(),
            not_yet_born_people: Vec::new(),
            unapplied_diplomacy_fields: BTreeMap::new(),
            missing_province_history: Vec::new(),
            province_discovery_states: 0,
            unresolved_discovery_identifiers: BTreeMap::new(),
            unbound_active_capitals: Vec::new(),
            unmapped_definition_ids: Vec::new(),
            unknown_map_colors: Vec::new(),
            ambiguous_country_histories: BTreeMap::new(),
            unapplied_country_fields: BTreeMap::new(),
            unapplied_province_fields: BTreeMap::new(),
            unapplied_examples: BTreeMap::new(),
            economy: EconomyReport::default(),
            setup: SetupReport::default(),
            attributes: AttributeReport::default(),
            administration: AdministrationReport::default(),
            governments: GovernmentReport::default(),
            ideas: IdeaReport::default(),
            estates: EstateReport::default(),
            religious_schools: crate::ReligiousSchoolReport::default(),
            casus_belli: crate::CasusBelliReport::default(),
            missions: crate::MissionReport::default(),
        };
        let mut provinces = BTreeMap::new();
        let mut province_ids_by_color = BTreeMap::new();
        let mut province_sources = BTreeMap::new();
        let definitions = root.read_text(&definition_file, ENCODING)?;
        let mut csv = csv::ReaderBuilder::new()
            .delimiter(b';')
            .flexible(true)
            .from_reader(definitions.bytes());
        let header = csv
            .byte_headers()
            .map_err(|e| Error::caused_by(definitions.source(), e))?;
        if header.get(0) != Some(b"province".as_slice()) {
            return Err(Error::message(
                definitions.source(),
                "expected province definition header",
            ));
        }
        for row in csv.byte_records() {
            let row = row.map_err(|e| Error::caused_by(definitions.source(), e))?;
            let line = row.position().map_or(1, |p| p.line() as usize);
            let fail = |m: &str| {
                Error::message(definitions.source(), m)
                    .with_location(SourceLocation { line, column: 1 })
            };
            let integer = |index| -> Result<u32, Error> {
                std::str::from_utf8(
                    row.get(index)
                        .ok_or_else(|| fail("missing definition column"))?,
                )
                .map_err(|_| fail("invalid definition integer"))?
                .trim()
                .parse()
                .map_err(|_| fail("invalid definition integer"))
            };
            let id = integer(0)?;
            if id == 0 || id >= max {
                continue;
            }
            let mut color = [0; 3];
            for (i, component) in color.iter_mut().enumerate() {
                *component =
                    u8::try_from(integer(i + 1)?).map_err(|_| fail("map color out of range"))?;
            }
            let name = ENCODING
                .decode(row.get(4).ok_or_else(|| fail("missing province name"))?)
                .into_owned();
            let class = if unused.contains(&id) || !used_colors.contains(&color) {
                ProvinceClass::Unused
            } else if sea.contains(&id) {
                ProvinceClass::Sea
            } else if lakes.contains(&id) {
                ProvinceClass::Lake
            } else if wasteland.contains(&id) {
                ProvinceClass::Wasteland
            } else {
                ProvinceClass::Land
            };
            if class != ProvinceClass::Unused && province_ids_by_color.insert(color, id).is_some() {
                return Err(fail("duplicate map color"));
            }
            let province = ProvinceState {
                id,
                name,
                color,
                class,
                has_port: None,
                owner: None,
                controller: None,
                cores: Vec::new(),
                culture: None,
                religion: None,
                trade_good: None,
                tax_milli: None,
                production_milli: None,
                manpower_milli: None,
                is_city: None,
                in_empire: None,
            };
            if provinces.insert(id, province).is_some() {
                return Err(fail("duplicate province identifier"));
            }
            province_sources.insert(
                id,
                vec![LoadedSource {
                    path: definitions.source().into(),
                    line,
                    column: 1,
                }],
            );
        }
        for id in sea
            .iter()
            .chain(lakes.iter())
            .chain(unused.iter())
            .chain(wasteland.iter())
        {
            if !provinces.contains_key(id) {
                return Err(Error::message(
                    map_document.source(),
                    format!("map category references unknown province {id}"),
                ));
            }
        }
        let defined_colors: HashSet<_> = provinces.values().map(|p| p.color).collect();
        report.unknown_map_colors = used_colors.difference(&defined_colors).copied().collect();
        report.unknown_map_colors.sort();
        report.unmapped_definition_ids = provinces
            .values()
            .filter(|p| p.class == ProvinceClass::Unused)
            .map(|p| p.id)
            .collect();
        let force_coastal = ids(&map_parsed, map.clone(), "force_coastal")?;
        for id in &force_coastal {
            if !provinces.contains_key(id) {
                return Err(Error::message(
                    map_document.source(),
                    format!("coastal map category references unknown province {id}"),
                ));
            }
        }
        let derived_province_map = crate::province_map::derive(
            &pixels,
            &province_ids_by_color,
            &provinces,
            &force_coastal,
        );
        for (id, has_port) in derived_province_map.port_presence {
            provinces
                .get_mut(&id)
                .expect("port derivation preserves province identifiers")
                .has_port = has_port;
        }
        let mut province_topology = derived_province_map.topology;
        let province_terrains = match (terrain_bitmap_file, terrain_definition_file) {
            (Some(bitmap_file), Some(definition_file)) => {
                let terrain_document = root.read_text(&bitmap_file, ENCODING)?;
                let terrain_bitmap = crate::terrain_map::IndexedBitmap::decode(&terrain_document)?;
                let definition_document = root.read_text(&definition_file, ENCODING)?;
                let definitions =
                    crate::terrain_map::TerrainDefinitions::parse(&definition_document.parse()?)?;
                let river_bitmap = rivers_bitmap_file
                    .as_deref()
                    .map(|path| {
                        root.read_text(path, ENCODING).and_then(|document| {
                            crate::terrain_map::IndexedBitmap::decode(&document)
                        })
                    })
                    .transpose()?;
                let tree_bitmap = tree_bitmap_file
                    .as_deref()
                    .map(|path| {
                        root.read_text(path, ENCODING).and_then(|document| {
                            crate::terrain_map::IndexedBitmap::decode(&document)
                        })
                    })
                    .transpose()?;
                definitions.validate_palette_indexes(
                    &terrain_bitmap,
                    tree_bitmap.as_ref(),
                    &automatic_tree_indices,
                    terrain_document.source(),
                )?;
                Some(crate::terrain_map::derive_province_terrains(
                    &pixels,
                    &province_ids_by_color,
                    &provinces,
                    crate::terrain_map::TerrainMap {
                        terrain: &terrain_bitmap,
                        rivers: river_bitmap.as_ref(),
                        trees: tree_bitmap.as_ref(),
                        automatic_tree_indexes: &automatic_tree_indices,
                        definitions: &definitions,
                    },
                )?)
            }
            _ => None,
        };
        let files = root.files()?;
        if let Some(adjacencies) = files.iter().find(|file| {
            file.relative_path
                .to_string_lossy()
                .replace('\\', "/")
                .eq_ignore_ascii_case("map/adjacencies.csv")
        }) {
            let document = root.read_text(&adjacencies.relative_path, ENCODING)?;
            province_topology.add_special_adjacencies(&document, &provinces)?;
        }
        let defines = crate::Defines::load(root, &files, crate::DefineDifficulty::Normal)?;
        crate::native_migration_defines::bind(&defines, &mut report.setup);
        let state_maintenance_development_scale_milli = defines
            .optional("NEconomy", "STATE_MAINTENANCE_DEV_FACTOR")?
            .map(|(value, source)| {
                i32::try_from(value / 1_000).map_err(|_| {
                    Error::message(
                        &source.path,
                        "STATE_MAINTENANCE_DEV_FACTOR exceeds the supported range",
                    )
                    .with_location(SourceLocation {
                        line: source.line,
                        column: source.column,
                    })
                })
            })
            .transpose()?;
        let mut countries = BTreeMap::new();
        let mut country_order = Vec::new();
        let mut country_sources = BTreeMap::new();
        for file in files
            .iter()
            .filter(|f| direct_txt(&f.relative_path, "common/country_tags"))
        {
            let doc = root.read_text(&file.relative_path, ENCODING)?;
            let parsed = doc.parse()?;
            for (key, op, value) in parsed.reader().fields() {
                let id = key.read_string();
                let location = parsed.field_location(&key);
                let fail = |m: &str| Error::message(doc.source(), m).with_location(location);
                equal(op, &fail)?;
                if id.len() != 3
                    || !id
                        .bytes()
                        .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
                {
                    return Err(fail("invalid country identifier"));
                }
                let file = value
                    .read_string()
                    .map_err(|_| fail("country definition must be a path"))?;
                if !file.starts_with("countries/") {
                    return Err(fail("expected countries/ definition path"));
                }
                let country_doc = root.read_text(format!("common/{file}"), ENCODING)?;
                let country_parsed = country_doc.parse()?;
                let rgb = array_strings(&country_parsed, country_parsed.reader(), "color")?;
                let color: [u8; 3] = rgb
                    .iter()
                    .map(|v| v.parse::<u8>().map_err(|_| fail("invalid country color")))
                    .collect::<Result<Vec<_>, _>>()?
                    .try_into()
                    .map_err(|_| fail("country color requires three components"))?;
                let country = CountryState {
                    id: id.clone(),
                    name: Path::new(&file)
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or(&id)
                        .into(),
                    color,
                    declared_capital: None,
                    primary_culture: None,
                    religion: None,
                    government: None,
                    technology_group: None,
                    unit_type: None,
                };
                if countries.insert(id.clone(), country).is_some() {
                    return Err(fail("duplicate country identifier"));
                }
                country_order.push(id.clone());
                country_sources.insert(
                    id,
                    vec![
                        source(&parsed, location),
                        LoadedSource {
                            path: country_doc.source().into(),
                            line: 1,
                            column: 1,
                        },
                    ],
                );
            }
        }
        if countries.is_empty() || provinces.is_empty() {
            return Err(Error::message(root.path(), "empty world definitions"));
        }
        let mut economy = EconomyDraft::load(&root, &files, &mut report.economy)?;
        let mut governments = GovernmentDraft::load(&root, &files, &mut report.governments)?;
        let mut status_draft = CountryStatusDraft::load(&root, &files, &mut report.ideas)?;
        let mut estates = EstateDraft::load(&root, &files, &defines, &mut report.estates)?;
        let mut markers = crate::country_markers::CountryMarkers::default();
        let mut global_markers = augustus_core::GlobalMarkers {
            known: true,
            ..Default::default()
        };
        let mut history_global_marker_names = BTreeSet::new();
        let mut dynamic_global_marker_history = false;
        let mut ideas = IdeaDraft::load(&root, &files, &mut report.ideas)?;
        let mut setup = SetupDraft::load(&root, &files, &mut report.setup)?;
        let attribute_draft =
            AttributeDraft::load(&root, &files, &defines, &mut report.attributes)?;
        let mut administration = AdministrationDraft::load(
            &root,
            &files,
            &defines,
            &map_parsed,
            &mut report.administration,
        )?;
        let mut political_drafts = BTreeMap::new();
        let mut religious_schools = crate::religious_schools::ReligiousSchoolsDraft::load(
            &root,
            &files,
            &mut report.religious_schools,
        )?;
        let mut country_histories = BTreeSet::new();
        let mut historical_truce_grants = Vec::new();
        let mut historical_prestige_deltas = Vec::new();
        let mut country_metric_history_unknown = BTreeSet::new();
        let mut country_metric_scoped_history_unknown = false;
        let mut liberty_history_unknown = BTreeSet::new();
        let mut liberty_scoped_history_unknown = false;
        let mut province_histories = BTreeSet::new();
        let mut province_history_state = ProvinceHistoryState::default();
        let mut unresolved_ownership = BTreeSet::new();
        let mut history_groups: BTreeMap<String, Vec<&Path>> = BTreeMap::new();
        for file in files
            .iter()
            .filter(|f| direct_txt(&f.relative_path, "history/countries"))
        {
            history_groups
                .entry(history_id(&file.relative_path)?)
                .or_default()
                .push(&file.relative_path);
        }
        for (id, paths) in &history_groups {
            if paths.len() > 1 {
                report.ambiguous_country_histories.insert(
                    id.clone(),
                    paths.iter().map(|p| root.physical_path(p)).collect(),
                );
            }
        }
        let mut metric_scoped_history_unknown = false;
        let mut subject_history_unknown = false;
        let mut imperial_history_unknown = false;
        let imperial_membership_effect_names =
            crate::imperial_membership::membership_effect_names(&root, &files)?;
        let subject_history_effect_names = politics::subject_history_effect_names(&root, &files)?;
        let (liberty_direct_history_effects, liberty_scoped_history_effects) =
            crate::country_metrics::history_scripted_effects(
                &root,
                &files,
                &["liberty_desire", "add_liberty_desire"],
            )?;
        let (stability_direct_history_effects, stability_scoped_history_effects) =
            crate::country_metrics::history_scripted_effects(
                &root,
                &files,
                &["stability", "add_stability"],
            )?;
        let mut stability_history_effects = stability_direct_history_effects.clone();
        stability_history_effects.extend(stability_scoped_history_effects.iter().cloned());
        for file in files
            .iter()
            .filter(|f| direct_txt(&f.relative_path, "history/countries"))
        {
            let id = history_id(&file.relative_path)?;
            let Some(country) = countries.get_mut(&id) else {
                return Err(Error::message(
                    root.physical_path(&file.relative_path),
                    format!("history references unknown country {id}"),
                ));
            };
            // Selection of duplicate history files has not been validated. Preserve the
            // ambiguity for country fields, while allowing independent subsystems to inspect
            // their own history and resolve only values that agree across every candidate.
            if history_groups[&id].len() > 1 {
                let doc = root.read_text(&file.relative_path, ENCODING)?;
                let parsed = doc.parse()?;
                visit_history(&parsed, date, |reader| {
                    match crate::country_markers::global_flag_mutations(&parsed, reader.clone(), 0)?
                    {
                        Some(names) if !names.is_empty() => {
                            global_markers.mark_unknown();
                            history_global_marker_names.extend(names);
                        }
                        Some(_) => (),
                        None => {
                            global_markers.mark_unknown();
                            dynamic_global_marker_history = true;
                        }
                    }
                    subject_history_unknown |= politics::has_subject_mutation(
                        &parsed,
                        reader.clone(),
                        &subject_history_effect_names,
                        0,
                    )?;
                    imperial_history_unknown |= crate::imperial_membership::has_unapplied_mutation(
                        &parsed,
                        reader.clone(),
                        &imperial_membership_effect_names,
                        false,
                        0,
                    )?;
                    let (direct, scoped) =
                        crate::province_metrics::history_mutations(reader.clone(), 0);
                    metric_scoped_history_unknown |= direct || scoped;
                    let (direct, scoped) =
                        crate::country_metrics::named_history_mutations_with_scripts(
                            reader.clone(),
                            &stability_history_effects,
                            0,
                            &["stability", "add_stability"],
                            &BTreeSet::new(),
                            &BTreeSet::new(),
                        );
                    country_metric_scoped_history_unknown |= scoped;
                    if direct {
                        country_metric_history_unknown.insert(id.clone());
                    }
                    let (direct, scoped) =
                        crate::country_metrics::named_history_mutations_with_scripts(
                            reader.clone(),
                            &BTreeSet::new(),
                            0,
                            &["liberty_desire", "add_liberty_desire"],
                            &liberty_direct_history_effects,
                            &liberty_scoped_history_effects,
                        );
                    liberty_scoped_history_unknown |= scoped;
                    if direct {
                        liberty_history_unknown.insert(id.clone());
                    }
                    religious_schools.country_history(
                        &parsed,
                        reader.clone(),
                        &id,
                        &mut report.religious_schools,
                    )?;
                    Ok(())
                })?;
                continue;
            }
            country_histories.insert(id.clone());
            let doc = root.read_text(&file.relative_path, ENCODING)?;
            let parsed = doc.parse()?;
            country_sources
                .get_mut(&id)
                .expect("registered country")
                .push(LoadedSource {
                    path: doc.source().into(),
                    line: 1,
                    column: 1,
                });
            visit_history_with_date(&parsed, date, |history_date, reader| {
                match crate::country_markers::global_flag_mutations(&parsed, reader.clone(), 0)? {
                    Some(names) if !names.is_empty() => {
                        global_markers.mark_unknown();
                        history_global_marker_names.extend(names);
                    }
                    Some(_) => (),
                    None => {
                        global_markers.mark_unknown();
                        dynamic_global_marker_history = true;
                    }
                }
                subject_history_unknown |= politics::has_subject_mutation(
                    &parsed,
                    reader.clone(),
                    &subject_history_effect_names,
                    0,
                )?;
                imperial_history_unknown |= crate::imperial_membership::has_unapplied_mutation(
                    &parsed,
                    reader.clone(),
                    &imperial_membership_effect_names,
                    false,
                    0,
                )?;
                let (direct, scoped) =
                    crate::province_metrics::history_mutations(reader.clone(), 0);
                metric_scoped_history_unknown |= direct || scoped;
                let (direct, scoped) = crate::country_metrics::named_history_mutations_with_scripts(
                    reader.clone(),
                    &stability_history_effects,
                    0,
                    &["stability", "add_stability"],
                    &BTreeSet::new(),
                    &BTreeSet::new(),
                );
                country_metric_scoped_history_unknown |= scoped;
                if direct {
                    country_metric_history_unknown.insert(id.clone());
                }
                let (direct, scoped) = crate::country_metrics::named_history_mutations_with_scripts(
                    reader.clone(),
                    &BTreeSet::new(),
                    0,
                    &["liberty_desire", "add_liberty_desire"],
                    &liberty_direct_history_effects,
                    &liberty_scoped_history_effects,
                );
                liberty_scoped_history_unknown |= scoped;
                if direct {
                    liberty_history_unknown.insert(id.clone());
                }
                religious_schools.country_history(
                    &parsed,
                    reader.clone(),
                    &id,
                    &mut report.religious_schools,
                )?;
                let marker_handled = markers.history(&parsed, reader.clone(), &id)?;
                status_draft.country_history(&parsed, reader.clone(), &id, &mut report.ideas)?;
                let estate_handled =
                    estates.country_history(&parsed, reader.clone(), &id, &mut report.estates)?;
                ideas.country_history(&parsed, reader.clone(), &id, &mut report.ideas)?;
                governments.country_history(
                    &parsed,
                    reader.clone(),
                    &id,
                    &mut report.governments,
                )?;
                let mut subsystem_handled = administration.country_history(
                    &parsed,
                    reader.clone(),
                    &id,
                    &mut report.administration,
                )?;
                subsystem_handled.extend(marker_handled);
                subsystem_handled.extend(estate_handled);
                apply_country(
                    &parsed,
                    reader,
                    country,
                    political_drafts
                        .entry(id.clone())
                        .or_insert_with(|| CountryDraft {
                            national_focus: Some(augustus_core::NationalFocus::default()),
                            ..Default::default()
                        }),
                    CountryHistoryContext {
                        date: crate::politics::label(history_date.unwrap_or(date)),
                        historical_truce_grants: &mut historical_truce_grants,
                        historical_prestige_deltas: &mut historical_prestige_deltas,
                        setup: &mut setup,
                        subsystem_handled: &subsystem_handled,
                        report: &mut report,
                    },
                )
            })?;
            if country.unit_type.is_none() {
                country.unit_type = country.technology_group.clone();
            }
        }
        let mut metric_history_unknown = BTreeSet::new();
        for file in files
            .iter()
            .filter(|f| direct_txt(&f.relative_path, "history/provinces"))
        {
            let id = history_id(&file.relative_path)?
                .parse::<u32>()
                .map_err(|_| {
                    Error::message(&file.relative_path, "invalid province history identifier")
                })?;
            let Some(province) = provinces.get_mut(&id) else {
                return Err(Error::message(
                    root.physical_path(&file.relative_path),
                    format!("history references unknown province {id}"),
                ));
            };
            if !province_histories.insert(id) {
                return Err(Error::message(
                    &file.relative_path,
                    "duplicate province history",
                ));
            }
            let doc = root.read_text(&file.relative_path, ENCODING)?;
            let parsed = doc.parse()?;
            province_history_state
                .center_of_trade_levels
                .insert(id, Some(0));
            province_sources
                .get_mut(&id)
                .expect("registered province")
                .push(LoadedSource {
                    path: doc.source().into(),
                    line: 1,
                    column: 1,
                });
            let mut unresolved_center_of_trade = false;
            visit_history(&parsed, date, |reader| {
                unresolved_center_of_trade |= nested_center_of_trade(reader.clone(), 0);
                match crate::country_markers::global_flag_mutations(&parsed, reader.clone(), 0)? {
                    Some(names) if !names.is_empty() => {
                        global_markers.mark_unknown();
                        history_global_marker_names.extend(names);
                    }
                    Some(_) => (),
                    None => {
                        global_markers.mark_unknown();
                        dynamic_global_marker_history = true;
                    }
                }
                subject_history_unknown |= politics::has_subject_mutation(
                    &parsed,
                    reader.clone(),
                    &subject_history_effect_names,
                    0,
                )?;
                imperial_history_unknown |= crate::imperial_membership::has_unapplied_mutation(
                    &parsed,
                    reader.clone(),
                    &imperial_membership_effect_names,
                    true,
                    0,
                )?;
                if nested_owner(reader.clone(), 0) {
                    unresolved_ownership.insert(id);
                }
                let (direct, scoped) = crate::country_metrics::named_history_mutations_with_scripts(
                    reader.clone(),
                    &stability_history_effects,
                    0,
                    &["stability", "add_stability"],
                    &BTreeSet::new(),
                    &BTreeSet::new(),
                );
                country_metric_scoped_history_unknown |= direct || scoped;
                let (direct, scoped) = crate::country_metrics::named_history_mutations_with_scripts(
                    reader.clone(),
                    &BTreeSet::new(),
                    0,
                    &["liberty_desire", "add_liberty_desire"],
                    &liberty_direct_history_effects,
                    &liberty_scoped_history_effects,
                );
                liberty_scoped_history_unknown |= direct || scoped;
                let (direct, scoped) =
                    crate::province_metrics::history_mutations(reader.clone(), 0);
                metric_scoped_history_unknown |= scoped;
                if direct {
                    metric_history_unknown.insert(id);
                }
                administration.province_history(
                    &parsed,
                    reader.clone(),
                    id,
                    &mut report.administration,
                )?;
                apply_province(
                    &parsed,
                    reader,
                    province,
                    &mut economy,
                    &mut setup,
                    &mut report,
                    &mut province_history_state,
                )
            })?;
            if unresolved_center_of_trade {
                province_history_state
                    .center_of_trade_levels
                    .insert(id, None);
            }
        }
        report.country_history_files = country_histories.len();
        report.province_history_files = province_histories.len();
        report.missing_province_history = provinces
            .keys()
            .filter(|id| !province_histories.contains(id))
            .copied()
            .collect();
        if let Some(owners) = overrides.province_owners {
            for (&id, owner) in owners {
                let Some(province) = provinces.get_mut(&id) else {
                    return Err(Error::message(
                        root.path(),
                        format!("external province ownership references unknown province {id}"),
                    ));
                };
                if let Some(owner) = owner
                    && !countries.contains_key(owner)
                {
                    return Err(Error::message(
                        root.path(),
                        format!("province {id} references unknown saved owner {owner}"),
                    ));
                }
                province.owner.clone_from(owner);
                unresolved_ownership.remove(&id);
            }
        }
        if let Some(cores) = overrides.province_cores {
            for (&id, tags) in cores {
                let Some(province) = provinces.get_mut(&id) else {
                    return Err(Error::message(
                        root.path(),
                        format!("external province cores reference unknown province {id}"),
                    ));
                };
                province.cores.clone_from(tags);
            }
        }
        if let Some(controllers) = overrides.province_controllers {
            for (&id, controller) in controllers {
                let Some(province) = provinces.get_mut(&id) else {
                    return Err(Error::message(
                        root.path(),
                        format!("external province controller references unknown province {id}"),
                    ));
                };
                if let Some(controller) = controller
                    && !countries.contains_key(controller)
                {
                    return Err(Error::message(
                        root.path(),
                        format!("province {id} references unknown saved controller {controller}"),
                    ));
                }
                province.controller.clone_from(controller);
            }
        }
        if let Some(development) = overrides.province_development {
            for (&id, values) in development {
                let Some(province) = provinces.get_mut(&id) else {
                    return Err(Error::message(
                        root.path(),
                        format!("external province development references unknown province {id}"),
                    ));
                };
                for (field, value, target) in [
                    ("base_tax", values.base_tax_milli, &mut province.tax_milli),
                    (
                        "base_production",
                        values.base_production_milli,
                        &mut province.production_milli,
                    ),
                    (
                        "base_manpower",
                        values.base_manpower_milli,
                        &mut province.manpower_milli,
                    ),
                ] {
                    if let Some(value) = value {
                        if value < 0 {
                            return Err(Error::message(
                                root.path(),
                                format!("province {id} has negative saved {field}"),
                            ));
                        }
                        *target = Some(value);
                    }
                }
            }
        }
        if let Some(trade_goods) = overrides.province_trade_goods {
            for (id, trade_good) in trade_goods {
                let Some(province) = provinces.get_mut(id) else {
                    return Err(Error::message(
                        root.path(),
                        format!("external province trade good references unknown province {id}"),
                    ));
                };
                province.trade_good.clone_from(trade_good);
            }
        }
        if let Some(levels) = overrides.province_center_of_trade_levels {
            for (&id, &level) in levels {
                if !provinces.contains_key(&id) {
                    return Err(Error::message(
                        root.path(),
                        format!("external center of trade references unknown province {id}"),
                    ));
                }
                province_history_state
                    .center_of_trade_levels
                    .insert(id, level);
            }
        }
        if let Some(cultures) = overrides.country_primary_cultures {
            for (tag, culture) in cultures {
                let Some(country) = countries.get_mut(tag) else {
                    return Err(Error::message(
                        root.path(),
                        format!("external country culture references unknown country {tag}"),
                    ));
                };
                country.primary_culture.clone_from(culture);
            }
        }
        if let Some(groups) = overrides.country_technology_groups {
            for (tag, group) in groups {
                let Some(country) = countries.get_mut(tag) else {
                    return Err(Error::message(
                        root.path(),
                        format!("saved technology group references unknown country {tag}"),
                    ));
                };
                country.technology_group.clone_from(group);
            }
        }
        if let Some(unit_types) = overrides.country_unit_types {
            for (tag, unit_type) in unit_types {
                let Some(country) = countries.get_mut(tag) else {
                    return Err(Error::message(
                        root.path(),
                        format!("saved unit type references unknown country {tag}"),
                    ));
                };
                country.unit_type.clone_from(unit_type);
            }
        }
        if let Some(religions) = overrides.country_religions {
            for (tag, religion) in religions {
                let Some(country) = countries.get_mut(tag) else {
                    return Err(Error::message(
                        root.path(),
                        format!("external country religion references unknown country {tag}"),
                    ));
                };
                country.religion.clone_from(religion);
            }
        }
        if let Some(cultures) = overrides.province_cultures {
            for (id, culture) in cultures {
                let Some(province) = provinces.get_mut(id) else {
                    return Err(Error::message(
                        root.path(),
                        format!("external province culture references unknown province {id}"),
                    ));
                };
                province.culture.clone_from(culture);
            }
        }
        if let Some(religions) = overrides.province_religions {
            for (id, religion) in religions {
                let Some(province) = provinces.get_mut(id) else {
                    return Err(Error::message(
                        root.path(),
                        format!("external province religion references unknown province {id}"),
                    ));
                };
                province.religion.clone_from(religion);
            }
        }
        if let Some(capitals) = overrides.country_capitals {
            for (tag, capital) in capitals {
                let Some(country) = countries.get_mut(tag) else {
                    return Err(Error::message(
                        root.path(),
                        format!("external country capital references unknown country {tag}"),
                    ));
                };
                if let Some(capital) = capital
                    && !provinces.contains_key(capital)
                {
                    return Err(Error::message(
                        root.path(),
                        format!("country {tag} capital references unknown province {capital}"),
                    ));
                }
                country.declared_capital = *capital;
            }
        }
        let mut initial = InitialWorld::new_with_eu4_seed(
            countries.into_values().collect(),
            provinces.into_values().collect(),
            seed,
        )
        .map_err(|e| Error::caused_by(root.path(), e))?;
        if let Some(original_capitals) = overrides.country_original_capitals {
            for (country, capital) in original_capitals {
                initial
                    .set_country_original_capital(country, *capital)
                    .map_err(|e| Error::caused_by(root.path(), e))?;
            }
        }
        if let Some(histories) = overrides.country_capital_history {
            for (country, history) in histories {
                initial
                    .set_country_capital_history(country, history.clone())
                    .map_err(|e| Error::caused_by(root.path(), e))?;
            }
        }
        if let Some(controllers) = overrides.province_controllers {
            for (&province, controller) in controllers {
                initial
                    .set_province_controller(province, controller.as_deref())
                    .map_err(|e| Error::caused_by(root.path(), e))?;
            }
        }
        if let Some(previous_controllers) = overrides.province_previous_controllers {
            for (&province, controller) in previous_controllers {
                initial
                    .set_province_previous_controller(province, controller.as_deref())
                    .map_err(|e| Error::caused_by(root.path(), e))?;
            }
        }
        if let Some(tribal_owners) = overrides.province_tribal_owners {
            for (&province, owner) in tribal_owners {
                initial
                    .set_province_tribal_owner(province, owner.as_deref())
                    .map_err(|e| Error::caused_by(root.path(), e))?;
            }
        }
        let country_tag_registry = crate::country_tags::build_registry(&country_order)
            .map_err(|e| Error::caused_by(root.path(), e))?;
        report.country_tag_slots = country_tag_registry.slots().len();
        report.generated_country_tag_slots = country_tag_registry.generated_count();
        let map_knowledge = bind_province_discoveries(
            &initial,
            &province_history_state.discovered_by,
            overrides.province_discovered_by,
            &setup.technology_group_names(),
            &mut report,
        )?;
        initial
            .set_map_knowledge(map_knowledge)
            .map_err(|e| Error::caused_by(root.path(), e))?;
        if let Some(province_terrains) = province_terrains {
            for (province, terrain) in province_terrains {
                initial
                    .set_province_terrain(province, terrain)
                    .map_err(|e| Error::caused_by(root.path(), e))?;
            }
        }
        for (&province, &level) in &province_history_state.center_of_trade_levels {
            initial
                .set_province_center_of_trade_level(province, level)
                .map_err(|e| Error::caused_by(root.path(), e))?;
        }
        initial.set_eu4_event_seed(fixed_seed);
        if let Some(random_state) = overrides.eu4_random_state {
            initial.set_eu4_random_state(random_state.seed, random_state.count);
        }
        initial
            .set_calendar(Calendar {
                start: report.start_date,
                month_lengths: [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31],
            })
            .map_err(|e| Error::caused_by(root.path(), e))?;
        let unowned = initial
            .provinces()
            .iter()
            .filter(|province| {
                province.owner.is_none()
                    && !unresolved_ownership.contains(&province.id)
                    && (overrides
                        .province_owners
                        .is_some_and(|owners| owners.contains_key(&province.id))
                        || province.class != ProvinceClass::Land
                        || province_histories.contains(&province.id))
            })
            .map(|province| {
                initial
                    .province_entity(province.id)
                    .expect("registered province")
            })
            .collect::<Vec<_>>();
        for entity in unowned {
            initial
                .mark_target_absent(entity, augustus_core::Target::Owner)
                .map_err(|e| Error::caused_by(root.path(), e))?;
        }
        let province_entities = initial
            .provinces()
            .iter()
            .filter_map(|province| {
                initial
                    .province_entity(province.id)
                    .map(|entity| (province.id, entity))
            })
            .collect();
        let connectivity = province_topology
            .to_core(&province_entities)
            .map_err(|e| Error::caused_by(root.path(), e))?;
        initial
            .set_province_connectivity(connectivity)
            .map_err(|e| Error::caused_by(root.path(), e))?;
        initial
            .set_markers(markers.bind(&initial))
            .map_err(|e| Error::caused_by(root.path(), e))?;
        bind_external_country_markers(&mut initial, overrides.country_markers)?;
        initial
            .set_global_markers(global_markers)
            .map_err(|e| Error::caused_by(root.path(), e))?;
        let mut politics = politics::bind_people(&initial, political_drafts, date, &mut report)?;
        politics.ruler_skill_bounds =
            politics::ruler_skill_bounds(&root, &files, &defines, &mut report.setup)?;
        let subject_types =
            politics::load_diplomacy(&root, &files, &initial, date, &mut politics, &mut report)?;
        initial
            .set_politics(politics)
            .map_err(|e| Error::caused_by(root.path(), e))?;
        let subject_history_known =
            !subject_history_unknown && report.unapplied_diplomacy_fields.is_empty();
        let mut subject_counts = if subject_history_known {
            initial
                .countries()
                .iter()
                .filter_map(|country| initial.country_entity(&country.id))
                .map(|entity| (entity, 0_u32))
                .collect::<BTreeMap<_, _>>()
        } else {
            BTreeMap::new()
        };
        for relation in &initial.world().politics().relations {
            if relation.kind == augustus_core::DiplomaticKind::Subject
                && let Some(count) = subject_counts.get_mut(&relation.first)
            {
                *count = count
                    .checked_add(1)
                    .ok_or_else(|| Error::caused_by(root.path(), augustus_core::Error::Overflow))?;
            }
        }
        initial
            .set_subjects(augustus_core::Subjects {
                types: subject_types,
                known: subject_history_known,
                counts: subject_counts,
            })
            .map_err(|e| Error::caused_by(root.path(), e))?;
        let subject_country_order = country_order
            .iter()
            .map(|id| initial.country_entity(id).expect("registered country"))
            .collect();
        initial
            .set_subject_country_order(subject_country_order)
            .map_err(|e| Error::caused_by(root.path(), e))?;
        let trade_node_route_orders = economy.trade_node_route_orders();
        let economy = economy.bind(&initial, &province_histories, &mut report.economy)?;
        initial
            .set_economy(economy)
            .map_err(|e| Error::caused_by(root.path(), e))?;
        bind_external_province_buildings(&mut initial, overrides.province_buildings)?;
        initial
            .set_trade_node_route_orders(&trade_node_route_orders)
            .map_err(|e| Error::caused_by(root.path(), e))?;
        bind_external_trade_good_prices(&mut initial, overrides.trade_good_prices_milli)?;
        bind_external_trade_good_price_impacts(
            &mut initial,
            overrides.trade_good_price_impacts_milli,
            overrides.trade_good_price_impact_entries_milli,
            overrides.trade_good_keyed_price_impacts_milli,
            overrides.unresolved_trade_good_price_impacts_milli,
        )?;
        setup.bind(&mut initial, date, &mut report.setup)?;
        let mut naval_construction_import_report = VanillaNavalConstructionImportReport::default();
        if let Some(construction_report) = overrides.naval_construction_report {
            let loaded_date = format!(
                "{}.{}.{}",
                report.start_date.year, report.start_date.month, report.start_date.day
            );
            let (mut inputs, mut import_report) =
                crate::naval_constructions::bind_naval_constructions(
                    construction_report,
                    &initial,
                    &loaded_date,
                );
            inputs.sort_by(|left, right| {
                left.location
                    .0
                    .cmp(&right.location.0)
                    .then_with(|| left.owner.0.cmp(&right.owner.0))
                    .then_with(|| left.unit_type.cmp(&right.unit_type))
                    .then_with(|| left.completion_day.cmp(&right.completion_day))
            });
            if !inputs.is_empty() {
                let ids = initial
                    .enqueue_naval_constructions(&inputs)
                    .map_err(|error| Error::caused_by(root.path(), error))?;
                import_report.imported_count = ids.len();
            }
            naval_construction_import_report = import_report;
        }
        if let Some(saved_levels) = overrides.country_technology_levels {
            let mut technology = initial.world().technology().clone();
            for (tag, levels) in saved_levels {
                let entity = initial.country_entity(tag).ok_or_else(|| {
                    Error::message(
                        root.path(),
                        format!("saved technology references unknown country {tag}"),
                    )
                })?;
                technology
                    .countries
                    .entry(entity)
                    .or_default()
                    .extend(levels.clone());
            }
            initial
                .set_technology(technology)
                .map_err(|error| Error::caused_by(root.path(), error))?;
        }
        let mut governments = governments.bind(
            &initial,
            &attribute_draft,
            &report.start_date,
            &mut report.governments,
        );
        let mut saved_government_countries = BTreeSet::new();
        if let Some(kinds) = overrides.country_government_kinds {
            saved_government_countries.extend(kinds.keys());
        }
        if let Some(reforms) = overrides.country_government_reforms {
            saved_government_countries.extend(reforms.keys());
        }
        if let Some(values) = overrides.country_government_metric_values {
            saved_government_countries.extend(values.keys());
        }
        if let Some(mechanics) = overrides.country_government_mechanic_powers {
            saved_government_countries.extend(mechanics.keys());
        }
        for tag in saved_government_countries {
            let entity = initial.country_entity(tag).ok_or_else(|| {
                Error::message(
                    root.path(),
                    format!("saved government state references unknown country {tag}"),
                )
            })?;
            let state = governments.countries.entry(entity).or_default();
            if let Some(kind) = overrides
                .country_government_kinds
                .and_then(|values| values.get(tag))
            {
                state.kind = kind
                    .clone()
                    .filter(|kind| governments.forms.contains_key(kind));
            }
            if let Some(reforms) = overrides
                .country_government_reforms
                .and_then(|values| values.get(tag))
            {
                state.reforms = reforms.clone().filter(|reforms| {
                    reforms
                        .iter()
                        .all(|reform| governments.reforms.contains_key(reform))
                });
            }
            if let Some(mechanics) = overrides
                .country_government_mechanic_powers
                .and_then(|values| values.get(tag))
            {
                state.mechanic_powers = mechanics.clone();
            }
            if let Some(form) = state
                .kind
                .as_ref()
                .and_then(|kind| governments.forms.get(kind))
            {
                let saved_metrics = overrides
                    .country_government_metric_values
                    .and_then(|values| values.get(tag));
                state.metrics = Some(
                    form.metrics
                        .keys()
                        .map(|name| {
                            (
                                name.clone(),
                                saved_metrics
                                    .and_then(|values| values.get(name))
                                    .copied()
                                    .unwrap_or(None),
                            )
                        })
                        .collect(),
                );
                state.inactive_metrics = governments
                    .metrics
                    .keys()
                    .filter(|name| !form.metrics.contains_key(*name))
                    .filter_map(|name| {
                        saved_metrics
                            .and_then(|values| values.get(name))
                            .map(|value| (name.clone(), *value))
                    })
                    .collect();
            } else {
                state.metrics = None;
                state.inactive_metrics.clear();
            }
        }
        initial
            .set_governments(governments)
            .map_err(|e| Error::caused_by(root.path(), e))?;
        if let Some(ranks) = overrides.country_government_ranks {
            for (tag, rank) in ranks {
                let entity = initial.country_entity(tag).ok_or_else(|| {
                    Error::message(
                        root.path(),
                        format!("saved government rank references unknown country {tag}"),
                    )
                })?;
                initial
                    .set_government_rank(entity, *rank)
                    .map_err(|error| Error::caused_by(root.path(), error))?;
            }
        }
        let mut affiliations = administration.affiliations(&initial);
        affiliations.0.insert(
            crate::imperial_membership::DIMENSION.into(),
            crate::imperial_membership::bind(
                &initial,
                &province_histories,
                imperial_history_unknown,
            ),
        );
        affiliations.0.insert(
            "religious_school".into(),
            religious_schools.bind(&initial, &mut report.religious_schools),
        );
        let mut technology_groups = augustus_core::AffiliationDimension::default();
        let mut unit_types = augustus_core::AffiliationDimension::default();
        for country in initial.countries() {
            let entity = initial
                .country_entity(&country.id)
                .expect("registered country");
            technology_groups
                .members
                .insert(entity, country.technology_group.clone());
            unit_types.members.insert(entity, country.unit_type.clone());
        }
        affiliations
            .0
            .insert("technology_group".into(), technology_groups);
        affiliations.0.insert("unit_type".into(), unit_types);
        initial
            .set_affiliations(affiliations)
            .map_err(|e| Error::caused_by(root.path(), e))?;
        bind_external_province_in_empire(&mut initial, overrides.province_in_empire)?;
        let geographic_scopes = administration.geographic_scopes();
        let mut metrics = crate::province_metrics::bind(
            &initial,
            &attribute_draft,
            report.start_date,
            &metric_history_unknown,
            metric_scoped_history_unknown,
        );
        let country_metrics = crate::country_metrics::bind(
            &initial,
            &attribute_draft,
            report.start_date,
            &country_histories,
            &country_metric_history_unknown,
            country_metric_scoped_history_unknown,
        );
        metrics.definitions.extend(country_metrics.definitions);
        metrics.entities.extend(country_metrics.entities);
        let prestige = crate::prestige::metrics(&initial, &attribute_draft);
        metrics.definitions.extend(prestige.definitions);
        for (entity, values) in prestige.entities {
            metrics.entities.entry(entity).or_default().extend(values);
        }
        let navy_tradition = crate::navy_tradition::metrics(&initial, &attribute_draft);
        metrics.definitions.extend(navy_tradition.definitions);
        for (entity, values) in navy_tradition.entities {
            metrics.entities.entry(entity).or_default().extend(values);
        }
        let liberty_metrics = crate::liberty_desire::metrics(
            &initial,
            report.start_date,
            &country_histories,
            &liberty_history_unknown,
            liberty_scoped_history_unknown,
        );
        metrics.definitions.extend(liberty_metrics.definitions);
        for (entity, values) in liberty_metrics.entities {
            metrics.entities.entry(entity).or_default().extend(values);
        }
        let power = crate::monarch_power::bind(&files, &defines, &initial, &mut report.setup)?;
        metrics.definitions.extend(power.definitions);
        for (entity, values) in power.entities {
            metrics.entities.entry(entity).or_default().extend(values);
        }
        metrics.ceilings.extend(power.ceilings);
        crate::native_development::register_metrics(&mut metrics, overrides.country_metric_values);
        initial
            .set_metrics(metrics)
            .map_err(|e| Error::caused_by("world metrics", e))?;
        bind_external_country_metrics(
            &mut initial,
            overrides.country_metric_values,
            overrides.country_metric_ceilings,
        )?;
        bind_external_province_metrics(&mut initial, overrides.province_metric_values)?;
        apply_historical_prestige_deltas(
            &mut initial,
            &historical_prestige_deltas,
            overrides.country_metric_values,
        )?;
        if let Some(resources) = overrides.country_resources {
            let bound = resources
                .iter()
                .map(|(country, resources)| {
                    let entity = initial.country_entity(country).ok_or_else(|| {
                        Error::message(
                            "external country resources",
                            format!("unknown EU4 resource country {country}"),
                        )
                    })?;
                    Ok((entity, resources.clone()))
                })
                .collect::<Result<BTreeMap<_, _>, Error>>()?;
            initial
                .set_country_resources(bound)
                .map_err(|e| Error::caused_by("external country resources", e))?;
        }
        if let Some(ledgers) = overrides.country_ledgers {
            let bound = ledgers
                .iter()
                .map(|(country, ledger)| {
                    let entity = initial.country_entity(country).ok_or_else(|| {
                        Error::message(
                            "external country ledgers",
                            format!("unknown EU4 ledger country {country}"),
                        )
                    })?;
                    Ok((entity, ledger.clone()))
                })
                .collect::<Result<BTreeMap<_, _>, Error>>()?;
            initial
                .set_country_ledgers(bound)
                .map_err(|e| Error::caused_by("external country ledgers", e))?;
        }
        if let Some(values) = overrides.province_is_city {
            for (province, value) in values {
                if initial.province(*province).is_none() {
                    return Err(Error::message(
                        "external province city status",
                        format!("unknown EU4 city-status province {province}"),
                    ));
                }
                initial
                    .set_province_is_city(*province, *value)
                    .map_err(|e| Error::caused_by("external province city status", e))?;
            }
        }
        let mut attributes =
            attribute_draft.bind(&initial, &mut report.attributes, &report.setup)?;
        let mut accruals = crate::power_income::bind(
            &files,
            &defines,
            &initial,
            &mut attributes,
            &mut report.setup,
        )?;
        if let Some(values) = overrides.country_monthly_power_income {
            crate::power_income::apply_external(&initial, &mut attributes, values)?;
        }
        crate::prestige::bind(&initial, &mut attributes, &mut accruals);
        crate::navy_tradition::bind(&initial, &mut attributes, &mut accruals);
        let convergence = crate::liberty_desire::bind(
            &files,
            &defines,
            &initial,
            &mut attributes,
            &mut report.setup,
        )?;
        administration.initialize_country_ranks(&mut initial, &mut report.administration)?;
        let mut administration = administration.bind(
            &initial,
            &province_histories,
            &mut report.administration,
            &attributes,
            &report.attributes,
            date,
        )?;
        bind_external_province_cores(&initial, &mut administration, overrides.province_cores)?;
        initial
            .set_administration(administration)
            .map_err(|e| Error::caused_by(root.path(), e))?;
        if let Some(values) = overrides.province_raw_autonomy_milli {
            let mut administration = initial.world().administration().clone();
            for (province, value) in values {
                let entity = initial.province_entity(*province).ok_or_else(|| {
                    Error::message(
                        "external province autonomy",
                        format!("unknown EU4 autonomy province {province}"),
                    )
                })?;
                let state = administration.provinces.get_mut(&entity).ok_or_else(|| {
                    Error::message(
                        "external province autonomy",
                        format!("province {province} has no administration state"),
                    )
                })?;
                state.raw_autonomy_micros = value
                    .map(|value| {
                        value.checked_mul(10).ok_or_else(|| {
                            Error::message(
                                "external province autonomy",
                                format!("autonomy value for province {province} overflows"),
                            )
                        })
                    })
                    .transpose()?;
            }
            initial
                .set_administration(administration)
                .map_err(|e| Error::caused_by("external province autonomy", e))?;
        }
        let politics = status_draft.bind(&initial, report.start_date, &mut report.ideas)?;
        initial
            .set_politics(politics)
            .map_err(|e| Error::caused_by(root.path(), e))?;
        bind_external_country_persons(&mut initial, overrides.country_persons)?;
        bind_external_political_office_holders(&mut initial, overrides.political_office_holders)?;
        bind_external_country_statuses(&mut initial, overrides.country_statuses)?;
        bind_external_country_relation_states(&mut initial, overrides.country_relation_states)?;
        bind_external_national_focus(&mut initial, overrides.country_national_focus)?;
        bind_external_advisors(
            &mut initial,
            overrides.advisors,
            &report.setup.advisor_types,
        )?;
        if let Some(values) = overrides.advisors {
            crate::power_income::apply_external_advisors(&initial, &mut attributes, values)?;
        }
        initial
            .set_attributes(attributes.clone())
            .map_err(|e| Error::caused_by(root.path(), e))?;
        let casus_belli =
            crate::CasusBelliTypes::from_root(&root, &files, initial.world().schema())?;
        initial
            .set_justifications(augustus_core::Justifications {
                definitions: casus_belli.definitions(),
                unknown_countries: if report.start_date
                    == (WorldDate {
                        year: 1444,
                        month: 11,
                        day: 11,
                    }) {
                    Default::default()
                } else {
                    initial
                        .countries()
                        .iter()
                        .filter_map(|country| initial.country_entity(&country.id))
                        .collect()
                },
                ..Default::default()
            })
            .map_err(|e| Error::caused_by("common/cb_types", e))?;
        let missions = crate::missions::MissionDraft::load(
            &root,
            &files,
            initial.world(),
            &geographic_scopes,
        )?;
        missions.register_identifiers(&mut initial)?;
        crate::truces::bind(&files, &defines, &mut report.setup)?;
        crate::truces::bind_history(
            &mut initial,
            &historical_truce_grants,
            report.setup.truce_months,
            &mut report,
        )?;
        let mut needed_privileges =
            overrides
                .estate_privileges
                .map_or_else(BTreeSet::new, |values| {
                    values
                        .values()
                        .flat_map(|estates| estates.values())
                        .filter_map(|privileges| privileges.as_ref())
                        .flatten()
                        .cloned()
                        .collect()
                });
        needed_privileges.extend(estates.history_privilege_ids());
        let startup = crate::estates::EstateStartup::load(
            &root,
            &files,
            initial.world().schema(),
            initial
                .world()
                .modifiers()
                .definitions
                .keys()
                .cloned()
                .collect(),
            initial.world().subjects().types.clone(),
            initial
                .world()
                .governments()
                .metrics
                .keys()
                .cloned()
                .collect(),
            initial
                .world()
                .governments()
                .forms
                .keys()
                .cloned()
                .collect(),
            initial
                .world()
                .metrics()
                .definitions
                .keys()
                .cloned()
                .collect(),
            report.setup.advisor_types.clone(),
            report.setup.truce_months,
            geographic_scopes,
            initial.world().affiliations().0["religious_school"]
                .groups
                .clone(),
            casus_belli.types,
            needed_privileges,
            &mut report.estates,
        )?;
        report.casus_belli = casus_belli.report;
        let estates = estates.bind(
            &initial,
            overrides.estate_privileges,
            overrides.estate_states,
            &mut report.estates,
        )?;
        initial
            .set_estates(estates)
            .map_err(|e| Error::caused_by(root.path(), e))?;
        bind_external_estate_privileges(&mut initial, overrides.estate_privileges)?;
        bind_external_estate_states(&mut initial, overrides.estate_states)?;
        if let Some(saved_ideas) = overrides.country_ideas {
            for tag in saved_ideas.keys() {
                if initial.country_entity(tag).is_none() {
                    return Err(Error::message(
                        "external country ideas",
                        format!("unknown saved country {tag}"),
                    ));
                }
            }
        }
        let ideas = ideas.bind(
            &initial,
            report.start_date,
            &mut report.ideas,
            overrides.country_ideas,
        );
        initial
            .set_ideas(ideas)
            .map_err(|e| Error::caused_by(root.path(), e))?;
        initial
            .set_attributes(attributes)
            .map_err(|e| Error::caused_by(root.path(), e))?;
        if overrides.production_inputs.is_some() || overrides.production_value_inputs.is_some() {
            let mut bound = BTreeMap::new();
            let mut price_inputs = BTreeMap::new();
            if let Some(inputs) = overrides.production_inputs {
                for (province, input) in inputs {
                    let entity = initial.province_entity(*province).ok_or_else(|| {
                        Error::message(
                            "external production inputs",
                            format!("unknown EU4 production province {province}"),
                        )
                    })?;
                    bound.insert(entity, input.clone());
                }
            }
            if let Some(inputs) = overrides.production_value_inputs {
                let ExternalProductionValueBindings {
                    states,
                    price_inputs: bound_price_inputs,
                } = bind_external_production_value_inputs(
                    &initial,
                    inputs,
                    overrides.trade_good_price_impacts_milli,
                    overrides.trade_good_price_impact_entries_milli,
                )?;
                price_inputs.extend(bound_price_inputs);
                for (entity, input) in states {
                    if bound.insert(entity, input).is_some() {
                        return Err(Error::message(
                            "external production inputs",
                            "a province cannot provide both production_inputs and production_value_inputs",
                        ));
                    }
                }
            }
            initial
                .set_monthly_production(
                    MonthlyProduction::new(bound).with_price_inputs(price_inputs),
                )
                .map_err(|e| Error::caused_by("external production inputs", e))?;
        }
        if let Some(targets) = overrides.estate_loyalty_targets {
            let bound = targets
                .iter()
                .map(|(country, estates)| {
                    let entity = initial.country_entity(country).ok_or_else(|| {
                        Error::message(
                            "external estate loyalty targets",
                            format!("unknown EU4 estate target country {country}"),
                        )
                    })?;
                    Ok((entity, estates.clone()))
                })
                .collect::<Result<BTreeMap<_, _>, Error>>()?;
            initial
                .set_estate_loyalty_targets(bound)
                .map_err(|e| Error::caused_by("external estate loyalty targets", e))?;
        }
        let countries: std::collections::BTreeSet<_> = initial
            .countries()
            .iter()
            .map(|country| {
                initial
                    .country_entity(&country.id)
                    .expect("registered country")
            })
            .collect();
        let has_country_tax_attribute = countries.iter().all(|entity| {
            initial
                .world()
                .attributes()
                .formulas
                .get(entity)
                .is_some_and(|formulas| {
                    formulas.contains_key(crate::attributes::COUNTRY_ANNUAL_TAX)
                })
        });
        let external_country_annual_tax = if let Some(values) = overrides.country_annual_tax_milli {
            Some(
                values
                    .iter()
                    .map(|(country, amount)| {
                        let entity = initial.country_entity(country).ok_or_else(|| {
                            Error::message(
                                "external annual country tax",
                                format!("unknown EU4 tax country {country}"),
                            )
                        })?;
                        Ok((entity, *amount))
                    })
                    .collect::<Result<BTreeMap<_, _>, Error>>()?,
            )
        } else {
            None
        };
        let external_province_annual_tax = if let Some(values) = overrides.province_annual_tax_milli
        {
            Some(
                values
                    .iter()
                    .map(|(province, amount)| {
                        let entity = initial.province_entity(*province).ok_or_else(|| {
                            Error::message(
                                "external annual province tax",
                                format!("unknown EU4 tax province {province}"),
                            )
                        })?;
                        Ok((entity, *amount))
                    })
                    .collect::<Result<BTreeMap<_, _>, _>>()?,
            )
        } else {
            None
        };
        let mut taxation = Taxation::new("tax_after_autonomy", countries);
        if let Some(values) = external_country_annual_tax {
            taxation = taxation.with_external_country_annual_milli(values);
        }
        if let Some(values) = external_province_annual_tax {
            taxation = taxation.with_external_province_annual_milli(values);
        }
        if has_country_tax_attribute {
            taxation =
                taxation.with_country_annual_attribute(crate::attributes::COUNTRY_ANNUAL_TAX);
        }
        initial
            .set_taxation(taxation)
            .map_err(|e| Error::caused_by(root.path(), e))?;
        let mut cashflow_breakdowns = overrides
            .country_monthly_cashflow_breakdowns
            .cloned()
            .unwrap_or_default();
        let bound_gold_inputs = overrides
            .gold_production_inputs
            .map(|inputs| bind_vanilla_gold_production_inputs(&initial, inputs))
            .transpose()?;
        if let Some(gold_inputs) = overrides.gold_production_inputs {
            let countries: Vec<_> = initial
                .countries()
                .iter()
                .map(|country| country.id.clone())
                .collect();
            for country in countries {
                let entity = initial.country_entity(&country).ok_or_else(|| {
                    Error::message(
                        "external gold production inputs",
                        format!("unknown EU4 gold-production country {country}"),
                    )
                })?;
                let result =
                    calculate_vanilla_gold_production_for_country(&initial, entity, gold_inputs)
                        .map_err(|error| {
                            Error::caused_by("external gold production inputs", error)
                        })?;
                if result.provinces.is_empty() {
                    continue;
                }
                let breakdown = cashflow_breakdowns
                    .entry(country.clone())
                    .or_insert_with(|| CashflowBreakdown {
                        trade_milli: None,
                        trade_income_pass: None,
                        gold_milli: None,
                        expenses_milli: None,
                        expense_categories_milli: BTreeMap::new(),
                    });
                if let (Some(explicit), Some(calculated)) =
                    (breakdown.gold_milli, result.monthly_value_milli)
                    && explicit != calculated
                {
                    return Err(Error::message(
                        "external gold production inputs",
                        format!(
                            "gold total for EU4 country {country} disagrees with its provinces"
                        ),
                    ));
                }
                if result.monthly_value_milli.is_some() || breakdown.gold_milli.is_none() {
                    breakdown.gold_milli = result.monthly_value_milli;
                }
            }
        }
        if overrides.country_monthly_cashflows_milli.is_some()
            || overrides.country_monthly_cashflow_breakdowns.is_some()
            || !cashflow_breakdowns.is_empty()
        {
            let mut bound = overrides
                .country_monthly_cashflows_milli
                .map(|cashflows| {
                    cashflows
                        .iter()
                        .map(|(country, amount)| {
                            let entity = initial.country_entity(country).ok_or_else(|| {
                                Error::message(
                                    "external monthly cashflows",
                                    format!("unknown EU4 cashflow country {country}"),
                                )
                            })?;
                            Ok((entity, *amount))
                        })
                        .collect::<Result<BTreeMap<_, _>, Error>>()
                })
                .transpose()?
                .unwrap_or_default();
            if bound.is_empty() && !cashflow_breakdowns.is_empty() {
                bound = cashflow_breakdowns
                    .iter()
                    .map(|(country, breakdown)| {
                        let entity = initial.country_entity(country).ok_or_else(|| {
                            Error::message(
                                "external monthly cashflow breakdowns",
                                format!("unknown EU4 cashflow country {country}"),
                            )
                        })?;
                        Ok((entity, breakdown.net_milli()))
                    })
                    .collect::<Result<BTreeMap<_, _>, Error>>()?;
            }
            let mut external = MonthlyExternalCashflows::new(bound);
            if !cashflow_breakdowns.is_empty() {
                let bound_breakdowns = cashflow_breakdowns
                    .iter()
                    .map(|(country, breakdown)| {
                        let entity = initial.country_entity(country).ok_or_else(|| {
                            Error::message(
                                "external monthly cashflow breakdowns",
                                format!("unknown EU4 cashflow country {country}"),
                            )
                        })?;
                        Ok((entity, breakdown.clone()))
                    })
                    .collect::<Result<BTreeMap<_, _>, Error>>()?;
                external = external.with_breakdowns(bound_breakdowns);
            }
            if let Some(gold_inputs) = bound_gold_inputs {
                external = external.with_vanilla_gold_production_inputs(gold_inputs);
            }
            initial
                .set_monthly_cashflows(Some(external))
                .map_err(|e| Error::caused_by("external monthly cashflows", e))?;
        }
        initial
            .set_monthly_accruals(accruals)
            .map_err(|e| Error::caused_by(root.path(), e))?;
        initial
            .set_monthly_convergence(Some(convergence))
            .map_err(|e| Error::caused_by(root.path(), e))?;
        if let Some(players) = players {
            for player in players {
                if initial.country_entity(player).is_none() {
                    return Err(Error::message(
                        root.path(),
                        format!("unknown player country {player}"),
                    ));
                }
            }
            let countries: Vec<_> = initial
                .countries()
                .iter()
                .map(|country| country.id.clone())
                .collect();
            for country in countries {
                initial
                    .set_country_controller(
                        initial
                            .country_entity(&country)
                            .expect("registered country"),
                        if players.contains(&country) {
                            augustus_core::CountryController::Human
                        } else {
                            augustus_core::CountryController::Ai
                        },
                    )
                    .map_err(|e| Error::caused_by(root.path(), e))?;
            }
        }
        if initial
            .world()
            .date()
            .map_err(|e| Error::caused_by("variables", e))?
            == Some(augustus_core::WorldDate {
                year: 1444,
                month: 11,
                day: 11,
            })
        {
            initial
                .set_named_values(augustus_core::NamedValues {
                    entities: (0..initial.world().entities().len())
                        .map(|id| (augustus_core::EntityId(id), BTreeMap::new()))
                        .collect(),
                    ..Default::default()
                })
                .map_err(|e| Error::caused_by("variables", e))?;
        }
        bind_external_country_variables(&mut initial, overrides.country_variables)?;
        if !dynamic_global_marker_history && !initial.world().global_markers().known {
            let mut global_markers = initial.world().global_markers().clone();
            for name in startup.global_marker_references() {
                if !history_global_marker_names.contains(&name) {
                    global_markers.values.insert(name, false);
                }
            }
            initial
                .set_global_markers(global_markers)
                .map_err(|e| Error::caused_by("common/on_actions", e))?;
        }
        bind_external_units(
            &mut initial,
            overrides.armies,
            overrides.naval_units,
            overrides.naval_fleets,
        )?;
        bind_external_state_edicts(&mut initial, overrides.state_edicts, &mut report.setup)?;
        report.missions = missions.bind(&mut initial, overrides.country_mission_completions)?;
        // Content identity is based on the bound initial world. Startup effects
        // and AI choices are runtime state and must not vary the hash by seed.
        let content_identity_initial = initial.clone();
        let event_texts = startup.event_texts.clone();
        startup.run(
            &mut initial,
            &mut report.estates,
            &country_order,
            clock_origin,
        )?;
        let monthly_event_rules = startup.monthly_event_rules(&initial, &country_order)?;
        let monthly_event_base = monthly_event_rules
            .as_ref()
            .map(|_| {
                monthly_event_base(&initial).map_err(|error| Error::caused_by(root.path(), error))
            })
            .transpose()?;
        let monthly_event_system_id = match (&monthly_event_rules, monthly_event_base) {
            (Some(rules), Some(base)) => {
                let id = monthly_event_pipeline_id(base, rules)?;
                let system = monthly_event_pipeline(id.clone(), base, rules)
                    .map_err(|error| Error::caused_by(root.path(), error))?;
                initial
                    .select_monthly_system(&system)
                    .map_err(|error| Error::caused_by(root.path(), error))?;
                Some(id)
            }
            (None, None) => None,
            _ => unreachable!("monthly event rules and base are selected together"),
        };
        let yearly_event_rules = startup.yearly_event_rules(&initial, &country_order)?;
        if let Some(system) = &yearly_event_rules {
            initial
                .select_yearly_system(system)
                .map_err(|error| Error::caused_by(root.path(), error))?;
        }
        if !report.estates.pending_startup_effects.is_empty() {
            bind_external_country_metrics(
                &mut initial,
                overrides.country_metric_values,
                overrides.country_metric_ceilings,
            )?;
            bind_external_province_metrics(&mut initial, overrides.province_metric_values)?;
            bind_external_province_in_empire(&mut initial, overrides.province_in_empire)?;
        }
        bind_external_country_persons(&mut initial, overrides.country_persons)?;
        bind_external_political_office_holders(&mut initial, overrides.political_office_holders)?;
        bind_external_country_statuses(&mut initial, overrides.country_statuses)?;
        bind_external_country_relation_states(&mut initial, overrides.country_relation_states)?;
        bind_external_country_markers(&mut initial, overrides.country_markers)?;
        bind_external_estate_privileges(&mut initial, overrides.estate_privileges)?;
        bind_external_estate_states(&mut initial, overrides.estate_states)?;
        startup.resolve_external_privilege_validity(&mut initial, overrides.estate_privileges)?;
        startup.resolve_current_privilege_validity(&mut initial)?;
        report.missions.known_countries = initial.world().objectives().entities.len();
        report.missions.unresolved.retain(|tag, _| {
            !initial
                .world()
                .objectives()
                .entities
                .contains_key(&initial.country_entity(tag).expect("registered country"))
        });
        if report.missions.series > 0 && !report.estates.startup_unresolved.is_empty() {
            report.missions.known_countries = 0;
            for country in initial.countries() {
                report
                    .missions
                    .unresolved
                    .insert(country.id.clone(), "startup effects are unresolved".into());
            }
        }
        report.countries = initial.countries().len();
        report.provinces = initial.provinces().len();
        for country in initial.countries() {
            if !initial.owned_provinces(&country.id).is_empty() {
                report.active_countries += 1;
                if initial.bound_capital(&country.id).is_none() {
                    report.unbound_active_capitals.push(country.id.clone());
                }
            }
        }
        // Fingerprint canonical supported content. Paths and diagnostics are
        // excluded, and the initial state is not duplicated inside every save.
        let mut digest = Sha256::new();
        digest.update(
            serde_json::to_vec(&(
                "augustus-vanilla-calendar-tax:1",
                report.ruleset,
                &report.game_version,
                content_identity_initial.countries(),
                content_identity_initial.provinces(),
            ))
            .map_err(|e| Error::caused_by(root.path(), e))?,
        );
        digest.update(b"\0");
        if let Some(identity) = startup
            .compiled_identity()
            .map_err(|e| Error::caused_by(root.path(), e))?
        {
            digest.update(b"\0startup-program\0");
            digest.update(identity);
        }
        digest.update(
            content_identity_initial
                .content_identity_state()
                .map_err(|e| Error::caused_by(root.path(), e))?
                .as_bytes(),
        );
        let catalog = initial.world().event_catalog();
        if !catalog.option_weights().is_empty() {
            digest.update(b"\0event-option-weights\0");
            digest.update(
                serde_json::to_vec(catalog.option_weights())
                    .map_err(|e| Error::caused_by(root.path(), e))?,
            );
        }
        if let Some(key) = catalog.decision_system_key() {
            digest.update(b"\0event-decision-system\0");
            digest.update(key.as_bytes());
        }
        if !catalog.definitions().is_empty() {
            digest.update(b"\0events\0");
            let events = if catalog.once_events().is_empty() {
                serde_json::to_vec(&(catalog.definitions(), catalog.conditions()))
            } else {
                serde_json::to_vec(&(
                    catalog.definitions(),
                    catalog.conditions(),
                    catalog.once_events(),
                ))
            };
            digest.update(events.map_err(|e| Error::caused_by(root.path(), e))?);
            if !catalog.option_conditions().is_empty() {
                digest.update(b"\0option-conditions\0");
                digest.update(
                    serde_json::to_vec(catalog.option_conditions())
                        .map_err(|e| Error::caused_by(root.path(), e))?,
                );
            }
            if !catalog.pictures().is_empty() {
                digest.update(b"\0event-pictures\0");
                digest.update(
                    serde_json::to_vec(catalog.pictures())
                        .map_err(|e| Error::caused_by(root.path(), e))?,
                );
            }
            if !catalog.locations().is_empty() {
                digest.update(b"\0event-locations\0");
                digest.update(
                    serde_json::to_vec(catalog.locations())
                        .map_err(|e| Error::caused_by(root.path(), e))?,
                );
            }
        }
        if let Some(rules) = &yearly_event_rules {
            digest.update(b"\0yearly-event-rules\0");
            digest.update(serde_json::to_vec(rules).map_err(|e| Error::caused_by(root.path(), e))?);
        }
        if let Some(id) = &monthly_event_system_id {
            digest.update(b"\0monthly-event-system\0");
            digest.update(id.as_bytes());
        }
        let fingerprint: String = digest
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        let content = format!("augustus-vanilla-calendar-tax:1:sha256:{fingerprint}");
        let mut province_mothball_status = initial
            .provinces()
            .iter()
            .map(|province| (province.id, 1))
            .collect::<BTreeMap<_, _>>();
        if let Some(saved_statuses) = overrides.province_mothball_status {
            for (&province, &status) in saved_statuses {
                if initial.province_entity(province).is_some() {
                    province_mothball_status.insert(province, status);
                }
            }
        }
        Ok(Self {
            initial,
            report,
            country_tag_registry,
            province_bitmap: bitmap.source().into(),
            country_sources,
            province_sources,
            province_mothball_status,
            state_maintenance_development_scale_milli,
            event_texts,
            monthly_event_rules,
            monthly_event_system_id,
            monthly_event_base,
            yearly_event_rules,
            naval_construction_import_report,
            content,
        })
    }

    pub fn event_text(&self, event: &str) -> Option<&crate::EventText> {
        self.event_texts.get(event)
    }

    /// Returns the translated yearly pulse rules when they are available.
    pub fn yearly_event_rules(&self) -> Option<&YearlyEventRules> {
        self.yearly_event_rules.as_ref()
    }

    /// Returns the save-order import count and any construction records left unresolved.
    pub fn naval_construction_import_report(&self) -> &VanillaNavalConstructionImportReport {
        &self.naval_construction_import_report
    }

    /// Imports supported multi-order province queues as explicit running state.
    ///
    /// Call after loading the matching save baseline and before snapshot restore
    /// or time advancement. Single orders are already handled by the loader.
    /// The content identity stays unchanged, so old snapshots can still restore
    /// against the same baseline. Repeating an import into a pending queue fails.
    pub fn import_naval_construction_queues(
        &mut self,
        report: &VanillaNavalConstructionReport,
    ) -> Result<(), Error> {
        let loaded_date = format!(
            "{}.{}.{}",
            self.report.start_date.year, self.report.start_date.month, self.report.start_date.day,
        );
        let (mut inputs, mut imported) = crate::naval_constructions::bind_naval_construction_orders(
            report,
            &self.initial,
            &loaded_date,
            true,
        );
        let mut counts = BTreeMap::<u32, usize>::new();
        for record in &report.records {
            *counts.entry(record.province_id).or_default() += 1;
        }
        let queue_locations = counts
            .into_iter()
            .filter(|(_, count)| *count > 1)
            .filter_map(|(province, _)| self.initial.province_entity(province))
            .collect::<BTreeSet<_>>();
        inputs.retain(|input| queue_locations.contains(&input.location));
        inputs.sort_by_key(|input| input.location.0);
        let ids = self
            .initial
            .enqueue_naval_construction_queues(&inputs)
            .map_err(|error| Error::caused_by("naval construction queues", error))?;
        imported.imported_count = self.naval_construction_import_report.imported_count + ids.len();
        self.naval_construction_import_report = imported;
        Ok(())
    }

    /// Returns the raw EU4 province `mothball_status` byte without assigning
    /// unverified game rules to its non-default values.
    pub fn province_mothball_status(&self, province: u32) -> Option<u8> {
        self.province_mothball_status.get(&province).copied()
    }

    pub fn advance(&mut self, days: u64) -> Result<(), augustus_core::Error> {
        self.advance_with_compat_systems(days, None, None, None)
    }

    fn advance_with_compat_systems(
        &mut self,
        days: u64,
        daily: Option<&dyn augustus_core::DailySystem>,
        monthly: Option<&dyn augustus_core::MonthlySystem>,
        yearly: Option<&dyn augustus_core::YearlySystem>,
    ) -> Result<(), augustus_core::Error> {
        let selected_monthly_event_pipeline = match (
            self.monthly_event_rules.as_ref(),
            self.monthly_event_system_id.as_deref(),
            self.monthly_event_base,
        ) {
            (Some(rules), Some(id), Some(base)) if self.initial.monthly_system_id() == Some(id) => {
                Some(monthly_event_pipeline(id, base, rules)?)
            }
            _ => None,
        };
        let selected_monthly = monthly.or_else(|| {
            selected_monthly_event_pipeline
                .as_ref()
                .map(|pipeline| pipeline as &dyn augustus_core::MonthlySystem)
        });
        let selected_yearly_event_rules = self
            .yearly_event_rules
            .as_ref()
            .filter(|rules| self.initial.yearly_system_id() == Some(rules.id.as_str()))
            .map(|rules| rules as &dyn augustus_core::YearlySystem);
        self.initial.advance_with_systems_and_yearly(
            days,
            daily,
            selected_monthly,
            yearly.or(selected_yearly_event_rules),
        )
    }

    pub fn select_daily_system(
        &mut self,
        system: &dyn augustus_core::DailySystem,
    ) -> Result<(), augustus_core::Error> {
        self.initial.select_daily_system(system)
    }

    pub fn select_monthly_system(
        &mut self,
        system: &dyn augustus_core::MonthlySystem,
    ) -> Result<(), augustus_core::Error> {
        self.initial.select_monthly_system(system)
    }

    /// Runs the shared province-autonomy phase for a replaceable monthly system.
    pub fn update_monthly_autonomy(&mut self) -> Result<bool, augustus_core::Error> {
        self.initial.update_monthly_autonomy()
    }

    pub fn select_yearly_system(
        &mut self,
        system: &dyn augustus_core::YearlySystem,
    ) -> Result<(), augustus_core::Error> {
        self.initial.select_yearly_system(system)
    }

    pub fn advance_with_daily_system(
        &mut self,
        days: u64,
        system: &dyn augustus_core::DailySystem,
    ) -> Result<(), augustus_core::Error> {
        self.advance_with_compat_systems(days, Some(system), None, None)
    }

    pub fn advance_with_yearly_system(
        &mut self,
        days: u64,
        system: &dyn augustus_core::YearlySystem,
    ) -> Result<(), augustus_core::Error> {
        self.advance_with_compat_systems(days, None, None, Some(system))
    }

    pub fn advance_with_systems(
        &mut self,
        days: u64,
        daily: Option<&dyn augustus_core::DailySystem>,
        monthly: Option<&dyn augustus_core::MonthlySystem>,
    ) -> Result<(), augustus_core::Error> {
        self.advance_with_compat_systems(days, daily, monthly, None)
    }

    pub fn advance_with_systems_and_yearly(
        &mut self,
        days: u64,
        daily: Option<&dyn augustus_core::DailySystem>,
        monthly: Option<&dyn augustus_core::MonthlySystem>,
        yearly: Option<&dyn augustus_core::YearlySystem>,
    ) -> Result<(), augustus_core::Error> {
        self.advance_with_compat_systems(days, daily, monthly, yearly)
    }

    pub fn save(&self) -> Result<String, augustus_core::Error> {
        self.initial.world().save(&self.content)
    }

    pub fn restore(&mut self, snapshot: &str) -> Result<(), augustus_core::Error> {
        self.initial.restore(snapshot, &self.content)
    }

    pub fn set_vanilla_trade_node_current_values_milli(
        &mut self,
        values: &BTreeMap<String, Option<i64>>,
    ) -> Result<(), Error> {
        self.initial
            .set_trade_node_current_values_milli(values)
            .map_err(|error| Error::caused_by("saved trade-node values", error))
    }

    pub fn set_vanilla_trade_node_runtime_states(
        &mut self,
        values: &BTreeMap<String, augustus_core::TradeNodeRuntimeState>,
    ) -> Result<(), Error> {
        self.initial
            .set_trade_node_runtime_states(values)
            .map_err(|error| Error::caused_by("saved trade-node values", error))
    }

    pub fn province_trade_node_current_milli(&self, id: u32) -> Option<i64> {
        let entity = self.initial.province_entity(id)?;
        let node = self.initial.world().economy().province_node(entity)?;
        self.initial
            .world()
            .trade_node_current_value_milli(node)
            .flatten()
    }

    /// Calculates the native province-growth cache from caller-resolved inputs.
    ///
    /// The compatibility layer must resolve native eligibility, current
    /// devastation, the net monthly change, burn, territory contribution, and
    /// the selected native development component. This method only validates
    /// the province identity and delegates the fixed-point calculation; it does
    /// not read or mutate metrics and does not install a monthly producer.
    pub fn calculate_vanilla_province_native_growth_cache(
        &self,
        id: u32,
        input: VanillaNativeGrowthCacheInput,
    ) -> Result<VanillaNativeGrowthCacheResult, augustus_core::Error> {
        if self.initial.province(id).is_none() {
            return Err(augustus_core::Error::Invalid(
                "unknown native-growth province".into(),
            ));
        }
        calculate_vanilla_native_growth_cache(input)
    }

    /// Applies native province and country growth from caller-resolved inputs.
    ///
    /// The method only validates the province identity and delegates the
    /// explicit write-order calculation. It does not mutate metrics, perform
    /// selector or eligibility checks, or install a monthly system.
    pub fn calculate_vanilla_province_native_growth_application(
        &self,
        id: u32,
        input: VanillaNativeGrowthApplicationInput,
    ) -> Result<VanillaNativeGrowthApplicationResult, augustus_core::Error> {
        if self.initial.province(id).is_none() {
            return Err(augustus_core::Error::Invalid(
                "unknown native-growth province".into(),
            ));
        }
        calculate_vanilla_native_growth_application(input)
    }

    /// Calculates native migration costs and the explicit power gate for a
    /// caller-resolved destination.
    ///
    /// This method only validates that the destination province exists and
    /// delegates the fixed-point calculation. It does not inspect native
    /// membership, submit a command, or mutate country resources.
    pub fn calculate_vanilla_native_migration_cost(
        &self,
        destination: u32,
        input: VanillaNativeMigrationCostInput,
    ) -> Result<VanillaNativeMigrationCostResult, augustus_core::Error> {
        if self.initial.province(destination).is_none() {
            return Err(augustus_core::Error::Invalid(
                "unknown native-migration destination province".into(),
            ));
        }
        calculate_vanilla_native_migration_cost(input)
    }

    /// Calculates a read-only native migration command-gate report.
    ///
    /// The destination is validated against the loaded province index. An
    /// omitted input destination is filled with that explicit target; a
    /// conflicting target is rejected. No command, resource, or migration
    /// state is changed.
    pub fn calculate_vanilla_native_migration_gate(
        &self,
        destination: u32,
        mut input: VanillaNativeMigrationGateInput,
    ) -> Result<VanillaNativeMigrationGateResult, augustus_core::Error> {
        if self.initial.province(destination).is_none() {
            return Err(augustus_core::Error::Invalid(
                "unknown native-migration destination province".into(),
            ));
        }
        if let Some(input_destination) = input.destination_province {
            if input_destination != destination {
                return Err(augustus_core::Error::Invalid(
                    "native-migration destination does not match input".into(),
                ));
            }
        } else {
            input.destination_province = Some(destination);
        }
        let source_country = input
            .source_country
            .map(|country| self.initial.world().entity(country))
            .transpose()?;
        if let Some(country) = source_country
            && country.kind != augustus_core::EntityKind::Country
        {
            return Err(augustus_core::Error::Invalid(
                "native-migration source requires country scope".into(),
            ));
        }
        if let Some(capital) = input.current_capital {
            let capital_entity = self.initial.province_entity(capital).ok_or_else(|| {
                augustus_core::Error::Invalid("unknown native-migration current capital".into())
            })?;
            if let Some(country) = source_country
                && country.capital != Some(capital_entity)
            {
                return Err(augustus_core::Error::Invalid(
                    "native-migration current capital does not match country".into(),
                ));
            }
        }
        calculate_vanilla_native_migration_gate(input)
    }

    pub fn province(&self, id: u32) -> Option<ProvinceView<'_>> {
        let state = self.initial.province(id)?;
        let economy = self.initial.world().economy();
        let entity = self.initial.province_entity(id)?;
        let economic_state = economy.provinces.get(&entity);
        let trade_node = economy.province_node(entity);
        Some(ProvinceView {
            state,
            previous_controller: self
                .initial
                .world()
                .province_previous_controller_state(entity)
                .expect("registered province")
                .map(|controller| {
                    controller.map(|country| {
                        self.initial
                            .world()
                            .entity(country)
                            .expect("validated country")
                            .name
                            .as_str()
                    })
                }),
            tribal_owner: self
                .initial
                .world()
                .province_tribal_owner_state(entity)
                .expect("registered province")
                .map(|owner| {
                    owner.map(|country| {
                        self.initial
                            .world()
                            .entity(country)
                            .expect("validated country")
                            .name
                            .as_str()
                    })
                }),
            terrain: self
                .initial
                .world()
                .province_terrain(entity)
                .expect("registered province"),
            metrics: self.initial.world().metrics().entities.get(&entity),
            administration: self
                .initial
                .world()
                .administration()
                .provinces
                .contains_key(&entity)
                .then(|| {
                    self.initial
                        .world()
                        .administration()
                        .evaluate(self.initial.world(), entity)
                        .expect("validated administration")
                }),
            buildings: economic_state.map(|s| s.buildings.iter().map(String::as_str).collect()),
            trade_node,
            trade_node_outgoing: trade_node.and_then(|name| {
                self.initial
                    .world()
                    .trade_node_route_order(name)
                    .flatten()
                    .map(|routes| routes.iter().map(String::as_str).collect())
            }),
            trade_good_definition: economic_state
                .and_then(|s| s.trade_good.as_ref())
                .and_then(|name| economy.trade_goods.get(name)),
            trade_good_price_milli: economic_state
                .and_then(|s| s.trade_good.as_deref())
                .and_then(|name| economy.current_trade_good_price_milli(name))
                .flatten(),
            modifiers: self.modifier_names(entity),
            modifier_attachments_known: self
                .initial
                .world()
                .modifier_attachments_known(entity)
                .expect("registered province"),
            attributes: self
                .initial
                .world()
                .attributes()
                .formulas
                .get(&entity)
                .map(|formulas| {
                    formulas
                        .keys()
                        .map(|name| {
                            (
                                name.clone(),
                                self.initial
                                    .world()
                                    .attributes()
                                    .evaluate(self.initial.world(), entity, name)
                                    .expect("validated attributes"),
                            )
                        })
                        .collect()
                })
                .unwrap_or_default(),
            declared_modifier_totals_micros: self
                .initial
                .world()
                .modifiers()
                .declared_totals(entity)
                .expect("validated modifier totals"),
        })
    }

    /// Returns the content-defined base price for a province's current trade
    /// good. This is the value loaded from `common/prices`; it is not a
    /// runtime market price and does not imply that trade settlement has run.
    pub fn province_trade_good_base_price_milli(
        &self,
        id: u32,
    ) -> Result<Option<i64>, augustus_core::Error> {
        let entity = self
            .initial
            .province_entity(id)
            .ok_or_else(|| augustus_core::Error::Invalid("unknown province".into()))?;
        let economy = self.initial.world().economy();
        let Some(trade_good) = economy
            .provinces
            .get(&entity)
            .and_then(|state| state.trade_good.as_deref())
        else {
            return Ok(None);
        };
        economy
            .trade_goods
            .get(trade_good)
            .map(|definition| definition.base_price_milli)
            .ok_or_else(|| {
                augustus_core::Error::Invalid(format!(
                    "province {id} references unknown trade good {trade_good}"
                ))
            })
    }

    /// Returns the current runtime price for a province's trade good when one
    /// has been supplied by a save reader; otherwise returns the content
    /// price. An explicit unresolved runtime price remains `None`.
    pub fn province_trade_good_price_milli(
        &self,
        id: u32,
    ) -> Result<Option<i64>, augustus_core::Error> {
        let entity = self
            .initial
            .province_entity(id)
            .ok_or_else(|| augustus_core::Error::Invalid("unknown province".into()))?;
        let economy = self.initial.world().economy();
        let Some(trade_good) = economy
            .provinces
            .get(&entity)
            .and_then(|state| state.trade_good.as_deref())
        else {
            return Ok(None);
        };
        Ok(economy.current_trade_good_price_milli(trade_good).flatten())
    }

    /// Applies the verified ordinary-production value stage using the
    /// province's content-defined trade-good base price. `None` means that
    /// the province has no resolved trade-good price; no value is invented.
    pub fn calculate_vanilla_province_production_base_value(
        &self,
        id: u32,
        input: VanillaProvinceProductionBaseValueInput,
    ) -> Result<Option<VanillaProductionBaseValueResult>, augustus_core::Error> {
        let Some(price) = self.province_trade_good_base_price_milli(id)? else {
            return Ok(None);
        };
        let price = i32::try_from(price).map_err(|_| {
            augustus_core::Error::Invalid(format!(
                "trade-good base price for province {id} is outside the production range"
            ))
        })?;
        Ok(Some(calculate_vanilla_production_base_value(
            VanillaProductionBaseValueInput {
                goods_size_milli: input.goods_size_milli,
                trade_good_price_milli: price,
                flat_modifier_milli: input.flat_modifier_milli,
                external_flat_modifier_milli: input.external_flat_modifier_milli,
                rate_modifier_milli: input.rate_modifier_milli,
                external_rate_modifier_milli: input.external_rate_modifier_milli,
            },
        )))
    }

    /// Applies the verified province state-maintenance formula using the
    /// current tax, production, and manpower development. `None` means one of
    /// those source values is unresolved; distance and modifier inputs remain
    /// the caller's responsibility.
    pub fn calculate_vanilla_province_state_maintenance(
        &self,
        id: u32,
        input: VanillaProvinceStateMaintenanceInput,
    ) -> Result<Option<VanillaStateMaintenanceResult>, augustus_core::Error> {
        let Some((base_tax_milli, base_production_milli, base_manpower_milli)) =
            self.province_state_maintenance_base_values(id)?
        else {
            return Ok(None);
        };

        Ok(Some(calculate_vanilla_state_maintenance(
            VanillaStateMaintenanceInput {
                base_tax_milli,
                base_production_milli,
                base_manpower_milli,
                development_scale_milli: input.development_scale_milli,
                special_development_scale_milli: input.special_development_scale_milli,
                distance: input.distance,
                region_difference_modifier_milli: input.region_difference_modifier_milli,
                owner_relation_modifier_milli: input.owner_relation_modifier_milli,
                country_key61_modifier_milli: input.country_key61_modifier_milli,
                province_key62_modifier_milli: input.province_key62_modifier_milli,
                caller_extra_modifier_milli: input.caller_extra_modifier_milli,
            },
        )?))
    }

    /// Binds current province development and the loaded EU4 factor to caller-resolved contexts.
    /// Contexts are keyed by EU4 province ID; missing defines or development remain unresolved.
    pub fn set_vanilla_state_maintenance_inputs_from_content(
        &mut self,
        contexts: BTreeMap<u32, VanillaContentStateMaintenanceInput>,
    ) -> Result<(), augustus_core::Error> {
        let mut inputs = BTreeMap::new();
        for (province, context) in contexts {
            let entity = self.initial.province_entity(province).ok_or_else(|| {
                augustus_core::Error::Invalid(format!(
                    "unknown EU4 state-maintenance province {province}"
                ))
            })?;
            let base_values = self.province_state_maintenance_base_values(province)?;
            let development_scale = self.state_maintenance_development_scale_milli;
            let mut unresolved = BTreeSet::new();
            if development_scale.is_none() {
                unresolved.insert("development_scale:unknown".into());
            }
            if base_values.is_none() {
                unresolved.insert("base_development:unknown".into());
            }
            let input = development_scale.zip(base_values).map(
                |(
                    development_scale_milli,
                    (base_tax_milli, base_production_milli, base_manpower_milli),
                )| {
                    VanillaStateMaintenanceInput {
                        base_tax_milli,
                        base_production_milli,
                        base_manpower_milli,
                        development_scale_milli,
                        special_development_scale_milli: context.special_development_scale_milli,
                        distance: context.distance,
                        region_difference_modifier_milli: context.region_difference_modifier_milli,
                        owner_relation_modifier_milli: context.owner_relation_modifier_milli,
                        country_key61_modifier_milli: context.country_key61_modifier_milli,
                        province_key62_modifier_milli: context.province_key62_modifier_milli,
                        caller_extra_modifier_milli: context.caller_extra_modifier_milli,
                    }
                },
            );
            inputs.insert(
                entity,
                augustus_core::VanillaStateMaintenanceProvinceInputState { input, unresolved },
            );
        }
        self.initial
            .set_monthly_vanilla_state_maintenance_inputs(Some(inputs))
    }

    fn province_state_maintenance_base_values(
        &self,
        id: u32,
    ) -> Result<Option<(i32, i32, i32)>, augustus_core::Error> {
        let province = self
            .initial
            .province(id)
            .ok_or_else(|| augustus_core::Error::Invalid("unknown province".into()))?;
        let (Some(tax), Some(production), Some(manpower)) = (
            province.tax_milli,
            province.production_milli,
            province.manpower_milli,
        ) else {
            return Ok(None);
        };
        let to_native = |value: i64, name: &str| {
            i32::try_from(value).map_err(|_| {
                augustus_core::Error::Invalid(format!(
                    "province {id} {name} is outside the state-maintenance range"
                ))
            })
        };
        Ok(Some((
            to_native(tax, "tax")?,
            to_native(production, "production")?,
            to_native(manpower, "manpower")?,
        )))
    }

    /// Applies state maintenance using the development factor loaded from EU4 defines.
    /// `None` means either the define or one of the province development values is unresolved.
    pub fn calculate_vanilla_province_state_maintenance_from_content(
        &self,
        id: u32,
        input: VanillaContentStateMaintenanceInput,
    ) -> Result<Option<VanillaStateMaintenanceResult>, augustus_core::Error> {
        if self.initial.province(id).is_none() {
            return Err(augustus_core::Error::Invalid("unknown province".into()));
        }
        let Some(development_scale_milli) = self.state_maintenance_development_scale_milli else {
            return Ok(None);
        };
        self.calculate_vanilla_province_state_maintenance(
            id,
            VanillaProvinceStateMaintenanceInput {
                development_scale_milli,
                special_development_scale_milli: input.special_development_scale_milli,
                distance: input.distance,
                region_difference_modifier_milli: input.region_difference_modifier_milli,
                owner_relation_modifier_milli: input.owner_relation_modifier_milli,
                country_key61_modifier_milli: input.country_key61_modifier_milli,
                province_key62_modifier_milli: input.province_key62_modifier_milli,
                caller_extra_modifier_milli: input.caller_extra_modifier_milli,
            },
        )
    }

    /// Applies the confirmed `A11B80` gold-base stage for a province whose
    /// current trade good is marked as gold. The compatibility layer still
    /// supplies the native fields and the selected global branch value.
    pub fn calculate_vanilla_province_gold_base(
        &self,
        id: u32,
        input: VanillaGoldBaseInput,
    ) -> Result<Option<VanillaGoldBaseResult>, augustus_core::Error> {
        let entity = self
            .initial
            .province_entity(id)
            .ok_or_else(|| augustus_core::Error::Invalid("unknown province".into()))?;
        let economy = self.initial.world().economy();
        let Some(trade_good) = economy
            .provinces
            .get(&entity)
            .and_then(|state| state.trade_good.as_deref())
        else {
            return Ok(None);
        };
        let Some(definition) = economy.trade_goods.get(trade_good) else {
            return Err(augustus_core::Error::Invalid(format!(
                "province {id} references unknown trade good {trade_good}"
            )));
        };
        if definition.gold_type != Some(true) {
            return Ok(None);
        }
        Ok(Some(calculate_vanilla_gold_base(input)))
    }

    /// Applies the confirmed gold-production value stage for a province whose
    /// current trade good is marked as gold. The mine base and gold quantity
    /// remain explicit compatibility inputs; no ordinary trade-good price is
    /// consulted.
    pub fn calculate_vanilla_province_gold_production(
        &self,
        id: u32,
        input: VanillaGoldProductionInput,
    ) -> Result<Option<VanillaGoldProductionResult>, augustus_core::Error> {
        let entity = self
            .initial
            .province_entity(id)
            .ok_or_else(|| augustus_core::Error::Invalid("unknown province".into()))?;
        let economy = self.initial.world().economy();
        let Some(trade_good) = economy
            .provinces
            .get(&entity)
            .and_then(|state| state.trade_good.as_deref())
        else {
            return Ok(None);
        };
        let Some(definition) = economy.trade_goods.get(trade_good) else {
            return Err(augustus_core::Error::Invalid(format!(
                "province {id} references unknown trade good {trade_good}"
            )));
        };
        if definition.gold_type != Some(true) {
            return Ok(None);
        }
        Ok(Some(calculate_vanilla_gold_production(input)))
    }

    pub fn country_attributes(
        &self,
        id: &str,
    ) -> Result<
        std::collections::BTreeMap<String, augustus_core::AttributeEvaluation>,
        augustus_core::Error,
    > {
        let entity = self
            .initial
            .country_entity(id)
            .ok_or_else(|| augustus_core::Error::Invalid("unknown country".into()))?;
        let world = self.initial.world();
        let mut attributes: std::collections::BTreeMap<String, augustus_core::AttributeEvaluation> =
            world
                .attributes()
                .formulas
                .get(&entity)
                .into_iter()
                .flat_map(|formulas| formulas.keys())
                .map(|name| {
                    Ok((
                        name.clone(),
                        world.attributes().evaluate(world, entity, name)?,
                    ))
                })
                .collect::<Result<_, augustus_core::Error>>()?;
        for (country_attribute, province_attribute) in [
            (
                "country_land_forcelimit_flat",
                "province_land_forcelimit_local",
            ),
            (
                "country_naval_forcelimit_flat",
                "province_naval_forcelimit_local",
            ),
        ] {
            let aggregate = world
                .attributes()
                .sum_owned(world, entity, province_attribute)?;
            let country = attributes
                .entry(country_attribute.into())
                .or_insert_with(|| augustus_core::AttributeEvaluation {
                    base_micros: None,
                    known_subtotal_micros: None,
                    value_micros: None,
                    unresolved: [format!("{country_attribute}:missing_formula")].into(),
                    steps: Vec::new(),
                });
            let input = country.known_subtotal_micros;
            let flat = aggregate.known_subtotal_micros;
            let total = input
                .map(|input| {
                    input
                        .checked_add(flat)
                        .ok_or(augustus_core::Error::Overflow)
                })
                .transpose()?;
            let aggregate_complete = aggregate.value_micros.is_some();
            country.unresolved.extend(aggregate.unresolved);
            let complete = country.value_micros.is_some()
                && aggregate_complete
                && country.unresolved.is_empty();
            country.steps.push(augustus_core::AttributeStep {
                input_micros: input,
                flat_micros: flat,
                rate_micros: 0,
                output_micros: total,
                contributions: vec![augustus_core::AttributeContribution {
                    source: "owned_provinces".into(),
                    property: province_attribute.into(),
                    micros: flat,
                    is_rate: false,
                }],
            });
            country.known_subtotal_micros = total;
            country.value_micros = if complete { total } else { None };
        }
        Ok(attributes)
    }

    pub fn country_vanilla_production(
        &self,
        id: &str,
        inputs: &std::collections::BTreeMap<u32, crate::VanillaProductionInputState>,
    ) -> Result<crate::VanillaProductionCountryResult, augustus_core::Error> {
        let country = self
            .initial
            .country_entity(id)
            .ok_or_else(|| augustus_core::Error::Invalid("unknown country".into()))?;
        augustus_core::calculate_vanilla_production_for_country(&self.initial, country, inputs)
    }

    /// Aggregates explicit gold-province inputs over the country's live owned
    /// provinces. Ordinary trade goods are skipped from the result; a loaded
    /// `goldtype = yes` province remains unresolved until its native inputs
    /// have been supplied.
    pub fn country_vanilla_gold_production(
        &self,
        id: &str,
        inputs: &BTreeMap<u32, VanillaGoldProductionInputState>,
    ) -> Result<VanillaGoldProductionCountryResult, augustus_core::Error> {
        let country = self
            .initial
            .country_entity(id)
            .ok_or_else(|| augustus_core::Error::Invalid("unknown country".into()))?;
        calculate_vanilla_gold_production_for_country(&self.initial, country, inputs)
    }

    /// Refreshes the next monthly gold-income calculation from explicit
    /// province inputs. Loaded gold provinces without inputs remain unresolved.
    pub fn set_vanilla_gold_production_inputs(
        &mut self,
        inputs: BTreeMap<u32, VanillaGoldProductionInputState>,
    ) -> Result<(), augustus_core::Error> {
        let bound = bind_vanilla_gold_production_inputs(&self.initial, &inputs)
            .map_err(|error| augustus_core::Error::Invalid(error.to_string()))?;
        self.initial
            .set_monthly_vanilla_gold_production_inputs(Some(bound))
    }

    /// Binds production inputs supplied by a compatibility provider.
    ///
    /// The provider owns the upstream EU4 or external-game translation. This
    /// method only maps EU4 province ids to core entities and installs or
    /// refreshes the monthly executor inputs; it does not infer missing
    /// production inputs.
    pub fn set_vanilla_production_inputs(
        &mut self,
        inputs: BTreeMap<u32, augustus_core::VanillaProductionInputState>,
    ) -> Result<(), augustus_core::Error> {
        let bound = inputs
            .into_iter()
            .map(|(province, state)| {
                let entity = self.initial.province_entity(province).ok_or_else(|| {
                    augustus_core::Error::Invalid(format!(
                        "unknown EU4 production province {province}"
                    ))
                })?;
                Ok((entity, state))
            })
            .collect::<Result<BTreeMap<_, _>, augustus_core::Error>>()?;
        let installed = self.initial.world().monthly_production().is_some();
        if installed {
            self.initial.set_monthly_production_inputs(bound)
        } else {
            self.initial
                .set_monthly_production(MonthlyProduction::new(bound))
        }
    }

    /// Bind production inputs while deriving the country production
    /// efficiency from the owning country's administrative technology.
    ///
    /// A missing technology value remains an unresolved production row. This
    /// method does not infer province quantity or modifier slots; callers must
    /// still provide those values from a verified compatibility source.
    fn country_production_efficiency_milli(
        &self,
        entity: augustus_core::EntityId,
        province: u32,
    ) -> Result<(i32, BTreeSet<String>), augustus_core::Error> {
        let mut unresolved = BTreeSet::new();
        let country_modifier_milli = match self.initial.world().entity(entity)?.owner {
            Some(owner) if self.initial.world().entity(owner)?.kind == EntityKind::Country => {
                match self.initial.world().technology().country_property(
                    owner,
                    "administrative",
                    "production_efficiency",
                ) {
                    TechnologyProperty::Value(value) => {
                        i32::try_from(value / 1_000).map_err(|_| {
                            augustus_core::Error::Invalid(format!(
                                "production efficiency for province {province} is out of range"
                            ))
                        })?
                    }
                    TechnologyProperty::Absent => 0,
                    TechnologyProperty::Unknown => {
                        unresolved.insert("country_production_efficiency:unknown".into());
                        0
                    }
                    TechnologyProperty::Flag(_) => {
                        unresolved.insert("country_production_efficiency:non_numeric".into());
                        0
                    }
                }
            }
            Some(_) => {
                unresolved.insert("country_production_efficiency:owner_not_country".into());
                0
            }
            None => {
                unresolved.insert("country_production_efficiency:unknown_owner".into());
                0
            }
        };
        Ok((country_modifier_milli, unresolved))
    }

    pub fn set_vanilla_production_inputs_from_content(
        &mut self,
        inputs: BTreeMap<u32, VanillaContentProductionInput>,
    ) -> Result<(), augustus_core::Error> {
        let mut resolved = BTreeMap::new();
        for (province, source) in inputs {
            let entity = self.initial.province_entity(province).ok_or_else(|| {
                augustus_core::Error::Invalid(format!("unknown EU4 production province {province}"))
            })?;
            let (country_modifier_milli, unresolved) =
                self.country_production_efficiency_milli(entity, province)?;
            resolved.insert(
                province,
                augustus_core::VanillaProductionInputState {
                    input: Some(VanillaProductionInput {
                        base_value_milli: source.base_value_milli,
                        country_modifier_milli,
                        legacy_province_modifier_milli: source.legacy_province_modifier_milli,
                        queried_province_modifier_milli: source.queried_province_modifier_milli,
                    }),
                    unresolved,
                },
            );
        }
        self.set_vanilla_production_inputs(resolved)
    }

    /// Binds production inputs while deriving the effective trade-good price,
    /// production value and country efficiency from loaded content. Goods
    /// quantity and province modifier slots remain explicit inputs.
    pub fn set_vanilla_production_inputs_from_loaded_content(
        &mut self,
        inputs: BTreeMap<u32, VanillaContentProductionValueInput>,
    ) -> Result<(), augustus_core::Error> {
        let mut resolved = BTreeMap::new();
        for (province, source) in inputs {
            let entity = self.initial.province_entity(province).ok_or_else(|| {
                augustus_core::Error::Invalid(format!("unknown EU4 production province {province}"))
            })?;
            let (country_modifier_milli, mut unresolved) =
                self.country_production_efficiency_milli(entity, province)?;
            let value = self.province_trade_good_price_milli(province)?;
            let state = match value {
                Some(price) => {
                    let price = i32::try_from(price).map_err(|_| {
                        augustus_core::Error::Invalid(format!(
                            "trade-good price for province {province} is outside the production range"
                        ))
                    })?;
                    let result =
                        calculate_vanilla_production_base_value(VanillaProductionBaseValueInput {
                            goods_size_milli: source.goods_size_milli,
                            trade_good_price_milli: price,
                            flat_modifier_milli: source.flat_modifier_milli,
                            external_flat_modifier_milli: source.external_flat_modifier_milli,
                            rate_modifier_milli: source.rate_modifier_milli,
                            external_rate_modifier_milli: source.external_rate_modifier_milli,
                        });
                    augustus_core::VanillaProductionInputState {
                        input: Some(VanillaProductionInput {
                            base_value_milli: result.value_milli,
                            country_modifier_milli,
                            legacy_province_modifier_milli: source.legacy_province_modifier_milli,
                            queried_province_modifier_milli: source.queried_province_modifier_milli,
                        }),
                        unresolved,
                    }
                }
                None => {
                    unresolved.insert("trade_good_price:unknown".into());
                    augustus_core::VanillaProductionInputState {
                        input: None,
                        unresolved,
                    }
                }
            };
            resolved.insert(province, state);
        }
        self.set_vanilla_production_inputs(resolved)
    }

    /// Binds production inputs after deriving goods quantity from the native
    /// six-term quantity stage. The compatibility layer still owns source
    /// mapping; the shared core owns the arithmetic and truncation order.
    pub fn set_vanilla_production_inputs_from_content_goods_size(
        &mut self,
        inputs: BTreeMap<u32, VanillaContentProductionGoodsSizeInput>,
    ) -> Result<(), augustus_core::Error> {
        let value_inputs = inputs
            .into_iter()
            .map(|(province, source)| {
                let goods_size_milli =
                    calculate_vanilla_trade_goods_size(source.goods_size_input).quantity_milli;
                (
                    province,
                    VanillaContentProductionValueInput {
                        goods_size_milli,
                        flat_modifier_milli: source.flat_modifier_milli,
                        external_flat_modifier_milli: source.external_flat_modifier_milli,
                        rate_modifier_milli: source.rate_modifier_milli,
                        external_rate_modifier_milli: source.external_rate_modifier_milli,
                        legacy_province_modifier_milli: source.legacy_province_modifier_milli,
                        queried_province_modifier_milli: source.queried_province_modifier_milli,
                    },
                )
            })
            .collect();
        self.set_vanilla_production_inputs_from_loaded_content(value_inputs)
    }

    /// Binds or refreshes production inputs whose value stage still needs the
    /// EU4 content-defined trade-good price. Upstream quantities and modifiers
    /// remain explicit; a missing price becomes an unresolved production row.
    pub fn set_vanilla_production_value_inputs(
        &mut self,
        inputs: BTreeMap<u32, VanillaExternalProductionInput>,
    ) -> Result<(), augustus_core::Error> {
        let translated = inputs
            .into_iter()
            .map(|(province, source)| {
                let value = self.calculate_vanilla_province_production_base_value(
                    province,
                    VanillaProvinceProductionBaseValueInput {
                        goods_size_milli: source.goods_size_milli,
                        flat_modifier_milli: source.flat_modifier_milli,
                        external_flat_modifier_milli: source.external_flat_modifier_milli,
                        rate_modifier_milli: source.rate_modifier_milli,
                        external_rate_modifier_milli: source.external_rate_modifier_milli,
                    },
                )?;
                let state = match value {
                    Some(value) => augustus_core::VanillaProductionInputState {
                        input: Some(VanillaProductionInput {
                            base_value_milli: value.value_milli,
                            country_modifier_milli: source.country_modifier_milli,
                            legacy_province_modifier_milli: source.legacy_province_modifier_milli,
                            queried_province_modifier_milli: source.queried_province_modifier_milli,
                        }),
                        unresolved: Default::default(),
                    },
                    None => augustus_core::VanillaProductionInputState {
                        input: None,
                        unresolved: ["missing_trade_good_base_price".into()].into(),
                    },
                };
                Ok((province, state))
            })
            .collect::<Result<BTreeMap<_, _>, augustus_core::Error>>()?;
        self.set_vanilla_production_inputs(translated)
    }

    /// Applies country metric values supplied by a compatibility provider.
    ///
    /// This is a state binding point for values obtained from another
    /// simulation or an external save reader. Definitions and omitted values
    /// remain unchanged; no EU4 metric producer is inferred here.
    pub fn set_country_metric_values(
        &mut self,
        values: BTreeMap<String, BTreeMap<String, Option<i64>>>,
    ) -> Result<(), augustus_core::Error> {
        let bound = values
            .into_iter()
            .map(|(country, metrics)| {
                let entity = self.initial.country_entity(&country).ok_or_else(|| {
                    augustus_core::Error::Invalid(format!("unknown EU4 metric country {country}"))
                })?;
                Ok((entity, metrics))
            })
            .collect::<Result<BTreeMap<_, _>, augustus_core::Error>>()?;
        self.initial.set_metric_values(bound)
    }

    /// Applies per-country metric growth ceilings supplied by a compatibility
    /// provider. Omitted countries and metrics keep their current knowledge.
    pub fn set_country_metric_ceilings(
        &mut self,
        ceilings: BTreeMap<String, BTreeMap<String, Option<i64>>>,
    ) -> Result<(), augustus_core::Error> {
        let bound = ceilings
            .into_iter()
            .map(|(country, metrics)| {
                let entity = self.initial.country_entity(&country).ok_or_else(|| {
                    augustus_core::Error::Invalid(format!(
                        "unknown EU4 metric ceiling country {country}"
                    ))
                })?;
                Ok((entity, metrics))
            })
            .collect::<Result<BTreeMap<_, _>, augustus_core::Error>>()?;
        self.initial.set_metric_ceilings(bound)
    }

    /// Applies country resources supplied by a compatibility provider.
    pub fn set_country_resources(
        &mut self,
        resources: BTreeMap<String, CountryResources>,
    ) -> Result<(), augustus_core::Error> {
        let bound = resources
            .into_iter()
            .map(|(country, resources)| {
                let entity = self.initial.country_entity(&country).ok_or_else(|| {
                    augustus_core::Error::Invalid(format!("unknown EU4 resource country {country}"))
                })?;
                Ok((entity, resources))
            })
            .collect::<Result<BTreeMap<_, _>, augustus_core::Error>>()?;
        self.initial.set_country_resources(bound)
    }

    /// Applies translated country-ledger state supplied by a compatibility
    /// provider. Category identifiers are kept opaque to this crate.
    pub fn set_country_ledgers(
        &mut self,
        ledgers: BTreeMap<String, CountryLedger>,
    ) -> Result<(), augustus_core::Error> {
        let bound = ledgers
            .into_iter()
            .map(|(country, ledger)| Ok((self.ledger_country_entity(&country)?, ledger)))
            .collect::<Result<BTreeMap<_, _>, augustus_core::Error>>()?;
        self.initial.set_country_ledgers(bound)
    }

    /// Records an immediate country income by EU4 tag and native ledger slot.
    pub fn record_country_income(
        &mut self,
        country: &str,
        category: u32,
        amount_milli: i64,
    ) -> Result<(), augustus_core::Error> {
        let country = self.ledger_country_entity(country)?;
        self.initial
            .record_country_income(country, category, amount_milli)
    }

    /// Records an immediate country expense by EU4 tag and native ledger slot.
    pub fn record_country_expense(
        &mut self,
        country: &str,
        category: u32,
        amount_milli: i64,
    ) -> Result<(), augustus_core::Error> {
        let country = self.ledger_country_entity(country)?;
        self.initial
            .record_country_expense(country, category, amount_milli)
    }

    /// Records delayed country income by EU4 tag and native ledger slot.
    pub fn record_delayed_country_income(
        &mut self,
        country: &str,
        category: u32,
        amount_milli: i64,
    ) -> Result<(), augustus_core::Error> {
        let country = self.ledger_country_entity(country)?;
        self.initial
            .record_delayed_country_income(country, category, amount_milli)
    }

    /// Records delayed country expense by EU4 tag and native ledger slot.
    pub fn record_delayed_country_expense(
        &mut self,
        country: &str,
        category: u32,
        amount_milli: i64,
    ) -> Result<(), augustus_core::Error> {
        let country = self.ledger_country_entity(country)?;
        self.initial
            .record_delayed_country_expense(country, category, amount_milli)
    }

    /// Consolidates translated country ledgers at the native monthly boundary.
    /// Omitted countries use the native filtered-expense set; an explicit
    /// empty set requests an unfiltered total. Unknown EU4 tags fail before
    /// any ledger is changed.
    pub fn consolidate_country_ledgers_monthly(
        &mut self,
        expense_filtered_categories: BTreeMap<String, BTreeSet<u32>>,
    ) -> Result<(), augustus_core::Error> {
        let bound = expense_filtered_categories
            .into_iter()
            .map(|(country, categories)| {
                let entity = self.initial.country_entity(&country).ok_or_else(|| {
                    augustus_core::Error::Invalid(format!(
                        "unknown EU4 ledger filter country {country}"
                    ))
                })?;
                Ok((entity, categories))
            })
            .collect::<Result<BTreeMap<_, _>, augustus_core::Error>>()?;
        self.initial.consolidate_country_ledgers_monthly(&bound)
    }

    fn ledger_country_entity(
        &self,
        country: &str,
    ) -> Result<augustus_core::EntityId, augustus_core::Error> {
        self.initial.country_entity(country).ok_or_else(|| {
            augustus_core::Error::Invalid(format!("unknown EU4 ledger country {country}"))
        })
    }

    /// Performs the translated country-ledger annual rollover.
    pub fn rollover_country_ledgers_annual(&mut self) -> Result<(), augustus_core::Error> {
        self.initial.rollover_country_ledgers_annual()
    }

    /// Flushes all translated country ledgers and returns balances by EU4 tag.
    pub fn flush_country_ledgers_pending(
        &mut self,
    ) -> Result<BTreeMap<String, i64>, augustus_core::Error> {
        self.initial
            .flush_country_ledgers_pending()?
            .into_iter()
            .map(|(entity, balance)| {
                let country = self.initial.countries().get(entity.0).ok_or_else(|| {
                    augustus_core::Error::Invalid("country ledger entity has no EU4 tag".into())
                })?;
                Ok((country.id.clone(), balance))
            })
            .collect()
    }

    /// Applies signed monthly cashflows supplied by another simulation or a
    /// translated save reader. Positive values are receipts; negative values
    /// are expenses. The core settles them after tax and production.
    pub fn set_country_monthly_cashflows(
        &mut self,
        cashflows: BTreeMap<String, Option<i64>>,
    ) -> Result<(), augustus_core::Error> {
        let bound = cashflows
            .into_iter()
            .map(|(country, amount)| {
                let entity = self.initial.country_entity(&country).ok_or_else(|| {
                    augustus_core::Error::Invalid(format!("unknown EU4 cashflow country {country}"))
                })?;
                Ok((entity, amount))
            })
            .collect::<Result<BTreeMap<_, _>, augustus_core::Error>>()?;
        if self.initial.monthly_cashflows().is_some() {
            self.initial.set_monthly_cashflow_inputs(bound)
        } else {
            self.initial
                .set_monthly_cashflows(Some(MonthlyExternalCashflows::new(bound)))
        }
    }

    /// Binds the verified monthly ledger categories for translated cashflows.
    /// Trade and gold are positive receipts; expenses are supplied as a
    /// positive amount and subtracted when all three categories are known.
    pub fn set_country_monthly_cashflow_breakdowns(
        &mut self,
        breakdowns: BTreeMap<String, CashflowBreakdown>,
    ) -> Result<(), augustus_core::Error> {
        let bound = breakdowns
            .into_iter()
            .map(|(country, breakdown)| {
                let entity = self.initial.country_entity(&country).ok_or_else(|| {
                    augustus_core::Error::Invalid(format!(
                        "unknown EU4 cashflow breakdown country {country}"
                    ))
                })?;
                Ok((entity, breakdown))
            })
            .collect::<Result<BTreeMap<_, _>, augustus_core::Error>>()?;
        if self.initial.monthly_cashflows().is_some() {
            self.initial.set_monthly_cashflow_breakdowns(bound)
        } else {
            let amounts = bound
                .iter()
                .map(|(&country, breakdown)| (country, breakdown.net_milli()))
                .collect();
            self.initial.set_monthly_cashflows(Some(
                MonthlyExternalCashflows::new(amounts).with_breakdowns(bound),
            ))
        }
    }

    /// Replaces category 8 rows with fort-maintenance costs from resolved province inputs.
    /// Inputs must cover every active fort row for each listed country. The method replaces only
    /// category 8 detail; aggregate expense and net-cashflow totals remain caller-supplied.
    pub fn set_vanilla_fort_maintenance_expense_categories(
        &mut self,
        inputs: BTreeMap<String, BTreeMap<u32, augustus_core::VanillaFortMaintenanceInput>>,
    ) -> Result<(), augustus_core::Error> {
        if inputs.is_empty() {
            return Ok(());
        }

        let mut totals = BTreeMap::new();
        let mut seen_provinces = BTreeSet::new();
        for (country, province_inputs) in &inputs {
            self.ledger_country_entity(country)?;
            let mut total = 0_i64;
            let mut has_owned_province = false;
            for (&province, &input) in province_inputs {
                if !seen_provinces.insert(province) {
                    return Err(augustus_core::Error::Invalid(format!(
                        "fort-maintenance province {province} is assigned to multiple countries"
                    )));
                }
                let entity = self.initial.province_entity(province).ok_or_else(|| {
                    augustus_core::Error::Invalid(format!(
                        "unknown EU4 fort-maintenance province {province}"
                    ))
                })?;
                if self.initial.world().entity(entity)?.owner.is_none() {
                    continue;
                }
                let amount = augustus_core::calculate_vanilla_fort_maintenance(input)?.amount_milli;
                if amount < 0 {
                    return Err(augustus_core::Error::Invalid(
                        "fort-maintenance expense must be nonnegative".into(),
                    ));
                }
                has_owned_province = true;
                total = total
                    .checked_add(amount)
                    .ok_or(augustus_core::Error::Overflow)?;
            }
            if has_owned_province {
                totals.insert(country.clone(), total);
            }
        }

        let mut breakdowns = BTreeMap::new();
        if let Some(current) = self.initial.monthly_cashflows() {
            for country in self.initial.countries() {
                let entity = self
                    .initial
                    .country_entity(&country.id)
                    .expect("registered country");
                if let Some(breakdown) = current.breakdowns.get(&entity) {
                    breakdowns.insert(country.id.clone(), breakdown.clone());
                }
            }
        }
        for country in inputs.keys() {
            let breakdown =
                breakdowns
                    .entry(country.clone())
                    .or_insert_with(|| CashflowBreakdown {
                        trade_milli: None,
                        trade_income_pass: None,
                        gold_milli: None,
                        expenses_milli: None,
                        expense_categories_milli: BTreeMap::new(),
                    });
            breakdown
                .expense_categories_milli
                .remove(&augustus_core::COUNTRY_LEDGER_EXPENSE_FORT_MAINTENANCE_CATEGORY);
            if let Some(&amount) = totals.get(country) {
                breakdown.expense_categories_milli.insert(
                    augustus_core::COUNTRY_LEDGER_EXPENSE_FORT_MAINTENANCE_CATEGORY,
                    amount,
                );
            }
        }
        self.set_country_monthly_cashflow_breakdowns(breakdowns)
    }

    /// Finds owner-held fort buildings and derives their local multiplier from
    /// the loaded building definition before replacing category 8.
    ///
    /// Each map entry must cover every fort province owned by that country.
    /// Modifier sources and special-case conditions remain explicit inputs;
    /// unknown building state or incomplete coverage is an error.
    pub fn set_vanilla_fort_maintenance_expense_categories_from_buildings(
        &mut self,
        contexts: BTreeMap<String, BTreeMap<u32, VanillaFortMaintenanceContextInput>>,
    ) -> Result<(), augustus_core::Error> {
        if contexts.is_empty() {
            return Ok(());
        }

        let resolved = {
            let world = self.initial.world();
            let economy = world.economy();
            let mut resolved = BTreeMap::new();

            for (country, mut province_contexts) in contexts {
                let country_entity = self.ledger_country_entity(&country)?;
                let mut province_inputs = BTreeMap::new();

                for province in self.initial.provinces() {
                    let province_entity = self
                        .initial
                        .province_entity(province.id)
                        .expect("registered province");
                    if world.entity(province_entity)?.owner != Some(country_entity) {
                        continue;
                    }

                    let state = economy.provinces.get(&province_entity).ok_or_else(|| {
                        augustus_core::Error::Invalid(format!(
                            "fort-maintenance building state is unknown for owned province {}",
                            province.id
                        ))
                    })?;
                    let mut fort_level_milli = None;
                    for building_name in &state.buildings {
                        let definition = economy.buildings.get(building_name).ok_or_else(|| {
                            augustus_core::Error::Invalid(format!(
                                "unknown building {building_name} in province {}",
                                province.id
                            ))
                        })?;
                        if definition.role != augustus_core::BuildingRole::Fortification {
                            continue;
                        }

                        let level = definition.fort_level_milli.ok_or_else(|| {
                            augustus_core::Error::Invalid(format!(
                                "fortification building {building_name} has no loaded fort level"
                            ))
                        })?;
                        if level <= 0 {
                            return Err(augustus_core::Error::Invalid(format!(
                                "fortification building {building_name} has a nonpositive fort level"
                            )));
                        }
                        if fort_level_milli.replace(level).is_some() {
                            return Err(augustus_core::Error::Invalid(format!(
                                "province {} has multiple fortification buildings",
                                province.id
                            )));
                        }
                    }

                    if let Some(fort_level_milli) = fort_level_milli {
                        let context = province_contexts.remove(&province.id).ok_or_else(|| {
                            augustus_core::Error::Invalid(format!(
                                "missing fort-maintenance context for province {}",
                                province.id
                            ))
                        })?;
                        let mothballed = self.province_mothball_status(province.id) == Some(2);
                        province_inputs.insert(
                            province.id,
                            augustus_core::VanillaFortMaintenanceInput {
                                base_cost_milli: EU4_1_37_5_FORT_BASE_COST_MILLI,
                                country_modifier_milli: context.country_modifier_milli,
                                province_modifier_milli: context.province_modifier_milli,
                                conversion_factor_micros:
                                    EU4_1_37_5_CURRENCY_CONVERSION_FACTOR_MICROS,
                                local_multiplier_milli: i32::try_from(fort_level_milli)
                                    .map_err(|_| augustus_core::Error::Overflow)?,
                                relationship_modifier_milli: context.relationship_modifier_milli,
                                apply_relationship_modifier: context.apply_relationship_modifier,
                                halve_for_special_case: context.halve_for_special_case
                                    || mothballed,
                            },
                        );
                    }
                }

                if let Some(province) = province_contexts.keys().next() {
                    return Err(augustus_core::Error::Invalid(format!(
                        "fort-maintenance context references a non-fort or non-owned province {province}"
                    )));
                }
                resolved.insert(country, province_inputs);
            }
            resolved
        };

        self.set_vanilla_fort_maintenance_expense_categories(resolved)
    }

    /// Refreshes verified monthly monarch-power income from a compatibility
    /// provider. Omitted countries and metrics keep their current formulas;
    /// explicit `None` keeps the selected income unresolved.
    pub fn set_country_monthly_power_income(
        &mut self,
        income: BTreeMap<String, BTreeMap<String, Option<i64>>>,
    ) -> Result<(), augustus_core::Error> {
        let mut attributes = self.initial.world().attributes().clone();
        crate::power_income::apply_external(&self.initial, &mut attributes, &income)
            .map_err(|error| augustus_core::Error::Invalid(error.to_string()))?;
        self.initial.set_attributes(attributes)
    }

    /// Rebinds authoritative advisor slots supplied by a compatibility
    /// provider while the world is running. The advisor skill terms become
    /// readable immediately; other country refreshes remain governed by their
    /// own resolved sources.
    pub fn set_advisors(
        &mut self,
        values: BTreeMap<String, Vec<VanillaExternalAdvisor>>,
    ) -> Result<(), augustus_core::Error> {
        bind_external_advisors(
            &mut self.initial,
            Some(&values),
            &self.report.setup.advisor_types,
        )
        .map_err(|error| augustus_core::Error::Invalid(error.to_string()))?;
        let mut attributes = self.initial.world().attributes().clone();
        crate::power_income::apply_external_advisors(&self.initial, &mut attributes, &values)
            .map_err(|error| augustus_core::Error::Invalid(error.to_string()))?;
        self.initial.set_attributes(attributes)
    }

    /// Applies estate loyalty targets supplied by a compatibility provider.
    ///
    /// The provider owns the upstream EU4 or external-game translation. This
    /// method only maps EU4 tags to core countries and requires each estate to
    /// already exist in the current world; it never creates an estate state.
    pub fn set_estate_loyalty_targets(
        &mut self,
        targets: BTreeMap<String, BTreeMap<String, Option<i64>>>,
    ) -> Result<(), augustus_core::Error> {
        let bound = targets
            .into_iter()
            .map(|(country, estates)| {
                let entity = self.initial.country_entity(&country).ok_or_else(|| {
                    augustus_core::Error::Invalid(format!(
                        "unknown EU4 estate target country {country}"
                    ))
                })?;
                Ok((entity, estates))
            })
            .collect::<Result<BTreeMap<_, _>, augustus_core::Error>>()?;
        self.initial.set_estate_loyalty_targets(bound)
    }

    pub fn country(&self, id: &str) -> Option<CountryView<'_>> {
        self.initial.country(id).map(|state| CountryView {
            metrics: self
                .initial
                .country_entity(id)
                .and_then(|entity| self.initial.world().metrics().entities.get(&entity)),
            metric_ceilings: self
                .initial
                .country_entity(id)
                .and_then(|entity| self.initial.world().metrics().ceilings.get(&entity)),
            advisors: self
                .initial
                .country_entity(id)
                .and_then(|entity| self.initial.world().advisors().countries.get(&entity))
                .and_then(Option::as_ref),
            advisors_known: self.initial.country_entity(id).is_some_and(|entity| {
                self.initial
                    .world()
                    .advisors()
                    .countries
                    .contains_key(&entity)
            }),
            advisor_slots: self.initial.country_entity(id).and_then(|entity| {
                let advisors = self.initial.world().advisors();
                let slots = [
                    augustus_core::RulerSkill::Administration,
                    augustus_core::RulerSkill::Diplomacy,
                    augustus_core::RulerSkill::Military,
                ]
                .into_iter()
                .filter_map(|category| {
                    advisors
                        .advisor_in_category(entity, category)
                        .map(|advisor| (category, advisor.cloned()))
                })
                .collect::<BTreeMap<_, _>>();
                (!slots.is_empty()).then_some(slots)
            }),
            estates: self
                .initial
                .country_entity(id)
                .and_then(|entity| self.initial.world().estates().countries.get(&entity)),
            declared_estate_effects: self
                .initial
                .world()
                .estates()
                .declared_effects_with_world(
                    self.initial.world(),
                    self.initial.country_entity(id).expect("registered country"),
                )
                .unwrap_or_else(|error| augustus_core::EstateEvaluation {
                    unresolved: [format!("estates:evaluation_failed:{error}")].into(),
                    ..Default::default()
                }),
            ideas: self
                .initial
                .country_entity(id)
                .and_then(|entity| self.initial.world().ideas().countries.get(&entity)),
            declared_idea_effects: self
                .initial
                .world()
                .ideas()
                .declared_effects(self.initial.country_entity(id).expect("registered country"))
                .expect("validated idea effects"),
            last_monthly_tax: self
                .initial
                .world()
                .taxation()
                .and_then(|t| t.last_settlement.as_ref())
                .and_then(|last| {
                    last.accounts
                        .get(&self.initial.country_entity(id).expect("registered country"))
                }),
            last_monthly_production: self
                .initial
                .world()
                .monthly_production()
                .and_then(|p| p.last_settlement.as_ref())
                .and_then(|last| {
                    last.accounts
                        .get(&self.initial.country_entity(id).expect("registered country"))
                }),
            last_monthly_cashflow: self
                .initial
                .world()
                .monthly_cashflows()
                .and_then(|c| c.last_settlement.as_ref())
                .and_then(|last| {
                    last.accounts
                        .get(&self.initial.country_entity(id).expect("registered country"))
                }),
            government: self
                .initial
                .world()
                .governments()
                .countries
                .get(&self.initial.country_entity(id).expect("registered country")),
            declared_government_effects: self
                .initial
                .world()
                .governments()
                .declared_effects(self.initial.country_entity(id).expect("registered country"))
                .expect("validated government effects"),
            administrative_areas: self.initial.country_entity(id).and_then(|entity| {
                self.initial
                    .world()
                    .administration()
                    .country_areas
                    .get(&entity)
            }),
            state,
            owned_provinces: self.initial.owned_provinces(id),
            bound_capital: self.initial.bound_capital(id),
            original_capital: self
                .initial
                .country_original_capital_state(id)
                .expect("registered country"),
            capital_history: self
                .initial
                .country_capital_history(id)
                .expect("registered country"),
            politics: self
                .initial
                .world()
                .politics()
                .countries
                .get(&self.initial.country_entity(id).expect("registered country")),
            diplomacy: self
                .diplomacy()
                .into_iter()
                .filter(|r| r.first == id || r.second == id)
                .collect(),
            political_offices: self
                .initial
                .world()
                .politics()
                .offices
                .iter()
                .filter_map(|(name, holder)| {
                    (self
                        .initial
                        .world()
                        .entity(*holder)
                        .expect("validated holder")
                        .name
                        == id)
                        .then_some(name.as_str())
                })
                .collect(),
            technology: self
                .initial
                .world()
                .technology()
                .countries
                .get(&self.initial.country_entity(id).expect("registered country")),
            province_attribute_totals: ATTRIBUTE_NAMES
                .into_iter()
                .map(|name| {
                    (
                        name.into(),
                        self.initial
                            .world()
                            .attributes()
                            .sum_owned(
                                self.initial.world(),
                                self.initial.country_entity(id).expect("registered country"),
                                name,
                            )
                            .unwrap_or_else(|error| augustus_core::AttributeAggregate {
                                province_count: self.initial.owned_provinces(id).len(),
                                known_subtotal_micros: 0,
                                value_micros: None,
                                unresolved: [format!(
                                    "attribute_aggregate:evaluation_failed:{name}:{error}"
                                )]
                                .into(),
                            }),
                    )
                })
                .collect(),
            resources: self
                .initial
                .world()
                .country_resources()
                .get(&self.initial.country_entity(id).expect("registered country")),
            modifiers: self
                .modifier_names(self.initial.country_entity(id).expect("registered country")),
            modifier_attachments_known: self
                .initial
                .world()
                .modifier_attachments_known(
                    self.initial.country_entity(id).expect("registered country"),
                )
                .expect("registered country"),
            declared_modifier_totals_micros: self
                .initial
                .world()
                .modifiers()
                .declared_totals(self.initial.country_entity(id).expect("registered country"))
                .expect("validated modifier totals"),
            flags: self
                .initial
                .world()
                .markers()
                .entities
                .get(&self.initial.country_entity(id).expect("registered country")),
            flag_namespace_known: self
                .initial
                .world()
                .marker_namespace_known(
                    self.initial.country_entity(id).expect("registered country"),
                )
                .expect("registered country"),
        })
    }

    fn modifier_names(&self, entity: augustus_core::EntityId) -> Vec<&str> {
        self.initial
            .world()
            .modifiers()
            .active
            .get(&entity)
            .map(|active| active.keys().map(String::as_str).collect())
            .unwrap_or_default()
    }

    pub fn diplomacy(&self) -> Vec<RelationView<'_>> {
        self.initial
            .world()
            .politics()
            .relations
            .iter()
            .map(|r| politics::relation_view(&self.initial, r))
            .collect()
    }

    pub fn palette(&self) -> Vec<MapEntry> {
        let mut entries: Vec<_> = self
            .initial
            .provinces()
            .iter()
            .filter(|p| p.class != ProvinceClass::Unused)
            .map(|p| {
                let color = match p.class {
                    ProvinceClass::Sea | ProvinceClass::Lake => [32, 64, 96],
                    ProvinceClass::Wasteland | ProvinceClass::Unused => [48, 48, 48],
                    ProvinceClass::Land => p
                        .owner
                        .as_deref()
                        .and_then(|id| self.initial.country(id))
                        .map_or([112, 112, 104], |c| c.color),
                };
                MapEntry {
                    province: p.id,
                    color: p.color,
                    display_color: color,
                }
            })
            .collect();
        entries.sort_by_key(|p| p.color);
        entries
    }
}

pub(super) fn direct_txt(path: &Path, directory: &str) -> bool {
    path.parent() == Some(Path::new(directory))
        && path
            .extension()
            .is_some_and(|s| s.eq_ignore_ascii_case("txt"))
}

fn history_id(path: &Path) -> Result<String, Error> {
    path.file_stem()
        .and_then(|s| s.to_str())
        .and_then(|s| s.split([' ', '-']).next())
        .filter(|s| !s.is_empty())
        .map(String::from)
        .ok_or_else(|| Error::message(path, "invalid history filename"))
}

pub(super) fn source(parsed: &ParsedText<'_>, location: SourceLocation) -> LoadedSource {
    LoadedSource {
        path: parsed.source().into(),
        line: location.line,
        column: location.column,
    }
}

pub(super) fn equal(op: Option<Operator>, fail: &impl Fn(&str) -> Error) -> Result<(), Error> {
    if op.is_some_and(|o| o != Operator::Equal) {
        Err(fail("expected assignment operator"))
    } else {
        Ok(())
    }
}

fn required_u32(
    parsed: &ParsedText<'_>,
    reader: ObjectReader<'_, '_, TextEncoding>,
    name: &str,
) -> Result<u32, Error> {
    for (key, op, value) in reader.fields() {
        if key.read_string() == name {
            let fail = |m: &str| {
                Error::message(parsed.source(), m).with_location(parsed.field_location(&key))
            };
            equal(op, &fail)?;
            return value
                .read_string()
                .map_err(|_| fail("expected integer"))?
                .parse()
                .map_err(|_| fail("invalid integer"));
        }
    }
    Err(Error::message(parsed.source(), format!("missing {name}")))
}

fn map_file(
    parsed: &ParsedText<'_>,
    reader: ObjectReader<'_, '_, TextEncoding>,
    name: &str,
) -> Result<String, Error> {
    for (key, op, value) in reader.fields() {
        if key.read_string() == name {
            let fail = |m: &str| {
                Error::message(parsed.source(), m).with_location(parsed.field_location(&key))
            };
            equal(op, &fail)?;
            return value
                .read_string()
                .map(|v| format!("map/{v}"))
                .map_err(|_| fail("expected map filename"));
        }
    }
    Err(Error::message(
        parsed.source(),
        format!("missing map {name}"),
    ))
}

fn optional_map_file(
    parsed: &ParsedText<'_>,
    reader: ObjectReader<'_, '_, TextEncoding>,
    name: &str,
) -> Result<Option<String>, Error> {
    for (key, op, value) in reader.fields() {
        if key.read_string() == name {
            let fail = |message: &str| {
                Error::message(parsed.source(), message).with_location(parsed.field_location(&key))
            };
            equal(op, &fail)?;
            return value
                .read_string()
                .map(|path| Some(format!("map/{path}")))
                .map_err(|_| fail("expected map filename"));
        }
    }
    Ok(None)
}

fn array_strings(
    parsed: &ParsedText<'_>,
    reader: ObjectReader<'_, '_, TextEncoding>,
    name: &str,
) -> Result<Vec<String>, Error> {
    for (key, op, value) in reader.fields() {
        if key.read_string() == name {
            let fail = |m: &str| {
                Error::message(parsed.source(), m).with_location(parsed.field_location(&key))
            };
            equal(op, &fail)?;
            return value
                .read_array()
                .map_err(|_| fail("expected scalar array"))?
                .values()
                .map(|v| {
                    v.read_string()
                        .map_err(|_| fail("expected scalar array entry"))
                })
                .collect();
        }
    }
    Ok(Vec::new())
}

fn ids(
    parsed: &ParsedText<'_>,
    reader: ObjectReader<'_, '_, TextEncoding>,
    name: &str,
) -> Result<BTreeSet<u32>, Error> {
    array_strings(parsed, reader, name)?
        .into_iter()
        .map(|v| {
            v.parse().map_err(|_| {
                Error::message(
                    parsed.source(),
                    format!("invalid {name} province identifier"),
                )
            })
        })
        .collect()
}

fn optional_ids(
    parsed: &ParsedText<'_>,
    reader: ObjectReader<'_, '_, TextEncoding>,
    name: &str,
) -> Result<BTreeSet<u32>, Error> {
    if reader
        .clone()
        .fields()
        .any(|(key, _, _)| key.read_string() == name)
    {
        ids(parsed, reader, name)
    } else {
        Ok(BTreeSet::new())
    }
}

fn visit_history<'a>(
    parsed: &'a ParsedText<'_>,
    date: Date,
    mut apply: impl FnMut(ObjectReader<'a, 'a, TextEncoding>) -> Result<(), Error>,
) -> Result<(), Error> {
    visit_history_with_date(parsed, date, |_, reader| apply(reader))
}

fn visit_history_with_date<'a>(
    parsed: &'a ParsedText<'_>,
    date: Date,
    mut apply: impl FnMut(Option<Date>, ObjectReader<'a, 'a, TextEncoding>) -> Result<(), Error>,
) -> Result<(), Error> {
    for (history_date, reader) in history_blocks(parsed, date)? {
        apply(history_date, reader)?;
    }
    Ok(())
}

type HistoryBlock<'a> = (Option<Date>, ObjectReader<'a, 'a, TextEncoding>);

pub(super) fn history_blocks<'a>(
    parsed: &'a ParsedText<'_>,
    date: Date,
) -> Result<Vec<HistoryBlock<'a>>, Error> {
    let mut blocks = vec![(None, parsed.reader())];
    for (key, op, value) in parsed.reader().fields() {
        let name = key.read_string();
        if dated_name(&name) {
            let fail = |m: &str| {
                Error::message(parsed.source(), m).with_location(parsed.field_location(&key))
            };
            equal(op, &fail)?;
            let timestamp = Date::parse(&name).map_err(|_| fail("invalid history date"))?;
            let block = value
                .read_object()
                .map_err(|_| fail("dated history requires a block"))?;
            if timestamp <= date {
                blocks.push((Some(timestamp), block));
            }
        }
    }
    blocks.sort_by_key(|entry| entry.0);
    Ok(blocks)
}

pub(super) fn dated_name(name: &str) -> bool {
    name.as_bytes().first().is_some_and(u8::is_ascii_digit) && name.contains('.')
}

fn unapplied(
    parsed: &ParsedText<'_>,
    name: String,
    location: SourceLocation,
    country: bool,
    report: &mut BootstrapReport,
) {
    let fields = if country {
        &mut report.unapplied_country_fields
    } else {
        &mut report.unapplied_province_fields
    };
    *fields.entry(name.clone()).or_default() += 1;
    let key = format!("{}:{name}", if country { "country" } else { "province" });
    report
        .unapplied_examples
        .entry(key)
        .or_insert_with(|| source(parsed, location));
}

struct CountryHistoryContext<'a> {
    date: WorldDate,
    historical_truce_grants: &'a mut Vec<crate::truces::HistoricalTruceGrant>,
    historical_prestige_deltas: &'a mut Vec<HistoricalPrestigeDelta>,
    setup: &'a mut SetupDraft,
    subsystem_handled: &'a BTreeSet<String>,
    report: &'a mut BootstrapReport,
}

fn apply_country(
    parsed: &ParsedText<'_>,
    reader: ObjectReader<'_, '_, TextEncoding>,
    country: &mut CountryState,
    politics: &mut CountryDraft,
    history: CountryHistoryContext<'_>,
) -> Result<(), Error> {
    let CountryHistoryContext {
        date,
        historical_truce_grants,
        historical_prestige_deltas,
        setup,
        subsystem_handled,
        report,
    } = history;
    let mut handled =
        setup.country_history(parsed, reader.clone(), &country.id, &mut report.setup)?;
    handled.extend(subsystem_handled.iter().cloned());
    for (key, op, value) in reader.fields() {
        let name = key.read_string();
        if dated_name(&name) || handled.contains(&name) {
            continue;
        }
        let location = parsed.field_location(&key);
        let fail = |m: &str| Error::message(parsed.source(), m).with_location(location);
        match name.as_str() {
            "national_focus" => {
                equal(op, &fail)?;
                let text = value
                    .read_string()
                    .map_err(|_| fail("expected national focus scalar"))?;
                let skill = match text.as_str() {
                    "ADM" => augustus_core::RulerSkill::Administration,
                    "DIP" => augustus_core::RulerSkill::Diplomacy,
                    "MIL" => augustus_core::RulerSkill::Military,
                    _ => return Err(fail("unsupported historical national focus")),
                };
                politics.national_focus = Some(augustus_core::NationalFocus { skill: Some(skill) });
                report
                    .setup
                    .national_focus_sources
                    .insert(country.id.clone(), source(parsed, location));
            }
            "legitimacy"
            | "religious_school"
            | "add_government_reform"
            | "remove_government_reform"
            | "elector"
            | "revolution_target" => (),
            "add_truce_with" => {
                equal(op, &fail)?;
                let target = value
                    .read_string()
                    .map_err(|_| fail("expected historical truce target"))?;
                historical_truce_grants.push(crate::truces::HistoricalTruceGrant {
                    country: country.id.clone(),
                    target,
                    date,
                    source: source(parsed, location),
                });
            }
            "add_prestige" => {
                equal(op, &fail)?;
                let text = value
                    .read_string()
                    .map_err(|_| fail("expected historical prestige value"))?;
                let value_milli = crate::setup::micros(&text)
                    .map(|value| value / 1_000)
                    .ok_or_else(|| {
                        fail("prestige requires a literal decimal with at most six fractional digits")
                    })?;
                let source = source(parsed, location);
                report
                    .setup
                    .history_sources
                    .insert(format!("country:{}:prestige", country.id), source.clone());
                historical_prestige_deltas.push(HistoricalPrestigeDelta {
                    country: country.id.clone(),
                    value_milli,
                    source,
                });
            }
            "monarch" | "heir" | "queen" | "monarch_consort" => {
                equal(op, &fail)?;
                let person = politics::person(parsed, value, &name, location, report)?;
                *match name.as_str() {
                    "monarch" => &mut politics.ruler,
                    "heir" => &mut politics.heir,
                    _ => &mut politics.consort,
                } = Some(person);
            }
            "capital" => {
                equal(op, &fail)?;
                let id: u32 = value
                    .read_string()
                    .map_err(|_| fail("expected capital identifier"))?
                    .parse()
                    .map_err(|_| fail("invalid capital identifier"))?;
                country.declared_capital = (id != 0).then_some(id);
            }
            "primary_culture" | "religion" | "government" | "technology_group" | "unit_type" => {
                equal(op, &fail)?;
                let text = value
                    .read_string()
                    .map_err(|_| fail("expected country history scalar"))?;
                let slot = match name.as_str() {
                    "primary_culture" => &mut country.primary_culture,
                    "religion" => &mut country.religion,
                    "government" => &mut country.government,
                    "technology_group" => &mut country.technology_group,
                    _ => &mut country.unit_type,
                };
                *slot = Some(text);
                if name == "unit_type" {
                    report.setup.history_sources.insert(
                        format!("country:{}:unit_type", country.id),
                        source(parsed, location),
                    );
                }
            }
            _ => {
                if value
                    .read_object()
                    .is_ok_and(|reader| nested_focus_mutation(reader, 0))
                {
                    politics.national_focus = None;
                }
                unapplied(parsed, name, location, true, report);
            }
        }
    }
    Ok(())
}

fn apply_historical_prestige_deltas(
    initial: &mut InitialWorld,
    deltas: &[HistoricalPrestigeDelta],
    external_values: Option<&BTreeMap<String, BTreeMap<String, Option<i64>>>>,
) -> Result<(), Error> {
    for delta in deltas {
        if external_values
            .and_then(|values| values.get(&delta.country))
            .is_some_and(|values| values.contains_key("prestige"))
        {
            continue;
        }
        let location = SourceLocation {
            line: delta.source.line,
            column: delta.source.column,
        };
        let entity = initial.country_entity(&delta.country).ok_or_else(|| {
            Error::message(
                &delta.source.path,
                "prestige history references unknown country",
            )
            .with_location(location)
        })?;
        initial
            .execute(
                entity,
                &[Action::UpdateNumeric {
                    target: Target::Current,
                    slot: NumericSlot::Metric("prestige".into()),
                    operation: NumericOperation::Add,
                    value: delta.value_milli,
                }],
            )
            .map_err(|error| Error::caused_by(&delta.source.path, error).with_location(location))?;
    }
    Ok(())
}

fn nested_focus_mutation(reader: ObjectReader<'_, '_, TextEncoding>, depth: usize) -> bool {
    if depth > 64 {
        return true;
    }
    reader.fields().any(|(key, _, value)| {
        matches!(
            key.read_string().as_str(),
            "national_focus" | "change_national_focus"
        ) || value
            .read_object()
            .is_ok_and(|nested| nested_focus_mutation(nested, depth + 1))
    })
}

// Dated blocks are visited separately. An unhandled conditional owner assignment
// cannot establish that this province is unowned.
fn nested_owner(reader: ObjectReader<'_, '_, TextEncoding>, depth: usize) -> bool {
    if depth > 64 {
        return true;
    }
    reader.fields().any(|(key, _, value)| {
        let name = key.read_string();
        !dated_name(&name)
            && value.read_object().is_ok_and(|block| {
                block
                    .fields()
                    .any(|(key, _, _)| key.read_string() == "owner")
                    || nested_owner(block, depth + 1)
            })
    })
}

fn nested_center_of_trade(reader: ObjectReader<'_, '_, TextEncoding>, depth: usize) -> bool {
    if depth > 64 {
        return true;
    }
    reader.fields().any(|(key, _, value)| {
        let name = key.read_string();
        !dated_name(&name)
            && value.read_object().is_ok_and(|block| {
                block
                    .fields()
                    .any(|(key, _, _)| key.read_string() == "center_of_trade")
                    || nested_center_of_trade(block, depth + 1)
            })
    })
}

#[derive(Default)]
struct ProvinceHistoryState {
    center_of_trade_levels: BTreeMap<u32, Option<u8>>,
    discovered_by: BTreeMap<u32, BTreeSet<String>>,
}

fn apply_province(
    parsed: &ParsedText<'_>,
    reader: ObjectReader<'_, '_, TextEncoding>,
    province: &mut ProvinceState,
    economy: &mut EconomyDraft,
    setup: &mut SetupDraft,
    report: &mut BootstrapReport,
    history_state: &mut ProvinceHistoryState,
) -> Result<(), Error> {
    let handled = setup.province_history(parsed, reader.clone(), province.id, &mut report.setup)?;
    for (key, op, value) in reader.fields() {
        let name = key.read_string();
        if dated_name(&name) || handled.contains(&name) {
            continue;
        }
        let location = parsed.field_location(&key);
        let fail = |m: &str| Error::message(parsed.source(), m).with_location(location);
        if economy.is_building(&name) {
            equal(op, &fail)?;
            economy.apply_building(
                parsed,
                &name,
                value,
                province.id,
                location,
                &mut report.economy,
            )?;
            continue;
        }
        match name.as_str() {
            "discovered_by" => {
                equal(op, &fail)?;
                let identifier = value
                    .read_string()
                    .map_err(|_| fail("expected province discovery country or technology group"))?;
                if identifier.is_empty() {
                    return Err(fail(
                        "expected province discovery country or technology group",
                    ));
                }
                history_state
                    .discovered_by
                    .entry(province.id)
                    .or_default()
                    .insert(identifier);
            }
            "center_of_trade" => {
                equal(op, &fail)?;
                let text = value
                    .read_string()
                    .map_err(|_| fail("expected province center-of-trade level"))?;
                let level = text
                    .parse::<u8>()
                    .ok()
                    .filter(|_| !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit()))
                    .ok_or_else(|| fail("expected nonnegative center-of-trade level"))?;
                history_state
                    .center_of_trade_levels
                    .insert(province.id, Some(level));
            }
            "owner" | "controller" | "culture" | "religion" | "trade_goods" => {
                equal(op, &fail)?;
                let text = value
                    .read_string()
                    .map_err(|_| fail("expected province history scalar"))?;
                if name == "trade_goods"
                    && text != "---"
                    && !text.is_empty()
                    && !economy.has_trade_good(&text)
                {
                    return Err(fail(&format!(
                        "province {} references unknown trade good {text}",
                        province.id
                    )));
                }
                let slot = match name.as_str() {
                    "owner" => &mut province.owner,
                    "controller" => &mut province.controller,
                    "culture" => &mut province.culture,
                    "religion" => &mut province.religion,
                    _ => &mut province.trade_good,
                };
                *slot = (text != "---" && !text.is_empty()).then_some(text);
            }
            "add_core" | "remove_core" => {
                equal(op, &fail)?;
                let text = value
                    .read_string()
                    .map_err(|_| fail("expected core country"))?;
                if name == "add_core" {
                    if !province.cores.contains(&text) {
                        province.cores.push(text);
                    }
                } else {
                    province.cores.retain(|c| c != &text);
                }
            }
            "add_local_autonomy" => (), // Applied and diagnosed by AdministrationDraft.
            "base_tax" | "base_production" | "base_manpower" => {
                equal(op, &fail)?;
                let text = value
                    .read_string()
                    .map_err(|_| fail("expected literal base value"))?;
                let number = nonnegative_milli(&text).ok_or_else(|| {
                    fail("expected nonnegative base value with at most three decimal digits")
                })?;
                *match name.as_str() {
                    "base_tax" => &mut province.tax_milli,
                    "base_production" => &mut province.production_milli,
                    _ => &mut province.manpower_milli,
                } = Some(number);
            }
            "is_city" | "hre" => {
                equal(op, &fail)?;
                let text = value
                    .read_string()
                    .map_err(|_| fail("expected yes or no"))?;
                let flag = match text.as_str() {
                    "yes" => true,
                    "no" => false,
                    _ => return Err(fail("expected yes or no")),
                };
                *if name == "is_city" {
                    &mut province.is_city
                } else {
                    &mut province.in_empire
                } = Some(flag);
            }
            _ => unapplied(parsed, name, location, false, report),
        }
    }
    Ok(())
}

pub(super) fn nonnegative_milli(text: &str) -> Option<i64> {
    let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));
    if whole.is_empty()
        || !whole.bytes().all(|b| b.is_ascii_digit())
        || fraction.len() > 3
        || !fraction.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    whole
        .parse::<i64>()
        .ok()?
        .checked_mul(1000)?
        .checked_add(if fraction.is_empty() {
            0
        } else {
            fraction.parse::<i64>().ok()? * 10_i64.pow(3 - fraction.len() as u32)
        })
}

#[cfg(test)]
mod center_of_trade_history_tests {
    use super::*;
    use crate::TextDocument;

    #[test]
    fn only_nested_center_of_trade_assignments_leave_history_unresolved() {
        let direct = TextDocument::from_bytes(
            "self-authored-center-of-trade-history.txt",
            b"center_of_trade = 1\n1440.1.1 = { center_of_trade = 2 }".to_vec(),
            TextEncoding::Utf8,
        );
        let parsed = direct.parse().unwrap();
        assert!(!nested_center_of_trade(parsed.reader(), 0));

        let conditional = TextDocument::from_bytes(
            "self-authored-conditional-center-of-trade-history.txt",
            b"if = { limit = { always = yes } center_of_trade = 2 }".to_vec(),
            TextEncoding::Utf8,
        );
        let parsed = conditional.parse().unwrap();
        assert!(nested_center_of_trade(parsed.reader(), 0));
    }
}
