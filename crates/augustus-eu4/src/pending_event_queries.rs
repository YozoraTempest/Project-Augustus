// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

//! Read-only introspection for pending EU4 event choices.

use augustus_core::PendingEventId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaPendingEventQuery {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VanillaPendingEventReport {
    pub id: u64,
    pub event: String,
    pub title: String,
    pub description: Option<String>,
    pub current: augustus_core::EntityId,
    pub current_name: String,
    pub day: u64,
    pub picture: Option<String>,
    pub location: Option<String>,
    pub options: Vec<String>,
    pub option_labels: BTreeMap<String, String>,
    pub option_locations: BTreeMap<String, Option<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VanillaPendingEventQueryReport {
    pub pending_events: Vec<VanillaPendingEventReport>,
}

impl crate::VanillaWorld {
    /// Reports pending event choices and localized presentation metadata
    /// without selecting an option or changing the world.
    pub fn query_vanilla_pending_events(
        &self,
        query: &VanillaPendingEventQuery,
    ) -> Result<VanillaPendingEventQueryReport, crate::Error> {
        let country = query
            .country
            .as_deref()
            .map(|tag| {
                self.initial.country_entity(tag).ok_or_else(|| {
                    crate::Error::message("pending event query", "unknown pending event country")
                })
            })
            .transpose()?;
        let pending_id = query.pending.map(PendingEventId);
        if let Some(id) = pending_id
            && !self
                .initial
                .world()
                .pending_events()
                .iter()
                .any(|pending| pending.id == id)
        {
            return Err(crate::Error::message(
                "pending event query",
                "unknown pending event",
            ));
        }

        let world = self.initial.world();
        let mut pending_events = Vec::new();
        for pending in world.pending_events() {
            if pending_id.is_some_and(|id| id != pending.id)
                || country.is_some_and(|entity| entity != pending.current)
            {
                continue;
            }
            let current = world
                .entity(pending.current)
                .map_err(|error| crate::Error::caused_by("pending event query", error))?;
            let options = world
                .available_event_options(pending.id)
                .map_err(|error| crate::Error::caused_by("pending event query", error))?;
            let event_text = self.event_text(&pending.event);
            let location = world
                .pending_event_location(pending.id)
                .map_err(|error| crate::Error::caused_by("pending event query", error))?
                .map(|entity| world.entity(entity).map(|state| state.name.clone()))
                .transpose()
                .map_err(|error| crate::Error::caused_by("pending event query", error))?;
            let mut option_labels = BTreeMap::new();
            let mut option_locations = BTreeMap::new();
            for option in &options {
                option_labels.insert(
                    option.id.clone(),
                    event_text
                        .and_then(|text| text.option_labels.get(&option.id))
                        .cloned()
                        .unwrap_or_else(|| option.id.clone()),
                );
                option_locations.insert(
                    option.id.clone(),
                    world
                        .pending_event_option_location(pending.id, &option.id)
                        .map_err(|error| crate::Error::caused_by("pending event query", error))?
                        .map(|entity| world.entity(entity).map(|state| state.name.clone()))
                        .transpose()
                        .map_err(|error| crate::Error::caused_by("pending event query", error))?,
                );
            }
            pending_events.push(VanillaPendingEventReport {
                id: pending.id.0,
                event: pending.event.clone(),
                title: event_text
                    .map(|text| text.title.clone())
                    .unwrap_or_else(|| pending.event.clone()),
                description: event_text.and_then(|text| text.description.clone()),
                current: pending.current,
                current_name: current.name.clone(),
                day: pending.day,
                picture: world
                    .pending_event_picture(pending.id)
                    .map_err(|error| crate::Error::caused_by("pending event query", error))?
                    .map(str::to_owned),
                location,
                options: options
                    .into_iter()
                    .map(|option| option.id.clone())
                    .collect(),
                option_labels,
                option_locations,
            });
        }
        Ok(VanillaPendingEventQueryReport { pending_events })
    }
}
