// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

//! Translate the state portion of a plaintext EU4 save into the external
//! compatibility boundary. The simulation core never reads EU4 save syntax.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use augustus_core::{CountryResources, IdeaState, NationalFocus, RulerSkill};
use jomini::{TextToken, text::ValueReader};
use serde::Serialize;

use crate::administration_validation::read_eu4_save_documents;
use crate::bootstrap::{
    VanillaExternalAdvisor, VanillaExternalArmy, VanillaExternalEstateState,
    VanillaExternalNavalUnit, VanillaExternalProvinceDevelopment, VanillaExternalState,
    VanillaWorld,
};
use crate::{Error, ParsedText, TextEncoding};

type Object<'a, 'b> = jomini::text::ObjectReader<'a, 'b, TextEncoding>;
type Field<'a, 'b> = ValueReader<'a, 'b, TextEncoding>;

/// State translated from a legal, uncompressed EU4 text save.
///
/// The parsed state is deliberately kept separate from [`VanillaWorld`]. A
/// caller can inspect it, filter it against a loaded content set, and only
/// then bind it before startup effects run.
#[derive(Debug, Clone, Serialize)]
pub struct VanillaSaveState {
    pub date: String,
    pub player: Option<String>,
    pub players: BTreeSet<String>,
    pub external_state: VanillaExternalState,
}

impl VanillaSaveState {
    /// Translate the save into a state bundle accepted by a loaded content set.
    ///
    /// Fields whose identifiers are not defined by the target content are
    /// omitted, matching the filtering used by direct EU4 save import.
    pub fn state_for_world(&self, world: &VanillaWorld) -> Result<VanillaExternalState, Error> {
        self.for_world(world).map(|(_, state)| state)
    }

    /// Keep only identifiers present in the loaded world and bind the content
    /// declared monarch-power ceiling to the saved power stocks.
    pub(crate) fn for_world(
        &self,
        world: &VanillaWorld,
    ) -> Result<(BTreeSet<String>, VanillaExternalState), Error> {
        let mut state = VanillaExternalState::default();
        let countries: BTreeSet<_> = world
            .initial
            .countries()
            .iter()
            .map(|country| country.id.as_str())
            .collect();
        let definitions = &world.initial.world().metrics().definitions;
        let government_definitions = world.initial.world().governments();

        for country in &countries {
            if let Some(values) = self.external_state.country_metric_values.get(*country) {
                let filtered = values
                    .iter()
                    .filter(|(name, _)| definitions.contains_key(name.as_str()))
                    .map(|(name, value)| (name.clone(), *value))
                    .collect::<BTreeMap<_, _>>();
                if !filtered.is_empty() {
                    state
                        .country_metric_values
                        .insert((*country).into(), filtered);
                }
            }
            if let Some(resources) = self.external_state.country_resources.get(*country) {
                state
                    .country_resources
                    .insert((*country).into(), resources.clone());
            }
            if let Some(focus) = self.external_state.country_national_focus.get(*country) {
                state
                    .country_national_focus
                    .insert((*country).into(), *focus);
            }
            if let Some(capital) = self.external_state.country_capitals.get(*country).copied()
                && (capital.is_none()
                    || capital
                        .is_some_and(|province| world.initial.province_entity(province).is_some()))
            {
                state.country_capitals.insert((*country).into(), capital);
            }
            if let Some(levels) = self.external_state.country_technology_levels.get(*country) {
                let tables = &world.initial.world().technology().tables;
                let mut filtered = BTreeMap::new();
                for (branch, level) in levels {
                    let Some(table) = tables.get(branch) else {
                        continue;
                    };
                    if usize::from(*level) >= table.len() {
                        return Err(Error::message(
                            "save state",
                            format!(
                                "country {country} has invalid saved {branch} technology level {level}"
                            ),
                        ));
                    }
                    filtered.insert(branch.clone(), *level);
                }
                if !filtered.is_empty() {
                    state
                        .country_technology_levels
                        .insert((*country).into(), filtered);
                }
            }
            if let Some(group) = self.external_state.country_technology_groups.get(*country) {
                state
                    .country_technology_groups
                    .insert((*country).into(), group.clone());
            }
            if let Some(unit_type) = self.external_state.country_unit_types.get(*country) {
                state
                    .country_unit_types
                    .insert((*country).into(), unit_type.clone());
            }
            if let Some(rank) = self.external_state.country_government_ranks.get(*country) {
                state
                    .country_government_ranks
                    .insert((*country).into(), *rank);
            }
            if let Some(kind) = self.external_state.country_government_kinds.get(*country) {
                state.country_government_kinds.insert(
                    (*country).into(),
                    kind.clone()
                        .filter(|kind| government_definitions.forms.contains_key(kind)),
                );
            }
            if let Some(reforms) = self.external_state.country_government_reforms.get(*country) {
                let supported = reforms.as_ref().filter(|reforms| {
                    reforms
                        .iter()
                        .all(|reform| government_definitions.reforms.contains_key(reform))
                });
                state
                    .country_government_reforms
                    .insert((*country).into(), supported.cloned());
            }
            if let Some(values) = self
                .external_state
                .country_government_metric_values
                .get(*country)
            {
                let filtered = values
                    .iter()
                    .filter(|(name, _)| government_definitions.metrics.contains_key(name.as_str()))
                    .map(|(name, value)| (name.clone(), *value))
                    .collect::<BTreeMap<_, _>>();
                if !filtered.is_empty() {
                    state
                        .country_government_metric_values
                        .insert((*country).into(), filtered);
                }
            }
            if let Some(mechanics) = self
                .external_state
                .country_government_mechanic_powers
                .get(*country)
            {
                state
                    .country_government_mechanic_powers
                    .insert((*country).into(), mechanics.clone());
            }
            if let Some(culture) = self.external_state.country_primary_cultures.get(*country) {
                state
                    .country_primary_cultures
                    .insert((*country).into(), culture.clone());
            }
            if let Some(religion) = self.external_state.country_religions.get(*country) {
                state
                    .country_religions
                    .insert((*country).into(), religion.clone());
            }
            if let Some(ideas) = self.external_state.country_ideas.get(*country) {
                state.country_ideas.insert((*country).into(), ideas.clone());
            }
            if let Some(advisors) = self.external_state.advisors.get(*country) {
                state.advisors.insert((*country).into(), advisors.clone());
            }
            if let Some(markers) = self.external_state.country_markers.get(*country) {
                state
                    .country_markers
                    .insert((*country).into(), markers.clone());
            }
            if let Some(statuses) = self.external_state.country_statuses.get(*country) {
                state
                    .country_statuses
                    .insert((*country).into(), statuses.clone());
            }
            if let Some(estates) = self.external_state.estate_privileges.get(*country) {
                let entity = world.initial.country_entity(country).ok_or_else(|| {
                    Error::message("save state", "save country is missing from the world")
                })?;
                let available = world
                    .initial
                    .world()
                    .estates()
                    .countries
                    .get(&entity)
                    .ok_or_else(|| {
                        Error::message("save state", "save country has no estate state")
                    })?;
                let filtered = estates
                    .iter()
                    .filter(|(estate, _)| available.contains_key(*estate))
                    .map(|(estate, privileges)| (estate.clone(), privileges.clone()))
                    .collect::<BTreeMap<_, _>>();
                if !filtered.is_empty() {
                    state.estate_privileges.insert((*country).into(), filtered);
                }
            }
            if let Some(estates) = self.external_state.estate_states.get(*country) {
                let entity = world.initial.country_entity(country).ok_or_else(|| {
                    Error::message("save state", "save country is missing from the world")
                })?;
                let available = world
                    .initial
                    .world()
                    .estates()
                    .countries
                    .get(&entity)
                    .ok_or_else(|| {
                        Error::message("save state", "save country has no estate state")
                    })?;
                let filtered = estates
                    .iter()
                    .filter(|(estate, _)| available.contains_key(*estate))
                    .map(|(estate, values)| (estate.clone(), values.clone()))
                    .collect::<BTreeMap<_, _>>();
                if !filtered.is_empty() {
                    state.estate_states.insert((*country).into(), filtered);
                }
            }
        }

        for (trade_good, price) in &self.external_state.trade_good_prices_milli {
            if world
                .initial
                .world()
                .economy()
                .trade_goods
                .contains_key(trade_good)
            {
                state
                    .trade_good_prices_milli
                    .insert(trade_good.clone(), *price);
            }
        }

        for army in &self.external_state.armies {
            if countries.contains(army.owner.as_str())
                && world.initial.province_entity(army.location).is_some()
            {
                state.armies.push(army.clone());
            }
        }

        for naval_unit in &self.external_state.naval_units {
            if !countries.contains(naval_unit.owner.as_str()) {
                continue;
            }
            let Some(entity) = world.initial.province_entity(naval_unit.location) else {
                continue;
            };
            if world
                .initial
                .world()
                .province_has_port(entity)
                .is_ok_and(|has_port| has_port)
            {
                state.naval_units.push(naval_unit.clone());
            }
        }

        if let Some(ceiling) = world.report.setup.monarch_power_base_maximum {
            for country in &countries {
                let Some(values) = state.country_metric_values.get(*country) else {
                    continue;
                };
                let ceilings = ["adm_power", "dip_power", "mil_power"]
                    .into_iter()
                    .filter(|name| values.contains_key(*name) && definitions.contains_key(*name))
                    .map(|name| (name.into(), Some(ceiling)))
                    .collect::<BTreeMap<_, _>>();
                if !ceilings.is_empty() {
                    state
                        .country_metric_ceilings
                        .insert((*country).into(), ceilings);
                }
            }
        }

        for (province, owner) in &self.external_state.province_owners {
            if world.initial.province_entity(*province).is_none() {
                continue;
            }
            if let Some(owner) = owner
                && !countries.contains(owner.as_str())
            {
                return Err(Error::message(
                    "save state",
                    format!("province {province} references unknown saved owner {owner}"),
                ));
            }
            state.province_owners.insert(*province, owner.clone());
        }

        for (province, trade_good) in &self.external_state.province_trade_goods {
            if world.initial.province_entity(*province).is_none() {
                continue;
            }
            if let Some(trade_good) = trade_good
                && !world
                    .initial
                    .world()
                    .economy()
                    .trade_goods
                    .contains_key(trade_good)
            {
                return Err(Error::message(
                    "save state",
                    format!("province {province} references unknown saved trade good {trade_good}"),
                ));
            }
            state
                .province_trade_goods
                .insert(*province, trade_good.clone());
        }

        for province in self.external_state.province_owners.keys() {
            if world.initial.province_entity(*province).is_none() {
                continue;
            }
            let controller = self
                .external_state
                .province_controllers
                .get(province)
                .and_then(|controller| controller.as_ref())
                .filter(|tag| countries.contains(tag.as_str()))
                .cloned();
            state.province_controllers.insert(*province, controller);
        }

        for (province, culture) in &self.external_state.province_cultures {
            if world.initial.province_entity(*province).is_some() {
                state.province_cultures.insert(*province, culture.clone());
            }
        }

        for (province, religion) in &self.external_state.province_religions {
            if world.initial.province_entity(*province).is_some() {
                state.province_religions.insert(*province, religion.clone());
            }
        }

        for (province, development) in &self.external_state.province_development {
            if world.initial.province_entity(*province).is_some() {
                state
                    .province_development
                    .insert(*province, development.clone());
            }
        }

        for (province, value) in &self.external_state.province_raw_autonomy_milli {
            let Some(entity) = world.initial.province_entity(*province) else {
                continue;
            };
            if world
                .initial
                .world()
                .administration()
                .provinces
                .contains_key(&entity)
            {
                state.province_raw_autonomy_milli.insert(*province, *value);
            }
        }

        for (province, value) in &self.external_state.province_is_city {
            if world.initial.province_entity(*province).is_some() {
                state.province_is_city.insert(*province, *value);
            }
        }

        for (province, value) in &self.external_state.province_in_empire {
            if world.initial.province_entity(*province).is_some() {
                state.province_in_empire.insert(*province, *value);
            }
        }

        let players = self
            .players
            .iter()
            .filter(|tag| countries.contains(tag.as_str()))
            .cloned()
            .collect();
        Ok((players, state))
    }
}

