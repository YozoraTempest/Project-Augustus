// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{EntityId, Error, FieldId, Governments, Schema, World};

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
    Resource(crate::CountryResource),
    Field(FieldId),
    Named(String),
    Government(String),
    /// Read active or retained inactive values; all writes use the active slot.
    StoredGovernment(String),
    /// Read the subject system's explicit, unscaled count.
    SubjectCount,
    /// Read the current ruler's authoritative skill, rather than a numeric copy.
    RulerSkill(crate::RulerSkill),
    /// Query a current calculation in content-selected integer units.
    Attribute {
        name: String,
        divisor: i64,
    },
    Metric(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NumericOperation {
    Set,
    Add,
    Subtract,
    Multiply {
        scale: i64,
    },
    Divide {
        scale: i64,
    },
    /// Move toward the supplied value without passing it, then apply slot bounds.
    Approach {
        step: i64,
    },
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
    pub(crate) fn validate(
        &self,
        schema: &Schema,
        governments: &Governments,
        metrics: &crate::Metrics,
    ) -> Result<(), Error> {
        match self {
            Self::Field(field) if field.0 >= schema.fields().len() => {
                Err(Error::Invalid(format!("unknown field {}", field.0)))
            }
            Self::Named(name) => validate_name(name),
            Self::Government(name) | Self::StoredGovernment(name) => {
                governments.metric_definition(name).map(|_| ())
            }
            Self::Metric(name) => metrics.definition(name).map(|_| ()),
            Self::Attribute { name, divisor } => {
                validate_name(name)?;
                if *divisor <= 0 {
                    return Err(Error::Invalid(
                        "attribute numeric divisor must be positive".into(),
                    ));
                }
                Ok(())
            }
            Self::Field(_) | Self::SubjectCount | Self::RulerSkill(_) | Self::Resource(_) => Ok(()),
        }
    }

    pub(crate) fn validate_write(&self) -> Result<(), Error> {
        if matches!(self, Self::StoredGovernment(_)) {
            return Err(Error::Invalid(
                "stored government metric slots are read only".into(),
            ));
        }
        if matches!(self, Self::SubjectCount) {
            return Err(Error::Invalid("subject count slots are read only".into()));
        }
        if matches!(self, Self::RulerSkill(_)) {
            return Err(Error::Invalid("ruler skill slots are read only".into()));
        }
        if matches!(self, Self::Attribute { .. }) {
            return Err(Error::Invalid(
                "attribute numeric slots are read only".into(),
            ));
        }
        Ok(())
    }
}

impl NumericOperation {
    pub(crate) fn validate(&self) -> Result<(), Error> {
        if matches!(self, Self::Approach { step } if *step < 0) {
            return Err(Error::Invalid(
                "numeric approach step must be nonnegative".into(),
            ));
        }
        if matches!(self, Self::Multiply { scale } | Self::Divide { scale } if *scale <= 0) {
            return Err(Error::Invalid("numeric scale must be positive".into()));
        }
        Ok(())
    }

    pub(crate) fn calculate(self, previous: i64, value: i64) -> Result<i128, Error> {
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
            Self::Approach { step } => {
                let previous = i128::from(previous);
                let step = i128::from(step);
                previous + (i128::from(value) - previous).clamp(-step, step)
            }
        };
        Ok(result)
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
        slot.validate(self.schema(), self.governments(), self.metrics())?;
        match slot {
            NumericSlot::Field(field) => self.value(entity, *field),
            NumericSlot::Resource(resource) => self.resource_value(entity, *resource),
            NumericSlot::Government(name) => self.government_metric_value(entity, name),
            NumericSlot::StoredGovernment(name) => {
                self.government_metric_stored_value(entity, name)
            }
            NumericSlot::Metric(name) => self.metrics().value(self, entity, name),
            NumericSlot::SubjectCount => self.subject_count(entity).map(i64::from),
            NumericSlot::RulerSkill(skill) => self.ruler_skill(entity, *skill).map(i64::from),
            NumericSlot::Attribute { name, divisor } => self
                .attributes()
                .evaluate(self, entity, name)?
                .value_micros
                .map(|value| value / divisor)
                .ok_or_else(|| Error::Invalid(format!("attribute {name} is unknown"))),
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
        if matches!(
            operation,
            NumericOperation::Add | NumericOperation::Subtract
        ) {
            self.require_running()?;
            self.entity(entity)?;
            slot.validate(self.schema(), self.governments(), self.metrics())?;
            slot.validate_write()?;
            operation.validate()?;

            let slot_is_unknown = match slot {
                NumericSlot::Resource(resource) => {
                    self.resource_country(entity)?;
                    self.country_resources
                        .get(&entity)
                        .and_then(|state| resource.get(state))
                        .is_none()
                }
                NumericSlot::Metric(name) => {
                    let definition = self.metrics().definition(name)?;
                    if self.entity(entity)?.kind != definition.kind {
                        return Err(Error::Invalid("metric scope does not match entity".into()));
                    }
                    self.metrics()
                        .entities
                        .get(&entity)
                        .and_then(|values| values.get(name))
                        .copied()
                        .flatten()
                        .is_none()
                }
                _ => false,
            };
            if slot_is_unknown {
                let delta = if operation == NumericOperation::Add {
                    value
                } else {
                    value.checked_neg().ok_or(Error::Overflow)?
                };
                self.record_unbased_numeric_delta(entity, slot, delta)?;
                return Ok(());
            }
        }
        let value = self.preview_numeric_update(entity, slot, operation, value)?;
        self.write_numeric_value(entity, slot, value)
    }

    fn record_unbased_numeric_delta(
        &mut self,
        entity: EntityId,
        slot: &NumericSlot,
        delta: i64,
    ) -> Result<(), Error> {
        match slot {
            NumericSlot::Resource(resource) => {
                let current = self
                    .unbased_resource_deltas
                    .get(&entity)
                    .and_then(|values| values.get(resource))
                    .copied()
                    .unwrap_or(0);
                let value = current.checked_add(delta).ok_or(Error::Overflow)?;
                let values = self.unbased_resource_deltas.entry(entity).or_default();
                if value == 0 {
                    values.remove(resource);
                } else {
                    values.insert(*resource, value);
                }
                if values.is_empty() {
                    self.unbased_resource_deltas.remove(&entity);
                }
            }
            NumericSlot::Metric(name) => {
                let current = self
                    .unbased_metric_deltas
                    .get(&entity)
                    .and_then(|values| values.get(name))
                    .copied()
                    .unwrap_or(0);
                let value = current.checked_add(delta).ok_or(Error::Overflow)?;
                let values = self.unbased_metric_deltas.entry(entity).or_default();
                if value == 0 {
                    values.remove(name);
                } else {
                    values.insert(name.clone(), value);
                }
                if values.is_empty() {
                    self.unbased_metric_deltas.remove(&entity);
                }
            }
            _ => return Err(Error::Invalid("unsupported unbased numeric slot".into())),
        }
        Ok(())
    }

    /// Returns the cumulative signed adjustments recorded while this slot's base is unknown.
    /// The slot's numeric value remains unknown until an authoritative value is supplied.
    pub fn unbased_numeric_delta(&self, entity: EntityId, slot: &NumericSlot) -> Option<i64> {
        match slot {
            NumericSlot::Resource(resource) => self
                .unbased_resource_deltas
                .get(&entity)
                .and_then(|values| values.get(resource))
                .copied(),
            NumericSlot::Metric(name) => self
                .unbased_metric_deltas
                .get(&entity)
                .and_then(|values| values.get(name))
                .copied(),
            _ => None,
        }
    }

    pub(crate) fn clear_unbased_numeric_delta(&mut self, entity: EntityId, slot: &NumericSlot) {
        match slot {
            NumericSlot::Resource(resource) => {
                if let Some(entries) = self.unbased_resource_deltas.get_mut(&entity) {
                    entries.remove(resource);
                    if entries.is_empty() {
                        self.unbased_resource_deltas.remove(&entity);
                    }
                }
            }
            NumericSlot::Metric(name) => {
                if let Some(entries) = self.unbased_metric_deltas.get_mut(&entity) {
                    entries.remove(name);
                    if entries.is_empty() {
                        self.unbased_metric_deltas.remove(&entity);
                    }
                }
            }
            _ => {}
        }
    }

    pub(crate) fn validate_unbased_numeric_deltas(&self) -> Result<(), Error> {
        for (&entity, deltas) in &self.unbased_resource_deltas {
            self.resource_country(entity)?;
            if deltas.is_empty() {
                return Err(Error::Invalid("empty unbased resource deltas".into()));
            }
            for (&resource, &delta) in deltas {
                if delta == 0
                    || self
                        .country_resources
                        .get(&entity)
                        .and_then(|state| resource.get(state))
                        .is_some()
                {
                    return Err(Error::Invalid("invalid unbased resource delta".into()));
                }
            }
        }
        for (&entity, deltas) in &self.unbased_metric_deltas {
            let kind = self.entity(entity)?.kind;
            if deltas.is_empty() {
                return Err(Error::Invalid("empty unbased metric deltas".into()));
            }
            for (name, &delta) in deltas {
                let definition = self.metrics().definition(name)?;
                if delta == 0
                    || definition.kind != kind
                    || self
                        .metrics()
                        .entities
                        .get(&entity)
                        .and_then(|values| values.get(name))
                        .copied()
                        .flatten()
                        .is_some()
                {
                    return Err(Error::Invalid("invalid unbased metric delta".into()));
                }
            }
        }
        Ok(())
    }

    pub(crate) fn preview_numeric_update(
        &self,
        entity: EntityId,
        slot: &NumericSlot,
        operation: NumericOperation,
        value: i64,
    ) -> Result<i64, Error> {
        self.require_running()?;
        self.entity(entity)?;
        slot.validate(self.schema(), self.governments(), self.metrics())?;
        slot.validate_write()?;
        if matches!(slot, NumericSlot::Resource(_)) {
            self.resource_country(entity)?;
        }
        if let NumericSlot::Government(name) = slot
            && !self.government_metric_active(entity, name)?
        {
            return Err(Error::Invalid(format!("inactive government metric {name}")));
        }
        let growth_ceiling = if let NumericSlot::Metric(name) = slot {
            self.metrics().definition(name)?.growth_ceiling
        } else {
            false
        };
        let is_assignment = operation == NumericOperation::Set;
        let previous = if is_assignment && !growth_ceiling {
            0
        } else {
            self.numeric_value(entity, slot)?
        };
        let value = operation.calculate(previous, value)?;
        let value = if let NumericSlot::Government(name) = slot {
            let definition = self.governments().metric_definition(name)?;
            value.clamp(
                i128::from(definition.minimum_milli),
                i128::from(definition.maximum_milli),
            )
        } else if let NumericSlot::Metric(name) = slot {
            let definition = self.metrics().definition(name)?;
            let value = value.clamp(
                i128::from(definition.minimum),
                i128::from(definition.maximum),
            );
            if growth_ceiling && value > i128::from(previous) {
                value.min(i128::from(
                    previous.max(self.metrics().ceiling(self, entity, name)?),
                ))
            } else {
                value
            }
        } else if let NumericSlot::Resource(resource) = slot {
            if is_assignment {
                resource.assigned(value)?
            } else {
                resource.bounded(value)
            }
        } else {
            value
        };
        let value =
            i64::try_from(value).map_err(|_| Error::Invalid("numeric state overflow".into()))?;
        Ok(value)
    }

    fn write_numeric_value(
        &mut self,
        entity: EntityId,
        slot: &NumericSlot,
        value: i64,
    ) -> Result<(), Error> {
        self.clear_unbased_numeric_delta(entity, slot);
        match slot {
            NumericSlot::StoredGovernment(_)
            | NumericSlot::SubjectCount
            | NumericSlot::RulerSkill(_)
            | NumericSlot::Attribute { .. } => {
                unreachable!("validated writable numeric slot")
            }
            NumericSlot::Field(field) => self.set(entity, *field, value),
            NumericSlot::Resource(resource) => self.write_resource_value(entity, *resource, value),
            NumericSlot::Metric(name) => self.set_metric_value(entity, name, Some(value)),
            NumericSlot::Government(name) => {
                let previous = self
                    .governments
                    .countries
                    .get_mut(&entity)
                    .and_then(|state| state.metrics.as_mut())
                    .expect("validated metric selection")
                    .insert(name.clone(), Some(value));
                if let Err(error) = self.governments.declared_effects(entity) {
                    self.governments
                        .countries
                        .get_mut(&entity)
                        .and_then(|state| state.metrics.as_mut())
                        .expect("validated metric selection")
                        .insert(name.clone(), previous.expect("active metric"));
                    return Err(error);
                }
                Ok(())
            }
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
