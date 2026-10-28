// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::BTreeSet;

use augustus_core::{NavalConstructionId, NavalConstructionInput, PaidNavalConstructionInput};
use serde::{Deserialize, Serialize};

use crate::{Error, VanillaWorld};

/// A caller-priced naval order with an explicit absolute core completion day.
///
/// This request is not a price quote or an EU4 construction-rule translation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaNavalConstructionRequest {
    pub country: String,
    pub province_id: u32,
    pub unit_type: String,
    pub completion_day: u64,
    pub cost_milli: i64,
}

impl VanillaWorld {
    /// Atomically applies caller-supplied costs and enqueues supported naval orders.
    ///
    /// Orders require the loaded country, the province's current core owner, its
    /// save-derived controller, a known port, and a loaded naval template. A
    /// province may have at most one pending order through this adapter.
    pub fn submit_naval_constructions(
        &mut self,
        requests: &[VanillaNavalConstructionRequest],
    ) -> Result<Vec<NavalConstructionId>, Error> {
        let world = self.initial.world();
        let pending_locations = world
            .naval_constructions()
            .iter()
            .map(|order| order.location)
            .collect::<BTreeSet<_>>();
        let mut requested_locations = BTreeSet::new();
        let mut inputs = Vec::with_capacity(requests.len());

        for request in requests {
            let invalid = |message: String| Error::message("naval construction request", message);
            if let Some(unresolved) = self
                .naval_construction_import_report()
                .unresolved
                .iter()
                .find(|entry| entry.province_id.is_none())
            {
                return Err(invalid(format!(
                    "cannot submit naval orders while construction import has a global unresolved state: {}",
                    unresolved.reason
                )));
            }
            if let Some(unresolved) = self
                .naval_construction_import_report()
                .unresolved
                .iter()
                .find(|entry| entry.province_id == Some(request.province_id))
            {
                return Err(invalid(format!(
                    "province {} has an unresolved existing military construction: {}",
                    request.province_id, unresolved.reason
                )));
            }
            let owner = self
                .initial
                .country_entity(&request.country)
                .ok_or_else(|| invalid(format!("unknown country {}", request.country)))?;
            let location = self
                .initial
                .province_entity(request.province_id)
                .ok_or_else(|| invalid(format!("unknown province {}", request.province_id)))?;
            let province = self
                .initial
                .province(request.province_id)
                .expect("province entity and initial province index agree");
            let live_owner = self
                .initial
                .world()
                .entity(location)
                .map_err(|error| Error::caused_by("naval construction request", error))?
                .owner;
            if live_owner != Some(owner)
                || province.controller.as_deref() != Some(request.country.as_str())
            {
                return Err(invalid(format!(
                    "province {} owner and controller must match country {}",
                    request.province_id, request.country
                )));
            }
            let has_port = self
                .initial
                .world()
                .province_has_port(location)
                .map_err(|error| Error::caused_by("naval construction request", error))?;
            if !has_port {
                return Err(invalid(format!(
                    "province {} is not a known port",
                    request.province_id
                )));
            }
            let Some(template) = self
                .initial
                .world()
                .unit_templates()
                .definitions
                .get(&request.unit_type)
            else {
                return Err(invalid(format!(
                    "unknown naval unit template {}",
                    request.unit_type
                )));
            };
            if !matches!(
                template.category.as_str(),
                "heavy_ship" | "light_ship" | "galley" | "transport"
            ) {
                return Err(invalid(format!(
                    "unit template {} is not naval",
                    request.unit_type
                )));
            }
            if pending_locations.contains(&location) || !requested_locations.insert(location) {
                return Err(invalid(format!(
                    "province {} already has a pending naval order",
                    request.province_id
                )));
            }
            inputs.push(PaidNavalConstructionInput {
                construction: NavalConstructionInput {
                    owner,
                    location,
                    unit_type: request.unit_type.clone(),
                    completion_day: request.completion_day,
                },
                cost_milli: request.cost_milli,
            });
        }

        self.initial
            .enqueue_paid_naval_constructions(&inputs)
            .map_err(|error| Error::caused_by("naval construction request", error))
    }
}
