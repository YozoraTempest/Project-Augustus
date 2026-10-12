// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{EntityId, Error, World};

/// Declared milestones and each entity's independently known completion records.
/// Recording a completion does not evaluate eligibility or execute rewards.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Completions {
    pub identifiers: BTreeSet<String>,
    /// Missing entities have unknown state; an empty set is known to be empty.
    pub entities: BTreeMap<EntityId, BTreeSet<String>>,
}

impl Completions {
    pub fn is_empty(&self) -> bool {
        self.identifiers.is_empty() && self.entities.is_empty()
    }

    pub(crate) fn identifier(&self, id: &str) -> Result<(), Error> {
        if id.is_empty() || !self.identifiers.contains(id) {
            return Err(Error::Invalid(format!(
                "unknown completion identifier {id}"
            )));
        }
        Ok(())
    }

    pub(crate) fn validate(&self, world: &World) -> Result<(), Error> {
        if self.identifiers.contains("") {
            return Err(Error::Invalid("empty completion identifier".into()));
        }
        for (entity, records) in &self.entities {
            world.entity(*entity)?;
            for id in records {
                self.identifier(id)?;
            }
        }
        Ok(())
    }
}

impl World {
    pub fn completions(&self) -> &Completions {
        &self.completions
    }

    pub fn set_completions(&mut self, completions: Completions) -> Result<(), Error> {
        self.require_running()?;
        completions.validate(self)?;
        let previous = std::mem::replace(&mut self.completions, completions);
        let validation = self
            .objectives
            .validate(self)
            .and_then(|()| self.event_catalog.validate(self))
            .and_then(|()| self.validate_event_content(&self.event_catalog));
        if let Err(error) = validation {
            self.completions = previous;
            return Err(error);
        }
        Ok(())
    }

    pub fn completion_recorded(&self, entity: EntityId, id: &str) -> Result<bool, Error> {
        self.entity(entity)?;
        self.completions.identifier(id)?;
        let records = self.completions.entities.get(&entity).ok_or_else(|| {
            Error::Invalid(format!(
                "completion state is unknown for entity {}",
                entity.0
            ))
        })?;
        Ok(records.contains(id))
    }

    /// Returns whether a new record was inserted. Repeated recording is idempotent.
    pub fn record_completion(&mut self, entity: EntityId, id: &str) -> Result<bool, Error> {
        self.require_running()?;
        self.completion_recorded(entity, id)?;
        Ok(self
            .completions
            .entities
            .get_mut(&entity)
            .expect("checked completion state")
            .insert(id.into()))
    }
}