/// Read supported state from an EU4 text save or a ZIP-wrapped text save.
pub fn read_vanilla_save_state(path: impl AsRef<Path>) -> Result<VanillaSaveState, Error> {
    let path = path.as_ref().to_path_buf();
    let parser_path = path.clone();
    let handle = std::thread::Builder::new()
        .name("augustus-eu4-save-parser".into())
        .stack_size(32 * 1024 * 1024)
        .spawn(move || read_vanilla_save_state_inner(&parser_path))
        .map_err(|error| Error::caused_by(&path, error))?;
    handle
        .join()
        .map_err(|_| Error::message(&path, "EU4 save parser thread panicked"))?
}

fn read_vanilla_save_state_inner(path: &Path) -> Result<VanillaSaveState, Error> {
    let documents = read_eu4_save_documents(path)?;
    let parsed = documents.game_state.parse()?;
    let metadata = documents
        .meta
        .as_ref()
        .map(|document| document.parse())
        .transpose()?;
    let root = parsed.reader();
    let date = match optional_scalar(&parsed, &root, "date")? {
        Some(date) => date,
        None => {
            let Some(metadata) = metadata.as_ref() else {
                return Err(Error::message(parsed.source(), "missing save field date"));
            };
            optional_scalar(metadata, &metadata.reader(), "date")?
                .ok_or_else(|| Error::message(parsed.source(), "missing save field date"))?
        }
    };
    let current_date =
        date_key(&date).ok_or_else(|| Error::message(parsed.source(), "invalid save date"))?;
    let player = match optional_scalar(&parsed, &root, "player")? {
        Some(player) => Some(player),
        None => metadata
            .as_ref()
            .map(|metadata| optional_scalar(metadata, &metadata.reader(), "player"))
            .transpose()?
            .flatten(),
    };
    let mut players = BTreeSet::new();
    if let Some(player) = &player
        && is_country_tag(player)
    {
        players.insert(player.clone());
    }

    let mut external_state = VanillaExternalState::default();
    if let Some(change_price) = optional_object(&parsed, &root, "change_price")? {
        for (key, op, value) in change_price.fields() {
            if op.is_some_and(|operator| operator != jomini::text::Operator::Equal) {
                return Err(Error::message(
                    parsed.source(),
                    "invalid saved trade-good price block",
                ));
            }
            let name = key.read_string();
            let price = optional_milli(&parsed, &object(&parsed, &value)?, "current_price")?;
            if external_state
                .trade_good_prices_milli
                .insert(name.clone(), price)
                .is_some()
            {
                return Err(Error::message(
                    parsed.source(),
                    format!("duplicate saved trade-good price {name}"),
                ));
            }
        }
    }
    let countries = object(&parsed, &required(&parsed, &root, "countries")?)?;
    for (key, op, value) in countries.fields() {
        let fail = || Error::message(parsed.source(), "invalid saved country block");
        if op.is_some_and(|operator| operator != jomini::text::Operator::Equal) {
            return Err(fail());
        }
        let tag = key.read_string();
        if !is_country_tag(&tag) {
            continue;
        }
        let country = object(&parsed, &value)?;
        let resources = CountryResources {
            treasury_milli: optional_milli(&parsed, &country, "treasury")?,
            manpower_milli: optional_milli(&parsed, &country, "manpower")?,
            mercantilism_milli: optional_milli(&parsed, &country, "mercantilism")?,
        };
        external_state
            .country_resources
            .insert(tag.clone(), resources);

        if let Some(value) = optional_scalar(&parsed, &country, "technology_group")? {
            external_state
                .country_technology_groups
                .insert(tag.clone(), Some(value));
        }
        if let Some(value) = optional_scalar(&parsed, &country, "unit_type")? {
            external_state
                .country_unit_types
                .insert(tag.clone(), Some(value));
        }
        if let Some(value) = optional_scalar(&parsed, &country, "government_rank")? {
            let rank = value.parse::<u8>().map_err(|error| {
                Error::message(
                    parsed.source(),
                    format!("country {tag} has invalid government rank: {error}"),
                )
            })?;
            if !(1..=3).contains(&rank) {
                return Err(Error::message(
                    parsed.source(),
                    format!("country {tag} government rank is outside 1..=3"),
                ));
            }
            external_state
                .country_government_ranks
                .insert(tag.clone(), Some(rank));
        }
        if let Some(government) = optional_object(&parsed, &country, "government")? {
            external_state.country_government_kinds.insert(
                tag.clone(),
                optional_scalar(&parsed, &government, "government")?,
            );

            let reforms =
                if let Some(stack) = optional_object(&parsed, &government, "reform_stack")? {
                    optional_field(&parsed, &stack, "reforms")?
                        .map(|value| string_id_set(&parsed, &value, "government reform"))
                        .transpose()?
                } else {
                    None
                };
            external_state
                .country_government_reforms
                .insert(tag.clone(), reforms);

            let mechanics = saved_government_mechanic_powers(&parsed, &government)?;
            if !mechanics.is_empty() {
                external_state
                    .country_government_mechanic_powers
                    .insert(tag.clone(), mechanics);
            }
        }

        if let Some(value) = optional_scalar(&parsed, &country, "primary_culture")? {
            external_state
                .country_primary_cultures
                .insert(tag.clone(), Some(value));
        }
        if let Some(value) = optional_scalar(&parsed, &country, "religion")? {
            external_state
                .country_religions
                .insert(tag.clone(), Some(value));
        }

        if let Some(value) = optional_scalar(&parsed, &country, "capital")? {
            let capital = value.parse::<u32>().map_err(|error| {
                Error::message(
                    parsed.source(),
                    format!("country {tag} has invalid capital province: {error}"),
                )
            })?;
            external_state
                .country_capitals
                .insert(tag.clone(), (capital != 0).then_some(capital));
        }

        if let Some(technology) = optional_object(&parsed, &country, "technology")? {
            let mut levels = BTreeMap::new();
            for (key, op, value) in technology.fields() {
                if op.is_some_and(|operator| operator != jomini::text::Operator::Equal) {
                    return Err(Error::message(
                        parsed.source(),
                        format!("country {tag} has an invalid technology level"),
                    ));
                }
                let branch = match key.read_string().as_str() {
                    "adm_tech" => "administrative",
                    "dip_tech" => "diplomatic",
                    "mil_tech" => "military",
                    _ => continue,
                };
                let level = scalar(&parsed, &value)?.parse::<u16>().map_err(|error| {
                    Error::message(
                        parsed.source(),
                        format!("country {tag} has invalid {branch} technology level: {error}"),
                    )
                })?;
                if levels.insert(branch.to_owned(), level).is_some() {
                    return Err(Error::message(
                        parsed.source(),
                        format!("country {tag} repeats its {branch} technology level"),
                    ));
                }
            }
            if !levels.is_empty() {
                external_state
                    .country_technology_levels
                    .insert(tag.clone(), levels);
            }
        }

        if let Some(active_ideas) = optional_object(&parsed, &country, "active_idea_groups")? {
            let mut groups = BTreeMap::new();
            for (key, op, value) in active_ideas.fields() {
                if op.is_some_and(|operator| operator != jomini::text::Operator::Equal) {
                    return Err(Error::message(
                        parsed.source(),
                        format!("country {tag} has an invalid active idea group"),
                    ));
                }
                let group = key.read_string();
                if group.is_empty() {
                    return Err(Error::message(
                        parsed.source(),
                        format!("country {tag} has an empty active idea group"),
                    ));
                }
                let progress = scalar(&parsed, &value)?.parse::<u16>().map_err(|error| {
                    Error::message(
                        parsed.source(),
                        format!("country {tag} has invalid idea progress: {error}"),
                    )
                })?;
                if groups.insert(group, Some(progress)).is_some() {
                    return Err(Error::message(
                        parsed.source(),
                        format!("country {tag} repeats an active idea group"),
                    ));
                }
            }
            external_state.country_ideas.insert(
                tag.clone(),
                IdeaState {
                    groups: Some(groups),
                },
            );
        }

        if let Some(value) = optional_scalar(&parsed, &country, "national_focus")? {
            let focus = match value.as_str() {
                "ADM" => NationalFocus {
                    skill: Some(RulerSkill::Administration),
                },
                "DIP" => NationalFocus {
                    skill: Some(RulerSkill::Diplomacy),
                },
                "MIL" => NationalFocus {
                    skill: Some(RulerSkill::Military),
                },
                "none" | "NONE" => NationalFocus { skill: None },
                _ => {
                    return Err(Error::message(
                        parsed.source(),
                        format!("country {tag} has unsupported national focus {value}"),
                    ));
                }
            };
            external_state
                .country_national_focus
                .insert(tag.clone(), focus);
        }

        let mut metrics = BTreeMap::new();
        if let Some(value) = optional_milli(&parsed, &country, "stability")? {
            if value % 1000 != 0 {
                return Err(Error::message(
                    parsed.source(),
                    format!("country {tag} has fractional stability"),
                ));
            }
            metrics.insert("stability".into(), Some(value / 1000));
        }
        if let Some(value) = optional_milli(&parsed, &country, "current_power_projection")? {
            metrics.insert("power_projection".into(), Some(value));
        }
        for (field, metric) in [
            ("prestige", "prestige"),
            ("legitimacy", "legitimacy"),
            ("republican_tradition", "republican_tradition"),
            ("devotion", "devotion"),
        ] {
            if let Some(value) = optional_milli(&parsed, &country, field)? {
                metrics.insert(metric.into(), Some(value));
            }
        }
        if let Some(values) = last_power_vector(&parsed, &country)? {
            metrics.insert("adm_power".into(), Some(values[0]));
            metrics.insert("dip_power".into(), Some(values[1]));
            metrics.insert("mil_power".into(), Some(values[2]));
        }
        if !metrics.is_empty() {
            external_state
                .country_government_metric_values
                .insert(tag.clone(), metrics.clone());
            external_state
                .country_metric_values
                .insert(tag.clone(), metrics);
        }

        if let Some(value) = optional_scalar(&parsed, &country, "is_at_war")? {
            let at_war = match value.as_str() {
                "yes" => true,
                "no" => false,
                _ => {
                    return Err(Error::message(
                        parsed.source(),
                        format!("country {tag} has an invalid is_at_war flag"),
                    ));
                }
            };
            external_state
                .country_statuses
                .entry(tag.clone())
                .or_default()
                .insert("at_war".into(), Some(at_war));
        }

        let mut markers = BTreeSet::new();
        if let Some(flags) = optional_object(&parsed, &country, "flags")? {
            for (flag, op, _) in flags.fields() {
                if op.is_some_and(|operator| operator != jomini::text::Operator::Equal) {
                    return Err(Error::message(
                        parsed.source(),
                        "invalid saved country flag",
                    ));
                }
                markers.insert(flag.read_string());
            }
        }
        if optional_object(&parsed, &country, "flags")?.is_some() {
            external_state
                .country_markers
                .insert(tag.clone(), Some(markers));
        }

        let mut estates = BTreeMap::new();
        let mut estate_states = BTreeMap::new();
        for (estate_key, estate_op, estate_value) in country
            .fields()
            .filter(|(key, _, _)| key.read_string() == "estate")
        {
            if estate_op.is_some_and(|operator| operator != jomini::text::Operator::Equal) {
                return Err(Error::message(
                    parsed.source(),
                    "invalid saved estate block",
                ));
            }
            let estate = object(&parsed, &estate_value)?;
            let Some(estate_name) = optional_scalar(&parsed, &estate, "type")? else {
                continue;
            };
            let loyalty_milli = optional_milli(&parsed, &estate, "loyalty")?;
            let land_share_micros = optional_milli(&parsed, &estate, "territory")?
                .map(|value| {
                    value.checked_mul(10).ok_or_else(|| {
                        Error::message(parsed.source(), "saved estate territory overflows")
                    })
                })
                .transpose()?;
            let active_influences = optional_field(&parsed, &estate, "active_influences")?
                .map(|value| numeric_id_set(&parsed, &value, "active_influences"))
                .transpose()?
                .or_else(|| Some(BTreeSet::new()));
            if loyalty_milli.is_some() || land_share_micros.is_some() || active_influences.is_some()
            {
                estate_states.insert(
                    estate_name.clone(),
                    VanillaExternalEstateState {
                        loyalty_milli,
                        land_share_micros,
                        active_influences,
                    },
                );
            }
            let privileges = optional_field(&parsed, &estate, "granted_privileges")?
                .map(|value| privilege_names(&value))
                .unwrap_or_default();
            estates.insert(estate_name, Some(privileges));
            let _ = estate_key;
        }
        if !estates.is_empty() {
            external_state
                .estate_privileges
                .insert(tag.clone(), estates);
        }
        if !estate_states.is_empty() {
            external_state
                .estate_states
                .insert(tag.clone(), estate_states);
        }

        for (key, op, value) in country
            .fields()
            .filter(|(key, _, _)| key.read_string() == "army")
        {
            if op.is_some_and(|operator| operator != jomini::text::Operator::Equal) {
                return Err(Error::message(parsed.source(), "invalid saved army block"));
            }
            let (location, name, regiments) = saved_army(&parsed, &value)?;
            external_state.armies.push(VanillaExternalArmy {
                owner: tag.clone(),
                location,
                name,
                regiments,
            });
            let _ = key;
        }

        for (key, op, value) in country
            .fields()
            .filter(|(key, _, _)| key.read_string() == "navy")
        {
            if op.is_some_and(|operator| operator != jomini::text::Operator::Equal) {
                return Err(Error::message(parsed.source(), "invalid saved navy block"));
            }
            let (location, ships) = saved_navy(&parsed, &value)?;
            for unit_type in ships {
                external_state.naval_units.push(VanillaExternalNavalUnit {
                    owner: tag.clone(),
                    location,
                    unit_type,
                });
            }
            let _ = key;
        }

        if optional_scalar(&parsed, &country, "human")?.as_deref() == Some("yes") {
            players.insert(tag);
        }
    }

    for tag in external_state
        .country_resources
        .keys()
        .cloned()
        .collect::<Vec<_>>()
    {
        external_state.advisors.entry(tag).or_default();
    }

    let provinces = object(&parsed, &required(&parsed, &root, "provinces")?)?;
    for (key, op, value) in provinces.fields() {
        if op.is_some_and(|operator| operator != jomini::text::Operator::Equal) {
            return Err(Error::message(
                parsed.source(),
                "invalid saved province block",
            ));
        }
        let Some(id) = key
            .read_string()
            .strip_prefix('-')
            .and_then(|text| text.parse::<u32>().ok())
            .filter(|id| *id > 0)
        else {
            return Err(Error::message(
                parsed.source(),
                "expected negative save province identifier",
            ));
        };
        let province = object(&parsed, &value)?;
        let owner = optional_scalar(&parsed, &province, "owner")?;
        if let Some(owner) = &owner
            && !is_country_tag(owner)
        {
            return Err(Error::message(
                parsed.source(),
                format!("province {id} has an invalid owner tag"),
            ));
        }
        external_state.province_owners.insert(id, owner.clone());
        if let Some(culture) = optional_scalar(&parsed, &province, "culture")? {
            external_state.province_cultures.insert(id, Some(culture));
        }
        if let Some(religion) = optional_scalar(&parsed, &province, "religion")? {
            external_state.province_religions.insert(id, Some(religion));
        }
        if let Some(trade_good) = optional_scalar(&parsed, &province, "trade_goods")? {
            external_state
                .province_trade_goods
                .insert(id, Some(trade_good));
        }
        if let Some(controller) = optional_scalar(&parsed, &province, "controller")? {
            if !is_country_tag(&controller) {
                return Err(Error::message(
                    parsed.source(),
                    format!("province {id} has an invalid controller tag"),
                ));
            }
            external_state
                .province_controllers
                .insert(id, Some(controller));
        }
        let development = VanillaExternalProvinceDevelopment {
            base_tax_milli: optional_milli(&parsed, &province, "base_tax")?,
            base_production_milli: optional_milli(&parsed, &province, "base_production")?,
            base_manpower_milli: optional_milli(&parsed, &province, "base_manpower")?,
        };
        if development.base_tax_milli.is_some_and(|value| value < 0)
            || development
                .base_production_milli
                .is_some_and(|value| value < 0)
            || development
                .base_manpower_milli
                .is_some_and(|value| value < 0)
        {
            return Err(Error::message(
                parsed.source(),
                format!("province {id} has negative saved development"),
            ));
        }
        if development.base_tax_milli.is_some()
            || development.base_production_milli.is_some()
            || development.base_manpower_milli.is_some()
        {
            external_state.province_development.insert(id, development);
        }
        if let Some(owner) = owner {
            let advisors = external_state.advisors.entry(owner).or_default();
            collect_saved_advisors(&parsed, &province, current_date, advisors, 0)?;
        }
        let autonomy = optional_milli(&parsed, &province, "local_autonomy")?.unwrap_or(0);
        if !(0..=100_000).contains(&autonomy) {
            return Err(Error::message(
                parsed.source(),
                "saved autonomy is outside 0..100",
            ));
        }
        external_state
            .province_raw_autonomy_milli
            .insert(id, Some(autonomy));
        if let Some(value) = optional_scalar(&parsed, &province, "is_city")? {
            let is_city = match value.as_str() {
                "yes" => true,
                "no" => false,
                _ => {
                    return Err(Error::message(
                        parsed.source(),
                        format!("province {id} has an invalid is_city flag"),
                    ));
                }
            };
            external_state.province_is_city.insert(id, Some(is_city));
        }
        if let Some(history) = optional_object(&parsed, &province, "history")?
            && let Some(value) = optional_scalar(&parsed, &history, "hre")?
        {
            let in_empire = match value.as_str() {
                "yes" => true,
                "no" => false,
                _ => {
                    return Err(Error::message(
                        parsed.source(),
                        format!("province {id} has an invalid HRE membership flag"),
                    ));
                }
            };
            external_state
                .province_in_empire
                .insert(id, Some(in_empire));
        }
    }

    Ok(VanillaSaveState {
        date,
        player,
        players,
        external_state,
    })
}

