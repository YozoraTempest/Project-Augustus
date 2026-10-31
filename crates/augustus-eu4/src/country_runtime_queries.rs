// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

//! Read-only queries for save-backed country mission and variable state.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

/// Select one country whose persistent mission and named-variable state should
/// be exposed. The query never refreshes mission objectives or mutates state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaCountryRuntimeQuery {
    pub country: String,
}

/// Save-backed mission and variable state for one country.
///
/// `None` means that the corresponding namespace has not been bound for this
/// country. An empty `Some` value is therefore meaningful and represents a
/// known empty namespace. Named values can contain known entries while
/// `variables_known` is false when the save reader marked the remainder of the
/// namespace as unknown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VanillaCountryRuntimeQueryReport {
    pub country: String,
    pub mission_ids: BTreeSet<String>,
    pub completed_missions: Option<BTreeSet<String>>,
    pub available_missions: Option<BTreeSet<String>>,
    pub variables: Option<BTreeMap<String, i64>>,
    pub variables_known: bool,
}

impl crate::VanillaWorld {
    /// Reads the current country runtime state without evaluating mission
    /// conditions, executing rewards, refreshing objectives, or changing the
    /// snapshot.
    pub fn query_vanilla_country_runtime(
        &self,
        query: &VanillaCountryRuntimeQuery,
    ) -> Result<VanillaCountryRuntimeQueryReport, crate::Error> {
        let entity = self.initial.country_entity(&query.country).ok_or_else(|| {
            crate::Error::message(
                "country runtime query",
                format!("unknown EU4 country {}", query.country),
            )
        })?;
        let world = self.initial.world();
        let completions = world.completions();
        let objectives = world.objectives();
        let named_values = world.named_values();

        let completed_missions = completions.entities.get(&entity).cloned();
        let available_missions = objectives.entities.get(&entity).cloned();
        let variables = named_values.entities.get(&entity).cloned();
        let variables_known =
            variables.is_some() && !named_values.unknown_entities.contains(&entity);

        Ok(VanillaCountryRuntimeQueryReport {
            country: query.country.clone(),
            mission_ids: completions.identifiers.clone(),
            completed_missions,
            available_missions,
            variables,
            variables_known,
        })
    }
}
