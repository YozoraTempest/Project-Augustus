// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    path::{Path, PathBuf},
};

use augustus_core::{
    Calendar, CountryResources, CountryState, InitialWorld, MonthlyExternalCashflows,
    MonthlyProduction, ProvinceClass, ProvinceState, Taxation, VanillaProductionBaseValueInput,
    VanillaProductionBaseValueResult, VanillaProductionInput, VanillaTradeGoodPriceInput,
    WorldDate, calculate_vanilla_production_base_value, calculate_vanilla_trade_good_price,
};
use jomini::{
    Encoding,
    common::{Date, PdsDate},
    text::{ObjectReader, Operator},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::administration::{AdministrationDraft, AdministrationReport};
use crate::attributes::{ATTRIBUTE_NAMES, AttributeDraft, AttributeReport};
use crate::country_status::CountryStatusDraft;
use crate::economy::{EconomyDraft, EconomyReport};
use crate::estates::{EstateDraft, EstateReport};
use crate::governments::{GovernmentDraft, GovernmentReport};
use crate::ideas::{IdeaDraft, IdeaReport};
use crate::politics::{self, CountryDraft, RelationSource, RelationView};
use crate::setup::{SetupDraft, SetupReport};
use crate::{
    ContentRoot, ContentSet, ContentView, Error, ParsedText, SourceLocation, TextEncoding,
};

const ENCODING: TextEncoding = TextEncoding::Windows1252;

#[derive(Debug, Clone, Serialize)]
pub struct LoadedSource {
    pub path: PathBuf,
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Serialize)]
pub struct BootstrapReport {
    pub ruleset: &'static str,
    pub game_version: String,
    pub start_date: WorldDate,
    pub map_width: u32,
    pub map_height: u32,
    pub countries: usize,
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
    pub metrics: Option<&'a BTreeMap<String, Option<i64>>>,
    pub buildings: Option<Vec<&'a str>>,
    pub trade_node: Option<&'a str>,
    pub trade_node_outgoing: Option<Vec<&'a str>>,
    pub trade_good_definition: Option<&'a augustus_core::TradeGoodDefinition>,
    pub modifiers: Vec<&'a str>,
    pub modifier_attachments_known: bool,
    pub declared_modifier_totals_micros: BTreeMap<String, i64>,
    pub attributes: BTreeMap<String, augustus_core::AttributeEvaluation>,
    pub administration: Option<augustus_core::AdministrationEvaluation>,
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
    pub flags: Option<&'a BTreeMap<String, bool>>,
    pub flag_namespace_known: bool,
    pub estates: Option<&'a BTreeMap<String, augustus_core::EstateState>>,
    pub declared_estate_effects: augustus_core::EstateEvaluation,
    pub ideas: Option<&'a augustus_core::IdeaState>,
    pub declared_idea_effects: augustus_core::IdeaEvaluation,
    pub last_monthly_tax: Option<&'a augustus_core::MonthlyTaxAccount>,
    pub last_monthly_production: Option<&'a augustus_core::ProductionCountryAccount>,
    pub government: Option<&'a augustus_core::GovernmentState>,
    pub declared_government_effects: augustus_core::GovernmentEvaluation,
    pub state: &'a CountryState,
    pub owned_provinces: Vec<u32>,
    pub bound_capital: Option<u32>,
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

/// A starting land stack translated by an external simulator or save reader.
/// The EU4 compatibility boundary resolves the country tag and province ID;
/// the shared core stores only the resulting regiment types.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaExternalArmy {
    pub owner: String,
    pub location: u32,
    #[serde(default)]
    pub name: String,
    pub regiments: Vec<String>,
}

/// A starting naval unit translated by an external simulator or save reader.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaExternalNavalUnit {
    pub owner: String,
    pub location: u32,
    pub unit_type: String,
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

#[derive(Debug)]
pub struct VanillaWorld {
    pub initial: InitialWorld,
    pub report: BootstrapReport,
    pub province_bitmap: PathBuf,
    pub country_sources: BTreeMap<String, Vec<LoadedSource>>,
    pub province_sources: BTreeMap<u32, Vec<LoadedSource>>,
    content: String,
}