fn is_country_tag(value: &str) -> bool {
    value.len() == 3
        && value
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
}

fn date_key(value: &str) -> Option<(i32, u8, u8)> {
    let mut parts = value.split('.');
    Some((
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
    ))
}

fn collect_saved_advisors(
    parsed: &ParsedText<'_>,
    reader: &Object<'_, '_>,
    current_date: (i32, u8, u8),
    output: &mut Vec<VanillaExternalAdvisor>,
    depth: usize,
) -> Result<(), Error> {
    if depth > 12 {
        return Ok(());
    }
    for (key, op, value) in reader.fields() {
        let name = key.read_string();
        if op.is_some_and(|operator| operator != jomini::text::Operator::Equal) {
            return Err(Error::message(
                parsed.source(),
                format!("invalid saved advisor field {name}"),
            ));
        }
        if name == "advisor" {
            let advisor = object(parsed, &value)?;
            let Some(hire_date) = optional_scalar(parsed, &advisor, "hire_date")? else {
                continue;
            };
            let Some(hire_date) = date_key(&hire_date) else {
                return Err(Error::message(
                    parsed.source(),
                    "invalid saved advisor hire date",
                ));
            };
            if hire_date > current_date {
                continue;
            }
            if let Some(death_date) = optional_scalar(parsed, &advisor, "death_date")? {
                let Some(death_date) = date_key(&death_date) else {
                    return Err(Error::message(
                        parsed.source(),
                        "invalid saved advisor death date",
                    ));
                };
                if death_date <= current_date {
                    continue;
                }
            }
            let type_name = optional_scalar(parsed, &advisor, "type")?.ok_or_else(|| {
                Error::message(parsed.source(), "saved advisor is missing its type")
            })?;
            let skill = optional_scalar(parsed, &advisor, "skill")?
                .ok_or_else(|| {
                    Error::message(parsed.source(), "saved advisor is missing its skill")
                })?
                .parse::<u8>()
                .map_err(|error| Error::caused_by(parsed.source(), error))?;
            let id = optional_object(parsed, &advisor, "id")?
                .and_then(|id| optional_scalar(parsed, &id, "id").ok().flatten())
                .and_then(|id| id.parse::<u64>().ok());
            let candidate = VanillaExternalAdvisor {
                type_name,
                skill,
                id,
            };
            if candidate.id.is_none() || !output.iter().any(|existing| existing.id == candidate.id)
            {
                output.push(candidate);
            }
        } else if let Ok(nested) = value.read_object() {
            collect_saved_advisors(parsed, &nested, current_date, output, depth + 1)?;
        }
    }
    Ok(())
}

