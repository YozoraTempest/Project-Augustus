// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

//! Read-only introspection for translated yearly pulse configuration.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaYearlyEventQuery {
    pub country: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VanillaYearlyEventQueryReport {
    pub country: String,
    pub events: Vec<String>,
    pub system_id: Option<String>,
    pub active: bool,
}

impl crate::VanillaWorld {
    /// Reports yearly pulse configuration without executing events,
    /// advancing the calendar, or consuming randomness.
    pub fn query_vanilla_yearly_event(
        &self,
        query: &VanillaYearlyEventQuery,
    ) -> Result<VanillaYearlyEventQueryReport, crate::Error> {
        let entity = self.initial.country_entity(&query.country).ok_or_else(|| {
            crate::Error::message("yearly event query", "unknown yearly event country")
        })?;
        let rules = self.yearly_event_rules();
        let events = rules
            .and_then(|rules| rules.countries.iter().find(|entry| entry.country == entity))
            .into_iter()
            .flat_map(|entry| entry.groups.iter())
            .flat_map(|group| group.candidates.iter())
            .filter_map(|candidate| candidate.event.clone())
            .collect();
        let system_id = rules.map(|rules| rules.id.clone());
        let active = rules.is_some_and(|rules| {
            Some(rules.id.as_str()) == self.initial.world().yearly_system_id()
        });
        Ok(VanillaYearlyEventQueryReport {
            country: query.country.clone(),
            events,
            system_id,
            active,
        })
    }
}
