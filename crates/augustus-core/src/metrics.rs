// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{EntityId, EntityKind, Error, ModifierDefinition, World};

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetricEffect {
    pub modifier: Option<ModifierDefinition>,
    pub unresolved_properties: BTreeSet<String>,
}

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
    /// When absent, the same effect applies on both sides of the baseline.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub below_baseline: Option<MetricEffect>,
    /// A per-entity ceiling limits growth without discarding existing excess.
    #[serde(default, skip_serializing_if = "is_false")]
    pub growth_ceiling: bool,
}

fn is_false(value: &bool) -> bool {
    !value
}

impl MetricDefinition {
    fn effect(&self, value: i64) -> (Option<&ModifierDefinition>, &BTreeSet<String>) {
        if value < self.baseline
            && let Some(effect) = &self.below_baseline
        {
            return (effect.modifier.as_ref(), &effect.unresolved_properties);
        }
        (self.modifier.as_ref(), &self.unresolved_properties)
    }

    fn scaled(&self, value: i64, coefficient: i64) -> Result<i64, Error> {
        i64::try_from(
            i128::from(coefficient) * (i128::from(value) - i128::from(self.baseline))
                / i128::from(self.divisor),
        )
        .map_err(|_| Error::Overflow)
    }

    pub(crate) fn validate_value(&self, value: Option<i64>) -> Result<(), Error> {
        if let Some(value) = value {
            if !(self.minimum..=self.maximum).contains(&value) {
                return Err(Error::Invalid("metric value outside content bounds".into()));
            }
            if let Some(modifier) = self.effect(value).0 {
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
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub ceilings: BTreeMap<EntityId, BTreeMap<String, Option<i64>>>,
}

impl Metrics {
    pub fn is_empty(&self) -> bool {
        self.definitions.is_empty() && self.entities.is_empty() && self.ceilings.is_empty()
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

    pub fn ceiling(&self, world: &World, entity: EntityId, name: &str) -> Result<i64, Error> {
        let definition = self.definition(name)?;
        if !definition.growth_ceiling || world.entity(entity)?.kind != definition.kind {
            return Err(Error::Invalid(
                "metric has no growth ceiling in this scope".into(),
            ));
        }
        self.ceilings
            .get(&entity)
            .and_then(|values| values.get(name))
            .copied()
            .flatten()
            .ok_or_else(|| {
                Error::Invalid(format!(
                    "metric growth ceiling is unknown for entity {} and name {name}",
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
            let value = self
                .entities
                .get(&entity)
                .and_then(|values| values.get(name))
                .copied()
                .flatten();
            let effects = if let Some(value) = value {
                [Some(definition.effect(value)), None]
            } else {
                [
                    (definition.maximum >= definition.baseline
                        || definition.below_baseline.is_none())
                    .then_some((
                        definition.modifier.as_ref(),
                        &definition.unresolved_properties,
                    )),
                    definition
                        .below_baseline
                        .as_ref()
                        .filter(|_| definition.minimum < definition.baseline)
                        .map(|effect| (effect.modifier.as_ref(), &effect.unresolved_properties)),
                ]
            };
            for (modifier, unresolved) in effects.into_iter().flatten() {
                if unresolved.contains(property) {
                    result
                        .unresolved
                        .insert(format!("{source}:unresolved:{property}"));
                }
                let Some(modifier) = modifier else {
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
                if let Some(value) = value {
                    result.contributions.push(MetricContribution {
                        source: source.clone(),
                        property: property.into(),
                        micros: definition.scaled(value, coefficient)?,
                    });
                } else {
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
                || invalid_effect(
                    definition.modifier.as_ref(),
                    &definition.unresolved_properties,
                )
                || definition.below_baseline.as_ref().is_some_and(|effect| {
                    invalid_effect(effect.modifier.as_ref(), &effect.unresolved_properties)
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
        for (&entity, values) in &self.ceilings {
            if values.is_empty() {
                return Err(Error::Invalid("empty metric ceiling entry".into()));
            }
            for (name, &value) in values {
                let definition = self.definition(name)?;
                if !definition.growth_ceiling || world.entity(entity)?.kind != definition.kind {
                    return Err(Error::Invalid("invalid metric growth ceiling scope".into()));
                }
                definition.validate_value(value)?;
            }
        }
        Ok(())
    }
}

fn invalid_effect(modifier: Option<&ModifierDefinition>, unresolved: &BTreeSet<String>) -> bool {
    unresolved.iter().any(String::is_empty)
        || modifier.is_some_and(|modifier| {
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
            .and_then(|()| self.objectives().validate(self))
            .and_then(|()| self.estates().validate(self))
            .and_then(|()| {
                self.monthly_accruals()
                    .map(|a| a.validate(self))
                    .transpose()
                    .map(|_| ())
            })
            .and_then(|()| {
                self.monthly_convergence()
                    .map(|state| state.validate(self))
                    .transpose()
                    .map(|_| ())
            })
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

    /// Changing a growth ceiling leaves the authoritative quantity untouched.
    pub fn set_metric_ceiling(
        &mut self,
        entity: EntityId,
        name: &str,
        value: Option<i64>,
    ) -> Result<(), Error> {
        self.require_running()?;
        let definition = self.metrics.definition(name)?;
        if !definition.growth_ceiling || self.entity(entity)?.kind != definition.kind {
            return Err(Error::Invalid(
                "metric has no growth ceiling in this scope".into(),
            ));
        }
        definition.validate_value(value)?;
        self.metrics
            .ceilings
            .entry(entity)
            .or_default()
            .insert(name.into(), value);
        Ok(())
    }
}