fn saved_army(
    parsed: &ParsedText<'_>,
    value: &Field<'_, '_>,
) -> Result<(u32, String, Vec<String>), Error> {
    let army = object(parsed, value)?;
    let mut location = None;
    let mut name = String::new();
    let mut regiments = Vec::new();
    for (key, op, value) in army.fields() {
        let field = key.read_string();
        if op.is_some_and(|operator| operator != jomini::text::Operator::Equal) {
            return Err(Error::message(
                parsed.source(),
                format!("invalid saved army field {field}"),
            ));
        }
        match field.as_str() {
            "location" => {
                let text = scalar(parsed, &value)?;
                let id = text
                    .parse::<u32>()
                    .map_err(|error| Error::caused_by(parsed.source(), error))?;
                if id == 0 {
                    return Err(Error::message(
                        parsed.source(),
                        "saved army location must be positive",
                    ));
                }
                location = Some(id);
            }
            "name" => name = scalar(parsed, &value)?,
            "regiment" => {
                let regiment = object(parsed, &value)?;
                let unit_type = optional_scalar(parsed, &regiment, "type")?.ok_or_else(|| {
                    Error::message(parsed.source(), "saved regiment is missing its type")
                })?;
                if unit_type.is_empty() || unit_type.bytes().any(|byte| byte.is_ascii_whitespace())
                {
                    return Err(Error::message(
                        parsed.source(),
                        "saved regiment has an invalid type",
                    ));
                }
                regiments.push(unit_type);
            }
            _ => {}
        }
    }
    let location = location
        .ok_or_else(|| Error::message(parsed.source(), "saved army is missing its location"))?;
    if regiments.is_empty() {
        return Err(Error::message(
            parsed.source(),
            "saved army has no regiments",
        ));
    }
    Ok((location, name, regiments))
}

fn saved_navy(parsed: &ParsedText<'_>, value: &Field<'_, '_>) -> Result<(u32, Vec<String>), Error> {
    let navy = object(parsed, value)?;
    let mut location = None;
    let mut ships = Vec::new();
    for (key, op, value) in navy.fields() {
        let field = key.read_string();
        if op.is_some_and(|operator| operator != jomini::text::Operator::Equal) {
            return Err(Error::message(
                parsed.source(),
                format!("invalid saved navy field {field}"),
            ));
        }
        match field.as_str() {
            "location" => {
                let text = scalar(parsed, &value)?;
                let id = text
                    .parse::<u32>()
                    .map_err(|error| Error::caused_by(parsed.source(), error))?;
                if id == 0 {
                    return Err(Error::message(
                        parsed.source(),
                        "saved navy location must be positive",
                    ));
                }
                location = Some(id);
            }
            "ship" => {
                let ship = object(parsed, &value)?;
                let unit_type = optional_scalar(parsed, &ship, "type")?.ok_or_else(|| {
                    Error::message(parsed.source(), "saved ship is missing its type")
                })?;
                if unit_type.is_empty() || unit_type.bytes().any(|byte| byte.is_ascii_whitespace())
                {
                    return Err(Error::message(
                        parsed.source(),
                        "saved ship has an invalid type",
                    ));
                }
                ships.push(unit_type);
            }
            _ => {}
        }
    }
    let location = location
        .ok_or_else(|| Error::message(parsed.source(), "saved navy is missing its location"))?;
    if ships.is_empty() {
        return Err(Error::message(parsed.source(), "saved navy has no ships"));
    }
    Ok((location, ships))
}

