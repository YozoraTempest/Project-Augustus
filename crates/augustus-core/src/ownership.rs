// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::{BTreeMap, BTreeSet};

use crate::{EntityId, EntityKind, Error, Target, World};

/// Members follow owner edges; acquisition order is retained by snapshots.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct OwnershipIndex {
    members: BTreeMap<EntityId, Vec<EntityId>>,
    unknown: usize,
}

impl OwnershipIndex {
    pub(crate) fn add_province(&mut self) {
        self.unknown += 1;
    }

    pub(crate) fn resolve_absence(&mut self) {
        self.unknown -= 1;
    }

    pub(crate) fn assign(
        &mut self,
        province: EntityId,
        previous: Option<EntityId>,
        unknown: bool,
        country: EntityId,
    ) {
        if previous == Some(country) {
            return;
        }
        if let Some(previous) = previous {
            let members = self.members.get_mut(&previous).expect("indexed owner");
            members.retain(|&member| member != province);
            if members.is_empty() {
                self.members.remove(&previous);
            }
        } else if unknown {
            self.resolve_absence();
        }
        self.members.entry(country).or_default().push(province);
    }

    pub(crate) fn order_overrides(&self) -> BTreeMap<EntityId, Vec<EntityId>> {
        self.members
            .iter()
            .filter(|(_, members)| !members.windows(2).all(|pair| pair[0] < pair[1]))
            .map(|(&country, members)| (country, members.clone()))
            .collect()
    }

    pub(crate) fn restore_order(
        &mut self,
        overrides: BTreeMap<EntityId, Vec<EntityId>>,
    ) -> Result<(), Error> {
        for (country, order) in overrides {
            let Some(members) = self.members.get_mut(&country) else {
                return Err(Error::Invalid("invalid saved owned province order".into()));
            };
            let unique: BTreeSet<_> = order.iter().copied().collect();
            if order.len() != members.len()
                || unique.len() != order.len()
                || unique != members.iter().copied().collect()
            {
                return Err(Error::Invalid("invalid saved owned province order".into()));
            }
            *members = order;
        }
        Ok(())
    }
}

impl World {
    /// Current members in acquisition order. Complete owner data is required
    /// because effect selection cannot safely omit an unknown member.
    pub fn owned_provinces(&self, country: EntityId) -> Result<&[EntityId], Error> {
        if self.entity(country)?.kind != EntityKind::Country {
            return Err(Error::Invalid(
                "province ownership query requires a country".into(),
            ));
        }
        if self.ownership.unknown != 0 {
            return Err(Error::Invalid(
                "unknown province ownership prevents a member selection".into(),
            ));
        }
        Ok(self
            .ownership
            .members
            .get(&country)
            .map(Vec::as_slice)
            .unwrap_or_default())
    }

    /// Positive ownership is known immediately; zero remains unknown when a
    /// province's owner has not been supplied or explicitly declared absent.
    pub fn has_owned_province(&self, country: EntityId) -> Result<bool, Error> {
        if self.entity(country)?.kind != EntityKind::Country {
            return Err(Error::Invalid(
                "province ownership query requires a country".into(),
            ));
        }
        if self.ownership.members.contains_key(&country) {
            Ok(true)
        } else if self.ownership.unknown == 0 {
            Ok(false)
        } else {
            Err(Error::Invalid(
                "unknown province ownership prevents an absence query".into(),
            ))
        }
    }

    /// Search known members without scanning unrelated provinces. A positive
    /// match is sufficient; a negative result requires complete owner edges.
    pub fn any_owned_province(
        &self,
        country: EntityId,
        condition: &crate::Condition,
    ) -> Result<bool, Error> {
        if self.entity(country)?.kind != EntityKind::Country {
            return Err(Error::Invalid(
                "province ownership query requires a country".into(),
            ));
        }
        if let Some(members) = self.ownership.members.get(&country) {
            for &province in members {
                if condition.evaluate(self, province)? {
                    return Ok(true);
                }
            }
        }
        if self.ownership.unknown != 0 {
            return Err(Error::Invalid(
                "unknown province ownership prevents an absence query".into(),
            ));
        }
        Ok(false)
    }

    pub(crate) fn rebuild_ownership(&mut self) {
        let mut index = OwnershipIndex::default();
        for (id, entity) in self.entities().iter().enumerate() {
            if entity.kind != EntityKind::Province {
                continue;
            }
            if let Some(country) = entity.owner {
                index.members.entry(country).or_default().push(EntityId(id));
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
