// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{AttributeTerm, EntityId, EntityKind, Error, ModifierDefinition, Target, World};

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GovernmentReform {
    /// Institutional features; None preserves an uncertain declaration.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub features: BTreeMap<String, Option<bool>>,
    pub modifier: ModifierDefinition,
    pub unresolved_properties: BTreeSet<String>,
}

/// Numeric effects scale by (value - baseline) / divisor, using thousandths
/// for metric inputs and millionths for modifier values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GovernmentMetric {
    pub minimum_milli: i64,
    pub maximum_milli: i64,
    pub baseline_milli: i64,
    pub divisor_milli: i64,
    pub modifier: Option<ModifierDefinition>,
    pub unresolved_properties: BTreeSet<String>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GovernmentState {
    pub kind: Option<String>,
    /// None means selection is unknown; an empty set means no selected reforms.
    pub reforms: Option<BTreeSet<String>>,
    /// Only active metrics are listed. None means their selection is unknown.
    pub metrics: Option<BTreeMap<String, Option<i64>>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GovernmentContribution {
    pub source: String,
    pub property: String,
    pub micros: i64,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
pub struct GovernmentEvaluation {
    pub known_totals_micros: BTreeMap<String, i64>,
    pub contributions: Vec<GovernmentContribution>,
    pub unresolved: BTreeSet<String>,
}

/// Selected institutional content and current metrics, shared by native and
/// translated worlds. Reform selection and metric changes do not cache totals.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Governments {
    pub reforms: BTreeMap<String, GovernmentReform>,
    pub metrics: BTreeMap<String, GovernmentMetric>,
    pub countries: BTreeMap<EntityId, GovernmentState>,
}

impl Governments {
    pub fn feature(&self, entity: EntityId, name: &str) -> Option<bool> {
        let selected = self.countries.get(&entity)?.reforms.as_ref()?;
        let mut explicit = None;
        for id in selected {
            let reform = self.reforms.get(id)?;
            if let Some(value) = reform.features.get(name) {
                let value = (*value)?;
                if explicit.is_some_and(|old| old != value) {
                    return None;
                }
                explicit = Some(value);
            }
        }
        Some(explicit.unwrap_or(false))
    }
    pub fn is_empty(&self) -> bool {
        self.reforms.is_empty() && self.metrics.is_empty() && self.countries.is_empty()
    }

    pub fn declared_effects(&self, entity: EntityId) -> Result<GovernmentEvaluation, Error> {
        self.evaluate(entity, None)
    }

    pub fn property_effects(
        &self,
        entity: EntityId,
        property: &str,
    ) -> Result<GovernmentEvaluation, Error> {
        self.evaluate(entity, Some(property))
    }

    fn evaluate(
        &self,
        entity: EntityId,
        property: Option<&str>,
    ) -> Result<GovernmentEvaluation, Error> {
        let mut result = GovernmentEvaluation::default();
        let Some(state) = self.countries.get(&entity) else {
            result
                .unresolved
                .insert("government:unknown_history".into());
            return Ok(result);
        };
        let selected = |name: &str| property.is_none_or(|p| p == name);
        let mut append = |source: String,
                          definition: &ModifierDefinition,
                          numerator: i128,
                          divisor: i128|
         -> Result<(), Error> {
            if property.is_some_and(|p| definition.flags.contains_key(p)) {
                return Err(Error::Invalid(
                    "government attribute requires numeric property".into(),
                ));
            }
            for (name, &value) in definition.values_micros.iter().filter(|(k, _)| selected(k)) {
                let micros = i64::try_from(i128::from(value) * numerator / divisor)
                    .map_err(|_| Error::Overflow)?;
                let total = result.known_totals_micros.entry(name.clone()).or_default();
                *total = total.checked_add(micros).ok_or(Error::Overflow)?;
                result.contributions.push(GovernmentContribution {
                    source: source.clone(),
                    property: name.clone(),
                    micros,
                });
            }
            Ok(())
        };
        if let Some(reforms) = &state.reforms {
            for name in reforms {
                let prefix = format!("government:reform:{name}");
                if let Some(definition) = self.reforms.get(name) {
                    append(prefix.clone(), &definition.modifier, 1, 1)?;
                    for name in definition
                        .unresolved_properties
                        .iter()
                        .filter(|p| selected(p))
                    {
                        result
                            .unresolved
                            .insert(format!("{prefix}:unresolved:{name}"));
                    }
                } else {
                    result
                        .unresolved
                        .insert(format!("{prefix}:missing_definition"));
                }
            }
        } else {
            result
                .unresolved
                .insert("government:unknown_reforms".into());
        }
        if let Some(metrics) = &state.metrics {
            for (name, value) in metrics {
                let definition = self
                    .metrics
                    .get(name)
                    .ok_or_else(|| Error::Invalid("unknown government metric".into()))?;
                let prefix = format!("government:metric:{name}");
                let Some(modifier) = &definition.modifier else {
                    result
                        .unresolved
                        .insert(format!("{prefix}:missing_definition"));
                    continue;
                };
                for property in definition
                    .unresolved_properties
                    .iter()
                    .filter(|p| selected(p))
                {
                    result
                        .unresolved
                        .insert(format!("{prefix}:unresolved:{property}"));
                }
                if let Some(value) = value {
                    append(
                        prefix,
                        modifier,
                        i128::from(*value) - i128::from(definition.baseline_milli),
                        i128::from(definition.divisor_milli),
                    )?;
                } else {
                    for property in modifier.values_micros.keys().filter(|p| selected(p)) {
                        result
                            .unresolved
                            .insert(format!("{prefix}:unknown_value:{property}"));
                    }
                }
            }
        } else {
            result
                .unresolved
                .insert("government:unknown_metrics".into());
        }
        Ok(result)
    }

    pub(crate) fn validate(&self, world: &World) -> Result<(), Error> {
        fn definition(name: &str, value: &ModifierDefinition) -> Result<(), Error> {
            if name.is_empty()
                || value
                    .values_micros
                    .keys()
                    .chain(value.flags.keys())
                    .any(String::is_empty)
                || value
                    .values_micros
                    .keys()
                    .any(|k| value.flags.contains_key(k))
            {
                return Err(Error::Invalid(
                    "invalid government effect definition".into(),
                ));
            }
            Ok(())
        }
        for (name, reform) in &self.reforms {
            definition(name, &reform.modifier)?;
            if reform.features.keys().any(String::is_empty) {
                return Err(Error::Invalid("empty government feature".into()));
            }
            if reform.unresolved_properties.iter().any(String::is_empty) {
                return Err(Error::Invalid("invalid unresolved reform property".into()));
            }
        }
        for (name, metric) in &self.metrics {
            if let Some(modifier) = &metric.modifier {
                definition(name, modifier)?;
            }
            if name.is_empty()
                || metric.minimum_milli > metric.maximum_milli
                || metric.divisor_milli <= 0
                || metric
                    .modifier
                    .as_ref()
                    .is_some_and(|m| !m.flags.is_empty())
                || metric.unresolved_properties.iter().any(String::is_empty)
            {
                return Err(Error::Invalid(
                    "invalid government metric definition".into(),
                ));
            }
        }
        for (&entity, state) in &self.countries {
            if world.entity(entity)?.kind != EntityKind::Country
                || state.kind.as_ref().is_some_and(String::is_empty)
                || state
                    .reforms
                    .as_ref()
                    .is_some_and(|r| r.iter().any(String::is_empty))
            {
                return Err(Error::Invalid("invalid government country state".into()));
            }
            if let Some(metrics) = &state.metrics {
                for (name, value) in metrics {
                    let definition = self
                        .metrics
                        .get(name)
                        .ok_or_else(|| Error::Invalid("unknown government metric".into()))?;
                    if value.is_some_and(|v| {
                        !(definition.minimum_milli..=definition.maximum_milli).contains(&v)
                    }) {
                        return Err(Error::Invalid("government metric outside bounds".into()));
                    }
                }
            }
            self.declared_effects(entity)?;
        }
        for (&entity, formulas) in &world.attributes().formulas {
            for term in formulas
                .values()
                .flat_map(|f| &f.channels)
                .flat_map(|c| c.flat.iter().chain(&c.rates))
            {
                if let AttributeTerm::Government { target, property } = term {
                    let entity = match target {
                        Target::Current => Some(entity),
                        Target::Owner => world.entity(entity)?.owner,
                        Target::Capital => world.entity(entity)?.capital,
                    };
                    if let Some(entity) = entity {
                        self.property_effects(entity, property)?;
                    }
                }
            }
        }
        Ok(())
    }
}
