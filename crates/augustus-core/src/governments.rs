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
    /// Explicitly retained inactive values do not contribute institutional effects.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub inactive_metrics: BTreeMap<String, Option<i64>>,
}

/// State installed when entering a different form. Metrics omitted here retain
/// their last value as inactive; explicit inactive values replace that storage.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GovernmentForm {
    pub reforms: BTreeSet<String>,
    pub metrics: BTreeMap<String, Option<i64>>,
    /// On entry, use the declared value as a floor for a retained metric.
    /// An explicitly unknown stored value remains unknown.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub retain_higher_metrics: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub inactive_metrics: BTreeMap<String, Option<i64>>,
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
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub forms: BTreeMap<String, GovernmentForm>,
    /// Reform membership by government and reform level; None marks ambiguous content.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub reform_levels: BTreeMap<String, BTreeMap<String, Option<String>>>,
    pub reforms: BTreeMap<String, GovernmentReform>,
    pub metrics: BTreeMap<String, GovernmentMetric>,
    pub countries: BTreeMap<EntityId, GovernmentState>,
}

impl Governments {
    pub(crate) fn reform_definition(&self, id: &str) -> Result<&GovernmentReform, Error> {
        validate_identifier(id)?;
        self.reforms
            .get(id)
            .ok_or_else(|| Error::Invalid(format!("unknown government reform {id}")))
    }

