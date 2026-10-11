// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{EntityId, Error, FieldId, Schema, World};

/// Sparse numeric state. An empty entity entry declares a known, zero-filled namespace.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NamedValues {
    pub entities: BTreeMap<EntityId, BTreeMap<String, i64>>,
    /// Stored individual values remain known even if other names are unknown.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub unknown_entities: BTreeSet<EntityId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NumericSlot {
    Field(FieldId),
    Named(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NumericOperation {
    Set,
    Add,
    Subtract,
    Multiply { scale: i64 },
    Divide { scale: i64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NumericComparison {
    AtLeast,
    Equal,
}

fn validate_name(name: &str) -> Result<(), Error> {
    if name.is_empty() {
        return Err(Error::Invalid("empty numeric state name".into()));
    }
    Ok(())
}

impl NamedValues {
    pub fn is_empty(&self) -> bool {
        self.entities.is_empty() && self.unknown_entities.is_empty()
    }

    pub(crate) fn validate(&self, world: &World) -> Result<(), Error> {
        for entity in self.unknown_entities.iter().chain(self.entities.keys()) {
            world.entity(*entity)?;
        }
        for values in self.entities.values() {
            for name in values.keys() {
                validate_name(name)?;
            }
        }
        Ok(())
    }
}

impl NumericSlot {
    pub(crate) fn validate(&self, schema: &Schema) -> Result<(), Error> {
        match self {
            Self::Field(field) if field.0 >= schema.fields().len() => {
                Err(Error::Invalid(format!("unknown field {}", field.0)))
            }
            Self::Named(name) => validate_name(name),
            Self::Field(_) => Ok(()),
        }
    }
}

impl NumericOperation {
    pub(crate) fn validate(&self) -> Result<(), Error> {
        if matches!(self, Self::Multiply { scale } | Self::Divide { scale } if *scale <= 0) {
            return Err(Error::Invalid("numeric scale must be positive".into()));
        }
        Ok(())
    }

    fn apply(self, previous: i64, value: i64) -> Result<i64, Error> {
        self.validate()?;
        let result = match self {
            Self::Set => i128::from(value),
            Self::Add => i128::from(previous) + i128::from(value),
            Self::Subtract => i128::from(previous) - i128::from(value),
            Self::Multiply { scale } => {
                i128::from(previous) * i128::from(value) / i128::from(scale)
            }
            Self::Divide { scale } => {
                if value == 0 {
                    return Err(Error::Invalid("numeric division by zero".into()));
                }
                i128::from(previous) * i128::from(scale) / i128::from(value)
            }
        };
        i64::try_from(result).map_err(|_| Error::Invalid("numeric state overflow".into()))
    }
}

impl World {
    pub fn named_values(&self) -> &NamedValues {
        &self.named_values
    }

    pub fn set_named_values(&mut self, values: NamedValues) -> Result<(), Error> {
        self.require_running()?;
        values.validate(self)?;
        self.named_values = values;
        Ok(())
    }

    pub fn numeric_value(&self, entity: EntityId, slot: &NumericSlot) -> Result<i64, Error> {
        self.entity(entity)?;
        slot.validate(self.schema())?;
        match slot {
            NumericSlot::Field(field) => self.value(entity, *field),
            NumericSlot::Named(name) => {
                if let Some(value) = self
                    .named_values
                    .entities
                    .get(&entity)
                    .and_then(|v| v.get(name))
                {
                    return Ok(*value);
                }
                if !self.named_values.entities.contains_key(&entity)
                    || self.named_values.unknown_entities.contains(&entity)
                {
                    return Err(Error::Invalid(format!(
                        "numeric state is unknown for entity {} and name {name}",
                        entity.0
                    )));
                }
                Ok(0)
            }
        }
    }

    /// Scaled multiplication and division truncate toward zero; errors precede writes.
    pub fn update_numeric(
        &mut self,
        entity: EntityId,
        slot: &NumericSlot,
        operation: NumericOperation,
        value: i64,
    ) -> Result<(), Error> {
        self.require_running()?;
        self.entity(entity)?;
        slot.validate(self.schema())?;
        let previous = if operation == NumericOperation::Set {
            0
        } else {
            self.numeric_value(entity, slot)?
        };
        let value = operation.apply(previous, value)?;
        match slot {
            NumericSlot::Field(field) => self.set(entity, *field, value),
            NumericSlot::Named(name) => {
                if !self.named_values.entities.contains_key(&entity) {
                    self.named_values.unknown_entities.insert(entity);
                }
                self.named_values
                    .entities
                    .entry(entity)
                    .or_default()
                    .insert(name.clone(), value);
                Ok(())
            }
        }
    }
}
