// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{EntityId, EntityKind, Error, World};

/// The known discovery roster for one province.
///
/// Countries listed in `discovered_by` know the province. Countries listed in
/// `unresolved_countries` have no authoritative result yet. Other countries
/// are known not to have discovered it when this record is present.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProvinceDiscovery {
    pub discovered_by: BTreeSet<EntityId>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub unresolved_countries: BTreeSet<EntityId>,
}

/// Country-to-province map knowledge, independent of game-file syntax.
/// A missing province record means that its discovery state is unknown.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapKnowledge {
    pub provinces: BTreeMap<EntityId, ProvinceDiscovery>,
}

impl MapKnowledge {
    pub fn is_empty(&self) -> bool {
        self.provinces.is_empty()
    }

    pub fn discovered_by(&self, province: EntityId, country: EntityId) -> Option<bool> {
        let state = self.provinces.get(&province)?;
        if state.discovered_by.contains(&country) {
            Some(true)
        } else if state.unresolved_countries.contains(&country) {
            None
        } else {
            Some(false)
        }
    }

    pub(crate) fn validate(&self, world: &World) -> Result<(), Error> {
        for (&province, state) in &self.provinces {
            if world.entity(province)?.kind != EntityKind::Province {
                return Err(Error::Invalid(
                    "map knowledge requires province entities".into(),
                ));
            }
            for &country in state
                .discovered_by
                .iter()
                .chain(state.unresolved_countries.iter())
            {
                if world.entity(country)?.kind != EntityKind::Country {
                    return Err(Error::Invalid(
                        "map knowledge requires country entities".into(),
                    ));
                }
            }
            if state
                .discovered_by
                .iter()
                .any(|country| state.unresolved_countries.contains(country))
            {
                return Err(Error::Invalid(
                    "map knowledge cannot be both known and unresolved".into(),
                ));
            }
        }
        Ok(())
    }
}
