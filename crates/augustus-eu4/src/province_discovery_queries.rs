// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

//! Read-only queries for EU4 province discovery and map knowledge.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

/// Select one province and optionally one country whose discovery state should
/// be reported. The query never refreshes map knowledge or changes the world.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaProvinceDiscoveryQuery {
    pub province: u32,
    pub country: Option<String>,
}

/// Current map-knowledge state for one EU4 province.
///
/// A missing discovery record leaves `state_known` false and the filtered
/// result unresolved. When a record exists, countries are partitioned into
/// discovered, explicitly unresolved, and known undiscovered sets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VanillaProvinceDiscoveryQueryReport {
    pub province: u32,
    pub country: Option<String>,
    pub state_known: bool,
    pub discovered: Option<bool>,
    pub discovered_by: BTreeSet<String>,
    pub undiscovered_by: BTreeSet<String>,
    pub unresolved_by: BTreeSet<String>,
}

impl crate::VanillaWorld {
    /// Reads the current core map knowledge without recalculating discovery,
    /// executing events, consuming random numbers, or mutating the snapshot.
    pub fn query_vanilla_province_discovery(
        &self,
        query: &VanillaProvinceDiscoveryQuery,
    ) -> Result<VanillaProvinceDiscoveryQueryReport, crate::Error> {
        let province_entity = self
            .initial
            .province_entity(query.province)
            .ok_or_else(|| {
                crate::Error::message(
                    "province discovery query",
                    format!("unknown EU4 discovery province {}", query.province),
                )
            })?;
        let country_entity = query
            .country
            .as_deref()
            .map(|country| {
                self.initial.country_entity(country).ok_or_else(|| {
                    crate::Error::message(
                        "province discovery query",
                        format!("unknown EU4 discovery country {country}"),
                    )
                })
            })
            .transpose()?;
        let world = self.initial.world();
        let discovery = world.map_knowledge().provinces.get(&province_entity);
        let state_known = discovery.is_some();
        let discovered = match country_entity {
            Some(country) if state_known => world
                .province_discovered_by(province_entity, country)
                .map_err(|error| crate::Error::caused_by("province discovery query", error))?,
            _ => None,
        };

        let mut discovered_by = BTreeSet::new();
        let mut undiscovered_by = BTreeSet::new();
        let mut unresolved_by = BTreeSet::new();
        if let Some(discovery) = discovery {
            for country in self.initial.countries() {
                let entity = self
                    .initial
                    .country_entity(&country.id)
                    .expect("registered country");
                if discovery.discovered_by.contains(&entity) {
                    discovered_by.insert(country.id.clone());
                } else if discovery.unresolved_countries.contains(&entity) {
                    unresolved_by.insert(country.id.clone());
                } else {
                    undiscovered_by.insert(country.id.clone());
                }
            }
        }

        Ok(VanillaProvinceDiscoveryQueryReport {
            province: query.province,
            country: query.country.clone(),
            state_known,
            discovered,
            discovered_by,
            undiscovered_by,
            unresolved_by,
        })
    }
}
