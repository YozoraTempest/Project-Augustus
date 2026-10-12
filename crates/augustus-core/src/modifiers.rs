// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use crate::{EntityId, Error, World};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

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
    /// First simulation day on which this attachment is no longer active.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_on: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModifierRenewal {
    Replace,
    /// Retain the later dated expiry, preferring any date to an undated attachment.
    KeepLatestDated,
}

/// Named definitions and established attachments. Declarations do not execute callbacks.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Modifiers {
    pub definitions: BTreeMap<String, ModifierDefinition>,
    pub active: BTreeMap<EntityId, BTreeMap<String, ModifierInstance>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub unresolved: BTreeMap<EntityId, BTreeSet<String>>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub unknown_entities: BTreeSet<EntityId>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub unresolved_properties: BTreeMap<String, BTreeSet<String>>,
}

impl Modifiers {
    pub fn is_empty(&self) -> bool {
        self.definitions.is_empty()
            && self.active.is_empty()
            && self.unresolved.is_empty()
            && self.unknown_entities.is_empty()
            && self.unresolved_properties.is_empty()
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
            if active
                .values()
                .any(|v| v.expires_on.is_some_and(|d| d <= world.day()))
            {
                return Err(Error::Invalid("expired modifier attachment".into()));
            }
            self.declared_totals(entity)?;
        }
        for (&entity, names) in &self.unresolved {
            world.entity(entity)?;
            for name in names {
                self.identifier(name)?;
                if self
                    .active
                    .get(&entity)
                    .is_some_and(|v| v.contains_key(name))
                {
                    return Err(Error::Invalid(
                        "modifier attachment is both known and unknown".into(),
                    ));
                }
            }
        }
        for &entity in &self.unknown_entities {
            world.entity(entity)?;
        }
        for (name, properties) in &self.unresolved_properties {
            self.identifier(name)?;
            if properties.iter().any(String::is_empty) {
                return Err(Error::Invalid("empty unresolved modifier property".into()));
            }
        }
        Ok(())
    }

    pub(crate) fn identifier(&self, name: &str) -> Result<(), Error> {
        if name.is_empty() || !self.definitions.contains_key(name) {
            return Err(Error::Invalid(format!("unknown modifier {name}")));
        }
        Ok(())
    }

    pub(crate) fn expiration_index(&self) -> BTreeMap<u64, BTreeSet<(EntityId, String)>> {
        let mut index = BTreeMap::<u64, BTreeSet<(EntityId, String)>>::new();
        for (&entity, active) in &self.active {
            for (name, instance) in active {
                if let Some(day) = instance.expires_on {
                    index.entry(day).or_default().insert((entity, name.clone()));
                }
            }
        }
        index
    }
}

impl World {
    pub fn modifier_attachments_known(&self, entity: EntityId) -> Result<bool, Error> {
        self.entity(entity)?;
        Ok(!self.modifiers.unknown_entities.contains(&entity)
            && !self
                .modifiers
                .unresolved
                .get(&entity)
                .is_some_and(|v| !v.is_empty()))
    }
    pub fn has_modifier(&self, entity: EntityId, name: &str) -> Result<bool, Error> {
        self.entity(entity)?;
        self.modifiers.identifier(name)?;
        if self.modifiers.unknown_entities.contains(&entity)
            || self
                .modifiers
                .unresolved
                .get(&entity)
                .is_some_and(|v| v.contains(name))
        {
            return Err(Error::Invalid(format!(
                "modifier state is unknown for entity {} and name {name}",
                entity.0
            )));
        }
        Ok(self
            .modifiers
            .active
            .get(&entity)
            .is_some_and(|v| v.contains_key(name)))
    }

    pub fn attach_modifier(
        &mut self,
        entity: EntityId,
        name: &str,
        duration: Option<u64>,
        hidden: Option<bool>,
        renewal: ModifierRenewal,
    ) -> Result<(), Error> {
        self.require_running()?;
        self.entity(entity)?;
        self.modifiers.identifier(name)?;
        if self.modifiers.unknown_entities.contains(&entity) {
            return Err(Error::Invalid("modifier namespace is unknown".into()));
        }
        if duration == Some(0) {
            return Err(Error::Invalid("modifier duration must be positive".into()));
        }
        if renewal == ModifierRenewal::KeepLatestDated
            && self
                .modifiers
                .unresolved
                .get(&entity)
                .is_some_and(|v| v.contains(name))
        {
            return Err(Error::Invalid(
                "previous modifier expiration is unknown".into(),
            ));
        }
        let requested = duration
            .map(|d| self.day().checked_add(d).ok_or(Error::Overflow))
            .transpose()?;
        let old = self
            .modifiers
            .active
            .get(&entity)
            .and_then(|v| v.get(name))
            .and_then(|v| v.expires_on);
        let expires_on = match renewal {
            ModifierRenewal::Replace => requested,
            ModifierRenewal::KeepLatestDated => old.max(requested),
        };
        self.remove_modifier(entity, name)?;
        self.modifiers
            .active
            .entry(entity)
            .or_default()
            .insert(name.into(), ModifierInstance { hidden, expires_on });
        if let Some(day) = expires_on {
            self.modifier_expirations
                .entry(day)
                .or_default()
                .insert((entity, name.into()));
        }
        Ok(())
    }

    pub fn remove_modifier(&mut self, entity: EntityId, name: &str) -> Result<(), Error> {
        self.require_running()?;
        self.entity(entity)?;
        self.modifiers.identifier(name)?;
        if self.modifiers.unknown_entities.contains(&entity) {
            return Err(Error::Invalid("modifier namespace is unknown".into()));
        }
        if let Some(active) = self.modifiers.active.get_mut(&entity) {
            if let Some(instance) = active.remove(name)
                && let Some(day) = instance.expires_on
                && let Some(entries) = self.modifier_expirations.get_mut(&day)
            {
                entries.remove(&(entity, name.into()));
                if entries.is_empty() {
                    self.modifier_expirations.remove(&day);
                }
            }
            if active.is_empty() {
                self.modifiers.active.remove(&entity);
            }
        }
        if let Some(names) = self.modifiers.unresolved.get_mut(&entity) {
            names.remove(name);
            if names.is_empty() {
                self.modifiers.unresolved.remove(&entity);
            }
        }
        Ok(())
    }

    pub(crate) fn expire_modifiers(&mut self, day: u64) {
        while self
            .modifier_expirations
            .first_key_value()
            .is_some_and(|(&d, _)| d <= day)
        {
            let (_, entries) = self
                .modifier_expirations
                .pop_first()
                .expect("checked expiration");
            for (entity, name) in entries {
                let active = self
                    .modifiers
                    .active
                    .get_mut(&entity)
                    .expect("indexed attachment");
                active.remove(&name);
                if active.is_empty() {
                    self.modifiers.active.remove(&entity);
                }
            }
        }
    }
}
