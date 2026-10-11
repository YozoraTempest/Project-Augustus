// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{
    AdministrativeCategory, AffiliationRelation, EntityId, EntityKind, Error, FieldId,
    ModifierDefinition, Target, World,
};

const ONE: i128 = 1_000_000;

type ResolvedContributions = (Vec<(String, String, i64)>, bool);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AttributeBase {
    Constant {
        micros: i64,
    },
    Field {
        field: FieldId,
        micros_per_unit: i64,
    },
    Unknown {
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AttributeTerm {
    /// Content chooses a static source for each category. Omitted categories
    /// intentionally add no effect; missing selected definitions remain unknown.
    Administration {
        mapping: String,
        property: String,
    },
    Ideas {
        target: Target,
        property: String,
    },
    Government {
        target: Target,
        property: String,
    },
    /// A separate multiplicative channel applies one minus effective autonomy.
    Autonomy,
    AffiliationRelation {
        dimension: String,
        target: Target,
        definitions: BTreeMap<AffiliationRelation, String>,
        property: String,
    },
    AffiliationModifiers {
        dimension: String,
        target: Target,
        scope: String,
        property: String,
    },
    Constant {
        source: String,
        micros: i64,
    },
    Static {
        definition: String,
        property: String,
    },
    Buildings {
        property: String,
    },
    TradeGood {
        property: String,
    },
    Modifiers {
        target: Target,
        property: String,
    },
}

/// Each channel evaluates (input + sum(flat)) * (1 + sum(rates)).
/// Successive channels multiply separately and can add values after a prior rate.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttributeChannel {
    pub flat: Vec<AttributeTerm>,
    pub rates: Vec<AttributeTerm>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttributeFormula {
    pub base: AttributeBase,
    pub channels: Vec<AttributeChannel>,
    pub minimum_micros: Option<i64>,
    pub maximum_micros: Option<i64>,
    pub unresolved: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AttributeContribution {
    pub source: String,
    pub property: String,
    pub micros: i64,
    pub is_rate: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AttributeStep {
    pub input_micros: Option<i64>,
    pub flat_micros: i64,
    pub rate_micros: i64,
    pub output_micros: Option<i64>,
    pub contributions: Vec<AttributeContribution>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AttributeEvaluation {
    pub base_micros: Option<i64>,
    pub known_subtotal_micros: Option<i64>,
    pub value_micros: Option<i64>,
    pub unresolved: BTreeSet<String>,
    pub steps: Vec<AttributeStep>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AttributeAggregate {
    pub province_count: usize,
    pub known_subtotal_micros: i64,
    pub value_micros: Option<i64>,
    pub unresolved: BTreeSet<String>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AffiliationEffect {
    pub modifier: ModifierDefinition,
    pub unresolved_properties: BTreeSet<String>,
}

/// Immutable calculation content. Inputs and attached effects are read from the
/// current world on every query; derived results are not persisted as a cache.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Attributes {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub administration_mappings: BTreeMap<String, BTreeMap<AdministrativeCategory, String>>,
    pub static_modifiers: BTreeMap<String, ModifierDefinition>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub static_unresolved_properties: BTreeMap<String, BTreeSet<String>>,
    pub building_modifiers: BTreeMap<String, ModifierDefinition>,
    pub trade_good_modifiers: BTreeMap<String, ModifierDefinition>,
    /// Dimension, effect scope, then member. Missing entries remain unknown.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub affiliation_modifiers:
        BTreeMap<String, BTreeMap<String, BTreeMap<String, AffiliationEffect>>>,
    pub formulas: BTreeMap<EntityId, BTreeMap<String, AttributeFormula>>,
}

fn narrow(value: i128) -> Result<i64, Error> {
    i64::try_from(value).map_err(|_| Error::Overflow)
}

impl Attributes {
    pub fn is_empty(&self) -> bool {
        self.static_modifiers.is_empty()
            && self.administration_mappings.is_empty()
            && self.building_modifiers.is_empty()
            && self.trade_good_modifiers.is_empty()
            && self.affiliation_modifiers.is_empty()
            && self.formulas.is_empty()
    }

    pub fn evaluate(
        &self,
        world: &World,
        entity: EntityId,
        name: &str,
    ) -> Result<AttributeEvaluation, Error> {
        world.entity(entity)?;
        let formula = self
            .formulas
            .get(&entity)
            .and_then(|f| f.get(name))
            .ok_or_else(|| Error::Invalid(format!("unknown attribute {name}")))?;
        let mut unresolved = formula.unresolved.clone();
        let base = match &formula.base {
            AttributeBase::Constant { micros } => Some(*micros),
            AttributeBase::Field {
                field,
                micros_per_unit,
            } => Some(narrow(
                i128::from(world.value(entity, *field)?) * i128::from(*micros_per_unit),
            )?),
            AttributeBase::Unknown { reason } => {
                unresolved.insert(reason.clone());
                None
            }
        };
        let mut value = base;
        let mut steps = Vec::new();
        for channel in &formula.channels {
            let input = value;
            let mut contributions = Vec::new();
            let mut flat = 0_i128;
            let mut rate = 0_i128;
            for (terms, is_rate) in [(&channel.flat, false), (&channel.rates, true)] {
                for term in terms {
                    let (resolved, known) = self.resolve(world, entity, term, &mut unresolved)?;
                    if !known {
                        value = None;
                    }
                    for (source, property, micros) in resolved {
                        if is_rate {
                            rate += i128::from(micros);
                        } else {
                            flat += i128::from(micros);
                        }
                        contributions.push(AttributeContribution {
                            source,
                            property,
                            micros,
                            is_rate,
                        });
                    }
                }
            }
            let flat = narrow(flat)?;
            let rate = narrow(rate)?;
            value = value
                .map(|v| {
                    (i128::from(v) + i128::from(flat))
                        .checked_mul(ONE + i128::from(rate))
                        .ok_or(Error::Overflow)
                        .and_then(|v| narrow(v / ONE))
                })
                .transpose()?;
            steps.push(AttributeStep {
                input_micros: input,
                flat_micros: flat,
                rate_micros: rate,
                output_micros: value,
                contributions,
            });
        }
        if let Some(v) = value.as_mut() {
            if let Some(min) = formula.minimum_micros {
                *v = (*v).max(min);
            }
            if let Some(max) = formula.maximum_micros {
                *v = (*v).min(max);
            }
        }
        Ok(AttributeEvaluation {
            base_micros: base,
            known_subtotal_micros: value,
            value_micros: if unresolved.is_empty() { value } else { None },
            unresolved,
            steps,
        })
    }

    pub fn sum_owned(
        &self,
        world: &World,
        country: EntityId,
        name: &str,
    ) -> Result<AttributeAggregate, Error> {
        if world.entity(country)?.kind != EntityKind::Country {
            return Err(Error::Invalid(
                "attribute aggregation requires a country".into(),
            ));
        }
        let mut subtotal = 0_i128;
        let mut unresolved = BTreeSet::new();
        let mut count = 0;
        for (index, province) in world.entities().iter().enumerate() {
            if province.kind != EntityKind::Province || province.owner != Some(country) {
                continue;
            }
            count += 1;
            let entity = EntityId(index);
            if !self
                .formulas
                .get(&entity)
                .is_some_and(|f| f.contains_key(name))
            {
                unresolved.insert(format!(
                    "province:{}:missing_attribute:{name}",
                    province.name
                ));
                continue;
            }
            let result = self.evaluate(world, entity, name)?;
            if let Some(value) = result.known_subtotal_micros {
                subtotal += i128::from(value);
            } else {
                let missing = if result.base_micros.is_none() {
                    "unknown_base"
                } else {
                    "unknown_calculation"
                };
                unresolved.insert(format!("province:{}:{missing}", province.name));
            }
            unresolved.extend(result.unresolved);
        }
        let subtotal = narrow(subtotal)?;
        Ok(AttributeAggregate {
            province_count: count,
            known_subtotal_micros: subtotal,
            value_micros: unresolved.is_empty().then_some(subtotal),
            unresolved,
        })
    }

    fn resolve(
        &self,
        world: &World,
        entity: EntityId,
        term: &AttributeTerm,
        unresolved: &mut BTreeSet<String>,
    ) -> Result<ResolvedContributions, Error> {
        let mut values = Vec::new();
        let mut append = |source: String,
                          property: &str,
                          definition: &ModifierDefinition|
         -> Result<(), Error> {
            if definition.flags.contains_key(property) {
                return Err(Error::Invalid(format!(
                    "attribute requires numeric property {property}"
                )));
            }
            if let Some(&micros) = definition.values_micros.get(property) {
                values.push((source, property.into(), micros));
            }
            Ok(())
        };
        match term {
            AttributeTerm::Administration { mapping, property } => {
                let definitions = self.administration_mappings.get(mapping).ok_or_else(|| {
                    Error::Invalid("unknown administrative attribute mapping".into())
                })?;
                if world.administration().provinces.contains_key(&entity) {
                    let state = world.administration().evaluate(world, entity)?;
                    if let Some(category) = state.category() {
                        if let Some(name) = definitions.get(&category) {
                            if let Some(data) = self.static_modifiers.get(name) {
                                append(format!("static:{name}"), property, data)?;
                                if self
                                    .static_unresolved_properties
                                    .get(name)
                                    .is_some_and(|properties| properties.contains(property))
                                {
                                    unresolved.insert(format!(
                                        "administration:unresolved_property:{name}:{property}"
                                    ));
                                }
                            } else {
                                unresolved
                                    .insert(format!("administration:missing_definition:{name}"));
                            }
                        }
                    } else {
                        unresolved.insert("administration:unknown_category".into());
                    }
                } else {
                    unresolved.insert("administration:missing_province".into());
                }
            }
            AttributeTerm::Ideas { target, property } => {
                if let Some(target) = attribute_target(world, entity, *target)? {
                    let result = world.ideas().property_effects(target, property)?;
                    unresolved.extend(result.unresolved);
                    values.extend(
                        result
                            .contributions
                            .into_iter()
                            .map(|v| (v.source, v.property, v.micros)),
                    );
                } else {
                    unresolved.insert(format!("target:{target:?}:unknown"));
                }
            }
            AttributeTerm::Government { target, property } => {
                if let Some(target) = attribute_target(world, entity, *target)? {
                    let result = world.governments().property_effects(target, property)?;
                    unresolved.extend(result.unresolved);
                    values.extend(
                        result
                            .contributions
                            .into_iter()
                            .map(|v| (v.source, v.property, v.micros)),
                    );
                } else {
                    unresolved.insert(format!("target:{target:?}:unknown"));
                }
            }
            AttributeTerm::AffiliationRelation {
                dimension,
                target,
                definitions,
                property,
            } => {
                let relation = attribute_target(world, entity, *target)?
                    .and_then(|other| world.affiliations().relation(dimension, entity, other));
                if let Some(relation) = relation {
                    if let Some(name) = definitions.get(&relation) {
                        if let Some(data) = self.static_modifiers.get(name) {
                            append(format!("static:{name}"), property, data)?;
                        } else {
                            unresolved.insert(format!(
                                "affiliation:{dimension}:missing_definition:{name}"
                            ));
                        }
                    }
                } else {
                    unresolved.insert(format!("affiliation:{dimension}:unknown_relation"));
                }
            }
            AttributeTerm::AffiliationModifiers {
                dimension,
                target,
                scope,
                property,
            } => {
                let member = attribute_target(world, entity, *target)?
                    .and_then(|other| world.affiliations().member(dimension, other));
                if let Some(member) = member {
                    let prefix = format!("affiliation:{dimension}:{scope}:{member}");
                    if let Some(data) = self
                        .affiliation_modifiers
                        .get(dimension)
                        .and_then(|scopes| scopes.get(scope))
                        .and_then(|members| members.get(member))
                    {
                        append(prefix.clone(), property, &data.modifier)?;
                        if data.unresolved_properties.contains(property) {
                            unresolved.insert(format!("{prefix}:ambiguous:{property}"));
                        }
                    } else {
                        unresolved.insert(format!("{prefix}:missing_definition"));
                    }
                } else {
                    unresolved.insert(format!("affiliation:{dimension}:{scope}:unknown_member"));
                }
            }
            AttributeTerm::Autonomy => {
                if world.administration().provinces.contains_key(&entity) {
                    let state = world.administration().evaluate(world, entity)?;
                    unresolved.extend(
                        state
                            .unresolved
                            .into_iter()
                            .map(|r| format!("administration:{r}")),
                    );
                    if let Some(value) = state.effective_autonomy_micros {
                        values.push((
                            "administration:autonomy".into(),
                            "effective_autonomy".into(),
                            -value,
                        ));
                    } else {
                        return Ok((values, false));
                    }
                } else {
                    unresolved.insert("administration:missing_province".into());
                    return Ok((values, false));
                }
            }
            AttributeTerm::Constant { source, micros } => {
                values.push((source.clone(), String::new(), *micros))
            }
            AttributeTerm::Static {
                definition,
                property,
            } => {
                let data = self
                    .static_modifiers
                    .get(definition)
                    .ok_or_else(|| Error::Invalid("unknown static attribute source".into()))?;
                append(format!("static:{definition}"), property, data)?;
            }
            AttributeTerm::Buildings { property } => {
                if let Some(state) = world.economy().provinces.get(&entity) {
                    for name in &state.buildings {
                        if let Some(data) = self.building_modifiers.get(name) {
                            append(format!("building:{name}"), property, data)?;
                        } else {
                            unresolved.insert(format!("building:{name}:missing_definition"));
                        }
                    }
                } else {
                    unresolved.insert("buildings:unknown_history".into());
                }
            }
            AttributeTerm::TradeGood { property } => {
                if let Some(name) = world
                    .economy()
                    .provinces
                    .get(&entity)
                    .and_then(|s| s.trade_good.as_ref())
                {
                    if let Some(data) = self.trade_good_modifiers.get(name) {
                        append(format!("trade_good:{name}"), property, data)?;
                    } else {
                        unresolved.insert(format!("trade_good:{name}:missing_definition"));
                    }
                } else {
                    unresolved.insert("trade_good:unknown_history".into());
                }
            }
            AttributeTerm::Modifiers { target, property } => {
                let target_entity = attribute_target(world, entity, *target)?;
                if let Some(target_entity) = target_entity {
                    if let Some(active) = world.modifiers().active.get(&target_entity) {
                        for name in active.keys() {
                            let data =
                                world.modifiers().definitions.get(name).ok_or_else(|| {
                                    Error::Invalid("unknown attached attribute source".into())
                                })?;
                            append(
                                format!("modifier:{}:{name}", world.entity(target_entity)?.name),
                                property,
                                data,
                            )?;
                        }
                    }
                } else {
                    unresolved.insert(format!("target:{target:?}:unknown"));
                }
            }
        }
        Ok((values, true))
    }

    pub(crate) fn validate(&self, world: &World) -> Result<(), Error> {
        for (name, definitions) in &self.administration_mappings {
            if name.is_empty() || definitions.values().any(String::is_empty) {
                return Err(Error::Invalid(
                    "invalid administrative attribute mapping".into(),
                ));
            }
        }
        for (name, properties) in &self.static_unresolved_properties {
            let definition = self.static_modifiers.get(name).ok_or_else(|| {
                Error::Invalid("unresolved properties reference an unknown static modifier".into())
            })?;
            if properties.iter().any(|property| {
                property.is_empty()
                    || definition.values_micros.contains_key(property)
                    || definition.flags.contains_key(property)
            }) {
                return Err(Error::Invalid("invalid unresolved static property".into()));
            }
        }
        for (dimension, scopes) in &self.affiliation_modifiers {
            if dimension.is_empty() || scopes.keys().any(String::is_empty) {
                return Err(Error::Invalid("invalid affiliation effect scope".into()));
            }
            for members in scopes.values() {
                for (name, effect) in members {
                    if name.is_empty() || effect.unresolved_properties.iter().any(String::is_empty)
                    {
                        return Err(Error::Invalid("invalid affiliation effect".into()));
                    }
                    validate_definition(name, &effect.modifier)?;
                }
            }
        }
        for catalog in [
            &self.static_modifiers,
            &self.building_modifiers,
            &self.trade_good_modifiers,
        ] {
            for (name, definition) in catalog {
                validate_definition(name, definition)?;
            }
        }
        for (&entity, formulas) in &self.formulas {
            world.entity(entity)?;
            for (name, formula) in formulas {
                if name.is_empty()
                    || matches!(&formula.base, AttributeBase::Unknown { reason } if reason.is_empty())
                    || formula.unresolved.iter().any(|s| s.is_empty())
                    || matches!((formula.minimum_micros, formula.maximum_micros), (Some(min),Some(max)) if min > max)
                {
                    return Err(Error::Invalid("invalid attribute formula".into()));
                }
                for term in formula
                    .channels
                    .iter()
                    .flat_map(|c| c.flat.iter().chain(&c.rates))
                {
                    let valid = match term {
                        AttributeTerm::Administration { mapping, property } => {
                            world.entity(entity)?.kind == EntityKind::Province
                                && !property.is_empty()
                                && self.administration_mappings.contains_key(mapping)
                        }
                        AttributeTerm::AffiliationRelation {
                            dimension,
                            definitions,
                            property,
                            ..
                        } => {
                            !dimension.is_empty()
                                && !property.is_empty()
                                && definitions.values().all(|s| !s.is_empty())
                        }
                        AttributeTerm::AffiliationModifiers {
                            dimension,
                            scope,
                            property,
                            ..
                        } => !dimension.is_empty() && !scope.is_empty() && !property.is_empty(),
                        AttributeTerm::Autonomy => {
                            world.entity(entity)?.kind == EntityKind::Province
                                && formula.channels.iter().all(|c| {
                                    !c.flat.iter().any(|t| matches!(t, AttributeTerm::Autonomy))
                                })
                        }
                        AttributeTerm::Constant { source, .. } => !source.is_empty(),
                        AttributeTerm::Static {
                            definition,
                            property,
                        } => !definition.is_empty() && !property.is_empty(),
                        AttributeTerm::Buildings { property }
                        | AttributeTerm::TradeGood { property } => {
                            world.entity(entity)?.kind == EntityKind::Province
                                && !property.is_empty()
                        }
                        AttributeTerm::Modifiers { property, .. }
                        | AttributeTerm::Government { property, .. }
                        | AttributeTerm::Ideas { property, .. } => !property.is_empty(),
                    };
                    if !valid {
                        return Err(Error::Invalid("invalid attribute term".into()));
                    }
                }
                self.evaluate(world, entity, name)?;
            }
        }
        let names: BTreeSet<_> = self
            .formulas
            .iter()
            .filter(|(entity, _)| {
                world
                    .entity(**entity)
                    .is_ok_and(|e| e.kind == EntityKind::Province)
            })
            .flat_map(|(_, formulas)| formulas.keys())
            .collect();
        for (index, _) in world
            .entities()
            .iter()
            .enumerate()
            .filter(|(_, e)| e.kind == EntityKind::Country)
        {
            for name in &names {
                self.sum_owned(world, EntityId(index), name)?;
            }
        }
        Ok(())
    }
}

fn attribute_target(
    world: &World,
    entity: EntityId,
    target: Target,
) -> Result<Option<EntityId>, Error> {
    let entity_data = world.entity(entity)?;
    Ok(match target {
        Target::Current => Some(entity),
        Target::Owner => entity_data.owner,
        Target::Capital => entity_data.capital,
    })
}

fn validate_definition(name: &str, definition: &ModifierDefinition) -> Result<(), Error> {
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
        return Err(Error::Invalid("invalid attribute source definition".into()));
    }
    Ok(())
}
