// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use crate::{EntityId, Error, World};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModifierDefinition {
    pub values_micros: BTreeMap<String, i64>,
    pub flags: BTreeMap<String, bool>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModifierInstance {
    pub hidden: Option<bool>,
}

/// Named definitions and currently established attachments. Conditions and timed
/// expiration require their own systems; declarations do not execute callbacks.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Modifiers {
    pub definitions: BTreeMap<String, ModifierDefinition>,
    pub active: BTreeMap<EntityId, BTreeMap<String, ModifierInstance>>,
}

impl Modifiers {
    pub fn is_empty(&self) -> bool {
        self.definitions.is_empty() && self.active.is_empty()
    }

    /// Sums attached numeric declarations. Consumers decide stat semantics and
    /// include other sources; this is not a complete derived gameplay statistic.
    pub fn declared_totals(&self, entity: EntityId) -> Result<BTreeMap<String, i64>, Error> {
        let mut totals = BTreeMap::<String, i64>::new();
        if let Some(active) = self.active.get(&entity) {
            for name in active.keys() {
                let definition = self
                    .definitions
                    .get(name)
                    .ok_or_else(|| Error::Invalid("unknown modifier definition".into()))?;
                for (key, &value) in &definition.values_micros {
                    let slot = totals.entry(key.clone()).or_default();
                    *slot = slot.checked_add(value).ok_or(Error::Overflow)?;
                }
            }
        }
        Ok(totals)
    }

    pub(crate) fn validate(&self, world: &World) -> Result<(), Error> {
        for (name, definition) in &self.definitions {
            if name.is_empty()
                || definition
                    .values_micros
                    .keys()
                    .chain(definition.flags.keys())
                    .any(|k| k.is_empty())
                || definition
                    .values_micros
                    .keys()
                    .any(|k| definition.flags.contains_key(k))
            {
                return Err(Error::Invalid(
                    "invalid modifier definition identifier".into(),
                ));
            }
        }
        for (&entity, active) in &self.active {
            world.entity(entity)?;
            if active
                .keys()
                .any(|name| !self.definitions.contains_key(name))
            {
                return Err(Error::Invalid("unknown active modifier".into()));
            }
            self.declared_totals(entity)?;
        }
        Ok(())
    }
}
