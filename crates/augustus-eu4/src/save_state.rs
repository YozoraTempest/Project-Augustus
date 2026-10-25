// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

//! Translate the state portion of a player-provided EU4 save into the external
//! compatibility boundary. The simulation core never reads EU4 save syntax.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use augustus_core::{CountryRelationState, CountryResources, IdeaState, NationalFocus, RulerSkill};
use jomini::{
    TextToken,
    binary::TokenResolver,
    common::Date,
    text::{Operator, ValueReader},
};
use serde::Serialize;

use crate::administration_validation::{
    SaveTokenResolver, read_eu4_save_documents, read_state_edicts,
};
use crate::bootstrap::{
    VanillaExternalAdvisor, VanillaExternalArmy, VanillaExternalEstateState,
    VanillaExternalEu4RandomState, VanillaExternalFleet, VanillaExternalNavalUnit,
    VanillaExternalPerson, VanillaExternalProvinceDevelopment, VanillaExternalRegimentState,
    VanillaExternalShipState, VanillaExternalState, VanillaWorld,
};
use crate::state_edicts::VanillaExternalStateEdicts;
use crate::{Error, ParsedText, TextEncoding};

type Object<'a, 'b> = jomini::text::ObjectReader<'a, 'b, TextEncoding>;
type Field<'a, 'b> = ValueReader<'a, 'b, TextEncoding>;
type SavedPersonId = (String, String);

/// State translated from a player-provided EU4 save.
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

/// Save state plus active state edicts, kept separate to preserve existing
/// `VanillaSaveState` and `VanillaExternalState` construction APIs.
#[derive(Debug, Clone, Serialize)]
pub struct VanillaSaveStateWithStateEdicts {
    pub save: VanillaSaveState,
    pub state_edicts: VanillaExternalStateEdicts,
}

/// Supported save state plus runtime trade-node current values.
///
/// Kept separate from VanillaSaveState so existing save-state struct
/// construction remains source-compatible.
#[derive(Debug, Clone, Serialize)]
pub struct VanillaSaveStateWithTradeNodes {
    pub state: VanillaSaveState,
    pub current_trade_node_values_milli: BTreeMap<String, Option<i64>>,
}

impl VanillaSaveStateWithTradeNodes {
    /// Keep trade-node values whose definitions exist in the loaded content.
    pub fn trade_node_current_values_for_world(
        &self,
        world: &VanillaWorld,
    ) -> BTreeMap<String, Option<i64>> {
        let nodes = &world.initial.world().economy().trade_nodes;
        self.current_trade_node_values_milli
            .iter()
            .filter(|(name, _)| nodes.contains_key(name.as_str()))
            .map(|(name, value)| (name.clone(), *value))
            .collect()
    }
}

/// Supported save state plus all currently translated trade-node runtime fields.
#[derive(Debug, Clone, Serialize)]
pub struct VanillaSaveStateWithTradeNodeRuntimeStates {
    pub state: VanillaSaveState,
    pub current_trade_node_values_milli: BTreeMap<String, Option<i64>>,
    pub trade_node_runtime_states: BTreeMap<String, augustus_core::TradeNodeRuntimeState>,
}

impl VanillaSaveStateWithTradeNodeRuntimeStates {
    /// Keep trade-node values whose definitions exist in the loaded content.
    pub fn trade_node_current_values_for_world(
        &self,
        world: &VanillaWorld,
    ) -> BTreeMap<String, Option<i64>> {
        let nodes = &world.initial.world().economy().trade_nodes;
        self.current_trade_node_values_milli
            .iter()
            .filter(|(name, _)| nodes.contains_key(name.as_str()))
            .map(|(name, value)| (name.clone(), *value))
            .collect()
    }

    /// Keep saved runtime fields whose node definitions exist in the content.
    pub fn trade_node_runtime_states_for_world(
        &self,
        world: &VanillaWorld,
    ) -> BTreeMap<String, augustus_core::TradeNodeRuntimeState> {
        let nodes = &world.initial.world().economy().trade_nodes;
        self.trade_node_runtime_states
            .iter()
            .filter(|(name, _)| nodes.contains_key(name.as_str()))
            .map(|(name, state)| (name.clone(), state.clone()))
            .collect()
    }
}

/// Supported save state with trade nodes in their serialized order.
///
/// This preserves save sequence without claiming it is the runtime scheduler
/// order for every EU4 version or ruleset.
#[derive(Debug, Clone, Serialize)]
pub struct VanillaSaveStateWithOrderedTradeNodeRuntimeStates {
    pub save: VanillaSaveStateWithTradeNodeRuntimeStates,
    pub trade_node_order: Vec<String>,
}