/// State supplied by a second simulation or an external save reader.
///
/// EU4 tags and province identifiers stay at the compatibility boundary. The
/// shared core receives only the already translated numeric state and keeps
/// omitted values unresolved.
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
    /// Current per-country growth ceilings for metrics such as monarch power.
    /// A missing country or metric remains unresolved; `Some(value)` is an
    /// authoritative ceiling supplied by the compatibility provider.
    #[serde(default)]
    pub country_metric_ceilings: BTreeMap<String, BTreeMap<String, Option<i64>>>,
    #[serde(default)]
    pub country_resources: BTreeMap<String, CountryResources>,
    #[serde(default)]
    pub production_inputs: BTreeMap<u32, augustus_core::VanillaProductionInputState>,
    #[serde(default)]
    pub production_value_inputs: BTreeMap<u32, VanillaExternalProductionInput>,
    /// Runtime price-impact totals keyed by the native trade-good table index.
    /// An omitted index means an empty impact chain, matching the vanilla
    /// zero-impact state rather than inventing a new price.
    #[serde(default)]
    pub trade_good_price_impacts_milli: BTreeMap<u32, i32>,
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
    /// Current city status supplied by a save reader. An omitted province
    /// remains unknown; `Some(false)` preserves an explicit non-city state.
    #[serde(default)]
    pub province_is_city: BTreeMap<u32, Option<bool>>,
    /// Current HRE membership supplied by a save reader. An omitted province
    /// remains unknown; `Some(false)` records explicit absence.
    #[serde(default)]
    pub province_in_empire: BTreeMap<u32, Option<bool>>,
    /// Current raw province autonomy supplied by a save reader, in EU4's
    /// thousandths. The core stores the same value in millionths and applies
    /// monthly autonomy rules from that state on the next month boundary.
    #[serde(default)]
    pub province_raw_autonomy_milli: BTreeMap<u32, Option<i64>>,
    /// Current estate privilege sets supplied by a save reader. `Some(empty)`
    /// means the estate is known to have no privileges; `None` remains unknown.
    #[serde(default)]
    pub estate_privileges: VanillaExternalEstatePrivileges,
    /// Current country flag namespaces supplied by a save reader. A complete
    /// set replaces the historical namespace for that country.
    #[serde(default)]
    pub country_markers: VanillaExternalCountryMarkers,
    /// Current country boolean statuses such as `at_war` supplied by a save
    /// reader. Omitted statuses remain at their content-derived state.
    #[serde(default)]
    pub country_statuses: VanillaExternalCountryStatuses,
    #[serde(default)]
    pub estate_loyalty_targets: BTreeMap<String, BTreeMap<String, Option<i64>>>,
    #[serde(default)]
    pub country_monthly_cashflows_milli: BTreeMap<String, Option<i64>>,
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
    #[serde(default)]
    pub advisors: BTreeMap<String, Vec<VanillaExternalAdvisor>>,
}

impl VanillaExternalState {
    /// Applies supplemental translated inputs on top of this state bundle.
    /// Nested country maps merge by country and field; non-empty unit rosters
    /// replace the existing roster so an external provider can supply an
    /// authoritative snapshot.
    pub fn apply_overlay(&mut self, overlay: &Self) {
        overlay_external_state(self, overlay);
    }
}

#[derive(Clone, Copy, Default)]
struct LoadOverrides<'a> {
    country_metric_values: Option<&'a BTreeMap<String, BTreeMap<String, Option<i64>>>>,
    country_national_focus: Option<&'a BTreeMap<String, augustus_core::NationalFocus>>,
    country_metric_ceilings: Option<&'a BTreeMap<String, BTreeMap<String, Option<i64>>>>,
    country_resources: Option<&'a BTreeMap<String, CountryResources>>,
    production_inputs: Option<&'a BTreeMap<u32, augustus_core::VanillaProductionInputState>>,
    production_value_inputs: Option<&'a BTreeMap<u32, VanillaExternalProductionInput>>,
    trade_good_price_impacts_milli: Option<&'a BTreeMap<u32, i32>>,
    country_annual_tax_milli: Option<&'a BTreeMap<String, Option<i64>>>,
    province_annual_tax_milli: Option<&'a BTreeMap<u32, Option<i64>>>,
    province_is_city: Option<&'a BTreeMap<u32, Option<bool>>>,
    province_in_empire: Option<&'a BTreeMap<u32, Option<bool>>>,
    province_raw_autonomy_milli: Option<&'a BTreeMap<u32, Option<i64>>>,
    estate_privileges: Option<&'a VanillaExternalEstatePrivileges>,
    country_markers: Option<&'a VanillaExternalCountryMarkers>,
    country_statuses: Option<&'a VanillaExternalCountryStatuses>,
    estate_loyalty_targets: Option<&'a BTreeMap<String, BTreeMap<String, Option<i64>>>>,
    country_monthly_cashflows_milli: Option<&'a BTreeMap<String, Option<i64>>>,
    country_monthly_power_income: Option<&'a BTreeMap<String, BTreeMap<String, Option<i64>>>>,
    armies: Option<&'a [VanillaExternalArmy]>,
    naval_units: Option<&'a [VanillaExternalNavalUnit]>,
    advisors: Option<&'a BTreeMap<String, Vec<VanillaExternalAdvisor>>>,
}

