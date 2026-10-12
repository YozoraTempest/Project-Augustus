// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{EntityId, EntityKind, Error, ModifierDefinition, World};

/// Content chooses the integer unit, bounds and scaling of a physical quantity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetricDefinition {
    pub kind: EntityKind,
    pub minimum: i64,
    pub maximum: i64,
    pub baseline: i64,
    pub divisor: i64,
    pub modifier: Option<ModifierDefinition>,
    pub unresolved_properties: BTreeSet<String>,
}

impl MetricDefinition {
    fn scaled(&self, value: i64, coefficient: i64) -> Result<i64, Error> {
        i64::try_from(
            i128::from(coefficient) * (i128::from(value) - i128::from(self.baseline))
                / i128::from(self.divisor),
        )
        .map_err(|_| Error::Overflow)
    }

    fn validate_value(&self, value: Option<i64>) -> Result<(), Error> {
        if let Some(value) = value {
            if !(self.minimum..=self.maximum).contains(&value) {
                return Err(Error::Invalid("metric value outside content bounds".into()));
            }
            if let Some(modifier) = &self.modifier {
                for &coefficient in modifier.values_micros.values() {
                    self.scaled(value, coefficient)?;
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MetricContribution {
    pub source: String,
    pub property: String,
    pub micros: i64,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
pub struct MetricEvaluation {
    pub contributions: Vec<MetricContribution>,
    pub unresolved: BTreeSet<String>,
}

/// Definitions and authoritative values. Missing or null values remain unknown.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Metrics {
    pub definitions: BTreeMap<String, MetricDefinition>,
    pub entities: BTreeMap<EntityId, BTreeMap<String, Option<i64>>>,
}

impl Metrics {
    pub fn is_empty(&self) -> bool {
        self.definitions.is_empty() && self.entities.is_empty()
    }

    pub(crate) fn definition(&self, name: &str) -> Result<&MetricDefinition, Error> {
        self.definitions
            .get(name)
            .ok_or_else(|| Error::Invalid(format!("unknown metric {name}")))
    }

    pub fn value(&self, world: &World, entity: EntityId, name: &str) -> Result<i64, Error> {
        if world.entity(entity)?.kind != self.definition(name)?.kind {
            return Err(Error::Invalid("metric scope does not match entity".into()));
        }
        self.entities
            .get(&entity)
            .and_then(|values| values.get(name))
            .copied()
            .flatten()
            .ok_or_else(|| {
                Error::Invalid(format!(
                    "metric state is unknown for entity {} and name {name}",
                    entity.0
                ))
            })
    }

    pub fn property_effects(
        &self,
        world: &World,
        entity: EntityId,
        property: &str,
    ) -> Result<MetricEvaluation, Error> {
        let kind = world.entity(entity)?.kind;
        let mut result = MetricEvaluation::default();
        for (name, definition) in self
            .definitions
            .iter()
            .filter(|(_, definition)| definition.kind == kind)
        {
            let source = format!("metric:{name}");
            if definition.unresolved_properties.contains(property) {
                result
                    .unresolved
                    .insert(format!("{source}:unresolved:{property}"));
            }
            let Some(modifier) = &definition.modifier else {
                continue;
            };
            if modifier.flags.contains_key(property) {
                return Err(Error::Invalid(
                    "metric attribute requires numeric property".into(),
                ));
            }
            let Some(&coefficient) = modifier.values_micros.get(property) else {
                continue;
            };
            match self
                .entities
                .get(&entity)
                .and_then(|values| values.get(name))
                .copied()
                .flatten()
            {
                Some(value) => result.contributions.push(MetricContribution {
                    source,
                    property: property.into(),
                    micros: definition.scaled(value, coefficient)?,
                }),
                None => {
                    result.unresolved.insert(format!("{source}:unknown_value"));
                }
            }
        }
        Ok(result)
    }

    pub(crate) fn validate(&self, world: &World) -> Result<(), Error> {
        for (name, definition) in &self.definitions {
            if name.trim().is_empty()
                || definition.minimum > definition.maximum
                || definition.divisor <= 0
                || definition
                    .unresolved_properties
                    .iter()
                    .any(String::is_empty)
                || definition.modifier.as_ref().is_some_and(|modifier| {
                    modifier
                        .values_micros
                        .keys()
                        .chain(modifier.flags.keys())
                        .any(String::is_empty)
                        || modifier
                            .values_micros
                            .keys()
                            .any(|key| modifier.flags.contains_key(key))
                })
            {
                return Err(Error::Invalid("invalid metric definition".into()));
            }
        }
        for (&entity, values) in &self.entities {
            let kind = world.entity(entity)?.kind;
            if values.is_empty() {
                return Err(Error::Invalid("empty metric state entry".into()));
            }
            for (name, &value) in values {
                let definition = self.definition(name)?;
                if kind != definition.kind {
                    return Err(Error::Invalid("invalid metric entity type".into()));
                }
                definition.validate_value(value)?;
            }
        }
        Ok(())
    }
}

impl World {
    pub fn metrics(&self) -> &Metrics {
        &self.metrics
    }

    pub fn set_metrics(&mut self, metrics: Metrics) -> Result<(), Error> {
        self.require_running()?;
        metrics.validate(self)?;
        let previous = std::mem::replace(&mut self.metrics, metrics);
        if let Err(error) = self
            .event_catalog
            .validate(self)
            .and_then(|()| self.validate_event_content(&self.event_catalog))
            .and_then(|()| self.attributes().validate(self))
        {
            self.metrics = previous;
            return Err(error);
        }
        Ok(())
    }

    pub fn set_metric_value(
        &mut self,
        entity: EntityId,
        name: &str,
        value: Option<i64>,
    ) -> Result<(), Error> {
        self.require_running()?;
        let definition = self.metrics.definition(name)?;
        if self.entity(entity)?.kind != definition.kind {
            return Err(Error::Invalid("metric scope does not match entity".into()));
        }
        definition.validate_value(value)?;
        self.metrics
            .entities
            .entry(entity)
            .or_default()
            .insert(name.into(), value);
        Ok(())
    }
}