fn optional_field<'a, 'b>(
    parsed: &ParsedText<'_>,
    reader: &Object<'a, 'b>,
    name: &str,
) -> Result<Option<Field<'a, 'b>>, Error> {
    let mut result = None;
    for (_key, op, value) in reader
        .fields()
        .filter(|(key, _, _)| key.read_string() == name)
    {
        if op.is_some_and(|operator| operator != jomini::text::Operator::Equal) {
            return Err(Error::message(
                parsed.source(),
                format!("invalid saved field {name}"),
            ));
        }
        result = Some(value);
    }
    Ok(result)
}

fn required<'a, 'b>(
    parsed: &ParsedText<'_>,
    reader: &Object<'a, 'b>,
    name: &str,
) -> Result<Field<'a, 'b>, Error> {
    optional_field(parsed, reader, name)?
        .ok_or_else(|| Error::message(parsed.source(), format!("missing save field {name}")))
}

fn optional_scalar(
    parsed: &ParsedText<'_>,
    reader: &Object<'_, '_>,
    name: &str,
) -> Result<Option<String>, Error> {
    optional_field(parsed, reader, name)?
        .map(|value| scalar(parsed, &value))
        .transpose()
}

fn scalar(parsed: &ParsedText<'_>, value: &Field<'_, '_>) -> Result<String, Error> {
    value
        .read_string()
        .map_err(|error| Error::caused_by(parsed.source(), error))
}

fn object<'a, 'b>(parsed: &ParsedText<'_>, value: &Field<'a, 'b>) -> Result<Object<'a, 'b>, Error> {
    if !matches!(value.token(), TextToken::Object { mixed: false, .. })
        && !(matches!(value.token(), TextToken::Array { mixed: false, .. })
            && value
                .read_array()
                .is_ok_and(|array| array.values().next().is_none()))
    {
        return Err(Error::message(parsed.source(), "expected save object"));
    }
    value
        .read_object()
        .map_err(|error| Error::caused_by(parsed.source(), error))
}

fn optional_object<'a, 'b>(
    parsed: &ParsedText<'_>,
    reader: &Object<'a, 'b>,
    name: &str,
) -> Result<Option<Object<'a, 'b>>, Error> {
    optional_field(parsed, reader, name)?
        .map(|value| object(parsed, &value))
        .transpose()
}

fn optional_milli(
    parsed: &ParsedText<'_>,
    reader: &Object<'_, '_>,
    name: &str,
) -> Result<Option<i64>, Error> {
    optional_scalar(parsed, reader, name)?
        .map(|value| {
            signed_milli(&value).ok_or_else(|| {
                Error::message(
                    parsed.source(),
                    format!("invalid saved decimal field {name}"),
                )
            })
        })
        .transpose()
}

fn signed_milli(value: &str) -> Option<i64> {
    let (negative, unsigned) = match value.strip_prefix('-') {
        Some(value) => (true, value),
        None => (false, value.strip_prefix('+').unwrap_or(value)),
    };
    let (whole, fraction) = unsigned.split_once('.').unwrap_or((unsigned, ""));
    if whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || fraction.len() > 3
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    let magnitude =
        whole
            .parse::<i64>()
            .ok()?
            .checked_mul(1000)?
            .checked_add(if fraction.is_empty() {
                0
            } else {
                fraction.parse::<i64>().ok()? * 10_i64.pow(3 - fraction.len() as u32)
            })?;
    if negative {
        magnitude.checked_neg()
    } else {
        Some(magnitude)
    }
}

fn last_power_vector(
    parsed: &ParsedText<'_>,
    reader: &Object<'_, '_>,
) -> Result<Option<[i64; 3]>, Error> {
    let mut result = None;
    for (_key, op, value) in reader
        .fields()
        .filter(|(key, _, _)| key.read_string() == "powers")
    {
        if op.is_some_and(|operator| operator != jomini::text::Operator::Equal) {
            return Err(Error::message(
                parsed.source(),
                "invalid saved powers field",
            ));
        }
        let array = value
            .read_array()
            .map_err(|error| Error::caused_by(parsed.source(), error))?;
        let values = array
            .values()
            .map(|value| {
                scalar(parsed, &value)?
                    .parse::<i64>()
                    .map_err(|error| Error::caused_by(parsed.source(), error))
            })
            .collect::<Result<Vec<_>, _>>()?;
        if values.len() != 3 {
            return Err(Error::message(
                parsed.source(),
                "saved powers requires three values",
            ));
        }
        result = Some([values[0], values[1], values[2]]);
    }
    Ok(result)
}

fn privilege_names(value: &Field<'_, '_>) -> BTreeSet<String> {
    fn collect(value: &Field<'_, '_>, depth: usize, result: &mut BTreeSet<String>) {
        if depth > 4 {
            return;
        }
        if let Ok(text) = value.read_string() {
            if !text.is_empty() && !text.contains('.') && text.parse::<i64>().is_err() {
                result.insert(text);
            }
            return;
        }
        if let Ok(array) = value.read_array() {
            for item in array.values() {
                collect(&item, depth + 1, result);
            }
            return;
        }
        if let Ok(object) = value.read_object() {
            for (key, _, item) in object.fields() {
                let name = key.read_string();
                if !name.is_empty() && name != "type" && name.parse::<i64>().is_err() {
                    result.insert(name);
                }
                collect(&item, depth + 1, result);
            }
        }
    }

    let mut result = BTreeSet::new();
    collect(value, 0, &mut result);
    result
}

fn numeric_id_set(
    parsed: &ParsedText<'_>,
    value: &Field<'_, '_>,
    field: &str,
) -> Result<BTreeSet<u32>, Error> {
    let array = value
        .read_array()
        .map_err(|error| Error::caused_by(parsed.source(), error))?;
    array
        .values()
        .map(|item| {
            scalar(parsed, &item)?.parse::<u32>().map_err(|error| {
                Error::message(
                    parsed.source(),
                    format!("invalid saved {field} identifier: {error}"),
                )
            })
        })
        .collect()
}

fn string_id_set(
    parsed: &ParsedText<'_>,
    value: &Field<'_, '_>,
    field: &str,
) -> Result<BTreeSet<String>, Error> {
    let array = value
        .read_array()
        .map_err(|error| Error::caused_by(parsed.source(), error))?;
    let mut result = BTreeSet::new();
    for item in array.values() {
        let id = scalar(parsed, &item)?;
        if id.is_empty() || !result.insert(id) {
            return Err(Error::message(
                parsed.source(),
                format!("invalid or duplicate saved {field} identifier"),
            ));
        }
    }
    Ok(result)
}

fn saved_government_mechanic_powers(
    parsed: &ParsedText<'_>,
    government: &Object<'_, '_>,
) -> Result<BTreeMap<String, BTreeMap<String, augustus_core::GovernmentMechanicResourceState>>, Error>
{
    let mut mechanics = BTreeMap::new();
    for (key, op, value) in government
        .fields()
        .filter(|(key, _, _)| key.read_string() == "mechanic_type")
    {
        if op.is_some_and(|operator| operator != jomini::text::Operator::Equal) {
            return Err(Error::message(
                parsed.source(),
                "invalid saved mechanic block",
            ));
        }
        let block = object(parsed, &value)?;
        let mechanic = optional_scalar(parsed, &block, "mechanic_type")?
            .filter(|id| !id.is_empty())
            .ok_or_else(|| Error::message(parsed.source(), "saved mechanic has no type"))?;
        let mut powers = BTreeMap::new();
        for (power_key, power_op, value) in block
            .fields()
            .filter(|(key, _, _)| key.read_string() != "mechanic_type")
        {
            if power_op.is_some_and(|operator| operator != jomini::text::Operator::Equal) {
                return Err(Error::message(
                    parsed.source(),
                    "invalid saved government power field",
                ));
            }
            let power_id = power_key.read_string();
            if power_id.is_empty() {
                return Err(Error::message(
                    parsed.source(),
                    "saved government power has an empty identifier",
                ));
            }
            let power = object(parsed, &value)?;
            let power_milli = optional_milli(parsed, &power, "power")?;
            let frozen = optional_scalar(parsed, &power, "frozen")?
                .map(|value| match value.as_str() {
                    "yes" => Ok(true),
                    "no" => Ok(false),
                    _ => Err(Error::message(
                        parsed.source(),
                        format!("saved government power {power_id} has an invalid frozen flag"),
                    )),
                })
                .transpose()?;
            if powers
                .insert(
                    power_id,
                    augustus_core::GovernmentMechanicResourceState {
                        power_milli,
                        frozen,
                    },
                )
                .is_some()
            {
                return Err(Error::message(
                    parsed.source(),
                    "duplicate saved government power",
                ));
            }
        }
        if mechanics.insert(mechanic, powers).is_some() {
            return Err(Error::message(
                parsed.source(),
                "duplicate saved government mechanic",
            ));
        }
        let _ = key;
    }
    Ok(mechanics)
}