impl<'a> LoadOverrides<'a> {
    fn from_external_state(state: &'a VanillaExternalState) -> Self {
        Self {
            country_metric_values: (!state.country_metric_values.is_empty())
                .then_some(&state.country_metric_values),
            country_national_focus: (!state.country_national_focus.is_empty())
                .then_some(&state.country_national_focus),
            country_metric_ceilings: (!state.country_metric_ceilings.is_empty())
                .then_some(&state.country_metric_ceilings),
            country_resources: (!state.country_resources.is_empty())
                .then_some(&state.country_resources),
            production_inputs: (!state.production_inputs.is_empty())
                .then_some(&state.production_inputs),
            production_value_inputs: (!state.production_value_inputs.is_empty())
                .then_some(&state.production_value_inputs),
            trade_good_price_impacts_milli: (!state.trade_good_price_impacts_milli.is_empty())
                .then_some(&state.trade_good_price_impacts_milli),
            country_annual_tax_milli: (!state.country_annual_tax_milli.is_empty())
                .then_some(&state.country_annual_tax_milli),
            province_annual_tax_milli: (!state.province_annual_tax_milli.is_empty())
                .then_some(&state.province_annual_tax_milli),
            province_is_city: (!state.province_is_city.is_empty())
                .then_some(&state.province_is_city),
            province_in_empire: (!state.province_in_empire.is_empty())
                .then_some(&state.province_in_empire),
            province_raw_autonomy_milli: (!state.province_raw_autonomy_milli.is_empty())
                .then_some(&state.province_raw_autonomy_milli),
            estate_privileges: (!state.estate_privileges.is_empty())
                .then_some(&state.estate_privileges),
            country_markers: (!state.country_markers.is_empty()).then_some(&state.country_markers),
            country_statuses: (!state.country_statuses.is_empty())
                .then_some(&state.country_statuses),
            estate_loyalty_targets: (!state.estate_loyalty_targets.is_empty())
                .then_some(&state.estate_loyalty_targets),
            country_monthly_cashflows_milli: (!state.country_monthly_cashflows_milli.is_empty())
                .then_some(&state.country_monthly_cashflows_milli),
            country_monthly_power_income: (!state.country_monthly_power_income.is_empty())
                .then_some(&state.country_monthly_power_income),
            armies: (!state.armies.is_empty()).then_some(state.armies.as_slice()),
            naval_units: (!state.naval_units.is_empty()).then_some(state.naval_units.as_slice()),
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

fn overlay_external_state(base: &mut VanillaExternalState, overlay: &VanillaExternalState) {
    overlay_nested_map(
        &mut base.country_metric_values,
        &overlay.country_metric_values,
    );
    base.country_national_focus
        .extend(overlay.country_national_focus.clone());
    overlay_nested_map(
        &mut base.country_metric_ceilings,
        &overlay.country_metric_ceilings,
    );
    base.country_resources
        .extend(overlay.country_resources.clone());
    base.production_inputs
        .extend(overlay.production_inputs.clone());
    base.production_value_inputs
        .extend(overlay.production_value_inputs.clone());
    base.trade_good_price_impacts_milli
        .extend(overlay.trade_good_price_impacts_milli.clone());
    base.country_annual_tax_milli
        .extend(overlay.country_annual_tax_milli.clone());
    base.province_annual_tax_milli
        .extend(overlay.province_annual_tax_milli.clone());
    base.province_is_city
        .extend(overlay.province_is_city.clone());
    base.province_in_empire
        .extend(overlay.province_in_empire.clone());
    base.province_raw_autonomy_milli
        .extend(overlay.province_raw_autonomy_milli.clone());
    overlay_nested_map(&mut base.estate_privileges, &overlay.estate_privileges);
    base.country_markers.extend(overlay.country_markers.clone());
    overlay_nested_map(&mut base.country_statuses, &overlay.country_statuses);
    overlay_nested_map(
        &mut base.estate_loyalty_targets,
        &overlay.estate_loyalty_targets,
    );
    base.country_monthly_cashflows_milli
        .extend(overlay.country_monthly_cashflows_milli.clone());
    overlay_nested_map(
        &mut base.country_monthly_power_income,
        &overlay.country_monthly_power_income,
    );
    base.advisors.extend(overlay.advisors.clone());
    if !overlay.armies.is_empty() {
        base.armies = overlay.armies.clone();
    }
    if !overlay.naval_units.is_empty() {
        base.naval_units = overlay.naval_units.clone();
    }
}

fn bind_external_units(
    initial: &mut InitialWorld,
    armies: Option<&[VanillaExternalArmy]>,
    naval_units: Option<&[VanillaExternalNavalUnit]>,
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
            initial
                .create_army(owner, location, &army.name, &army.regiments)
                .map_err(|error| Error::caused_by("external armies", error))?;
        }
    }
    if let Some(naval_units) = naval_units {
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
            initial
                .create_naval_unit(owner, location, &unit.unit_type)
                .map_err(|error| Error::caused_by("external naval units", error))?;
        }
    }
    Ok(())
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

fn bind_external_production_value_inputs(
    initial: &InitialWorld,
    inputs: &BTreeMap<u32, VanillaExternalProductionInput>,
    price_impacts: Option<&BTreeMap<u32, i32>>,
) -> Result<BTreeMap<augustus_core::EntityId, augustus_core::VanillaProductionInputState>, Error> {
    let economy = initial.world().economy();
    inputs
        .iter()
        .map(|(province, source)| {
            let entity = initial.province_entity(*province).ok_or_else(|| {
                Error::message(
                    "external production value inputs",
                    format!("unknown EU4 production province {province}"),
                )
            })?;
            let definition = economy
                .provinces
                .get(&entity)
                .and_then(|state| state.trade_good.as_deref())
                .and_then(|trade_good| economy.trade_goods.get(trade_good));
            let state = match definition.and_then(|definition| definition.base_price_milli) {
                Some(price) => {
                    let base_price = i32::try_from(price).map_err(|_| {
                        Error::message(
                            "external production value inputs",
                            format!(
                                "trade-good base price for province {province} is outside the production range"
                            ),
                        )
                    })?;
                    let price_impact_milli = definition
                        .and_then(|definition| definition.runtime_index)
                        .and_then(|index| {
                            price_impacts.and_then(|impacts| impacts.get(&index))
                        })
                        .copied()
                        .unwrap_or_default();
                    let price = calculate_vanilla_trade_good_price(VanillaTradeGoodPriceInput {
                        base_price_milli: base_price,
                        price_impact_milli,
                    })
                    .effective_price_milli;
                    let value = calculate_vanilla_production_base_value(
                        VanillaProductionBaseValueInput {
                            goods_size_milli: source.goods_size_milli,
                            trade_good_price_milli: price,
                            flat_modifier_milli: source.flat_modifier_milli,
                            external_flat_modifier_milli: source.external_flat_modifier_milli,
                            rate_modifier_milli: source.rate_modifier_milli,
                            external_rate_modifier_milli: source.external_rate_modifier_milli,
                        },
                    );
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
            Ok((entity, state))
        })
        .collect()
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
        let save = crate::read_vanilla_save_state(path)?;
        Self::load_from_content_with_save_state(content, &save)
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
        let save = crate::read_vanilla_save_state(path)?;
        Self::load_from_content_with_save_state_and_external_state(content, &save, Some(overlay))
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
                    Self::load_from_content_with_save_state_inner(content, &save, overlay.as_ref())
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
    ) -> Result<Self, Error> {
        let baseline = Self::load_from_content(content, &save.date)?;
        let (players, mut state) = save.for_world(&baseline)?;
        if let Some(overlay) = overlay {
            state.apply_overlay(overlay);
        }
        Self::load_from_content_with_players_and_external_state(
            content, &save.date, &players, &state,
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
        for (id, has_port) in crate::province_map::port_presence(
            &pixels,
            &province_ids_by_color,
            &provinces,
            &force_coastal,
        ) {
            provinces
                .get_mut(&id)
                .expect("port derivation preserves province identifiers")
                .has_port = has_port;
        }
        let files = root.files()?;
        let defines = crate::Defines::load(root, &files, crate::DefineDifficulty::Normal)?;
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
        let mut country_metric_history_unknown = BTreeSet::new();
        let mut country_metric_scoped_history_unknown = false;
        let mut liberty_history_unknown = BTreeSet::new();
        let mut liberty_scoped_history_unknown = false;
        let mut province_histories = BTreeSet::new();
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
            visit_history(&parsed, date, |reader| {
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
                    &mut setup,
                    &subsystem_handled,
                    &mut report,
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
            province_sources
                .get_mut(&id)
                .expect("registered province")
                .push(LoadedSource {
                    path: doc.source().into(),
                    line: 1,
                    column: 1,
                });
            visit_history(&parsed, date, |reader| {
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
                )
            })?;
        }
        report.country_history_files = country_histories.len();
        report.province_history_files = province_histories.len();
        report.missing_province_history = provinces
            .keys()
            .filter(|id| !province_histories.contains(id))
            .copied()
            .collect();
        let mut initial = InitialWorld::new_with_eu4_seed(
            countries.into_values().collect(),
            provinces.into_values().collect(),
            seed,
        )
        .map_err(|e| Error::caused_by(root.path(), e))?;
        initial.set_eu4_event_seed(fixed_seed);
        let unowned = initial
            .provinces()
            .iter()
            .filter(|province| {
                province.owner.is_none()
                    && !unresolved_ownership.contains(&province.id)
                    && (province.class != ProvinceClass::Land
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
        let economy = economy.bind(&initial, &province_histories, &mut report.economy)?;
        initial
            .set_economy(economy)
            .map_err(|e| Error::caused_by(root.path(), e))?;
        setup.bind(&mut initial, date, &mut report.setup)?;
        let governments = governments.bind(
            &initial,
            &attribute_draft,
            &report.start_date,
            &mut report.governments,
        );
        initial
            .set_governments(governments)
            .map_err(|e| Error::caused_by(root.path(), e))?;
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
        if let Some(values) = overrides.province_in_empire {
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
                .map_err(|e| Error::caused_by("external HRE membership", e))?;
        }
        let area_scopes = administration.ordered_areas();
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
        initial
            .set_metrics(metrics)
            .map_err(|e| Error::caused_by("world metrics", e))?;
        if let Some(values) = overrides.country_metric_values {
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
        if let Some(values) = overrides.country_metric_ceilings {
            let bound = values
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
        let convergence = crate::liberty_desire::bind(
            &files,
            &defines,
            &initial,
            &mut attributes,
            &mut report.setup,
        )?;
        administration.initialize_country_ranks(&mut initial, &mut report.administration)?;
        let administration = administration.bind(
            &initial,
            &province_histories,
            &mut report.administration,
            &attributes,
            &report.attributes,
            date,
        )?;
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
        bind_external_country_statuses(&mut initial, overrides.country_statuses)?;
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
        let missions =
            crate::missions::MissionDraft::load(&root, &files, initial.world(), &area_scopes)?;
        missions.register_identifiers(&mut initial)?;
        crate::truces::bind(&files, &defines, &mut report.setup)?;
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
            report.setup.truce_months,
            area_scopes,
            initial.world().affiliations().0["religious_school"]
                .groups
                .clone(),
            casus_belli.types,
            &mut report.estates,
        )?;
        report.casus_belli = casus_belli.report;
        let estates = estates.bind(&initial, &mut report.estates)?;
        initial
            .set_estates(estates)
            .map_err(|e| Error::caused_by(root.path(), e))?;
        bind_external_estate_privileges(&mut initial, overrides.estate_privileges)?;
        let ideas = ideas.bind(&initial, report.start_date, &mut report.ideas);
        initial
            .set_ideas(ideas)
            .map_err(|e| Error::caused_by(root.path(), e))?;
        initial
            .set_attributes(attributes)
            .map_err(|e| Error::caused_by(root.path(), e))?;
        initial
            .set_calendar(Calendar {
                start: report.start_date,
                month_lengths: [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31],
            })
            .map_err(|e| Error::caused_by(root.path(), e))?;
        if overrides.production_inputs.is_some() || overrides.production_value_inputs.is_some() {
            let mut bound = BTreeMap::new();
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
                for (entity, input) in bind_external_production_value_inputs(
                    &initial,
                    inputs,
                    overrides.trade_good_price_impacts_milli,
                )? {
                    if bound.insert(entity, input).is_some() {
                        return Err(Error::message(
                            "external production inputs",
                            "a province cannot provide both production_inputs and production_value_inputs",
                        ));
                    }
                }
            }
            initial
                .set_monthly_production(MonthlyProduction::new(bound))
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
        if let Some(cashflows) = overrides.country_monthly_cashflows_milli {
            let bound = cashflows
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
                .collect::<Result<BTreeMap<_, _>, Error>>()?;
            initial
                .set_monthly_cashflows(Some(MonthlyExternalCashflows::new(bound)))
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
        bind_external_units(&mut initial, overrides.armies, overrides.naval_units)?;
        report.missions = missions.bind(&mut initial)?;
        // Content identity is based on the bound initial world. Startup effects
        // and AI choices are runtime state and must not vary the hash by seed.
        let content_identity_initial = initial.clone();
        startup.run(
            &mut initial,
            &mut report.estates,
            &country_order,
            clock_origin,
        )?;
        bind_external_country_statuses(&mut initial, overrides.country_statuses)?;
        bind_external_country_markers(&mut initial, overrides.country_markers)?;
        bind_external_estate_privileges(&mut initial, overrides.estate_privileges)?;
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
                initial.countries(),
                initial.provinces(),
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
        let fingerprint: String = digest
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        let content = format!("augustus-vanilla-calendar-tax:1:sha256:{fingerprint}");
        Ok(Self {
            initial,
            report,
            province_bitmap: bitmap.source().into(),
            country_sources,
            province_sources,
            content,
        })
    }

    pub fn advance(&mut self, days: u64) -> Result<(), augustus_core::Error> {
        self.initial.advance(days)
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
        self.initial.advance_with_daily_system(days, system)
    }

    pub fn advance_with_yearly_system(
        &mut self,
        days: u64,
        system: &dyn augustus_core::YearlySystem,
    ) -> Result<(), augustus_core::Error> {
        self.initial.advance_with_yearly_system(days, system)
    }

    pub fn advance_with_systems(
        &mut self,
        days: u64,
        daily: Option<&dyn augustus_core::DailySystem>,
        monthly: Option<&dyn augustus_core::MonthlySystem>,
    ) -> Result<(), augustus_core::Error> {
        self.initial.advance_with_systems(days, daily, monthly)
    }

    pub fn advance_with_systems_and_yearly(
        &mut self,
        days: u64,
        daily: Option<&dyn augustus_core::DailySystem>,
        monthly: Option<&dyn augustus_core::MonthlySystem>,
        yearly: Option<&dyn augustus_core::YearlySystem>,
    ) -> Result<(), augustus_core::Error> {
        self.initial
            .advance_with_systems_and_yearly(days, daily, monthly, yearly)
    }

    pub fn save(&self) -> Result<String, augustus_core::Error> {
        self.initial.world().save(&self.content)
    }

    pub fn restore(&mut self, snapshot: &str) -> Result<(), augustus_core::Error> {
        self.initial.restore(snapshot, &self.content)
    }

    pub fn province(&self, id: u32) -> Option<ProvinceView<'_>> {
        let state = self.initial.province(id)?;
        let economy = self.initial.world().economy();
        let entity = self.initial.province_entity(id)?;
        let economic_state = economy.provinces.get(&entity);
        Some(ProvinceView {
            state,
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
            trade_node: economy.province_node(entity),
            trade_node_outgoing: economy.province_node(entity).map(|name| {
                economy.trade_nodes[name]
                    .outgoing
                    .iter()
                    .map(String::as_str)
                    .collect()
            }),
            trade_good_definition: economic_state
                .and_then(|s| s.trade_good.as_ref())
                .and_then(|name| economy.trade_goods.get(name)),
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
            .collect()
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

fn visit_history<'a>(
    parsed: &'a ParsedText<'_>,
    date: Date,
    mut apply: impl FnMut(ObjectReader<'a, 'a, TextEncoding>) -> Result<(), Error>,
) -> Result<(), Error> {
    for (_, reader) in history_blocks(parsed, date)? {
        apply(reader)?;
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

fn apply_country(
    parsed: &ParsedText<'_>,
    reader: ObjectReader<'_, '_, TextEncoding>,
    country: &mut CountryState,
    politics: &mut CountryDraft,
    setup: &mut SetupDraft,
    subsystem_handled: &BTreeSet<String>,
    report: &mut BootstrapReport,
) -> Result<(), Error> {
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
            | "revolution_target" => (),
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

fn apply_province(
    parsed: &ParsedText<'_>,
    reader: ObjectReader<'_, '_, TextEncoding>,
    province: &mut ProvinceState,
    economy: &mut EconomyDraft,
    setup: &mut SetupDraft,
    report: &mut BootstrapReport,
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
