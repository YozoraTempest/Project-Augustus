// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use serde::{Deserialize, Serialize};

use crate::{EntityId, EntityKind, Error, Target, World};

/// Static province adjacency supplied by a ruleset, with incomplete map edges
/// kept explicit so ownership-derived regions are never guessed.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProvinceConnectivity {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    adjacent: BTreeMap<EntityId, BTreeSet<EntityId>>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    uncertain_adjacency: BTreeSet<EntityId>,
}

impl ProvinceConnectivity {
    pub fn new(
        edges: impl IntoIterator<Item = (EntityId, EntityId)>,
        uncertain_adjacency: impl IntoIterator<Item = EntityId>,
    ) -> Self {
        let mut connectivity = Self {
            adjacent: BTreeMap::new(),
            uncertain_adjacency: uncertain_adjacency.into_iter().collect(),
        };
        for (left, right) in edges {
            connectivity.connect(left, right);
        }
        connectivity
    }

    fn connect(&mut self, left: EntityId, right: EntityId) {
        if left == right {
            return;
        }
        self.adjacent.entry(left).or_default().insert(right);
        self.adjacent.entry(right).or_default().insert(left);
    }

    pub(crate) fn validate(&self, world: &World) -> Result<(), Error> {
        for (&province, neighbors) in &self.adjacent {
            if world.entity(province)?.kind != EntityKind::Province {
                return Err(Error::Invalid(
                    "province connectivity requires province entities".into(),
                ));
            }
            if neighbors.contains(&province) {
                return Err(Error::Invalid(
                    "province connectivity has a self edge".into(),
                ));
            }
            for &neighbor in neighbors {
                if world.entity(neighbor)?.kind != EntityKind::Province
                    || !self
                        .adjacent
                        .get(&neighbor)
                        .is_some_and(|reverse| reverse.contains(&province))
                {
                    return Err(Error::Invalid(
                        "province connectivity must contain symmetric province edges".into(),
                    ));
                }
            }
        }
        for &province in &self.uncertain_adjacency {
            if world.entity(province)?.kind != EntityKind::Province {
                return Err(Error::Invalid(
                    "uncertain province connectivity requires province entities".into(),
                ));
            }
        }
        Ok(())
    }
}

impl World {
    pub fn province_connectivity(&self) -> Option<&ProvinceConnectivity> {
        self.province_connectivity.as_ref()
    }

    pub fn set_province_connectivity(
        &mut self,
        connectivity: ProvinceConnectivity,
    ) -> Result<(), Error> {
        connectivity.validate(self)?;
        self.province_connectivity = Some(connectivity);
        Ok(())
    }

    /// Returns the connected provinces with the seed's current owner. A
    /// missing graph or an unknown owner/edge keeps the result unresolved.
    pub fn province_owner_area(&self, province: EntityId) -> Result<Option<Vec<EntityId>>, Error> {
        if self.entity(province)?.kind != EntityKind::Province {
            return Err(Error::Invalid(
                "owner-area query requires a province".into(),
            ));
        }
        let Some(owner) = self.entity(province)?.owner else {
            return Ok(None);
        };
        let Some(connectivity) = &self.province_connectivity else {
            return Ok(None);
        };

        let mut pending = VecDeque::from([province]);
        let mut area = BTreeSet::new();
        while let Some(current) = pending.pop_front() {
            if !area.insert(current) {
                continue;
            }
            if connectivity.uncertain_adjacency.contains(&current) {
                return Ok(None);
            }
            if let Some(neighbors) = connectivity.adjacent.get(&current) {
                for &neighbor in neighbors {
                    let neighbor_state = self.entity(neighbor)?;
                    match neighbor_state.owner {
                        Some(neighbor_owner) if neighbor_owner == owner => {
                            pending.push_back(neighbor)
                        }
                        Some(_) => {}
                        None if self
                            .known_absent_targets
                            .get(&neighbor)
                            .is_some_and(|targets| targets.contains(&Target::Owner)) => {}
                        None => return Ok(None),
                    }
                }
            }
        }
        Ok(Some(area.into_iter().collect()))
    }
}