#[cfg(test)]
mod tests {
    use super::{NationalFocus, RulerSkill, read_vanilla_save_state};
    use crate::{ContentRoot, ContentSet, ContentSource, VanillaExternalProvinceDevelopment};
    use std::{
        collections::{BTreeMap, BTreeSet},
        fs::File,
        io::Write,
        path::Path,
    };
    use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

    #[test]
    fn reads_startup_state_from_plaintext_save() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("state.eu4");
        std::fs::write(
            &path,
            br#"EU4txt
date=1444.11.11
player="AAA"
change_price={
	grain={
		current_price=3.500
	}
	missing_good={}
}
countries={
	AAA={
		capital=1
		government_rank=3
		technology_group=test_group
		unit_type=saved_unit_type
		primary_culture=saved_country_culture
		religion=saved_country_religion
		technology={
			adm_tech=3
			dip_tech=4
			mil_tech=5
		}
		national_focus=DIP
		active_idea_groups={
			test_alpha=2
		}
		treasury=59.802
		manpower=9.754
		mercantilism=10.000
		stability=0.000
		is_at_war=yes
		current_power_projection=50.000
		prestige=5.125
		legitimacy=100.000
		powers={
			0 0 0
		}
		powers={
			98 98 42
		}
		human=yes
		flags={
			sample_flag=1444.11.11
		}
		estate={
			type="estate_nobles"
			loyalty=44.600
			territory=40.000
			active_influences={
				4 7
			}
			granted_privileges={
				{
					estate_nobles_habsburg_disunity 1444.11.11
				}
			}
		}
		army={
			name="Main Army"
			location=1
			regiment={
				type="infantry"
			}
		}
		navy={
			name="Main Navy"
			location=1
			ship={
				type="galley"
			}
		}
	}
}
provinces={
	-1={
		owner="AAA"
		culture=saved_province_culture
		religion=saved_province_religion
		trade_goods=cloth
		is_city=yes
		local_autonomy=3.500
		history={
			hre=yes
			advisor={
				type=treasurer
				skill=1
				hire_date=1444.11.11
				id={
					id=7
					type=51
				}
			}
		}
	}
}
"#,
        )
        .unwrap();

