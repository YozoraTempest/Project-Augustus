// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::BTreeMap;

use crate::{EntityId, EntityKind, Error, Target, World};

/// Derived from owner edges and explicit absence declarations; never persisted.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct OwnershipIndex {
    counts: BTreeMap<EntityId, usize>,
    unknown: usize,
}

impl OwnershipIndex {
    pub(crate) fn add_province(&mut self) {
        self.unknown += 1;
    }

    pub(crate) fn resolve_absence(&mut self) {
        self.unknown -= 1;
    }

    pub(crate) fn assign(&mut self, previous: Option<EntityId>, unknown: bool, country: EntityId) {
        if let Some(previous) = previous {
            let count = self.counts.get_mut(&previous).expect("indexed owner");
            *count -= 1;
            if *count == 0 {
                self.counts.remove(&previous);
            }
        } else if unknown {
            self.resolve_absence();
        }
        *self.counts.entry(country).or_default() += 1;
    }
}

impl World {
    /// Positive ownership is known immediately; zero remains unknown when a
    /// province's owner has not been supplied or explicitly declared absent.
    pub fn has_owned_province(&self, country: EntityId) -> Result<bool, Error> {
        if self.entity(country)?.kind != EntityKind::Country {
            return Err(Error::Invalid(
                "province ownership query requires a country".into(),
            ));
        }
        if self.ownership.counts.contains_key(&country) {
            Ok(true)
        } else if self.ownership.unknown == 0 {
            Ok(false)
        } else {
            Err(Error::Invalid(
                "unknown province ownership prevents an absence query".into(),
            ))
        }
    }

    pub(crate) fn rebuild_ownership(&mut self) {
        let mut index = OwnershipIndex::default();
        for (id, entity) in self.entities().iter().enumerate() {
            if entity.kind != EntityKind::Province {
                continue;
            }
            if let Some(country) = entity.owner {
                *index.counts.entry(country).or_default() += 1;
            } else if !self
                .known_absent_targets()
                .get(&EntityId(id))
                .is_some_and(|targets| targets.contains(&Target::Owner))
            {
                index.add_province();
            }
        }
        self.ownership = index;
    }
}