impl VanillaSaveStateWithOrderedTradeNodeRuntimeStates {
    /// Returns saved node names in serialized order, filtered to loaded content.
    pub fn trade_node_order_for_world(&self, world: &VanillaWorld) -> Vec<String> {
        let nodes = &world.initial.world().economy().trade_nodes;
        self.trade_node_order
            .iter()
            .filter(|name| nodes.contains_key(*name))
            .cloned()
            .collect()
    }
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
        let mut state = VanillaExternalState {
            eu4_random_state: self.external_state.eu4_random_state,
            ..Default::default()
        };
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
            if let Some(relations) = self.external_state.country_relation_states.get(*country) {
                let filtered = relations
                    .iter()
                    .filter(|(target, _)| countries.contains(target.as_str()))
                    .map(|(target, values)| (target.clone(), values.clone()))
                    .collect::<BTreeMap<_, _>>();
                if !filtered.is_empty() {
                    state
                        .country_relation_states
                        .insert((*country).into(), filtered);
                }
            }
            if let Some(focus) = self.external_state.country_national_focus.get(*country) {
                state
                    .country_national_focus
                    .insert((*country).into(), *focus);
            }
            if let Some(persons) = self.external_state.country_persons.get(*country) {
                let mut filtered = BTreeMap::new();
                for (role, person) in persons {
                    if !matches!(role.as_str(), "ruler" | "heir" | "consort") {
                        return Err(Error::message(
                            "save state",
                            format!(
                                "country {country} has an unsupported saved person role {role}"
                            ),
                        ));
                    }
                    if let Some(person) = person
                        && let Some(origin) = &person.country_of_origin
                        && !countries.contains(origin.as_str())
                    {
                        return Err(Error::message(
                            "save state",
                            format!(
                                "country {country} {role} references unknown origin country {origin}"
                            ),
                        ));
                    }
                    filtered.insert(role.clone(), person.clone());
                }
                if !filtered.is_empty() {
                    state.country_persons.insert((*country).into(), filtered);
                }
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
            if let Some(completed) = self
                .external_state
                .country_mission_completions
                .get(*country)
            {
                let identifiers = &world.initial.world().completions().identifiers;
                state.country_mission_completions.insert(
                    (*country).into(),
                    completed.intersection(identifiers).cloned().collect(),
                );
            }
            if let Some(variables) = self.external_state.country_variables.get(*country) {
                state
                    .country_variables
                    .insert((*country).into(), variables.clone());
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

        for (office, holder) in &self.external_state.political_office_holders {
            if office.is_empty() {
                return Err(Error::message(
                    "save state",
                    "saved political office is empty",
                ));
            }
            if let Some(holder) = holder
                && !countries.contains(holder.as_str())
            {
                return Err(Error::message(
                    "save state",
                    format!("office {office} references unknown saved holder {holder}"),
                ));
            }
            state
                .political_office_holders
                .insert(office.clone(), holder.clone());
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

        if self.external_state.naval_fleets.is_empty() {
            for naval_unit in &self.external_state.naval_units {
                if countries.contains(naval_unit.owner.as_str())
                    && world.initial.province_entity(naval_unit.location).is_some()
                {
                    state.naval_units.push(naval_unit.clone());
                }
            }
        } else {
            for fleet in &self.external_state.naval_fleets {
                if !countries.contains(fleet.owner.as_str())
                    || world.initial.province_entity(fleet.location).is_none()
                {
                    continue;
                }
                state.naval_fleets.push(fleet.clone());
                state
                    .naval_units
                    .extend(
                        fleet
                            .ship_types
                            .iter()
                            .enumerate()
                            .map(|(index, unit_type)| VanillaExternalNavalUnit {
                                owner: fleet.owner.clone(),
                                location: fleet.location,
                                unit_type: unit_type.clone(),
                                state: fleet.ship_states.get(index).cloned(),
                            }),
                    );
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

        for (province, values) in &self.external_state.province_metric_values {
            if world.initial.province_entity(*province).is_none() {
                continue;
            }
            let filtered = values
                .iter()
                .filter(|(name, _)| {
                    definitions.get(name.as_str()).is_some_and(|definition| {
                        definition.kind == augustus_core::EntityKind::Province
                    })
                })
                .map(|(name, value)| (name.clone(), *value))
                .collect::<BTreeMap<_, _>>();
            if !filtered.is_empty() {
                state.province_metric_values.insert(*province, filtered);
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

        for (province, buildings) in &self.external_state.province_buildings {
            if world.initial.province_entity(*province).is_none() {
                continue;
            }
            if let Some(building) = buildings.iter().find(|name| {
                !world
                    .initial
                    .world()
                    .economy()
                    .buildings
                    .contains_key(*name)
            }) {
                return Err(Error::message(
                    "save state",
                    format!("province {province} references unknown saved building {building}"),
                ));
            }
            state
                .province_buildings
                .insert(*province, buildings.clone());
        }

        for (province, cores) in &self.external_state.province_cores {
            if world.initial.province_entity(*province).is_none() {
                continue;
            }
            let mut seen = BTreeSet::new();
            for tag in cores {
                if !countries.contains(tag.as_str()) {
                    return Err(Error::message(
                        "save state",
                        format!("province {province} references unknown core country {tag}"),
                    ));
                }
                if !seen.insert(tag) {
                    return Err(Error::message(
                        "save state",
                        format!("province {province} has a duplicate saved core country {tag}"),
                    ));
                }
            }
            state.province_cores.insert(*province, cores.clone());
        }

        for (province, discovered_by) in &self.external_state.province_discovered_by {
            if world.initial.province_entity(*province).is_some() {
                state
                    .province_discovered_by
                    .insert(*province, discovered_by.clone());
            }
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

        for (province, level) in &self.external_state.province_center_of_trade_levels {
            if world.initial.province_entity(*province).is_some() {
                state
                    .province_center_of_trade_levels
                    .insert(*province, *level);
            }
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

        for (province, value) in &self.external_state.province_mothball_status {
            if world.initial.province_entity(*province).is_some() {
                state.province_mothball_status.insert(*province, *value);
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
    read_vanilla_save_state_inner_thread(path.as_ref(), None, None, false, false)
        .map(|(save, _)| save.save.state)
}

/// Read supported state and active state edicts from an EU4 save.
pub fn read_vanilla_save_state_with_state_edicts(
    path: impl AsRef<Path>,
) -> Result<VanillaSaveStateWithStateEdicts, Error> {
    read_vanilla_save_state_inner_thread(path.as_ref(), None, None, false, true).map(
        |(save, state_edicts)| VanillaSaveStateWithStateEdicts {
            save: save.save.state,
            state_edicts,
        },
    )
}

/// Read supported state and current trade-node values from an EU4 save.
pub fn read_vanilla_save_state_and_trade_nodes(
    path: impl AsRef<Path>,
) -> Result<VanillaSaveStateWithTradeNodes, Error> {
    read_vanilla_save_state_and_trade_node_runtime_states(path).map(|save| {
        VanillaSaveStateWithTradeNodes {
            state: save.state,
            current_trade_node_values_milli: save.current_trade_node_values_milli,
        }
    })
}

/// Read supported state and the supported runtime fields for each saved node.
pub fn read_vanilla_save_state_and_trade_node_runtime_states(
    path: impl AsRef<Path>,
) -> Result<VanillaSaveStateWithTradeNodeRuntimeStates, Error> {
    read_vanilla_save_state_inner_thread(path.as_ref(), None, None, true, false)
        .map(|(save, _)| save.save)
}

/// Read supported state and trade nodes in their serialized save order.
pub fn read_vanilla_save_state_and_ordered_trade_node_runtime_states(
    path: impl AsRef<Path>,
) -> Result<VanillaSaveStateWithOrderedTradeNodeRuntimeStates, Error> {
    read_vanilla_save_state_inner_thread(path.as_ref(), None, None, true, false)
        .map(|(save, _)| save)
}

/// Read supported state from an EU4 save using a player-provided binary token map when needed.
///
/// Each token map line contains a hexadecimal token and its field name, for example
/// `0x1234 date`; optional string lookup lines use `lookup 7 value`. Augustus does not
/// distribute EU4 token maps.
pub fn read_vanilla_save_state_with_token_map(
    path: impl AsRef<Path>,
    token_map_path: impl AsRef<Path>,
) -> Result<VanillaSaveState, Error> {
    read_vanilla_save_state_inner_thread(
        path.as_ref(),
        Some(token_map_path.as_ref()),
        None,
        false,
        false,
    )
    .map(|(save, _)| save.save.state)
}

/// Read supported state and active state edicts using a player-provided token map.
pub fn read_vanilla_save_state_with_state_edicts_with_token_map(
    path: impl AsRef<Path>,
    token_map_path: impl AsRef<Path>,
) -> Result<VanillaSaveStateWithStateEdicts, Error> {
    read_vanilla_save_state_inner_thread(
        path.as_ref(),
        Some(token_map_path.as_ref()),
        None,
        false,
        true,
    )
    .map(|(save, state_edicts)| VanillaSaveStateWithStateEdicts {
        save: save.save.state,
        state_edicts,
    })
}

/// Read supported state and current trade-node values using a player-provided binary token map.
pub fn read_vanilla_save_state_and_trade_nodes_with_token_map(
    path: impl AsRef<Path>,
    token_map_path: impl AsRef<Path>,
) -> Result<VanillaSaveStateWithTradeNodes, Error> {
    read_vanilla_save_state_and_trade_node_runtime_states_with_token_map(path, token_map_path).map(
        |save| VanillaSaveStateWithTradeNodes {
            state: save.state,
            current_trade_node_values_milli: save.current_trade_node_values_milli,
        },
    )
}

pub fn read_vanilla_save_state_and_trade_node_runtime_states_with_token_map(
    path: impl AsRef<Path>,
    token_map_path: impl AsRef<Path>,
) -> Result<VanillaSaveStateWithTradeNodeRuntimeStates, Error> {
    read_vanilla_save_state_inner_thread(
        path.as_ref(),
        Some(token_map_path.as_ref()),
        None,
        true,
        false,
    )
    .map(|(save, _)| save.save)
}

/// Read saved trade-node order using a player-provided binary token map.
pub fn read_vanilla_save_state_and_ordered_trade_node_runtime_states_with_token_map(
    path: impl AsRef<Path>,
    token_map_path: impl AsRef<Path>,
) -> Result<VanillaSaveStateWithOrderedTradeNodeRuntimeStates, Error> {
    read_vanilla_save_state_inner_thread(
        path.as_ref(),
        Some(token_map_path.as_ref()),
        None,
        true,
        false,
    )
    .map(|(save, _)| save)
}

/// Read supported state using an integration-provided token resolver.
///
/// Custom resolvers can supply both binary identifiers and string lookup values without
/// adding EU4 token data to Augustus. The resolver trait is re-exported as
/// [`crate::Eu4TokenResolver`].
pub fn read_vanilla_save_state_with_token_resolver<R>(
    path: impl AsRef<Path>,
    resolver: R,
) -> Result<VanillaSaveState, Error>
where
    R: TokenResolver + Send + 'static,
{
    read_vanilla_save_state_inner_thread(
        path.as_ref(),
        None,
        Some(Box::new(resolver)),
        false,
        false,
    )
    .map(|(save, _)| save.save.state)
}

/// Read supported state and active state edicts using an integration-provided token resolver.
pub fn read_vanilla_save_state_with_state_edicts_with_token_resolver<R>(
    path: impl AsRef<Path>,
    resolver: R,
) -> Result<VanillaSaveStateWithStateEdicts, Error>
where
    R: TokenResolver + Send + 'static,
{
    read_vanilla_save_state_inner_thread(path.as_ref(), None, Some(Box::new(resolver)), false, true)
        .map(|(save, state_edicts)| VanillaSaveStateWithStateEdicts {
            save: save.save.state,
            state_edicts,
        })
}

/// Read supported state and current trade-node values using an integration-provided token resolver.
pub fn read_vanilla_save_state_and_trade_nodes_with_token_resolver<R>(
    path: impl AsRef<Path>,
    resolver: R,
) -> Result<VanillaSaveStateWithTradeNodes, Error>
where
    R: TokenResolver + Send + 'static,
{
    read_vanilla_save_state_and_trade_node_runtime_states_with_token_resolver(path, resolver).map(
        |save| VanillaSaveStateWithTradeNodes {
            state: save.state,
            current_trade_node_values_milli: save.current_trade_node_values_milli,
        },
    )
}

pub fn read_vanilla_save_state_and_trade_node_runtime_states_with_token_resolver<R>(
    path: impl AsRef<Path>,
    resolver: R,
) -> Result<VanillaSaveStateWithTradeNodeRuntimeStates, Error>
where
    R: TokenResolver + Send + 'static,
{
    read_vanilla_save_state_inner_thread(path.as_ref(), None, Some(Box::new(resolver)), true, false)
        .map(|(save, _)| save.save)
}

/// Read saved trade-node order using an integration-provided token resolver.
pub fn read_vanilla_save_state_and_ordered_trade_node_runtime_states_with_token_resolver<R>(
    path: impl AsRef<Path>,
    resolver: R,
) -> Result<VanillaSaveStateWithOrderedTradeNodeRuntimeStates, Error>
where
    R: TokenResolver + Send + 'static,
{
    read_vanilla_save_state_inner_thread(path.as_ref(), None, Some(Box::new(resolver)), true, false)
        .map(|(save, _)| save)
}

fn read_vanilla_save_state_inner_thread(
    path: &Path,
    token_map_path: Option<&Path>,
    token_resolver: Option<SaveTokenResolver>,
    capture_trade_nodes: bool,
    capture_state_edicts: bool,
) -> Result<
    (
        VanillaSaveStateWithOrderedTradeNodeRuntimeStates,
        VanillaExternalStateEdicts,
    ),
    Error,
> {
    let path = path.to_path_buf();
    let token_map_path = token_map_path.map(Path::to_path_buf);
    let parser_path = path.clone();
    let parser_token_map_path = token_map_path.clone();
    let mut parser_token_resolver = token_resolver;
    let handle = std::thread::Builder::new()
        .name("augustus-eu4-save-parser".into())
        .stack_size(32 * 1024 * 1024)
        .spawn(move || {
            read_vanilla_save_state_inner(
                &parser_path,
                parser_token_map_path.as_deref(),
                &mut parser_token_resolver,
                capture_trade_nodes,
                capture_state_edicts,
            )
        })
        .map_err(|error| Error::caused_by(&path, error))?;
    handle
        .join()
        .map_err(|_| Error::message(&path, "EU4 save parser thread panicked"))?
}

fn read_vanilla_save_state_inner(
    path: &Path,
    token_map_path: Option<&Path>,
    token_resolver: &mut Option<SaveTokenResolver>,
    capture_trade_nodes: bool,
    capture_state_edicts: bool,
) -> Result<
    (
        VanillaSaveStateWithOrderedTradeNodeRuntimeStates,
        VanillaExternalStateEdicts,
    ),
    Error,
> {
    let documents = read_eu4_save_documents(path, token_map_path, token_resolver)?;
    let parsed = documents.game_state.parse()?;
    let state_edicts = if capture_state_edicts {
        read_state_edicts(&parsed)?
    } else {
        VanillaExternalStateEdicts::new()
    };
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
    let save_date = Date::parse(&date).map_err(|error| Error::caused_by(parsed.source(), error))?;
    let eu4_random_state = saved_eu4_random_state(&parsed, &root)?;
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

    let empire = optional_object(&parsed, &root, "empire")?;
    let saved_electors = empire
        .as_ref()
        .map(|empire| {
            optional_field(&parsed, empire, "electors")?
                .map(|value| string_id_set(&parsed, &value, "elector"))
                .transpose()
        })
        .transpose()?
        .flatten();
    let saved_emperor = empire
        .as_ref()
        .map(|empire| optional_scalar(&parsed, empire, "emperor"))
        .transpose()?
        .flatten();
    if let Some(tag) = saved_electors
        .as_ref()
        .and_then(|electors| electors.iter().find(|tag| !is_country_tag(tag)))
    {
        return Err(Error::message(
            parsed.source(),
            format!("invalid saved elector country tag {tag}"),
        ));
    }
    if let Some(emperor) = &saved_emperor
        && !is_country_tag(emperor)
    {
        return Err(Error::message(
            parsed.source(),
            format!("invalid saved imperial emperor tag {emperor}"),
        ));
    }

    let mut external_state = VanillaExternalState {
        eu4_random_state,
        ..Default::default()
    };
    if let Some(emperor) = saved_emperor {
        external_state
            .political_office_holders
            .insert("imperial_emperor".into(), Some(emperor));
    }
    let mut current_trade_node_values_milli = BTreeMap::new();
    let mut trade_node_runtime_states = BTreeMap::new();
    let mut trade_node_order = Vec::new();
    if capture_trade_nodes && let Some(trade) = optional_object(&parsed, &root, "trade")? {
        for (key, op, value) in trade.fields() {
            if key.read_string() != "node" {
                continue;
            }
            if op.is_some_and(|operator| operator != jomini::text::Operator::Equal) {
                return Err(Error::message(
                    parsed.source(),
                    "invalid saved trade-node block",
                ));
            }
            let node = object(&parsed, &value)?;
            let name = optional_scalar(&parsed, &node, "definitions")?
                .filter(|name| !name.is_empty())
                .ok_or_else(|| Error::message(parsed.source(), "missing saved trade-node name"))?;
            let state = saved_trade_node_runtime_state(&parsed, &node)?;
            let current = state.current_milli;
            if current_trade_node_values_milli
                .insert(name.clone(), current)
                .is_some()
            {
                return Err(Error::message(
                    parsed.source(),
                    format!("duplicate saved trade node {name}"),
                ));
            }
            trade_node_runtime_states.insert(name.clone(), state);
            trade_node_order.push(name);
        }
    }
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
        let persons = saved_country_persons(&parsed, &country, &save_date)?;
        if !persons.is_empty() {
            external_state.country_persons.insert(tag.clone(), persons);
        }
        if let Some(electors) = &saved_electors {
            external_state
                .country_statuses
                .entry(tag.clone())
                .or_default()
                .insert("elector".into(), Some(electors.contains(&tag)));
        }
        let relation_states = saved_country_relation_states(&parsed, &country, &tag)?;
        if !relation_states.is_empty() {
            external_state
                .country_relation_states
                .insert(tag.clone(), relation_states);
        }
        if let Some(rival_statuses) = saved_country_rival_statuses(&parsed, &country, &tag)? {
            external_state
                .country_statuses
                .entry(tag.clone())
                .or_default()
                .extend(rival_statuses);
        }
        let resources = CountryResources {
            treasury_milli: optional_milli(&parsed, &country, "treasury")?,
            manpower_milli: optional_milli(&parsed, &country, "manpower")?,
            mercantilism_milli: optional_milli(&parsed, &country, "mercantilism")?,
        };
        external_state
            .country_resources
            .insert(tag.clone(), resources);
        let completed_missions = optional_field(&parsed, &country, "completed_missions")?
            .map(|value| string_id_set(&parsed, &value, "mission"))
            .transpose()?
            .unwrap_or_default();
        external_state
            .country_mission_completions
            .insert(tag.clone(), completed_missions);
        let mut variables = BTreeMap::new();
        if let Some(saved_variables) = optional_object(&parsed, &country, "variables")? {
            for (key, operator, value) in saved_variables.fields() {
                if operator.is_some_and(|operator| operator != jomini::text::Operator::Equal) {
                    return Err(Error::message(
                        parsed.source(),
                        format!("country {tag} has an invalid saved variable"),
                    ));
                }
                let name = key.read_string();
                let raw = scalar(&parsed, &value)?;
                let value = signed_milli(&raw).ok_or_else(|| {
                    Error::message(
                        parsed.source(),
                        format!("country {tag} has invalid value for saved variable {name}"),
                    )
                })?;
                if name.is_empty() || variables.insert(name.clone(), value).is_some() {
                    return Err(Error::message(
                        parsed.source(),
                        format!("country {tag} has an empty or duplicate saved variable {name}"),
                    ));
                }
            }
        }
        external_state
            .country_variables
            .insert(tag.clone(), variables);

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
                        .map(|value| saved_government_reform_set(&parsed, &value))
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
        if let Some(value) = optional_milli(&parsed, &country, "navy_tradition")? {
            external_state
                .country_metric_values
                .entry(tag.clone())
                .or_default()
                .insert("navy_tradition".into(), Some(value));
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
            let (location, name, regiments, regiment_states) = saved_army(&parsed, &value)?;
            external_state.armies.push(VanillaExternalArmy {
                owner: tag.clone(),
                location,
                name,
                regiments,
                regiment_states,
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
            let SavedNavy {
                location,
                name,
                ships,
                ship_states,
            } = saved_navy(&parsed, &value)?;
            external_state.naval_fleets.push(VanillaExternalFleet {
                owner: tag.clone(),
                location,
                name,
                ship_types: ships.clone(),
                ship_states: ship_states.clone(),
            });
            for (unit_type, ship_state) in ships.into_iter().zip(ship_states) {
                external_state.naval_units.push(VanillaExternalNavalUnit {
                    owner: tag.clone(),
                    location,
                    unit_type,
                    state: Some(ship_state),
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
        let mothball_status = optional_scalar(&parsed, &province, "mothball_status")?
            .map(|value| {
                value
                    .parse::<u8>()
                    .ok()
                    .filter(|_| {
                        !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())
                    })
                    .ok_or_else(|| {
                        Error::message(
                            parsed.source(),
                            format!("province {id} has an invalid mothball status"),
                        )
                    })
            })
            .transpose()?
            .unwrap_or(1);
        external_state
            .province_mothball_status
            .insert(id, mothball_status);
        let mut saved_cores = Vec::new();
        let mut seen_cores = BTreeSet::new();
        if let Some(cores) = optional_field(&parsed, &province, "cores")? {
            let mut insert = |tag: String| -> Result<(), Error> {
                if !is_country_tag(&tag) || !seen_cores.insert(tag.clone()) {
                    return Err(Error::message(
                        parsed.source(),
                        format!("province {id} has an invalid or duplicate saved core country"),
                    ));
                }
                saved_cores.push(tag);
                Ok(())
            };
            match cores.token() {
                TextToken::Array { .. } => {
                    let array = cores.read_array().expect("array token");
                    for value in array.values() {
                        let tag = scalar(&parsed, &value)?;
                        insert(tag)?;
                    }
                }
                TextToken::Object { .. } => {
                    let entries = cores.read_object().expect("object token");
                    for (key, op, value) in entries.fields() {
                        if op.is_some_and(|operator| operator != jomini::text::Operator::Equal) {
                            return Err(Error::message(
                                parsed.source(),
                                format!("province {id} has an invalid saved core entry"),
                            ));
                        }
                        let tag = key.read_string();
                        if op.is_some() {
                            match scalar(&parsed, &value)?.as_str() {
                                "yes" => insert(tag)?,
                                "no" => (),
                                _ => {
                                    return Err(Error::message(
                                        parsed.source(),
                                        format!("province {id} has an invalid saved core value"),
                                    ));
                                }
                            }
                        } else {
                            insert(tag)?;
                        }
                    }
                }
                _ => {
                    return Err(Error::message(
                        parsed.source(),
                        format!("province {id} has an invalid saved core list"),
                    ));
                }
            }
        }
        external_state.province_cores.insert(id, saved_cores);
        let mut saved_buildings = BTreeSet::new();
        if let Some(buildings) = optional_object(&parsed, &province, "buildings")? {
            let mut seen = BTreeSet::new();
            for (key, op, value) in buildings.fields() {
                if op.is_some_and(|operator| operator != jomini::text::Operator::Equal) {
                    return Err(Error::message(
                        parsed.source(),
                        format!("province {id} has an invalid saved building entry"),
                    ));
                }
                let name = key.read_string();
                if name.is_empty() || !seen.insert(name.clone()) {
                    return Err(Error::message(
                        parsed.source(),
                        format!("province {id} has an invalid or duplicate saved building"),
                    ));
                }
                match scalar(&parsed, &value)?.as_str() {
                    "yes" => {
                        saved_buildings.insert(name);
                    }
                    "no" => {}
                    _ => {
                        return Err(Error::message(
                            parsed.source(),
                            format!("province {id} has a non-boolean saved building entry"),
                        ));
                    }
                }
            }
        }
        external_state
            .province_buildings
            .insert(id, saved_buildings);
        if let Some(discovery) = optional_field(&parsed, &province, "discovered_by")? {
            let tags = saved_discovery_tags(&parsed, id, discovery)?;
            external_state.province_discovered_by.insert(id, tags);
        }
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
        let center_of_trade_level = optional_scalar(&parsed, &province, "center_of_trade")?
            .map(|text| {
                text.parse::<u8>()
                    .ok()
                    .filter(|_| !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit()))
                    .ok_or_else(|| {
                        Error::message(
                            parsed.source(),
                            format!("province {id} has an invalid saved center-of-trade level"),
                        )
                    })
            })
            .transpose()?
            .unwrap_or(0);
        external_state
            .province_center_of_trade_levels
            .insert(id, Some(center_of_trade_level));
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
        for name in ["devastation", "prosperity"] {
            if let Some(value) = optional_milli(&parsed, &province, name)? {
                if !(0..=100_000).contains(&value) {
                    return Err(Error::message(
                        parsed.source(),
                        format!("province {id} has saved {name} outside 0..100"),
                    ));
                }
                external_state
                    .province_metric_values
                    .entry(id)
                    .or_default()
                    .insert(name.into(), Some(value));
            }
        }
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

    Ok((
        VanillaSaveStateWithOrderedTradeNodeRuntimeStates {
            save: VanillaSaveStateWithTradeNodeRuntimeStates {
                state: VanillaSaveState {
                    date,
                    player,
                    players,
                    external_state,
                },
                current_trade_node_values_milli,
                trade_node_runtime_states,
            },
            trade_node_order,
        },
        state_edicts,
    ))
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
) -> Result<(u32, String, Vec<String>, Vec<VanillaExternalRegimentState>), Error> {
    let army = object(parsed, value)?;
    let mut location = None;
    let mut name = String::new();
    let mut regiments = Vec::new();
    let mut regiment_states = Vec::new();
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
                let (unit_type, state) = saved_regiment(parsed, &value)?;
                if unit_type.is_empty() || unit_type.bytes().any(|byte| byte.is_ascii_whitespace())
                {
                    return Err(Error::message(
                        parsed.source(),
                        "saved regiment has an invalid type",
                    ));
                }
                regiments.push(unit_type);
                regiment_states.push(state);
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
    Ok((location, name, regiments, regiment_states))
}

fn saved_regiment(
    parsed: &ParsedText<'_>,
    value: &Field<'_, '_>,
) -> Result<(String, VanillaExternalRegimentState), Error> {
    let regiment = object(parsed, value)?;
    let mut seen = BTreeSet::new();
    let mut unit_type = None;
    let mut state = VanillaExternalRegimentState::default();
    for (key, op, value) in regiment.fields() {
        let field = key.read_string();
        if !matches!(
            field.as_str(),
            "type" | "name" | "strength" | "morale" | "home"
        ) {
            continue;
        }
        if op.is_some_and(|operator| operator != jomini::text::Operator::Equal) {
            return Err(Error::message(
                parsed.source(),
                format!("invalid saved regiment field {field}"),
            ));
        }
        if !seen.insert(field.clone()) {
            return Err(Error::message(
                parsed.source(),
                format!("duplicate saved regiment field {field}"),
            ));
        }
        let text = scalar(parsed, &value)?;
        match field.as_str() {
            "type" => unit_type = Some(text),
            "name" => state.name = Some(text),
            "strength" => {
                let strength = signed_milli(&text).ok_or_else(|| {
                    Error::message(parsed.source(), "invalid saved regiment strength")
                })?;
                if strength < 0 {
                    return Err(Error::message(
                        parsed.source(),
                        "saved regiment strength must not be negative",
                    ));
                }
                state.strength_milli = Some(strength);
            }
            "morale" => {
                let morale = signed_milli(&text).ok_or_else(|| {
                    Error::message(parsed.source(), "invalid saved regiment morale")
                })?;
                if morale < 0 {
                    return Err(Error::message(
                        parsed.source(),
                        "saved regiment morale must not be negative",
                    ));
                }
                state.morale_milli = Some(morale);
            }
            "home" => {
                let province = text
                    .parse::<u32>()
                    .map_err(|error| Error::caused_by(parsed.source(), error))?;
                if province == 0 {
                    return Err(Error::message(
                        parsed.source(),
                        "saved regiment home province must be positive",
                    ));
                }
                state.home_province = Some(province);
            }
            _ => unreachable!(),
        }
    }
    let unit_type = unit_type
        .ok_or_else(|| Error::message(parsed.source(), "saved regiment is missing its type"))?;
    Ok((unit_type, state))
}

fn saved_navy(parsed: &ParsedText<'_>, value: &Field<'_, '_>) -> Result<SavedNavy, Error> {
    let navy = object(parsed, value)?;
    let mut location = None;
    let mut name = None;
    let mut ships = Vec::new();
    let mut ship_states = Vec::new();
    for (key, op, value) in navy.fields() {
        let field = key.read_string();
        if op.is_some_and(|operator| operator != jomini::text::Operator::Equal) {
            return Err(Error::message(
                parsed.source(),
                format!("invalid saved navy field {field}"),
            ));
        }
        match field.as_str() {
            "name" => name = Some(scalar(parsed, &value)?),
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
                let (unit_type, state) = saved_ship(parsed, &value)?;
                ships.push(unit_type);
                ship_states.push(state);
            }
            _ => {}
        }
    }
    let location = location
        .ok_or_else(|| Error::message(parsed.source(), "saved navy is missing its location"))?;
    if ships.is_empty() {
        return Err(Error::message(parsed.source(), "saved navy has no ships"));
    }
    Ok(SavedNavy {
        location,
        name,
        ships,
        ship_states,
    })
}

struct SavedNavy {
    location: u32,
    name: Option<String>,
    ships: Vec<String>,
    ship_states: Vec<VanillaExternalShipState>,
}

fn saved_ship(
    parsed: &ParsedText<'_>,
    value: &Field<'_, '_>,
) -> Result<(String, VanillaExternalShipState), Error> {
    let ship = object(parsed, value)?;
    let mut seen = BTreeSet::new();
    let mut unit_type = None;
    let mut state = VanillaExternalShipState::default();
    for (key, op, value) in ship.fields() {
        let field = key.read_string();
        if !matches!(field.as_str(), "type" | "name" | "morale" | "home") {
            continue;
        }
        if op.is_some_and(|operator| operator != jomini::text::Operator::Equal) {
            return Err(Error::message(
                parsed.source(),
                format!("invalid saved ship field {field}"),
            ));
        }
        if !seen.insert(field.clone()) {
            return Err(Error::message(
                parsed.source(),
                format!("duplicate saved ship field {field}"),
            ));
        }
        let text = scalar(parsed, &value)?;
        match field.as_str() {
            "type" => unit_type = Some(text),
            "name" => state.name = Some(text),
            "morale" => {
                let morale = signed_milli(&text)
                    .ok_or_else(|| Error::message(parsed.source(), "invalid saved ship morale"))?;
                if morale < 0 {
                    return Err(Error::message(
                        parsed.source(),
                        "saved ship morale must not be negative",
                    ));
                }
                state.morale_milli = Some(morale);
            }
            "home" => {
                let province = text
                    .parse::<u32>()
                    .map_err(|error| Error::caused_by(parsed.source(), error))?;
                if province == 0 {
                    return Err(Error::message(
                        parsed.source(),
                        "saved ship home province must be positive",
                    ));
                }
                state.home_province = Some(province);
            }
            _ => unreachable!(),
        }
    }
    let unit_type = unit_type
        .filter(|unit_type| {
            !unit_type.is_empty() && !unit_type.bytes().any(|byte| byte.is_ascii_whitespace())
        })
        .ok_or_else(|| {
            Error::message(parsed.source(), "saved ship is missing or has invalid type")
        })?;
    Ok((unit_type, state))
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

fn saved_eu4_random_state(
    parsed: &ParsedText<'_>,
    root: &Object<'_, '_>,
) -> Result<Option<VanillaExternalEu4RandomState>, Error> {
    let seed = optional_scalar(parsed, root, "multiplayer_random_seed")?;
    let count = optional_scalar(parsed, root, "multiplayer_random_count")?;
    match (seed, count) {
        (Some(seed), Some(count)) => Ok(Some(VanillaExternalEu4RandomState {
            seed: seed
                .parse()
                .map_err(|error| Error::caused_by(parsed.source(), error))?,
            count: count
                .parse()
                .map_err(|error| Error::caused_by(parsed.source(), error))?,
        })),
        (None, None) => Ok(None),
        _ => Err(Error::message(
            parsed.source(),
            "saved EU4 random state requires both multiplayer_random_seed and multiplayer_random_count",
        )),
    }
}

fn saved_person_id(parsed: &ParsedText<'_>, value: &Field<'_, '_>) -> Result<SavedPersonId, Error> {
    let person = object(parsed, value)?;
    let id = optional_field(parsed, &person, "id")?
        .ok_or_else(|| Error::message(parsed.source(), "saved person has no identifier"))?;
    let (id, kind) = if matches!(id.token(), TextToken::Object { .. }) {
        let identifier = object(parsed, &id)?;
        let id = optional_scalar(parsed, &identifier, "id")?
            .ok_or_else(|| Error::message(parsed.source(), "saved person id has no value"))?;
        let kind = optional_scalar(parsed, &identifier, "type")?
            .ok_or_else(|| Error::message(parsed.source(), "saved person id has no type"))?;
        (id, kind)
    } else {
        let id = scalar(parsed, &id)?;
        let kind = optional_scalar(parsed, &person, "type")?
            .ok_or_else(|| Error::message(parsed.source(), "saved person has no id type"))?;
        (id, kind)
    };
    if id.is_empty() || kind.is_empty() {
        return Err(Error::message(
            parsed.source(),
            "saved person has an empty id",
        ));
    }
    Ok((id, kind))
}

fn saved_person_role(name: &str) -> Option<&'static str> {
    match name {
        "monarch" => Some("ruler"),
        "heir" => Some("heir"),
        "queen" | "monarch_consort" => Some("consort"),
        _ => None,
    }
}

const CURRENT_SAVED_PERSON_ROLES: [&str; 3] = ["ruler", "heir", "consort"];

fn saved_country_person_roles(
    references: &BTreeMap<String, SavedPersonId>,
    saved_people: &BTreeMap<SavedPersonId, VanillaExternalPerson>,
) -> BTreeMap<String, Option<VanillaExternalPerson>> {
    CURRENT_SAVED_PERSON_ROLES
        .into_iter()
        .map(|role| {
            let person = references.get(role).map(|id| {
                saved_people
                    .get(id)
                    .cloned()
                    .unwrap_or_else(|| VanillaExternalPerson {
                        state: augustus_core::PersonState::default(),
                        country_of_origin: None,
                    })
            });
            (role.to_owned(), person)
        })
        .collect()
}

fn saved_country_persons(
    parsed: &ParsedText<'_>,
    country: &Object<'_, '_>,
    save_date: &Date,
) -> Result<BTreeMap<String, Option<VanillaExternalPerson>>, Error> {
    let mut references = BTreeMap::new();
    for (field, role) in [
        ("monarch", "ruler"),
        ("heir", "heir"),
        ("queen", "consort"),
        ("monarch_consort", "consort"),
    ] {
        for (key, op, value) in country
            .fields()
            .filter(|(key, _, _)| key.read_string() == field)
        {
            if op.is_some_and(|operator| operator != Operator::Equal) {
                return Err(Error::message(
                    parsed.source(),
                    "invalid saved current person reference",
                )
                .with_location(parsed.field_location(&key)));
            }
            let id = saved_person_id(parsed, &value)?;
            if let Some(previous) = references.insert(role.to_owned(), id.clone())
                && previous != id
            {
                return Err(Error::message(
                    parsed.source(),
                    format!("country has conflicting saved {role} references"),
                ));
            }
        }
    }
    if references.is_empty() {
        return Ok(saved_country_person_roles(&references, &BTreeMap::new()));
    }

    let Some(history) = optional_object(parsed, country, "history")? else {
        return Ok(saved_country_person_roles(&references, &BTreeMap::new()));
    };

    let wanted: BTreeSet<_> = references.values().cloned().collect();
    let mut history_blocks = vec![(None, history.clone())];
    for (key, op, value) in history.fields() {
        let name = key.read_string();
        if !crate::bootstrap::dated_name(&name) {
            continue;
        }
        if op.is_some_and(|operator| operator != Operator::Equal) {
            return Err(
                Error::message(parsed.source(), "invalid saved country history date")
                    .with_location(parsed.field_location(&key)),
            );
        }
        let date = Date::parse(&name).map_err(|error| Error::caused_by(parsed.source(), error))?;
        if &date <= save_date {
            history_blocks.push((Some(date), object(parsed, &value)?));
        }
    }
    history_blocks.sort_by_key(|(date, _)| *date);

    let mut saved_people = BTreeMap::new();
    for (_, block) in history_blocks {
        for (key, op, value) in block.fields() {
            let name = key.read_string();
            let Some(role) = saved_person_role(&name) else {
                continue;
            };
            if op.is_some_and(|operator| operator != Operator::Equal) {
                return Err(
                    Error::message(parsed.source(), "invalid saved person history")
                        .with_location(parsed.field_location(&key)),
                );
            }
            let id = saved_person_id(parsed, &value)?;
            if !wanted.contains(&id) {
                continue;
            }
            let location = parsed.field_location(&key);
            let draft = crate::politics::saved_person(parsed, value, role, location)?;
            let (state, country_of_origin) = draft.into_saved_parts();
            saved_people.insert(
                id,
                VanillaExternalPerson {
                    state,
                    country_of_origin,
                },
            );
        }
    }

    Ok(saved_country_person_roles(&references, &saved_people))
}

fn saved_country_relation_states(
    parsed: &ParsedText<'_>,
    country: &Object<'_, '_>,
    owner: &str,
) -> Result<BTreeMap<String, CountryRelationState>, Error> {
    let mut states: BTreeMap<String, CountryRelationState> = BTreeMap::new();
    for (key, operator, value) in country.fields() {
        if key.read_string() != "active_relations" {
            continue;
        }
        if operator.is_some_and(|operator| operator != jomini::text::Operator::Equal) {
            return Err(Error::message(
                parsed.source(),
                format!("country {owner} has an invalid active-relations block"),
            ));
        }
        let relations = object(parsed, &value)?;
        for (target_key, operator, value) in relations.fields() {
            if operator.is_some_and(|operator| operator != jomini::text::Operator::Equal) {
                return Err(Error::message(
                    parsed.source(),
                    format!("country {owner} has an invalid active relation"),
                ));
            }
            let target = target_key.read_string();
            if !is_country_tag(&target) {
                return Err(Error::message(
                    parsed.source(),
                    format!("country {owner} has an invalid relation target {target}"),
                ));
            }
            let relation = object(parsed, &value)?;
            let mut current = CountryRelationState::default();
            let mut seen = BTreeSet::new();
            for (field_key, operator, value) in relation.fields() {
                let field = field_key.read_string();
                if !matches!(field.as_str(), "attitude" | "trust_value") {
                    continue;
                }
                if operator.is_some_and(|operator| operator != jomini::text::Operator::Equal) {
                    return Err(Error::message(
                        parsed.source(),
                        format!("country {owner} relation to {target} has invalid {field}"),
                    ));
                }
                if !seen.insert(field.clone()) {
                    return Err(Error::message(
                        parsed.source(),
                        format!("country {owner} relation to {target} duplicates {field}"),
                    ));
                }
                let raw = scalar(parsed, &value)?;
                match field.as_str() {
                    "attitude" if !raw.is_empty() => current.attitude = Some(raw),
                    "attitude" => {
                        return Err(Error::message(
                            parsed.source(),
                            format!("country {owner} relation to {target} has empty attitude"),
                        ));
                    }
                    "trust_value" => {
                        current.trust_value = Some(raw.parse::<i64>().map_err(|error| {
                            Error::message(
                                parsed.source(),
                                format!(
                                    "country {owner} relation to {target} has invalid trust value: {error}"
                                ),
                            )
                        })?);
                    }
                    _ => unreachable!(),
                }
            }
            if current.is_empty() {
                continue;
            }
            let saved = states.entry(target.clone()).or_default();
            if let Some(attitude) = current.attitude {
                if saved
                    .attitude
                    .as_ref()
                    .is_some_and(|existing| existing != &attitude)
                {
                    return Err(Error::message(
                        parsed.source(),
                        format!("country {owner} has conflicting attitudes for {target}"),
                    ));
                }
                saved.attitude.get_or_insert(attitude);
            }
            if let Some(trust_value) = current.trust_value {
                if saved
                    .trust_value
                    .is_some_and(|existing| existing != trust_value)
                {
                    return Err(Error::message(
                        parsed.source(),
                        format!("country {owner} has conflicting trust values for {target}"),
                    ));
                }
                saved.trust_value.get_or_insert(trust_value);
            }
        }
    }
    Ok(states)
}

fn saved_country_rival_statuses(
    parsed: &ParsedText<'_>,
    country: &Object<'_, '_>,
    owner: &str,
) -> Result<Option<BTreeMap<String, Option<bool>>>, Error> {
    let mut initialized = None;
    for (_key, operator, value) in country
        .fields()
        .filter(|(key, _, _)| key.read_string() == "initialized_rivals")
    {
        if initialized.is_some()
            || operator.is_some_and(|operator| operator != jomini::text::Operator::Equal)
        {
            return Err(Error::message(
                parsed.source(),
                format!("country {owner} has an invalid initialized-rivals field"),
            ));
        }
        initialized = Some(match scalar(parsed, &value)?.as_str() {
            "yes" => true,
            "no" => false,
            _ => {
                return Err(Error::message(
                    parsed.source(),
                    format!("country {owner} has an invalid initialized-rivals flag"),
                ));
            }
        });
    }
    if initialized != Some(true) {
        return Ok(None);
    }
    let mut rivals = BTreeSet::new();
    for (_key, operator, value) in country
        .fields()
        .filter(|(key, _, _)| key.read_string() == "rival")
    {
        if operator.is_some_and(|operator| operator != jomini::text::Operator::Equal) {
            return Err(Error::message(
                parsed.source(),
                format!("country {owner} has an invalid rival entry"),
            ));
        }
        let entry = object(parsed, &value)?;
        let mut target = None;
        for (field, operator, value) in entry.fields() {
            if field.read_string() != "country" {
                continue;
            }
            if target.is_some()
                || operator.is_some_and(|operator| operator != jomini::text::Operator::Equal)
            {
                return Err(Error::message(
                    parsed.source(),
                    format!("country {owner} has an invalid rival target"),
                ));
            }
            let name = scalar(parsed, &value)?;
            if !is_country_tag(&name) {
                return Err(Error::message(
                    parsed.source(),
                    format!("country {owner} has an invalid rival tag {name}"),
                ));
            }
            target = Some(name);
        }
        let target = target.ok_or_else(|| {
            Error::message(
                parsed.source(),
                format!("country {owner} has a rival entry without a country"),
            )
        })?;
        if !rivals.insert(target.clone()) {
            return Err(Error::message(
                parsed.source(),
                format!("country {owner} duplicates rival {target}"),
            ));
        }
    }
    let mut statuses = BTreeMap::from([("rivals_initialized".into(), Some(true))]);
    statuses.extend(
        rivals
            .into_iter()
            .map(|rival| (format!("is_rival:{rival}"), Some(true))),
    );
    Ok(Some(statuses))
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

fn saved_discovery_tags(
    parsed: &ParsedText<'_>,
    province: u32,
    value: Field<'_, '_>,
) -> Result<BTreeSet<String>, Error> {
    let mut tags = BTreeSet::new();
    let mut insert = |tag: String| -> Result<(), Error> {
        if !is_country_tag(&tag) {
            return Err(Error::message(
                parsed.source(),
                format!("province {province} has an invalid saved discovery country"),
            ));
        }
        if !tags.insert(tag) {
            return Err(Error::message(
                parsed.source(),
                format!("province {province} has a duplicate discovery country"),
            ));
        }
        Ok(())
    };

    match value.token() {
        TextToken::Array { .. } => {
            let array = value.read_array().expect("array token");
            for value in array.values() {
                let tag = value.read_string().map_err(|_| {
                    Error::message(
                        parsed.source(),
                        format!("province {province} has an invalid saved discovery entry"),
                    )
                })?;
                insert(tag)?;
            }
        }
        TextToken::Object { .. } => {
            let discovery = value.read_object().expect("object token");
            for (key, op, value) in discovery.fields() {
                if op.is_some_and(|operator| operator != Operator::Equal) {
                    return Err(Error::message(
                        parsed.source(),
                        format!("province {province} has an invalid saved discovery entry"),
                    ));
                }
                let tag = key.read_string();
                if op.is_some() {
                    let present = value.read_string().map_err(|_| {
                        Error::message(
                            parsed.source(),
                            format!("province {province} has an invalid saved discovery value"),
                        )
                    })?;
                    match present.as_str() {
                        "yes" => insert(tag)?,
                        "no" => (),
                        _ => {
                            return Err(Error::message(
                                parsed.source(),
                                format!("province {province} has an invalid saved discovery value"),
                            ));
                        }
                    }
                } else {
                    insert(tag)?;
                }
            }
        }
        _ => {
            return Err(Error::message(
                parsed.source(),
                format!("province {province} has an invalid saved discovery set"),
            ));
        }
    }
    Ok(tags)
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

fn saved_trade_node_runtime_state(
    parsed: &ParsedText<'_>,
    reader: &Object<'_, '_>,
) -> Result<augustus_core::TradeNodeRuntimeState, Error> {
    let mut state = augustus_core::TradeNodeRuntimeState::default();
    let mut seen = BTreeSet::new();
    for (key, operator, value) in reader.fields() {
        let name = key.read_string();
        let slot = match name.as_str() {
            "current" => &mut state.current_milli,
            "local_value" => &mut state.local_value_milli,
            "outgoing" => &mut state.outgoing_milli,
            "value_added_outgoing" => &mut state.value_added_outgoing_milli,
            "retention" => &mut state.retention_milli,
            "p_pow" => &mut state.p_pow_milli,
            "max" => &mut state.max_milli,
            "collector_power" => &mut state.collector_power_milli,
            "collector_power_including_pirates" => {
                &mut state.collector_power_including_pirates_milli
            }
            "pull_power" => &mut state.pull_power_milli,
            "retain_power" => &mut state.retain_power_milli,
            "highest_power" => &mut state.highest_power_milli,
            _ => continue,
        };
        if operator.is_some_and(|operator| operator != jomini::text::Operator::Equal) {
            return Err(Error::message(
                parsed.source(),
                format!("invalid saved trade-node field {name}"),
            ));
        }
        if !seen.insert(name.clone()) {
            return Err(Error::message(
                parsed.source(),
                format!("duplicate saved trade-node field {name}"),
            ));
        }
        let text = scalar(parsed, &value)?;
        *slot = Some(signed_milli(&text).ok_or_else(|| {
            Error::message(
                parsed.source(),
                format!("invalid saved decimal field {name}"),
            )
        })?);
    }
    Ok(state)
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

fn saved_government_reform_set(
    parsed: &ParsedText<'_>,
    value: &Field<'_, '_>,
) -> Result<BTreeSet<String>, Error> {
    let array = value
        .read_array()
        .map_err(|error| Error::caused_by(parsed.source(), error))?;
    let mut result = BTreeSet::new();
    for item in array.values() {
        let id = scalar(parsed, &item)?;
        if id.is_empty() {
            // EU4 saves include empty entries for unselected reform slots.
            continue;
        }
        if !result.insert(id) {
            return Err(Error::message(
                parsed.source(),
                "duplicate saved government reform identifier",
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
    use super::{
        NationalFocus, RulerSkill, read_vanilla_save_state,
        read_vanilla_save_state_and_ordered_trade_node_runtime_states,
        read_vanilla_save_state_and_trade_node_runtime_states,
        read_vanilla_save_state_and_trade_nodes,
        read_vanilla_save_state_and_trade_nodes_with_token_resolver,
        read_vanilla_save_state_with_token_map,
    };
    use crate::{
        ContentRoot, ContentSet, ContentSource, VanillaExternalEu4RandomState,
        VanillaExternalProvinceDevelopment, VanillaExternalState, VanillaWorld,
    };
    use augustus_core::CountryRelationState;
    use jomini::{
        binary::{Token, TokenResolver},
        common::Date,
    };
    use std::{
        collections::{BTreeMap, BTreeSet},
        fs::File,
        io::Write,
        path::Path,
    };
    use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

    struct TestTokenResolver;

    impl TokenResolver for TestTokenResolver {
        fn resolve(&self, token: u16) -> Option<&str> {
            match token {
                0x3000 => Some("date"),
                0x3001 => Some("player"),
                0x3002 => Some("countries"),
                0x3003 => Some("AAA"),
                0x3004 => Some("provinces"),
                _ => None,
            }
        }

        fn lookup(&self, index: u32) -> Option<&str> {
            (index == 7).then_some("AAA")
        }
    }

    fn binary_startup_save() -> Vec<u8> {
        let binary_date = Date::parse("1444.11.11").unwrap().to_binary();
        let mut bytes = b"EU4bin".to_vec();
        for token in [
            Token::Id(0x3000),
            Token::Equal,
            Token::I32(binary_date),
            Token::Id(0x3006),
            Token::Equal,
            Token::Lookup(7),
            Token::Id(0x3001),
            Token::Equal,
            Token::Open,
            Token::Id(0x3002),
            Token::Equal,
            Token::Open,
            Token::Id(0x3005),
            Token::Equal,
            Token::F32(12_345_i32.to_le_bytes()),
            Token::Close,
            Token::Close,
            Token::Id(0x3003),
            Token::Equal,
            Token::Open,
            Token::Close,
        ] {
            token.write(&mut bytes).unwrap();
        }
        bytes
    }

    fn binary_startup_save_with_lookup() -> Vec<u8> {
        let binary_date = Date::parse("1444.11.11").unwrap().to_binary();
        let mut bytes = b"EU4bin".to_vec();
        for token in [
            Token::Id(0x3000),
            Token::Equal,
            Token::I32(binary_date),
            Token::Id(0x3001),
            Token::Equal,
            Token::Lookup(7),
            Token::Id(0x3002),
            Token::Equal,
            Token::Open,
            Token::Id(0x3003),
            Token::Equal,
            Token::Open,
            Token::Close,
            Token::Close,
            Token::Id(0x3004),
            Token::Equal,
            Token::Open,
            Token::Close,
        ] {
            token.write(&mut bytes).unwrap();
        }
        bytes
    }

    fn binary_token_map(directory: &std::path::Path) -> std::path::PathBuf {
        let path = directory.join("tokens.txt");
        std::fs::write(
            &path,
            "0x3000 date\n0x3001 countries\n0x3002 AAA\n0x3003 provinces\n0x3005 treasury\n0x3006 player\nlookup 7 AAA\n",
        )
        .unwrap();
        path
    }

    #[test]
    fn saved_elector_roster_sets_membership_and_leaves_missing_rosters_unknown() {
        let directory = tempfile::tempdir().unwrap();
        let save_path = directory.path().join("electors.eu4");
        std::fs::write(
            &save_path,
            r#"EU4txt
date=1444.11.11
empire={ electors={ AAA CCC } }
countries={ AAA={} BBB={} CCC={} }
provinces={}
"#,
        )
        .unwrap();

        let save = read_vanilla_save_state(&save_path).unwrap();
        assert_eq!(
            save.external_state.country_statuses["AAA"]["elector"],
            Some(true)
        );
        assert_eq!(
            save.external_state.country_statuses["BBB"]["elector"],
            Some(false)
        );
        assert_eq!(
            save.external_state.country_statuses["CCC"]["elector"],
            Some(true)
        );

        std::fs::write(
            &save_path,
            r#"EU4txt
date=1444.11.11
empire={ electors={} }
countries={ AAA={} BBB={} }
provinces={}
"#,
        )
        .unwrap();
        let save = read_vanilla_save_state(&save_path).unwrap();
        assert_eq!(
            save.external_state.country_statuses["AAA"]["elector"],
            Some(false)
        );
        assert_eq!(
            save.external_state.country_statuses["BBB"]["elector"],
            Some(false)
        );

        std::fs::write(
            &save_path,
            r#"EU4txt
date=1444.11.11
empire={}
countries={ AAA={} BBB={} }
provinces={}
"#,
        )
        .unwrap();
        let save = read_vanilla_save_state(&save_path).unwrap();
        assert!(
            !save
                .external_state
                .country_statuses
                .get("AAA")
                .is_some_and(|statuses| statuses.contains_key("elector"))
        );
    }

    #[test]
    fn saved_emperor_is_imported_and_missing_emperor_stays_unset() {
        let directory = tempfile::tempdir().unwrap();
        let save_path = directory.path().join("emperor.eu4");
        std::fs::write(
            &save_path,
            r#"EU4txt
date=1444.11.11
empire={ emperor="BBB" }
countries={ AAA={} BBB={} }
provinces={}
"#,
        )
        .unwrap();

        let save = read_vanilla_save_state(&save_path).unwrap();
        assert_eq!(
            save.external_state.political_office_holders["imperial_emperor"].as_deref(),
            Some("BBB")
        );

        std::fs::write(
            &save_path,
            r#"EU4txt
date=1444.11.11
empire={}
countries={ AAA={} BBB={} }
provinces={}
"#,
        )
        .unwrap();
        let save = read_vanilla_save_state(&save_path).unwrap();
        assert!(
            !save
                .external_state
                .political_office_holders
                .contains_key("imperial_emperor")
        );
    }

    #[test]
    fn saved_current_person_references_resolve_from_dated_country_history() {
        let directory = tempfile::tempdir().unwrap();
        let save_path = directory.path().join("people.eu4");
        std::fs::write(
            &save_path,
            r#"EU4txt
date=1445.1.1
countries={
    AAA={
        monarch={id=7457 type=48}
        heir={id=7458 type=48}
        queen={id=7459 type=48}
        history={
            1444.11.11={
                monarch={id={id=7457 type=48} name="Saved Ruler" ADM=4 DIP=5 MIL=6 birth_date=1420.1.1}
                heir={id={id=7458 type=48} name="Saved Heir" ADM=2 DIP=3 MIL=4 birth_date=1430.1.1 claim=0.750}
                queen={id={id=7459 type=48} name="Saved Consort" birth_date=1425.1.1 country_of_origin=BBB}
            }
            1445.2.1={
                monarch={id={id=7457 type=48} name="Future Ruler" ADM=6 DIP=6 MIL=6 birth_date=1420.1.1}
            }
        }
    }
    BBB={ monarch={id=9999 type=48} }
}
provinces={}
"#,
        )
        .unwrap();

        let save = read_vanilla_save_state(&save_path).unwrap();
        let people = &save.external_state.country_persons["AAA"];
        let ruler = people["ruler"].as_ref().unwrap();
        assert_eq!(ruler.state.name.as_deref(), Some("Saved Ruler"));
        assert_eq!(
            (
                ruler.state.administration,
                ruler.state.diplomacy,
                ruler.state.military
            ),
            (Some(4), Some(5), Some(6))
        );
        let heir = people["heir"].as_ref().unwrap();
        assert_eq!(heir.state.name.as_deref(), Some("Saved Heir"));
        assert_eq!(heir.state.claim_milli, Some(750));
        let consort = people["consort"].as_ref().unwrap();
        assert_eq!(consort.state.name.as_deref(), Some("Saved Consort"));
        assert_eq!(consort.country_of_origin.as_deref(), Some("BBB"));

        assert_eq!(
            save.external_state.country_persons["BBB"]["ruler"]
                .as_ref()
                .unwrap()
                .state,
            augustus_core::PersonState::default()
        );
        assert!(save.external_state.country_persons["BBB"]["heir"].is_none());
        assert!(save.external_state.country_persons["BBB"]["consort"].is_none());
    }

    #[test]
    fn saved_eu4_random_state_reads_seed_and_draw_count() {
        let directory = tempfile::tempdir().unwrap();
        let save_path = directory.path().join("random.eu4");
        std::fs::write(
            &save_path,
            "EU4txt\ndate=1444.11.11\nmultiplayer_random_seed=8351375\nmultiplayer_random_count=341520\ncountries={}\nprovinces={}\n",
        )
        .unwrap();

        let save = read_vanilla_save_state(&save_path).unwrap();
        assert_eq!(
            save.external_state.eu4_random_state,
            Some(VanillaExternalEu4RandomState {
                seed: 8_351_375,
                count: 341_520,
            })
        );
    }

    #[test]
    fn partial_saved_eu4_random_state_is_rejected() {
        let directory = tempfile::tempdir().unwrap();
        let save_path = directory.path().join("partial-random.eu4");
        std::fs::write(
            &save_path,
            "EU4txt\ndate=1444.11.11\nmultiplayer_random_seed=8351375\ncountries={}\nprovinces={}\n",
        )
        .unwrap();

        let error = read_vanilla_save_state(&save_path).unwrap_err();
        assert!(error.to_string().contains("requires both"));
    }

    #[test]
    fn saved_armies_preserve_regiment_names_strength_morale_and_home_province() {
        let directory = tempfile::tempdir().unwrap();
        let save_path = directory.path().join("armies.eu4");
        std::fs::write(
            &save_path,
            r#"EU4txt
date=1444.11.11
countries={
    AAA={
        army={
            location=1
            name="Northern Army"
            regiment={ type=infantry name="Marwar's 1st Regiment" strength=0.569 morale=1.234 home=1 }
            regiment={ type=cavalry strength=0 morale=0 home=2 }
            regiment={ type=artillery }
        }
    }
}
provinces={}
"#,
        )
        .unwrap();

        let save = read_vanilla_save_state(&save_path).unwrap();
        let army = &save.external_state.armies[0];
        assert_eq!(army.owner, "AAA");
        assert_eq!(army.location, 1);
        assert_eq!(army.name, "Northern Army");
        assert_eq!(
            army.regiments,
            ["infantry", "cavalry", "artillery"].map(str::to_owned)
        );
        assert_eq!(
            army.regiment_states[0].name.as_deref(),
            Some("Marwar's 1st Regiment")
        );
        assert_eq!(army.regiment_states[0].strength_milli, Some(569));
        assert_eq!(army.regiment_states[0].morale_milli, Some(1_234));
        assert_eq!(army.regiment_states[0].home_province, Some(1));
        assert_eq!(army.regiment_states[1].name, None);
        assert_eq!(army.regiment_states[1].strength_milli, Some(0));
        assert_eq!(army.regiment_states[1].morale_milli, Some(0));
        assert_eq!(army.regiment_states[1].home_province, Some(2));
        assert_eq!(army.regiment_states[2].name, None);
        assert_eq!(army.regiment_states[2].strength_milli, None);
        assert_eq!(army.regiment_states[2].morale_milli, None);
        assert_eq!(army.regiment_states[2].home_province, None);
    }

    #[test]
    fn saved_regiment_state_rejects_invalid_values() {
        let directory = tempfile::tempdir().unwrap();
        let save_path = directory.path().join("invalid-army.eu4");
        for regiment in [
            "type=infantry strength=-0.001",
            "type=infantry strength=invalid",
            "type=infantry strength=0.1234",
            "type=infantry strength=1 strength=2",
            "type=infantry name=first name=second",
            "type=infantry morale=-0.001",
            "type=infantry morale=invalid",
            "type=infantry home=0",
            "type=infantry home=invalid",
            "type=infantry morale=1 morale=2",
        ] {
            let text = format!(
                "EU4txt\ndate=1444.11.11\ncountries={{ AAA={{ army={{ location=1 regiment={{ {regiment} }} }} }} }}\nprovinces={{}}\n"
            );
            std::fs::write(&save_path, text).unwrap();
            assert!(read_vanilla_save_state(&save_path).is_err(), "{regiment}");
        }
    }

    #[test]
    fn saved_country_relation_inputs_filter_targets_overlay_and_restore() {
        let directory = tempfile::tempdir().unwrap();
        let save_path = directory.path().join("relations.eu4");
        std::fs::write(
            &save_path,
            r#"EU4txt
date=1444.11.11
player=AAA
countries={
    AAA={
        initialized_rivals=yes
        rival={ country="BBB" date=1444.11.11 }
        rival={ country="ZZZ" date=1444.11.11 }
        active_relations={
            BBB={ attitude=attitude_probe trust_value=85 }
            ZZZ={ attitude=attitude_unknown_target trust_value=12 }
        }
    }
    BBB={ initialized_rivals=yes }
    CCC={ initialized_rivals=no }
}
provinces={}
"#,
        )
        .unwrap();
        let save = read_vanilla_save_state(&save_path).unwrap();
        let parsed = &save.external_state.country_relation_states["AAA"];
        assert_eq!(parsed["BBB"].attitude.as_deref(), Some("attitude_probe"));
        assert_eq!(parsed["BBB"].trust_value, Some(85));
        assert!(parsed.contains_key("ZZZ"));
        let saved_rivals = &save.external_state.country_statuses["AAA"];
        assert_eq!(saved_rivals["rivals_initialized"], Some(true));
        assert_eq!(saved_rivals["is_rival:BBB"], Some(true));
        assert_eq!(saved_rivals["is_rival:ZZZ"], Some(true));
        assert_eq!(
            save.external_state.country_statuses["BBB"]["rivals_initialized"],
            Some(true)
        );
        assert!(!save.external_state.country_statuses.contains_key("CCC"));

        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world");
        let content = ContentSet::resolve(vec![ContentSource::base(
            ContentRoot::open(fixture).unwrap(),
        )])
        .unwrap();
        let baseline = VanillaWorld::load_from_content(&content, "1444.11.11").unwrap();
        let filtered = save.state_for_world(&baseline).unwrap();
        assert!(filtered.country_relation_states["AAA"].contains_key("BBB"));
        assert!(!filtered.country_relation_states["AAA"].contains_key("ZZZ"));
        assert_eq!(
            filtered.country_statuses["AAA"]["rivals_initialized"],
            Some(true)
        );
        assert_eq!(filtered.country_statuses["AAA"]["is_rival:BBB"], Some(true));
        assert_eq!(filtered.country_statuses["AAA"]["is_rival:ZZZ"], Some(true));
        assert_eq!(
            filtered.country_statuses["BBB"]["rivals_initialized"],
            Some(true)
        );

        let overlay = VanillaExternalState {
            country_relation_states: BTreeMap::from([(
                "AAA".into(),
                BTreeMap::from([(
                    "BBB".into(),
                    CountryRelationState {
                        attitude: None,
                        trust_value: Some(42),
                    },
                )]),
            )]),
            ..Default::default()
        };
        let mut loaded = VanillaWorld::load_from_content_with_save_state_and_external_state(
            &content,
            &save,
            Some(&overlay),
        )
        .unwrap();
        let owner = loaded.initial.country_entity("AAA").unwrap();
        let target = loaded.initial.country_entity("BBB").unwrap();
        let state = loaded
            .initial
            .world()
            .country_relation_state(owner, target)
            .unwrap();
        assert_eq!(state.attitude.as_deref(), Some("attitude_probe"));
        assert_eq!(state.trust_value, Some(42));
        assert_eq!(
            loaded
                .initial
                .world()
                .politics()
                .status(owner, "is_rival:BBB"),
            Some(true)
        );
        assert_eq!(
            loaded
                .initial
                .world()
                .politics()
                .status(owner, "is_rival:ZZZ"),
            Some(true)
        );
        assert_eq!(
            loaded
                .initial
                .world()
                .politics()
                .status(target, "is_rival:AAA"),
            Some(false)
        );

        let snapshot = loaded.save().unwrap();
        loaded.restore(&snapshot).unwrap();
        let state = loaded
            .initial
            .world()
            .country_relation_state(owner, target)
            .unwrap();
        assert_eq!(state.attitude.as_deref(), Some("attitude_probe"));
        assert_eq!(state.trust_value, Some(42));
        assert_eq!(
            loaded
                .initial
                .world()
                .politics()
                .status(owner, "is_rival:BBB"),
            Some(true)
        );
        assert_eq!(
            loaded
                .initial
                .world()
                .politics()
                .status(owner, "is_rival:ZZZ"),
            Some(true)
        );
        assert_eq!(
            loaded
                .initial
                .world()
                .politics()
                .status(target, "is_rival:AAA"),
            Some(false)
        );
    }

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
		completed_missions={
			"saved_mission"
		}
		variables={
			saved_counter=2.375
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
			ship={
				type="light_ship"
			}
		}
	}
	BBB={
		completed_missions={}
		variables={}
	}
	CCC={}
}
provinces={
	-1={
		owner="AAA"
		discovered_by={AAA=yes BBB=yes}
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
	-2={
		discovered_by={}
	}
	-3={
		discovered_by={AAA BBB}
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
            state.external_state.country_mission_completions["AAA"],
            BTreeSet::from(["saved_mission".into()])
        );
        assert_eq!(
            state.external_state.country_mission_completions["BBB"],
            BTreeSet::new()
        );
        assert_eq!(
            state.external_state.country_mission_completions["CCC"],
            BTreeSet::new()
        );
        assert_eq!(
            state.external_state.country_variables["AAA"]["saved_counter"],
            2_375
        );
        assert_eq!(
            state.external_state.country_variables["BBB"],
            BTreeMap::new()
        );
        assert_eq!(
            state.external_state.country_variables["CCC"],
            BTreeMap::new()
        );
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
        assert_eq!(state.external_state.naval_units.len(), 2);
        assert_eq!(state.external_state.naval_units[0].owner, "AAA");
        assert_eq!(state.external_state.naval_units[0].location, 1);
        assert_eq!(state.external_state.naval_units[0].unit_type, "galley");
        assert_eq!(state.external_state.naval_units[1].unit_type, "light_ship");
        assert_eq!(state.external_state.naval_fleets.len(), 1);
        assert_eq!(state.external_state.naval_fleets[0].owner, "AAA");
        assert_eq!(state.external_state.naval_fleets[0].location, 1);
        assert_eq!(
            state.external_state.naval_fleets[0].name.as_deref(),
            Some("Main Navy")
        );
        assert_eq!(
            state.external_state.naval_fleets[0].ship_types,
            ["galley", "light_ship"]
        );
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
            state.external_state.province_discovered_by[&1],
            BTreeSet::from(["AAA".into(), "BBB".into()])
        );
        assert_eq!(
            state.external_state.province_discovered_by[&2],
            BTreeSet::new()
        );
        assert_eq!(
            state.external_state.province_discovered_by[&3],
            BTreeSet::from(["AAA".into(), "BBB".into()])
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
    fn saved_ship_state_rejects_invalid_values() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("invalid-ship-state.eu4");
        for ship in [
            "type=galley morale=-0.001",
            "type=galley morale=invalid",
            "type=galley morale=0.1234",
            "type=galley home=0",
            "type=galley home=invalid",
            "type=galley name=first name=second",
            "type=galley type=light_ship",
        ] {
            let text = format!(
                "EU4txt\ndate=1444.11.11\ncountries={{ AAA={{ navy={{ location=1 ship={{ {ship} }} }} }} }}\nprovinces={{}}\n"
            );
            std::fs::write(&path, text).unwrap();
            assert!(read_vanilla_save_state(&path).is_err(), "{ship}");
        }
    }

    #[test]
    fn saved_fleet_groups_load_at_non_port_provinces() {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world");
        let content = ContentSet::resolve(vec![ContentSource::base(
            ContentRoot::open(fixture).unwrap(),
        )])
        .unwrap();
        let baseline = VanillaWorld::load_from_content(&content, "1444.11.11").unwrap();
        let location = 3;
        let province = baseline.initial.province_entity(location).unwrap();
        assert!(
            !baseline
                .initial
                .world()
                .province_has_port(province)
                .unwrap()
        );

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("fleet.eu4");
        std::fs::write(
            &path,
            format!(
                "EU4txt\ndate=1444.11.11\nplayer=\"AAA\"\ncountries={{ AAA={{ human=yes navy={{ name=\"North Sea Patrol\" location={location} ship={{ name=\"North Star\" home=1 type=galley morale=0.950 }} ship={{ type=light_ship }} }} }} }}\nprovinces={{}}\n"
            ),
        )
        .unwrap();

        let save = read_vanilla_save_state(&path).unwrap();
        let filtered = save.state_for_world(&baseline).unwrap();
        assert_eq!(filtered.naval_fleets.len(), 1);
        assert_eq!(
            filtered.naval_fleets[0].name.as_deref(),
            Some("North Sea Patrol")
        );
        assert_eq!(
            filtered.naval_fleets[0].ship_types,
            ["galley", "light_ship"]
        );
        assert_eq!(filtered.naval_fleets[0].ship_states.len(), 2);
        assert_eq!(
            filtered.naval_fleets[0].ship_states[0].name.as_deref(),
            Some("North Star")
        );
        assert_eq!(
            filtered.naval_fleets[0].ship_states[0].morale_milli,
            Some(950)
        );
        assert_eq!(
            filtered.naval_fleets[0].ship_states[0].home_province,
            Some(1)
        );
        assert_eq!(filtered.naval_fleets[0].ship_states[1].name, None);

        let loaded = VanillaWorld::load_from_content_with_save_state(&content, &save).unwrap();
        assert_eq!(loaded.initial.fleets().len(), 1);
        assert_eq!(
            loaded.initial.fleets()[0].name.as_deref(),
            Some("North Sea Patrol")
        );
        assert_eq!(loaded.initial.fleets()[0].location, province);
        assert_eq!(loaded.initial.fleets()[0].ships.len(), 2);
        assert_eq!(
            loaded.initial.fleets()[0]
                .ships
                .iter()
                .map(|id| loaded.initial.naval_units()[id.0 as usize]
                    .unit_type
                    .as_str())
                .collect::<Vec<_>>(),
            ["galley", "light_ship"]
        );
        assert_eq!(
            loaded.initial.naval_units()[0].name.as_deref(),
            Some("North Star")
        );
        assert_eq!(loaded.initial.naval_units()[0].morale_milli, Some(950));
        assert_eq!(
            loaded.initial.naval_units()[0].home_province,
            loaded.initial.province_entity(1)
        );
        assert_eq!(loaded.initial.naval_units()[1].name, None);
    }

    #[test]
    fn imports_saved_province_development_controller_and_center_of_trade_state() {
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
        center_of_trade=3
        base_tax=12.345
        base_production=7.890
        base_manpower=4.250
        devastation=12.345
        prosperity=75.500
        mothball_status=0
    }
    -2={
        owner="AAA"
        mothball_status=1
    }
    -3={
        owner="AAA"
        mothball_status=2
    }
    -4={
        owner="AAA"
    }
    -999={
        center_of_trade=1
        base_tax=99.000
        base_production=88.000
        base_manpower=77.000
        mothball_status=2
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
            save.external_state.province_metric_values[&1]["devastation"],
            Some(12_345)
        );
        assert_eq!(
            save.external_state.province_metric_values[&1]["prosperity"],
            Some(75_500)
        );
        assert!(!save.external_state.province_metric_values.contains_key(&2));
        assert_eq!(save.external_state.province_mothball_status[&1], 0);
        assert_eq!(save.external_state.province_mothball_status[&2], 1);
        assert_eq!(save.external_state.province_mothball_status[&3], 2);
        assert_eq!(save.external_state.province_mothball_status[&4], 1);
        assert_eq!(save.external_state.province_mothball_status[&999], 2);
        assert_eq!(
            save.external_state.province_controllers[&1].as_deref(),
            Some("AAA")
        );
        assert!(!save.external_state.province_controllers.contains_key(&2));
        assert_eq!(
            save.external_state.province_center_of_trade_levels[&1],
            Some(3)
        );
        assert_eq!(
            save.external_state.province_center_of_trade_levels[&2],
            Some(0)
        );
        assert_eq!(
            save.external_state.province_center_of_trade_levels[&999],
            Some(1)
        );

        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world");
        let content = ContentSet::resolve(vec![ContentSource::base(
            ContentRoot::open(fixture).unwrap(),
        )])
        .unwrap();
        let baseline = crate::VanillaWorld::load_from_content(&content, "1444.11.11").unwrap();
        assert_eq!(baseline.province_mothball_status(1), Some(1));
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
        assert_eq!(
            filtered.province_metric_values[&1]["devastation"],
            Some(12_345)
        );
        assert_eq!(
            filtered.province_metric_values[&1]["prosperity"],
            Some(75_500)
        );
        assert!(!filtered.province_metric_values.contains_key(&2));
        assert!(!filtered.province_metric_values.contains_key(&999));
        assert_eq!(filtered.province_controllers[&1].as_deref(), Some("AAA"));
        assert!(filtered.province_controllers.contains_key(&2));
        assert_eq!(filtered.province_controllers[&2], None);
        assert_eq!(filtered.province_center_of_trade_levels[&1], Some(3));
        assert_eq!(filtered.province_center_of_trade_levels[&2], Some(0));
        assert!(!filtered.province_center_of_trade_levels.contains_key(&999));
        assert_eq!(filtered.province_mothball_status[&1], 0);
        assert_eq!(filtered.province_mothball_status[&2], 1);
        assert_eq!(filtered.province_mothball_status[&3], 2);
        assert!(!filtered.province_mothball_status.contains_key(&999));

        let loaded =
            crate::VanillaWorld::load_from_content_with_save_state(&content, &save).unwrap();
        assert_eq!(loaded.province_mothball_status(1), Some(0));
        assert_eq!(loaded.province_mothball_status(2), Some(1));
        assert_eq!(loaded.province_mothball_status(3), Some(2));
        assert_eq!(loaded.province_mothball_status(999), None);
        let province = loaded.initial.province(1).unwrap();
        assert_eq!(province.tax_milli, Some(12_345));
        assert_eq!(province.production_milli, Some(7_890));
        assert_eq!(province.manpower_milli, Some(4_250));
        assert_eq!(province.controller.as_deref(), Some("AAA"));
        assert_eq!(
            loaded.initial.province_center_of_trade_level(1).unwrap(),
            Some(3)
        );
        assert_eq!(
            loaded.initial.province_center_of_trade_level(2).unwrap(),
            Some(0)
        );
        assert_eq!(loaded.initial.province(2).unwrap().controller, None);
        assert!(loaded.initial.province(999).is_none());
        let province_one = loaded.initial.province_entity(1).unwrap();
        assert_eq!(
            loaded.initial.world().metrics().entities[&province_one]["devastation"],
            Some(12_345)
        );
        assert_eq!(
            loaded.initial.world().metrics().entities[&province_one]["prosperity"],
            Some(75_500)
        );
        let province_two = loaded.initial.province_entity(2).unwrap();
        let baseline_devastation = baseline
            .initial
            .world()
            .metrics()
            .entities
            .get(&province_two)
            .and_then(|metrics| metrics.get("devastation"))
            .copied();
        assert_eq!(
            loaded
                .initial
                .world()
                .metrics()
                .entities
                .get(&province_two)
                .and_then(|metrics| metrics.get("devastation"))
                .copied(),
            baseline_devastation
        );
        assert_eq!(
            loaded.initial.world().metrics().entities[&province_two]["prosperity"],
            None
        );

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
    fn rejects_saved_province_metrics_outside_supported_range() {
        let directory = tempfile::tempdir().unwrap();
        for (metric, value) in [
            ("devastation", "100.001"),
            ("prosperity", "100.001"),
            ("prosperity", "-0.001"),
        ] {
            let path = directory.path().join(format!("invalid-{metric}.eu4"));
            std::fs::write(
                &path,
                format!(
                    r#"EU4txt
date=1444.11.11
countries={{ AAA={{ human=yes }} }}
provinces={{
    -1={{ {metric}={value} }}
}}
"#
                ),
            )
            .unwrap();

            let error = read_vanilla_save_state(&path).unwrap_err();

            assert!(
                error
                    .to_string()
                    .contains(&format!("saved {metric} outside 0..100")),
                "unexpected error: {error}"
            );
        }
    }

    #[test]
    #[ignore = "requires EU4_INSTALL_DIR and EU4_TEST_SAVE for a local EU4 1.37.5 install"]
    fn local_eu4_install_and_save_load_center_of_trade_levels() {
        let install = std::env::var_os("EU4_INSTALL_DIR")
            .expect("set EU4_INSTALL_DIR to the local Europa Universalis IV install");
        let save_path =
            std::env::var_os("EU4_TEST_SAVE").expect("set EU4_TEST_SAVE to a local EU4 text save");
        let save = read_vanilla_save_state(save_path).unwrap();
        assert_eq!(
            save.external_state
                .province_center_of_trade_levels
                .get(&101),
            Some(&Some(3))
        );

        let content = ContentSet::from_installation(
            Path::new(&install),
            &[],
            None,
            None,
            crate::TextEncoding::Utf8,
        )
        .unwrap();
        let loaded = VanillaWorld::load_from_content_with_save_state(&content, &save).unwrap();
        assert_eq!(
            loaded.initial.province_center_of_trade_level(101).unwrap(),
            Some(3)
        );
    }

    #[test]
    #[ignore = "requires EU4_TEST_SAVE with a local EU4 text save"]
    fn local_eu4_save_preserves_regiment_morale_and_home_province() {
        let save_path =
            std::env::var_os("EU4_TEST_SAVE").expect("set EU4_TEST_SAVE to a local EU4 text save");
        let save = read_vanilla_save_state(save_path).unwrap();
        assert!(!save.external_state.armies.is_empty());
        assert!(
            save.external_state
                .armies
                .iter()
                .all(|army| { army.regiments.len() == army.regiment_states.len() })
        );
        assert!(save.external_state.armies.iter().any(|army| {
            army.regiment_states
                .iter()
                .any(|state| state.morale_milli.is_some())
        }));
        assert!(save.external_state.armies.iter().any(|army| {
            army.regiment_states
                .iter()
                .any(|state| state.home_province.is_some())
        }));
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
                    ""
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
    fn saved_government_reforms_reject_duplicate_nonempty_identifiers() {
        let directory = tempfile::tempdir().unwrap();
        let save_path = directory.path().join("duplicate-reforms.eu4");
        std::fs::write(
            &save_path,
            r#"EU4txt
date=1444.11.11
countries={
    AAA={
        government={
            reform_stack={
                reforms={
                    "test_council"
                    "test_council"
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

        let error = match read_vanilla_save_state(&save_path) {
            Ok(_) => panic!("duplicate government reforms were accepted"),
            Err(error) => error.to_string(),
        };
        assert!(
            error.contains("duplicate saved government reform identifier"),
            "{error}"
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
    fn saved_province_buildings_replace_content_state_and_validate_definitions() {
        let directory = tempfile::tempdir().unwrap();
        let save_path = directory.path().join("province-buildings.eu4");
        std::fs::write(
            &save_path,
            r#"EU4txt
date=1444.11.11
countries={ AAA={ human=yes } }
provinces={
    -1={ owner="AAA" buildings={ test_market=yes test_fort=no } }
    -2={ owner="AAA" buildings={} }
    -3={ owner="AAA" }
    -999={ buildings={ missing_building=yes } }
}
"#,
        )
        .unwrap();

        let save = read_vanilla_save_state(&save_path).unwrap();
        assert_eq!(
            save.external_state.province_buildings[&1],
            BTreeSet::from(["test_market".into()])
        );
        assert!(save.external_state.province_buildings[&2].is_empty());
        assert!(save.external_state.province_buildings[&3].is_empty());
        assert!(save.external_state.province_buildings[&999].contains("missing_building"));

        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world");
        let content = ContentSet::resolve(vec![ContentSource::base(
            ContentRoot::open(fixture).unwrap(),
        )])
        .unwrap();
        let baseline = crate::VanillaWorld::load_from_content(&content, "1444.11.11").unwrap();
        let filtered = save.state_for_world(&baseline).unwrap();
        assert_eq!(
            filtered.province_buildings[&1],
            BTreeSet::from(["test_market".into()])
        );
        assert!(filtered.province_buildings[&2].is_empty());
        assert!(filtered.province_buildings[&3].is_empty());
        assert!(!filtered.province_buildings.contains_key(&999));

        let loaded =
            crate::VanillaWorld::load_from_content_with_save_state(&content, &save).unwrap();
        let market_province = loaded.initial.province_entity(1).unwrap();
        let empty_province = loaded.initial.province_entity(2).unwrap();
        let omitted_buildings_province = loaded.initial.province_entity(3).unwrap();
        assert_eq!(
            loaded.initial.world().economy().provinces[&market_province].buildings,
            BTreeSet::from(["test_market".into()])
        );
        assert!(
            loaded.initial.world().economy().provinces[&empty_province]
                .buildings
                .is_empty()
        );
        assert!(
            loaded.initial.world().economy().provinces[&omitted_buildings_province]
                .buildings
                .is_empty()
        );
        let world = loaded.initial.world();
        let tax = world
            .attributes()
            .evaluate(world, market_province, "tax_basis")
            .unwrap();
        assert!(tax.steps[0].contributions.iter().any(|contribution| {
            contribution.source == "building:test_market"
                && contribution.property == "local_tax_modifier"
        }));

        let snapshot = loaded.save().unwrap();
        let mut resumed =
            crate::VanillaWorld::load_from_content_with_save_state(&content, &save).unwrap();
        resumed.restore(&snapshot).unwrap();
        assert_eq!(
            resumed.initial.world().economy().provinces[&market_province].buildings,
            BTreeSet::from(["test_market".into()])
        );

        let mut invalid = save.clone();
        invalid
            .external_state
            .province_buildings
            .insert(1, BTreeSet::from(["missing_building".into()]));
        assert!(invalid.state_for_world(&baseline).is_err());
    }

    #[test]
    fn saved_province_cores_replace_history_and_preserve_unknown_core_types() {
        let directory = tempfile::tempdir().unwrap();
        let save_path = directory.path().join("province-cores.eu4");
        std::fs::write(
            &save_path,
            r#"EU4txt
date=1444.11.11
countries={ AAA={ human=yes } BBB={} }
provinces={
    -1={ owner="AAA" cores={ BBB AAA } }
    -2={ owner="AAA" cores={} }
    -999={ cores={ ZZZ } }
}
"#,
        )
        .unwrap();

        let save = read_vanilla_save_state(&save_path).unwrap();
        assert_eq!(
            save.external_state.province_cores[&1],
            ["BBB".to_string(), "AAA".to_string()]
        );
        assert!(save.external_state.province_cores[&2].is_empty());

        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world");
        let content = ContentSet::resolve(vec![ContentSource::base(
            ContentRoot::open(fixture).unwrap(),
        )])
        .unwrap();
        let baseline = crate::VanillaWorld::load_from_content(&content, "1444.11.11").unwrap();
        let filtered = save.state_for_world(&baseline).unwrap();
        assert_eq!(
            filtered.province_cores[&1],
            ["BBB".to_string(), "AAA".to_string()]
        );
        assert!(filtered.province_cores[&2].is_empty());
        assert!(!filtered.province_cores.contains_key(&999));

        let loaded =
            crate::VanillaWorld::load_from_content_with_save_state(&content, &save).unwrap();
        let north = loaded.initial.province_entity(1).unwrap();
        let south = loaded.initial.province_entity(2).unwrap();
        assert_eq!(
            loaded.initial.province(1).unwrap().owner.as_deref(),
            Some("AAA")
        );
        assert_eq!(
            loaded.initial.province(1).unwrap().cores,
            ["BBB".to_string(), "AAA".to_string()]
        );
        let administration = loaded.initial.world().administration();
        let saved_cores = &administration.provinces[&north];
        assert!(saved_cores.cores_known);
        assert_eq!(saved_cores.cores.len(), 2);
        assert_eq!(
            saved_cores.cores[&loaded.initial.country_entity("BBB").unwrap()],
            augustus_core::CoreType::Unspecified
        );
        assert!(administration.provinces[&south].cores_known);
        assert!(administration.provinces[&south].cores.is_empty());

        let mut invalid = save.clone();
        invalid
            .external_state
            .province_cores
            .insert(1, vec!["ZZZ".into()]);
        assert!(invalid.state_for_world(&baseline).is_err());
    }

    #[test]
    fn saved_trade_node_currents_bind_to_known_nodes_and_survive_restore() {
        let directory = tempfile::tempdir().unwrap();
        let save_path = directory.path().join("trade-node-values.eu4");
        std::fs::write(
            &save_path,
            r#"EU4txt
date=1444.11.11
player=AAA
countries={
    AAA={ human=yes navy_tradition=9.000 }
}
provinces={
    -1={ owner="AAA" }
}
trade={
    node={
        definitions=river
        current=1.234
        local_value=5.945
        outgoing=5.626
        value_added_outgoing=0.416
        retention=0.744
        p_pow=36.912
        max=163.575
        collector_power=135.369
        collector_power_including_pirates=136.369
        pull_power=46.674
        retain_power=135.369
        highest_power=14.880
    }
    node={ definitions=harbor current=0.000 }
    node={ definitions=missing_node current=9.999 }
}
"#,
        )
        .unwrap();

        let save = read_vanilla_save_state_and_trade_node_runtime_states(&save_path).unwrap();
        assert_eq!(
            save.state.external_state.country_metric_values["AAA"]["navy_tradition"],
            Some(9_000)
        );
        let ordered =
            read_vanilla_save_state_and_ordered_trade_node_runtime_states(&save_path).unwrap();
        assert_eq!(
            ordered.trade_node_order,
            ["river", "harbor", "missing_node"]
        );
        assert_eq!(save.current_trade_node_values_milli["river"], Some(1_234));
        assert_eq!(save.current_trade_node_values_milli["harbor"], Some(0));
        assert_eq!(
            save.current_trade_node_values_milli["missing_node"],
            Some(9_999)
        );
        let saved_wien = &save.trade_node_runtime_states["river"];
        assert_eq!(saved_wien.local_value_milli, Some(5_945));
        assert_eq!(saved_wien.outgoing_milli, Some(5_626));
        assert_eq!(saved_wien.value_added_outgoing_milli, Some(416));
        assert_eq!(saved_wien.retention_milli, Some(744));
        assert_eq!(saved_wien.p_pow_milli, Some(36_912));
        assert_eq!(saved_wien.max_milli, Some(163_575));
        assert_eq!(saved_wien.collector_power_milli, Some(135_369));
        assert_eq!(
            saved_wien.collector_power_including_pirates_milli,
            Some(136_369)
        );
        assert_eq!(saved_wien.pull_power_milli, Some(46_674));
        assert_eq!(saved_wien.retain_power_milli, Some(135_369));
        assert_eq!(saved_wien.highest_power_milli, Some(14_880));
        let current_only = read_vanilla_save_state_and_trade_nodes(&save_path).unwrap();
        assert_eq!(
            current_only.current_trade_node_values_milli["river"],
            Some(1_234)
        );
        assert_eq!(current_only.state.date, save.state.date);

        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world");
        let content = ContentSet::resolve(vec![ContentSource::base(
            ContentRoot::open(fixture).unwrap(),
        )])
        .unwrap();
        let baseline = crate::VanillaWorld::load_from_content(&content, "1444.11.11").unwrap();
        let bound_state = save.state.state_for_world(&baseline).unwrap();
        assert_eq!(
            bound_state.country_metric_values["AAA"]["navy_tradition"],
            Some(9_000)
        );
        assert_eq!(
            ordered.trade_node_order_for_world(&baseline),
            ["river", "harbor"]
        );
        let trade_nodes = save.trade_node_current_values_for_world(&baseline);
        assert_eq!(trade_nodes.len(), 2);
        assert_eq!(trade_nodes["river"], Some(1_234));
        assert_eq!(trade_nodes["harbor"], Some(0));
        assert!(!trade_nodes.contains_key("missing_node"));
        let trade_node_states = save.trade_node_runtime_states_for_world(&baseline);
        assert_eq!(trade_node_states.len(), 2);
        assert_eq!(trade_node_states["river"].local_value_milli, Some(5_945));

        let mut loaded =
            crate::VanillaWorld::load_from_content_with_save_state(&content, &save.state).unwrap();
        loaded
            .set_vanilla_trade_node_runtime_states(&trade_node_states)
            .unwrap();
        assert_eq!(
            loaded
                .initial
                .world()
                .trade_node_current_value_milli("river"),
            Some(Some(1_234))
        );
        assert_eq!(loaded.province_trade_node_current_milli(1), Some(1_234));

        let snapshot = loaded.save().unwrap();
        loaded.restore(&snapshot).unwrap();
        assert_eq!(loaded.province_trade_node_current_milli(1), Some(1_234));
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
    fn external_province_metric_overlay_merges_by_name() {
        let mut base = crate::VanillaExternalState {
            province_metric_values: BTreeMap::from([(
                1,
                BTreeMap::from([
                    ("devastation".into(), Some(10_000)),
                    ("prosperity".into(), Some(20_000)),
                ]),
            )]),
            ..Default::default()
        };
        let overlay = crate::VanillaExternalState {
            province_metric_values: BTreeMap::from([(
                1,
                BTreeMap::from([("devastation".into(), Some(25_000))]),
            )]),
            ..Default::default()
        };

        base.apply_overlay(&overlay);

        assert_eq!(
            base.province_metric_values[&1],
            BTreeMap::from([
                ("devastation".into(), Some(25_000)),
                ("prosperity".into(), Some(20_000)),
            ])
        );
    }

    #[test]
    fn empty_saved_building_overlay_clears_the_existing_province_set() {
        let mut base = crate::VanillaExternalState {
            province_buildings: BTreeMap::from([(1, BTreeSet::from(["test_fort".into()]))]),
            ..Default::default()
        };
        let overlay = crate::VanillaExternalState {
            province_buildings: BTreeMap::from([(1, BTreeSet::new())]),
            ..Default::default()
        };

        base.apply_overlay(&overlay);

        assert!(base.province_buildings[&1].is_empty());
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
        cores={ AAA }
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
        assert_eq!(state.external_state.province_cores[&1], ["AAA".to_string()]);
    }

    #[test]
    fn reads_startup_state_from_binary_save_with_player_token_map() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("state.eu4");
        let token_map_path = binary_token_map(directory.path());
        std::fs::write(&path, binary_startup_save()).unwrap();

        let state = read_vanilla_save_state_with_token_map(&path, &token_map_path).unwrap();

        assert_eq!(state.date, "1444.11.11");
        assert_eq!(
            state.external_state.country_resources["AAA"].treasury_milli,
            Some(12_345)
        );
    }

    #[test]
    fn reads_binary_save_with_custom_token_and_lookup_resolver() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("state.eu4");
        std::fs::write(&path, binary_startup_save_with_lookup()).unwrap();

        let save =
            read_vanilla_save_state_and_trade_nodes_with_token_resolver(&path, TestTokenResolver)
                .unwrap();

        assert_eq!(save.state.date, "1444.11.11");
        assert_eq!(save.state.player.as_deref(), Some("AAA"));
        assert!(
            save.state
                .external_state
                .country_resources
                .contains_key("AAA")
        );
        assert!(save.current_trade_node_values_milli.is_empty());
    }

    #[test]
    fn reports_missing_token_map_for_binary_save() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("state.eu4");
        std::fs::write(&path, binary_startup_save()).unwrap();

        let error = read_vanilla_save_state(&path).unwrap_err();

        assert!(error.to_string().contains("requires a local token map"));
    }

    #[test]
    fn reads_zip_wrapped_binary_save_with_player_token_map() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("state.eu4");
        let token_map_path = binary_token_map(directory.path());
        let mut archive = ZipWriter::new(File::create(&path).unwrap());
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        archive.start_file("gamestate", options).unwrap();
        archive.write_all(&binary_startup_save()).unwrap();
        archive.start_file("meta", options).unwrap();
        archive.write_all(&binary_startup_save()).unwrap();
        archive.finish().unwrap();

        let state = read_vanilla_save_state_with_token_map(&path, &token_map_path).unwrap();

        assert_eq!(state.date, "1444.11.11");
        assert_eq!(
            state.external_state.country_resources["AAA"].treasury_milli,
            Some(12_345)
        );
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