        let state = read_vanilla_save_state(&path).unwrap();
        assert_eq!(state.date, "1444.11.11");
        assert_eq!(state.player.as_deref(), Some("AAA"));
        assert!(state.players.contains("AAA"));
        assert_eq!(
            state.external_state.trade_good_prices_milli["grain"],
            Some(3_500)
        );
        assert_eq!(
            state.external_state.trade_good_prices_milli["missing_good"],
            None
        );
        assert_eq!(
            state.external_state.country_resources["AAA"].treasury_milli,
            Some(59_802)
        );
        assert_eq!(
            state.external_state.country_resources["AAA"].manpower_milli,
            Some(9_754)
        );
        assert_eq!(state.external_state.country_capitals["AAA"], Some(1));
        assert_eq!(
            state.external_state.country_technology_groups["AAA"].as_deref(),
            Some("test_group")
        );
        assert_eq!(
            state.external_state.country_unit_types["AAA"].as_deref(),
            Some("saved_unit_type")
        );
        assert_eq!(
            state.external_state.country_government_ranks["AAA"],
            Some(3)
        );
        assert_eq!(
            state.external_state.country_primary_cultures["AAA"].as_deref(),
            Some("saved_country_culture")
        );
        assert_eq!(
            state.external_state.country_religions["AAA"].as_deref(),
            Some("saved_country_religion")
        );
        assert_eq!(
            state.external_state.country_technology_levels["AAA"],
            BTreeMap::from([
                ("administrative".into(), 3),
                ("diplomatic".into(), 4),
                ("military".into(), 5),
            ])
        );
        assert_eq!(
            state.external_state.country_metric_values["AAA"]["adm_power"],
            Some(98)
        );
        assert_eq!(
            state.external_state.country_metric_values["AAA"]["power_projection"],
            Some(50_000)
        );
        assert_eq!(
            state.external_state.country_metric_values["AAA"]["prestige"],
            Some(5_125)
        );
        assert_eq!(
            state.external_state.country_statuses["AAA"]["at_war"],
            Some(true)
        );
        assert_eq!(
            state.external_state.country_national_focus["AAA"],
            NationalFocus {
                skill: Some(RulerSkill::Diplomacy)
            }
        );
        assert_eq!(
            state.external_state.country_ideas["AAA"].groups,
            Some(BTreeMap::from([("test_alpha".into(), Some(2))]))
        );
        assert_eq!(state.external_state.armies.len(), 1);
        assert_eq!(state.external_state.armies[0].owner, "AAA");
        assert_eq!(state.external_state.armies[0].location, 1);
        assert_eq!(state.external_state.armies[0].name, "Main Army");
        assert_eq!(
            state.external_state.armies[0].regiments,
            vec!["infantry".to_owned()]
        );
        assert_eq!(state.external_state.naval_units.len(), 1);
        assert_eq!(state.external_state.naval_units[0].owner, "AAA");
        assert_eq!(state.external_state.naval_units[0].location, 1);
        assert_eq!(state.external_state.naval_units[0].unit_type, "galley");
        assert_eq!(state.external_state.advisors["AAA"].len(), 1);
        assert_eq!(
            state.external_state.advisors["AAA"][0].type_name,
            "treasurer"
        );
        assert_eq!(state.external_state.advisors["AAA"][0].skill, 1);
        assert_eq!(state.external_state.advisors["AAA"][0].id, Some(7));
        assert!(
            state.external_state.country_markers["AAA"]
                .as_ref()
                .unwrap()
                .contains("sample_flag")
        );
        assert_eq!(
            state.external_state.province_raw_autonomy_milli[&1],
            Some(3_500)
        );
        assert_eq!(state.external_state.province_is_city[&1], Some(true));
        assert_eq!(state.external_state.province_in_empire[&1], Some(true));
        assert_eq!(
            state.external_state.province_owners[&1].as_deref(),
            Some("AAA")
        );
        assert_eq!(
            state.external_state.province_cultures[&1].as_deref(),
            Some("saved_province_culture")
        );
        assert_eq!(
            state.external_state.province_religions[&1].as_deref(),
            Some("saved_province_religion")
        );
        assert_eq!(
            state.external_state.province_trade_goods[&1].as_deref(),
            Some("cloth")
        );
        assert_eq!(
            state.external_state.estate_states["AAA"]["estate_nobles"].loyalty_milli,
            Some(44_600)
        );
        assert_eq!(
            state.external_state.estate_states["AAA"]["estate_nobles"].land_share_micros,
            Some(400_000)
        );
        assert_eq!(
            state.external_state.estate_states["AAA"]["estate_nobles"].active_influences,
            Some(std::collections::BTreeSet::from([4, 7]))
        );
        assert_eq!(
            state.external_state.estate_privileges["AAA"]["estate_nobles"]
                .as_ref()
                .unwrap(),
            &std::collections::BTreeSet::from(["estate_nobles_habsburg_disunity".into()])
        );
    }

    #[test]
    fn imports_saved_province_development_and_controller_state() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("development.eu4");
        std::fs::write(
            &path,
            r#"EU4txt
date=1444.11.11
player="AAA"
countries={
    AAA={ human=yes }
}
provinces={
    -1={
        owner="AAA"
        controller="AAA"
        base_tax=12.345
        base_production=7.890
        base_manpower=4.250
    }
    -2={
        owner="AAA"
    }
    -999={
        base_tax=99.000
        base_production=88.000
        base_manpower=77.000
    }
}
"#,
        )
        .unwrap();

        let save = read_vanilla_save_state(&path).unwrap();
        assert_eq!(
            save.external_state.province_development[&1].base_tax_milli,
            Some(12_345)
        );
        assert_eq!(
            save.external_state.province_development[&1].base_production_milli,
            Some(7_890)
        );
        assert_eq!(
            save.external_state.province_development[&1].base_manpower_milli,
            Some(4_250)
        );
        assert!(save.external_state.province_development.contains_key(&999));
        assert_eq!(
            save.external_state.province_controllers[&1].as_deref(),
            Some("AAA")
        );
        assert!(!save.external_state.province_controllers.contains_key(&2));

        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world");
        let content = ContentSet::resolve(vec![ContentSource::base(
            ContentRoot::open(fixture).unwrap(),
        )])
        .unwrap();
        let baseline = crate::VanillaWorld::load_from_content(&content, "1444.11.11").unwrap();
        let baseline_tax = baseline.initial.province(1).unwrap().tax_milli;
        assert_ne!(baseline_tax, Some(12_345));
        assert_eq!(
            baseline.initial.province(1).unwrap().controller.as_deref(),
            Some("BBB")
        );
        assert_eq!(
            baseline.initial.province(2).unwrap().controller.as_deref(),
            Some("AAA")
        );
        let (_, filtered) = save.for_world(&baseline).unwrap();
        assert_eq!(filtered.province_controllers[&1].as_deref(), Some("AAA"));
        assert!(filtered.province_controllers.contains_key(&2));
        assert_eq!(filtered.province_controllers[&2], None);

        let loaded =
            crate::VanillaWorld::load_from_content_with_save_state(&content, &save).unwrap();
        let province = loaded.initial.province(1).unwrap();
        assert_eq!(province.tax_milli, Some(12_345));
        assert_eq!(province.production_milli, Some(7_890));
        assert_eq!(province.manpower_milli, Some(4_250));
        assert_eq!(province.controller.as_deref(), Some("AAA"));
        assert_eq!(loaded.initial.province(2).unwrap().controller, None);
        assert!(loaded.initial.province(999).is_none());

        let unknown_controller_path = directory.path().join("unknown-controller.eu4");
        std::fs::write(
            &unknown_controller_path,
            r#"EU4txt
date=1444.11.11
player="AAA"
countries={
    AAA={ human=yes }
}
provinces={
    -2={
        owner="AAA"
        controller="ZZZ"
    }
}
"#,
        )
        .unwrap();

        let unknown_controller_save = read_vanilla_save_state(&unknown_controller_path).unwrap();
        assert_eq!(
            unknown_controller_save.external_state.province_controllers[&2].as_deref(),
            Some("ZZZ")
        );
        let (_, filtered) = unknown_controller_save.for_world(&baseline).unwrap();
        assert_eq!(filtered.province_controllers[&2], None);
        let loaded = crate::VanillaWorld::load_from_content_with_save_state(
            &content,
            &unknown_controller_save,
        )
        .unwrap();
        assert_eq!(loaded.initial.province(2).unwrap().controller, None);
    }

    #[test]
    fn saved_capitals_override_clear_and_filter_unknown_provinces() {
        let directory = tempfile::tempdir().unwrap();
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world");
        let content = ContentSet::resolve(vec![ContentSource::base(
            ContentRoot::open(fixture).unwrap(),
        )])
        .unwrap();
        let baseline = crate::VanillaWorld::load_from_content(&content, "1444.11.11").unwrap();
        assert_eq!(baseline.initial.bound_capital("AAA"), Some(2));

        for (name, saved_capital, parsed_capital, expected_capital) in [
            ("changed", 1, Some(1), Some(1)),
            ("cleared", 0, None, None),
            ("unknown", 999, Some(999), Some(2)),
        ] {
            let path = directory.path().join(format!("{name}.eu4"));
            std::fs::write(
                &path,
                format!(
                    "EU4txt\ndate=1444.11.11\nplayer=\"AAA\"\ncountries={{\n    AAA={{ capital={saved_capital} human=yes }}\n}}\nprovinces={{\n    -1={{ owner=\"AAA\" }}\n}}\n"
                ),
            )
            .unwrap();

            let save = read_vanilla_save_state(&path).unwrap();
            assert_eq!(save.external_state.country_capitals["AAA"], parsed_capital);
            let filtered = save.state_for_world(&baseline).unwrap();
            if saved_capital == 999 {
                assert!(!filtered.country_capitals.contains_key("AAA"));
            } else {
                assert_eq!(filtered.country_capitals.get("AAA"), Some(&parsed_capital));
            }

            let loaded =
                crate::VanillaWorld::load_from_content_with_save_state(&content, &save).unwrap();
            assert_eq!(
                loaded.initial.bound_capital("AAA"),
                expected_capital,
                "unexpected bound capital for {name} save"
            );
        }
    }

    #[test]
    fn saved_technology_levels_override_content_start_levels() {
        let directory = tempfile::tempdir().unwrap();
        let save_path = directory.path().join("technology.eu4");
        std::fs::write(
            &save_path,
            r#"EU4txt
date=1444.11.11
countries={
    AAA={
        technology={
            adm_tech=1
            dip_tech=2
            mil_tech=3
        }
        human=yes
    }
}
provinces={}
"#,
        )
        .unwrap();

        let save = read_vanilla_save_state(&save_path).unwrap();
        assert_eq!(
            save.external_state.country_technology_levels["AAA"],
            BTreeMap::from([
                ("administrative".into(), 1),
                ("diplomatic".into(), 2),
                ("military".into(), 3),
            ])
        );

        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world");
        let content = ContentSet::resolve(vec![ContentSource::base(
            ContentRoot::open(fixture).unwrap(),
        )])
        .unwrap();
        let baseline = crate::VanillaWorld::load_from_content(&content, "1444.11.11").unwrap();
        let filtered = save.state_for_world(&baseline).unwrap();
        assert_eq!(
            filtered.country_technology_levels["AAA"],
            save.external_state.country_technology_levels["AAA"]
        );

        let mut invalid = save.clone();
        invalid
            .external_state
            .country_technology_levels
            .get_mut("AAA")
            .unwrap()
            .insert("administrative".into(), u16::MAX);
        assert!(invalid.state_for_world(&baseline).is_err());

        let loaded =
            crate::VanillaWorld::load_from_content_with_save_state(&content, &save).unwrap();
        let country = loaded.initial.country_entity("AAA").unwrap();
        let technology = loaded.initial.world().technology();
        assert_eq!(technology.country_level(country, "administrative"), Some(1));
        assert_eq!(technology.country_level(country, "diplomatic"), Some(2));
        assert_eq!(technology.country_level(country, "military"), Some(3));
    }

    #[test]
    fn saved_country_technology_group_and_unit_type_override_content_state() {
        let directory = tempfile::tempdir().unwrap();
        let save_path = directory.path().join("country-types.eu4");
        std::fs::write(
            &save_path,
            r#"EU4txt
date=1444.11.11
countries={
    AAA={
        technology_group=test_group
        unit_type=saved_unit_type
        human=yes
    }
}
provinces={}
"#,
        )
        .unwrap();

        let save = read_vanilla_save_state(&save_path).unwrap();
        assert_eq!(
            save.external_state.country_technology_groups["AAA"].as_deref(),
            Some("test_group")
        );
        assert_eq!(
            save.external_state.country_unit_types["AAA"].as_deref(),
            Some("saved_unit_type")
        );

        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world");
        let content = ContentSet::resolve(vec![ContentSource::base(
            ContentRoot::open(fixture).unwrap(),
        )])
        .unwrap();
        let loaded =
            crate::VanillaWorld::load_from_content_with_save_state(&content, &save).unwrap();
        let country = loaded.initial.country("AAA").unwrap();
        assert_eq!(country.technology_group.as_deref(), Some("test_group"));
        assert_eq!(country.unit_type.as_deref(), Some("saved_unit_type"));

        let mut invalid = save.clone();
        invalid
            .external_state
            .country_technology_groups
            .insert("AAA".into(), Some("missing_group".into()));
        assert!(
            crate::VanillaWorld::load_from_content_with_save_state(&content, &invalid).is_err()
        );
    }

    #[test]
    fn saved_government_state_overrides_history_and_validates_rank() {
        let directory = tempfile::tempdir().unwrap();
        let save_path = directory.path().join("government-rank.eu4");
        std::fs::write(
            &save_path,
            r#"EU4txt
date=1444.11.11
countries={
    AAA={
        government_rank=3
        legitimacy=82.750
        republican_tradition=63.125
        government={
            government="republic"
            reform_stack={
                reforms={
                    "test_council"
                    "test_charter"
                }
                history={
                    "test_court"
                }
            }
            mechanic_type={
                mechanic_type=test_mechanic
                test_power={
                    power=1.744
                    frozen=no
                }
                reserved_power={
                    power=0.250
                    frozen=yes
                }
            }
        }
        human=yes
    }
}
provinces={}
"#,
        )
        .unwrap();

        let save = read_vanilla_save_state(&save_path).unwrap();
        assert_eq!(save.external_state.country_government_ranks["AAA"], Some(3));
        assert_eq!(
            save.external_state.country_government_kinds["AAA"].as_deref(),
            Some("republic")
        );
        assert_eq!(
            save.external_state.country_government_reforms["AAA"],
            Some(BTreeSet::from([
                "test_charter".to_owned(),
                "test_council".to_owned()
            ]))
        );
        assert_eq!(
            save.external_state.country_government_mechanic_powers["AAA"]["test_mechanic"]["test_power"],
            augustus_core::GovernmentMechanicResourceState {
                power_milli: Some(1_744),
                frozen: Some(false),
            }
        );
        assert_eq!(
            save.external_state.country_government_mechanic_powers["AAA"]["test_mechanic"]["reserved_power"],
            augustus_core::GovernmentMechanicResourceState {
                power_milli: Some(250),
                frozen: Some(true),
            }
        );

        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world");
        let content = ContentSet::resolve(vec![ContentSource::base(
            ContentRoot::open(fixture).unwrap(),
        )])
        .unwrap();
        let loaded =
            crate::VanillaWorld::load_from_content_with_save_state(&content, &save).unwrap();
        let country = loaded.initial.country_entity("AAA").unwrap();
        assert_eq!(loaded.initial.world().government_rank(country).unwrap(), 3);
        let government = &loaded.initial.world().governments().countries[&country];
        assert_eq!(government.kind.as_deref(), Some("republic"));
        assert_eq!(
            government.reforms,
            Some(BTreeSet::from([
                "test_charter".to_owned(),
                "test_council".to_owned()
            ]))
        );
        assert_eq!(
            government.metrics,
            Some(BTreeMap::from([(
                "republican_tradition".into(),
                Some(63_125)
            )]))
        );
        assert_eq!(government.inactive_metrics["legitimacy"], Some(82_750));
        assert_eq!(
            government.mechanic_powers["test_mechanic"]["test_power"],
            augustus_core::GovernmentMechanicResourceState {
                power_milli: Some(1_744),
                frozen: Some(false),
            }
        );

        let snapshot = loaded
            .initial
            .world()
            .save("saved-government-mechanics")
            .unwrap();
        let restored = augustus_core::World::restore(
            &snapshot,
            "saved-government-mechanics",
            loaded.initial.world().schema(),
        )
        .unwrap();
        assert_eq!(
            restored.governments().countries[&country].mechanic_powers["test_mechanic"]["test_power"],
            augustus_core::GovernmentMechanicResourceState {
                power_milli: Some(1_744),
                frozen: Some(false),
            }
        );

        let mut unsupported = save.clone();
        unsupported
            .external_state
            .country_government_reforms
            .insert("AAA".into(), Some(["missing_reform".into()].into()));
        let filtered = unsupported.state_for_world(&loaded).unwrap();
        assert_eq!(filtered.country_government_reforms["AAA"], None);
        let unknown_loaded = crate::VanillaWorld::load_from_content_with_external_state(
            &content,
            &unsupported.date,
            &filtered,
        )
        .unwrap();
        let country = unknown_loaded.initial.country_entity("AAA").unwrap();
        assert_eq!(
            unknown_loaded.initial.world().governments().countries[&country].reforms,
            None
        );

        let mut invalid = save.clone();
        invalid
            .external_state
            .country_government_ranks
            .insert("AAA".into(), Some(4));
        assert!(
            crate::VanillaWorld::load_from_content_with_save_state(&content, &invalid).is_err()
        );
    }

    #[test]
    fn saved_country_and_province_culture_religion_override_content_state() {
        let directory = tempfile::tempdir().unwrap();
        let save_path = directory.path().join("identity.eu4");
        std::fs::write(
            &save_path,
            r#"EU4txt
date=1444.11.11
countries={
    AAA={
        primary_culture=saved_country_culture
        religion=saved_country_religion
        human=yes
    }
}
provinces={
    -1={
        owner="AAA"
        culture=saved_province_culture
        religion=saved_province_religion
    }
    -999={
        culture=missing_culture
        religion=missing_religion
    }
}
"#,
        )
        .unwrap();

        let save = read_vanilla_save_state(&save_path).unwrap();
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world");
        let content = ContentSet::resolve(vec![ContentSource::base(
            ContentRoot::open(fixture).unwrap(),
        )])
        .unwrap();
        let baseline = crate::VanillaWorld::load_from_content(&content, "1444.11.11").unwrap();
        let filtered = save.state_for_world(&baseline).unwrap();
        assert!(filtered.province_cultures.contains_key(&1));
        assert!(filtered.province_religions.contains_key(&1));
        assert!(!filtered.province_cultures.contains_key(&999));
        assert!(!filtered.province_religions.contains_key(&999));

        let loaded =
            crate::VanillaWorld::load_from_content_with_save_state(&content, &save).unwrap();
        let country = loaded.initial.country("AAA").unwrap();
        assert_eq!(
            country.primary_culture.as_deref(),
            Some("saved_country_culture")
        );
        assert_eq!(country.religion.as_deref(), Some("saved_country_religion"));
        let province = loaded.initial.province(1).unwrap();
        assert_eq!(province.culture.as_deref(), Some("saved_province_culture"));
        assert_eq!(
            province.religion.as_deref(),
            Some("saved_province_religion")
        );
        assert!(loaded.initial.province(999).is_none());
    }

    #[test]
    fn saved_trade_goods_override_content_and_filter_unknown_provinces() {
        let directory = tempfile::tempdir().unwrap();
        let save_path = directory.path().join("trade-goods.eu4");
        std::fs::write(
            &save_path,
            r#"EU4txt
date=1444.11.11
countries={
    AAA={ human=yes }
}
provinces={
    -1={
        owner="AAA"
        trade_goods=cloth
    }
    -999={
        trade_goods=missing_good
    }
}
"#,
        )
        .unwrap();

        let save = read_vanilla_save_state(&save_path).unwrap();
        assert_eq!(
            save.external_state.province_trade_goods[&1].as_deref(),
            Some("cloth")
        );
        assert_eq!(
            save.external_state.province_trade_goods[&999].as_deref(),
            Some("missing_good")
        );

        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world");
        let content = ContentSet::resolve(vec![ContentSource::base(
            ContentRoot::open(fixture).unwrap(),
        )])
        .unwrap();
        let baseline = crate::VanillaWorld::load_from_content(&content, "1444.11.11").unwrap();
        let filtered = save.state_for_world(&baseline).unwrap();
        assert_eq!(filtered.province_trade_goods[&1].as_deref(), Some("cloth"));
        assert!(!filtered.province_trade_goods.contains_key(&999));

        let mut invalid = save.clone();
        invalid
            .external_state
            .province_trade_goods
            .insert(1, Some("missing_good".into()));
        assert!(invalid.state_for_world(&baseline).is_err());

        let loaded =
            crate::VanillaWorld::load_from_content_with_save_state(&content, &save).unwrap();
        let province = loaded.initial.province(1).unwrap();
        assert_eq!(province.trade_good.as_deref(), Some("cloth"));
        let entity = loaded.initial.province_entity(1).unwrap();
        assert_eq!(
            loaded.initial.world().economy().provinces[&entity]
                .trade_good
                .as_deref(),
            Some("cloth")
        );
    }

    #[test]
    fn external_province_development_overlay_merges_by_component() {
        let mut base = crate::VanillaExternalState {
            province_development: BTreeMap::from([(
                1,
                VanillaExternalProvinceDevelopment {
                    base_tax_milli: Some(1_000),
                    base_production_milli: Some(2_000),
                    base_manpower_milli: Some(3_000),
                },
            )]),
            ..Default::default()
        };
        let overlay = crate::VanillaExternalState {
            province_development: BTreeMap::from([(
                1,
                VanillaExternalProvinceDevelopment {
                    base_tax_milli: None,
                    base_production_milli: Some(8_000),
                    base_manpower_milli: None,
                },
            )]),
            ..Default::default()
        };

        base.apply_overlay(&overlay);

        assert_eq!(
            base.province_development[&1],
            VanillaExternalProvinceDevelopment {
                base_tax_milli: Some(1_000),
                base_production_milli: Some(8_000),
                base_manpower_milli: Some(3_000),
            }
        );
    }

    #[test]
    fn rejects_negative_external_province_development() {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world");
        let content = ContentSet::resolve(vec![ContentSource::base(
            ContentRoot::open(fixture).unwrap(),
        )])
        .unwrap();
        let state = crate::VanillaExternalState {
            province_development: BTreeMap::from([(
                1,
                VanillaExternalProvinceDevelopment {
                    base_tax_milli: Some(-1),
                    ..Default::default()
                },
            )]),
            ..Default::default()
        };

        let error = crate::VanillaWorld::load_from_content_with_external_state(
            &content,
            "1444.11.11",
            &state,
        )
        .unwrap_err();

        assert!(error.to_string().contains("negative saved base_tax"));
    }

    #[test]
    fn reads_startup_state_from_zip_wrapped_text_save() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("state.eu4");
        let mut archive = ZipWriter::new(File::create(&path).unwrap());
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        archive.start_file("meta", options).unwrap();
        archive
            .write_all(b"EU4txt\ndate=1444.11.11\nplayer=\"AAA\"\n")
            .unwrap();
        archive.start_file("gamestate", options).unwrap();
        archive
            .write_all(
                br#"EU4txt
start_date=1444.11.11
countries={
    AAA={
        treasury=59.802
        human=yes
    }
}
provinces={
    -1={
        local_autonomy=3.500
    }
}
"#,
            )
            .unwrap();
        archive.start_file("ai", options).unwrap();
        archive.write_all(b"EU4txt\nai={}\n").unwrap();
        archive.finish().unwrap();

        let state = read_vanilla_save_state(&path).unwrap();

        assert_eq!(state.date, "1444.11.11");
        assert_eq!(state.player.as_deref(), Some("AAA"));
        assert_eq!(
            state.external_state.country_resources["AAA"].treasury_milli,
            Some(59_802)
        );
        assert_eq!(
            state.external_state.province_raw_autonomy_milli[&1],
            Some(3_500)
        );
        assert_eq!(state.external_state.province_owners[&1], None);
    }

    #[test]
    fn rejects_zip_save_without_gamestate_entry() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("state.eu4");
        let mut archive = ZipWriter::new(File::create(&path).unwrap());
        archive
            .start_file("meta", SimpleFileOptions::default())
            .unwrap();
        archive.write_all(b"EU4txt\ndate=1444.11.11\n").unwrap();
        archive.finish().unwrap();

        assert!(read_vanilla_save_state(&path).is_err());
    }
}