    pub(crate) fn metric_definition(&self, name: &str) -> Result<&GovernmentMetric, Error> {
        self.metrics
            .get(name)
            .ok_or_else(|| Error::Invalid(format!("unknown government metric {name}")))
    }
    pub fn feature(&self, entity: EntityId, name: &str) -> Option<bool> {
        let selected = self.countries.get(&entity)?.reforms.as_ref()?;
        let mut explicit = None;
        for id in selected {
            let reform = self.reforms.get(id)?;
            if reform.unresolved_properties.contains(name) {
                return None;
            }
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
        self.forms.is_empty()
            && self.reform_levels.is_empty()
            && self.reforms.is_empty()
            && self.metrics.is_empty()
            && self.countries.is_empty()
    }

    pub(crate) fn form_definition(&self, kind: &str) -> Result<&GovernmentForm, Error> {
        validate_identifier(kind)?;
        self.forms
            .get(kind)
            .ok_or_else(|| Error::Invalid(format!("unknown government form {kind}")))
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
        for (kind, levels) in &self.reform_levels {
            validate_identifier(kind)?;
            for (reform, level) in levels {
                validate_identifier(reform)?;
                if let Some(level) = level {
                    validate_identifier(level)?;
                }
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
        for (kind, form) in &self.forms {
            self.form_definition(kind)?;
            if form
                .retain_higher_metrics
                .iter()
                .any(|name| form.metrics.get(name).is_none_or(Option::is_none))
            {
                return Err(Error::Invalid(
                    "retained metric requires a known active entry value".into(),
                ));
            }
            for reform in &form.reforms {
                self.reform_definition(reform)?;
            }
            for (name, value) in form.metrics.iter().chain(&form.inactive_metrics) {
                let definition = self.metric_definition(name)?;
                if value.is_some_and(|v| {
                    !(definition.minimum_milli..=definition.maximum_milli).contains(&v)
                }) {
                    return Err(Error::Invalid(
                        "government form metric outside bounds".into(),
                    ));
                }
            }
            if form
                .metrics
                .keys()
                .any(|name| form.inactive_metrics.contains_key(name))
            {
                return Err(Error::Invalid(
                    "government form metric is both active and inactive".into(),
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
            for (name, value) in &state.inactive_metrics {
                let definition = self.metric_definition(name)?;
                if state.metrics.as_ref().is_some_and(|m| m.contains_key(name)) {
                    return Err(Error::Invalid(
                        "government metric is both active and inactive".into(),
                    ));
                }
                if value.is_some_and(|v| {
                    !(definition.minimum_milli..=definition.maximum_milli).contains(&v)
                }) {
                    return Err(Error::Invalid(
                        "inactive government metric outside bounds".into(),
                    ));
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

impl World {
    pub(crate) fn add_government_reform(
        &mut self,
        entity: EntityId,
        id: &str,
    ) -> Result<(), Error> {
        self.governments.reform_definition(id)?;
        let kind = self.government_kind(entity)?.to_owned();
        let levels = self.governments.reform_levels.get(&kind).ok_or_else(|| {
            Error::Invalid(format!("unknown reform levels for government {kind}"))
        })?;
        let level = match levels.get(id) {
            Some(Some(level)) => Some(level.clone()),
            Some(None) => {
                return Err(Error::Invalid(format!(
                    "ambiguous reform level for {id} under government {kind}"
                )));
            }
            None => None,
        };
        let replaced = level.as_ref().map(|level| {
            levels
                .iter()
                .filter(|(_, candidate)| candidate.as_ref() == Some(level))
                .map(|(candidate, _)| candidate.clone())
                .collect::<BTreeSet<_>>()
        });
        let state = self
            .governments
            .countries
            .get_mut(&entity)
            .ok_or_else(|| Error::Invalid("unknown government state".into()))?;
        let reforms = state
            .reforms
            .as_mut()
            .ok_or_else(|| Error::Invalid("unknown government reforms".into()))?;
        if let Some(replaced) = replaced {
            reforms.retain(|selected| !replaced.contains(selected));
        }
        reforms.insert(id.into());
        Ok(())
    }

    pub(crate) fn remove_government_reform(
        &mut self,
        entity: EntityId,
        id: &str,
    ) -> Result<(), Error> {
        self.governments.reform_definition(id)?;
        self.government_kind(entity)?;
        let state = self
            .governments
            .countries
            .get_mut(&entity)
            .ok_or_else(|| Error::Invalid("unknown government state".into()))?;
        let reforms = state
            .reforms
            .as_mut()
            .ok_or_else(|| Error::Invalid("unknown government reforms".into()))?;
        reforms.remove(id);
        Ok(())
    }

    pub(crate) fn change_government(
        &mut self,
        entity: EntityId,
        kind: &str,
        before: &crate::Plan,
        budget: &mut usize,
        depth: usize,
    ) -> Result<(), Error> {
        // Validate the request before the callback can write anything.
        self.governments.form_definition(kind)?;
        self.government_kind(entity)?;
        self.execute_inner(crate::ScopeContext::new(entity), before, budget, depth + 1)?;
        // Callbacks may change the current state. Apply the outer request to the
        // state that remains when the callback returns.
        if self.government_kind(entity)? == kind {
            return Ok(());
        }
        let mut form = self.governments.form_definition(kind)?.clone();
        let state = self
            .governments
            .countries
            .get_mut(&entity)
            .expect("validated country");
        let previous = state.metrics.as_ref().ok_or_else(|| {
            Error::Invalid("unknown government metric selection during transition".into())
        })?;
        let mut inactive = state.inactive_metrics.clone();
        inactive.extend(previous.iter().map(|(k, v)| (k.clone(), *v)));
        inactive.extend(form.inactive_metrics);
        for name in &form.retain_higher_metrics {
            if let Some(previous) = inactive.get(name) {
                let value = form.metrics.get_mut(name).expect("validated active metric");
                let floor = (*value).expect("validated entry value");
                *value = previous.map(|previous| previous.max(floor));
            }
        }
        for name in form.metrics.keys() {
            inactive.remove(name);
        }
        state.kind = Some(kind.into());
        state.reforms = Some(form.reforms);
        state.metrics = Some(form.metrics);
        state.inactive_metrics = inactive;
        Ok(())
    }

    pub fn government_kind(&self, entity: EntityId) -> Result<&str, Error> {
        self.government_country(entity)?
            .kind
            .as_deref()
            .ok_or_else(|| {
                Error::Invalid(format!("unknown government kind for entity {}", entity.0))
            })
    }

    pub fn government_reform_selected(&self, entity: EntityId, id: &str) -> Result<bool, Error> {
        self.governments().reform_definition(id)?;
        let reforms = self
            .government_country(entity)?
            .reforms
            .as_ref()
            .ok_or_else(|| {
                Error::Invalid(format!(
                    "unknown government reforms for entity {}",
                    entity.0
                ))
            })?;
        Ok(reforms.contains(id))
    }

    pub fn government_feature(&self, entity: EntityId, name: &str) -> Result<bool, Error> {
        validate_identifier(name)?;
        self.government_country(entity)?;
        self.governments().feature(entity, name).ok_or_else(|| {
            Error::Invalid(format!(
                "unknown government feature {name} for entity {}",
                entity.0
            ))
        })
    }

    fn government_country(&self, entity: EntityId) -> Result<&GovernmentState, Error> {
        if self.entity(entity)?.kind != EntityKind::Country {
            return Err(Error::Invalid(
                "government queries require a country".into(),
            ));
        }
        self.governments().countries.get(&entity).ok_or_else(|| {
            Error::Invalid(format!("unknown government state for entity {}", entity.0))
        })
    }

    pub fn government_metric_active(&self, entity: EntityId, name: &str) -> Result<bool, Error> {
        self.governments().metric_definition(name)?;
        if self.entity(entity)?.kind != EntityKind::Country {
            return Err(Error::Invalid(
                "government metrics require a country".into(),
            ));
        }
        let metrics = self
            .governments()
            .countries
            .get(&entity)
            .and_then(|state| state.metrics.as_ref())
            .ok_or_else(|| {
                Error::Invalid(format!(
                    "unknown government metric selection for entity {}",
                    entity.0
                ))
            })?;
        Ok(metrics.contains_key(name))
    }

    pub fn government_metric_value(&self, entity: EntityId, name: &str) -> Result<i64, Error> {
        if !self.government_metric_active(entity, name)? {
            return Err(Error::Invalid(format!("inactive government metric {name}")));
        }
        self.governments().countries[&entity]
            .metrics
            .as_ref()
            .and_then(|metrics| metrics[name])
            .ok_or_else(|| {
                Error::Invalid(format!(
                    "unknown government metric value {name} for entity {}",
                    entity.0
                ))
            })
    }

    /// Whether an active or explicitly retained inactive value is stored.
    /// Presence does not imply that the stored value is known.
    pub fn government_metric_stored(&self, entity: EntityId, name: &str) -> Result<bool, Error> {
        self.governments().metric_definition(name)?;
        let state = self.government_country(entity)?;
        let active = state.metrics.as_ref().ok_or_else(|| {
            Error::Invalid(format!(
                "unknown government metric selection for entity {}",
                entity.0
            ))
        })?;
        Ok(active.contains_key(name) || state.inactive_metrics.contains_key(name))
    }

    /// Read a stored metric without changing its activity or applying its effects.
    pub fn government_metric_stored_value(
        &self,
        entity: EntityId,
        name: &str,
    ) -> Result<i64, Error> {
        self.government_metric_stored(entity, name)?;
        let state = self.government_country(entity)?;
        state
            .metrics
            .as_ref()
            .expect("validated metric selection")
            .get(name)
            .or_else(|| state.inactive_metrics.get(name))
            .copied()
            .flatten()
            .ok_or_else(|| {
                Error::Invalid(format!(
                    "unknown stored government metric {name} for entity {}",
                    entity.0
                ))
            })
    }
}

pub(crate) fn validate_identifier(name: &str) -> Result<(), Error> {
    if name.trim().is_empty() {
        Err(Error::Invalid("empty government identifier".into()))
    } else {
        Ok(())
    }
}
