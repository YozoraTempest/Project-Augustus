// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{EntityId, Error, World};

/// Persistent named boolean state. An empty entity entry declares a known namespace.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Markers {
    /// Explicit values can resolve individual names in an otherwise unknown namespace.
    pub entities: BTreeMap<EntityId, BTreeMap<String, bool>>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub unknown_entities: BTreeSet<EntityId>,
}

pub(crate) fn validate_name(name: &str) -> Result<(), Error> {
    if name.is_empty() {
        return Err(Error::Invalid("empty marker name".into()));
    }
    Ok(())
}

impl Markers {
    pub fn is_empty(&self) -> bool {
        self.entities.is_empty() && self.unknown_entities.is_empty()
    }

    pub(crate) fn validate(&self, world: &World) -> Result<(), Error> {
        for entity in self.entities.keys().chain(&self.unknown_entities) {
            world.entity(*entity)?;
        }
        for values in self.entities.values() {
            for name in values.keys() {
                validate_name(name)?;
            }
        }
        Ok(())
    }

    /// Discard stale values when an unexecuted operation could change this namespace.
    pub fn mark_unknown(&mut self, entity: EntityId) {
        self.entities.remove(&entity);
        self.unknown_entities.insert(entity);
    }
}

impl World {
    pub fn markers(&self) -> &Markers {
        &self.markers
    }

    pub fn set_markers(&mut self, markers: Markers) -> Result<(), Error> {
        self.require_running()?;
        markers.validate(self)?;
        self.markers = markers;
        Ok(())
    }

    pub fn marker_namespace_known(&self, entity: EntityId) -> Result<bool, Error> {
        self.entity(entity)?;
        Ok(self.markers.entities.contains_key(&entity)
            && !self.markers.unknown_entities.contains(&entity))
    }

    pub fn has_marker(&self, entity: EntityId, name: &str) -> Result<bool, Error> {
        validate_name(name)?;
        self.entity(entity)?;
        if let Some(value) = self.markers.entities.get(&entity).and_then(|v| v.get(name)) {
            return Ok(*value);
        }
        if !self.marker_namespace_known(entity)? {
            return Err(Error::Invalid(format!(
                "marker state is unknown for entity {} and name {name}",
                entity.0
            )));
        }
        Ok(false)
    }

    /// A clear records known absence even when other names are still unknown.
    pub fn set_marker(&mut self, entity: EntityId, name: &str, present: bool) -> Result<(), Error> {
        self.require_running()?;
        validate_name(name)?;
        self.entity(entity)?;
        let known = self.marker_namespace_known(entity)?;
        if !known {
            self.markers.unknown_entities.insert(entity);
        }
        let values = self.markers.entities.entry(entity).or_default();
        if present || !known {
            values.insert(name.into(), present);
        } else {
            values.remove(name);
        }
        Ok(())
    }
}
