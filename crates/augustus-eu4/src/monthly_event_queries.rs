// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

//! Read-only introspection for translated monthly pulse configuration.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaMonthlyEventQuery {
    pub country: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VanillaMonthlyEventQueryReport {
    pub country: String,
    pub events: Vec<String>,
    pub base: Option<&'static str>,
    pub system_id: Option<String>,
    pub active: bool,
}

impl crate::VanillaWorld {
    /// Reports monthly pulse configuration without executing events,
    /// advancing the calendar, or consuming randomness.
    pub fn query_vanilla_monthly_event(
        &self,
        query: &VanillaMonthlyEventQuery,
    ) -> Result<VanillaMonthlyEventQueryReport, crate::Error> {
        let entity = self.initial.country_entity(&query.country).ok_or_else(|| {
            crate::Error::message("monthly event query", "unknown monthly event country")
        })?;
        let events = self
            .monthly_event_rules()
            .and_then(|rules| rules.countries.iter().find(|entry| entry.country == entity))
            .into_iter()
            .flat_map(|entry| entry.groups.iter())
            .flat_map(|group| group.candidates.iter())
            .filter_map(|candidate| candidate.event.clone())
            .collect();
        let system_id = self.monthly_event_system_id().map(str::to_owned);
        let active = self.monthly_event_rules().is_some()
            && system_id.as_deref() == self.initial.world().monthly_system_id();
        Ok(VanillaMonthlyEventQueryReport {
            country: query.country.clone(),
            events,
            base: self.monthly_event_base(),
            system_id,
            active,
        })
    }
}
