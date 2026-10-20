// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{Condition, EntityId, EntityKind, Error, ModifierDefinition, ScopeContext, World};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EstateConditionalScale {
    Fixed,
    Influence,
    Loyalty,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EstateConditionalModifier {
    pub condition: Condition,
    pub scale: EstateConditionalScale,
    pub modifier: ModifierDefinition,
    pub unresolved_properties: BTreeSet<String>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EstateEffect {
    pub modifier: ModifierDefinition,
    pub unresolved_properties: BTreeSet<String>,
    pub unresolved_effects: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conditional_modifiers: Vec<EstateConditionalModifier>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EstateDefinition {
    pub angry: EstateEffect,
    pub neutral: EstateEffect,
    pub happy: EstateEffect,
    pub land_ownership: EstateEffect,
    pub privileges: BTreeSet<String>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EstatePrivilege {
    pub effect: EstateEffect,
    pub land_ownership: EstateEffect,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub influence_milli: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EstateInfluenceBand {
    pub minimum_milli: i64,
    pub scale_micros: i64,
}

/// Inputs for the verified influence-to-modifier scale stage.
///
/// The compatibility layer supplies the queried influence, its additional
/// context value, and the three runtime thresholds. The queried influence is
/// clamped before the additional value is added.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaEstateInfluenceScaleInput {
    pub raw_influence_milli: i32,
    pub additional_value_milli: i32,
    pub first_threshold_milli: i32,
    pub second_threshold_milli: i32,
    pub third_threshold_milli: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VanillaEstateInfluenceScaleResult {
    pub clamped_influence_milli: i32,
    pub tier_input_milli: i32,
    pub scale_milli: i32,
}

/// Calculates the scale selected for an estate source's modifier values.
///
/// A zero tier input contributes no scale. Other values use strict
/// less-than comparisons and map to 250, 500, 750, or 1000 thousandths.
pub fn calculate_vanilla_estate_influence_scale(
    input: VanillaEstateInfluenceScaleInput,
) -> VanillaEstateInfluenceScaleResult {
    let clamped_influence_milli = input.raw_influence_milli.clamp(0, 100_000);
    let tier_input_milli = clamped_influence_milli.wrapping_add(input.additional_value_milli);
    let scale_milli = if tier_input_milli == 0 {
        0
    } else if tier_input_milli < input.first_threshold_milli {
        250
    } else if tier_input_milli < input.second_threshold_milli {
        500
    } else if tier_input_milli < input.third_threshold_milli {
        750
    } else {
        1_000
    };

    VanillaEstateInfluenceScaleResult {
        clamped_influence_milli,
        tier_input_milli,
        scale_milli,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EstateLoyaltyDecay {
    pub base_min_annual_milli: i64,
    pub base_max_annual_milli: i64,
}

impl EstateLoyaltyDecay {
    /// Return the loyalty value after one scheduled update, using signed
    /// interpolation, monthly division, and the 0-to-100 clamp.
    pub fn next_loyalty_milli(&self, current_milli: i64, target_milli: i64) -> Result<i64, Error> {
        self.validate()?;
        if !(0..=100_000).contains(&current_milli) {
            return Err(Error::Invalid("invalid current estate loyalty".into()));
        }
        if !(i64::from(i32::MIN)..=i64::from(i32::MAX)).contains(&target_milli) {
            return Err(Error::Invalid("invalid estate loyalty target".into()));
        }

        let difference = i128::from(target_milli) - i128::from(current_milli);
        if difference == 0 {
            return Ok(current_milli);
        }

        let minimum = i128::from(self.base_min_annual_milli);
        let spread = i128::from(self.base_max_annual_milli - self.base_min_annual_milli);
        let interpolation = spread.checked_mul(difference).ok_or(Error::Overflow)? / 50_000;
        let signed_minimum = minimum * difference.signum();
        let monthly_delta = (signed_minimum + interpolation) / 12;
        let next = i128::from(current_milli) + monthly_delta;
        Ok(next.clamp(0, 100_000) as i64)
    }

    fn validate(&self) -> Result<(), Error> {
        if self.base_min_annual_milli < 0
            || self.base_max_annual_milli < self.base_min_annual_milli
            || self.base_max_annual_milli > i64::from(i32::MAX)
        {
            return Err(Error::Invalid("invalid estate loyalty decay rules".into()));
        }
        Ok(())
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EstateRules {
    pub angry_below_milli: Option<i64>,
    pub happy_from_milli: Option<i64>,
    /// None means that the influence-to-effect rule has not been established.
    pub influence_bands: Option<Vec<EstateInfluenceBand>>,
    /// Base term for a loyalty target; this is not an estate's initial loyalty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loyalty_target_base_milli: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loyalty_decay: Option<EstateLoyaltyDecay>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EstateState {
    pub active: Option<bool>,
    pub loyalty_milli: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loyalty_target_milli: Option<i64>,
    pub influence_milli: Option<i64>,
    pub land_share_micros: Option<i64>,
    pub privileges: Option<BTreeSet<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EstateContribution {
    pub source: String,
    pub property: String,
    pub micros: i64,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
pub struct EstateEvaluation {
    pub known_totals_micros: BTreeMap<String, i64>,
    pub contributions: Vec<EstateContribution>,
    pub unresolved: BTreeSet<String>,
}

/// Current social groups and their content-defined effects. Native systems may
/// replace the complete state; queries never persist calculated modifier totals.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Estates {
    pub definitions: BTreeMap<String, EstateDefinition>,
    pub privileges: BTreeMap<String, EstatePrivilege>,
    pub rules: EstateRules,
    pub countries: BTreeMap<EntityId, BTreeMap<String, EstateState>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub unresolved_countries: BTreeMap<EntityId, BTreeSet<String>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub resolved_privilege_effects: BTreeMap<EntityId, BTreeMap<String, BTreeSet<String>>>,
}

impl Estates {
    pub fn is_empty(&self) -> bool {
        self.definitions.is_empty()
            && self.privileges.is_empty()
            && self.countries.is_empty()
            && self.unresolved_countries.is_empty()
            && self.resolved_privilege_effects.is_empty()
            && self.rules == EstateRules::default()
    }

    pub fn declared_effects(&self, entity: EntityId) -> Result<EstateEvaluation, Error> {
        self.evaluate(None, entity, None)
    }

    pub fn declared_effects_with_world(
        &self,
        world: &World,
        entity: EntityId,
    ) -> Result<EstateEvaluation, Error> {
        self.evaluate(Some(world), entity, None)
    }

    pub fn property_effects(
        &self,
        entity: EntityId,
        property: &str,
    ) -> Result<EstateEvaluation, Error> {
        self.evaluate(None, entity, Some(property))
    }

    pub fn property_effects_with_world(
        &self,
        world: &World,
        entity: EntityId,
        property: &str,
    ) -> Result<EstateEvaluation, Error> {
        self.evaluate(Some(world), entity, Some(property))
    }

    fn evaluate(
        &self,
        world: Option<&World>,
        entity: EntityId,
        property: Option<&str>,
    ) -> Result<EstateEvaluation, Error> {
        let mut result = EstateEvaluation::default();
        if let Some(reasons) = self.unresolved_countries.get(&entity) {
            result
                .unresolved
                .extend(reasons.iter().map(|r| format!("estates:{r}")));
        }
        for (id, definition) in &self.definitions {
            let prefix = format!("estates:{id}");
            let state = self
                .countries
                .get(&entity)
                .and_then(|states| states.get(id));
            if state.and_then(|s| s.active) == Some(false) {
                continue;
            }
            let effects = [
                &definition.angry,
                &definition.neutral,
                &definition.happy,
                &definition.land_ownership,
            ];
            if state.and_then(|s| s.active).is_none() {
                let candidates = state
                    .and_then(|s| s.privileges.as_ref())
                    .unwrap_or(&definition.privileges);
                if effects.iter().any(|e| relevant(e, property))
                    || candidates.iter().any(|id| {
                        self.privileges.get(id).is_none_or(|p| {
                            relevant(&p.effect, property) || relevant(&p.land_ownership, property)
                        })
                    })
                {
                    result
                        .unresolved
                        .insert(format!("{prefix}:unknown_presence"));
                }
                continue;
            }
            let state = state.expect("known presence");
            let band = state.loyalty_milli.and_then(|loyalty| {
                Some(if loyalty < self.rules.angry_below_milli? {
                    ("angry", &definition.angry)
                } else if loyalty >= self.rules.happy_from_milli? {
                    ("happy", &definition.happy)
                } else {
                    ("neutral", &definition.neutral)
                })
            });
            if let Some((name, effect)) = band {
                let scale = state.influence_milli.and_then(|influence| {
                    self.rules
                        .influence_bands
                        .as_ref()?
                        .iter()
                        .rev()
                        .find(|b| influence >= b.minimum_milli)
                        .map(|b| b.scale_micros)
                });
                append_with_conditionals(
                    &mut result,
                    world,
                    entity,
                    Some(state),
                    format!("{prefix}:{name}"),
                    effect,
                    scale,
                    property,
                )?;
            } else if effects[..3].iter().any(|e| relevant(e, property)) {
                result
                    .unresolved
                    .insert(format!("{prefix}:unknown_loyalty_band"));
            }
            append_with_conditionals(
                &mut result,
                world,
                entity,
                Some(state),
                format!("{prefix}:land"),
                &definition.land_ownership,
                state.land_share_micros,
                property,
            )?;
            if let Some(selected) = &state.privileges {
                for id in selected {
                    let source = format!("{prefix}:privilege:{id}");
                    if let Some(privilege) = self.privileges.get(id) {
                        append_with_conditionals(
                            &mut result,
                            world,
                            entity,
                            Some(state),
                            source.clone(),
                            &self.resolved_effect(entity, id, &privilege.effect),
                            Some(1_000_000),
                            property,
                        )?;
                        append_with_conditionals(
                            &mut result,
                            world,
                            entity,
                            Some(state),
                            format!("{source}:land"),
                            &privilege.land_ownership,
                            state.land_share_micros,
                            property,
                        )?;
                    } else {
                        result
                            .unresolved
                            .insert(format!("{source}:missing_definition"));
                    }
                }
            } else if definition.privileges.iter().any(|id| {
                self.privileges.get(id).is_none_or(|p| {
                    relevant(&p.effect, property) || relevant(&p.land_ownership, property)
                })
            }) {
                result
                    .unresolved
                    .insert(format!("{prefix}:unknown_privileges"));
            }
        }
        Ok(result)
    }

    fn resolved_effect(&self, entity: EntityId, id: &str, effect: &EstateEffect) -> EstateEffect {
        let mut result = effect.clone();
        if let Some(resolved) = self
            .resolved_privilege_effects
            .get(&entity)
            .and_then(|p| p.get(id))
        {
            result
                .unresolved_effects
                .retain(|effect| !resolved.contains(effect));
        }
        result
    }

    pub(crate) fn validate(&self, world: &World) -> Result<(), Error> {
        for (&entity, privileges) in &self.resolved_privilege_effects {
            if world.entity(entity)?.kind != EntityKind::Country || privileges.is_empty() {
                return Err(Error::Invalid("invalid resolved estate effects".into()));
            }
            for (id, effects) in privileges {
                if effects.is_empty()
                    || self.countries.get(&entity).is_none_or(|states| {
                        !states.values().any(|s| {
                            s.active == Some(true)
                                && s.privileges.as_ref().is_some_and(|ids| ids.contains(id))
                        })
                    })
                    || self
                        .privileges
                        .get(id)
                        .is_none_or(|p| !effects.is_subset(&p.effect.unresolved_effects))
                {
                    return Err(Error::Invalid("invalid resolved privilege effect".into()));
                }
            }
        }
        let ratio = |v: Option<i64>| v.is_none_or(|v| (0..=1_000_000).contains(&v));
        let percent = |v: Option<i64>| v.is_none_or(|v| (0..=100_000).contains(&v));
        if !percent(self.rules.angry_below_milli)
            || !percent(self.rules.happy_from_milli)
            || self
                .rules
                .angry_below_milli
                .zip(self.rules.happy_from_milli)
                .is_some_and(|(a, h)| a > h)
        {
            return Err(Error::Invalid("invalid estate loyalty thresholds".into()));
        }
        if self
            .rules
            .loyalty_target_base_milli
            .is_some_and(|value| !(0..=i64::from(i32::MAX)).contains(&value))
        {
            return Err(Error::Invalid("invalid estate loyalty target base".into()));
        }
        if let Some(bands) = &self.rules.influence_bands
            && (bands.first().is_none_or(|b| b.minimum_milli != 0)
                || bands
                    .iter()
                    .any(|b| b.minimum_milli < 0 || b.scale_micros < 0)
                || bands
                    .windows(2)
                    .any(|v| v[0].minimum_milli >= v[1].minimum_milli))
        {
            return Err(Error::Invalid("invalid estate influence bands".into()));
        }
        if let Some(decay) = &self.rules.loyalty_decay {
            decay.validate()?;
        }
        for (id, definition) in &self.definitions {
            if id.is_empty() || definition.privileges.iter().any(String::is_empty) {
                return Err(Error::Invalid("invalid estate definition".into()));
            }
            for e in [
                &definition.angry,
                &definition.neutral,
                &definition.happy,
                &definition.land_ownership,
            ] {
                validate_effect(e, world)?;
            }
        }
        for (id, privilege) in &self.privileges {
            if id.is_empty() {
                return Err(Error::Invalid("empty estate privilege".into()));
            }
            validate_effect(&privilege.effect, world)?;
            validate_effect(&privilege.land_ownership, world)?;
        }
        for (&entity, states) in &self.countries {
            if world.entity(entity)?.kind != EntityKind::Country {
                return Err(Error::Invalid("estate state requires country".into()));
            }
            let mut land = 0_i64;
            for (id, state) in states {
                let definition = self
                    .definitions
                    .get(id)
                    .ok_or_else(|| Error::Invalid("unknown estate state".into()))?;
                if !percent(state.loyalty_milli)
                    || state
                        .loyalty_target_milli
                        .is_some_and(|v| !(0..=i64::from(i32::MAX)).contains(&v))
                    || state.influence_milli.is_some_and(|v| v < 0)
                    || !ratio(state.land_share_micros)
                    || state
                        .privileges
                        .as_ref()
                        .is_some_and(|ids| ids.iter().any(|p| !definition.privileges.contains(p)))
                {
                    return Err(Error::Invalid("invalid estate state".into()));
                }
                if state.active == Some(true) {
                    land = land
                        .checked_add(state.land_share_micros.unwrap_or(0))
                        .ok_or(Error::Overflow)?;
                }
            }
            if land > 1_000_000 {
                return Err(Error::Invalid(
                    "estate land shares exceed total land".into(),
                ));
            }
            self.declared_effects_with_world(world, entity)?;
        }
        for (&entity, reasons) in &self.unresolved_countries {
            if world.entity(entity)?.kind != EntityKind::Country
                || reasons.iter().any(String::is_empty)
            {
                return Err(Error::Invalid("invalid unresolved estate country".into()));
            }
        }
        Ok(())
    }
}

fn relevant(effect: &EstateEffect, property: Option<&str>) -> bool {
    !effect.unresolved_effects.is_empty()
        || property.map_or_else(
            || {
                !effect.modifier.values_micros.is_empty()
                    || !effect.modifier.flags.is_empty()
                    || !effect.unresolved_properties.is_empty()
            },
            |p| {
                effect.modifier.values_micros.contains_key(p)
                    || effect.modifier.flags.contains_key(p)
                    || effect.unresolved_properties.contains(p)
            },
        )
        || effect
            .conditional_modifiers
            .iter()
            .any(|modifier| conditional_relevant(modifier, property))
}

fn conditional_relevant(modifier: &EstateConditionalModifier, property: Option<&str>) -> bool {
    property.map_or_else(
        || {
            !modifier.modifier.values_micros.is_empty()
                || !modifier.modifier.flags.is_empty()
                || !modifier.unresolved_properties.is_empty()
        },
        |property| {
            modifier.modifier.values_micros.contains_key(property)
                || modifier.modifier.flags.contains_key(property)
                || modifier.unresolved_properties.contains(property)
        },
    )
}

#[allow(clippy::too_many_arguments)]
fn append_with_conditionals(
    result: &mut EstateEvaluation,
    world: Option<&World>,
    entity: EntityId,
    state: Option<&EstateState>,
    source: String,
    effect: &EstateEffect,
    scale: Option<i64>,
    property: Option<&str>,
) -> Result<(), Error> {
    append(result, source.clone(), effect, scale, property)?;
    let conditional_prefix = format!("{source}:conditional:");
    let mut tainted_properties = BTreeSet::new();
    for (index, conditional) in effect.conditional_modifiers.iter().enumerate() {
        if !conditional_relevant(conditional, property) {
            continue;
        }
        let conditional_source = format!("{source}:conditional:{index}");
        let Some(world) = world else {
            let properties = conditional_properties(conditional, property);
            result.unresolved.extend(
                properties
                    .iter()
                    .map(|property| format!("{conditional_source}:world_required:{property}")),
            );
            tainted_properties.extend(properties);
            continue;
        };
        match conditional
            .condition
            .evaluate_in_context(world, ScopeContext::new(entity))
        {
            Ok(false) => continue,
            Ok(true) => (),
            Err(error) => {
                let properties = conditional_properties(conditional, property);
                result.unresolved.extend(
                    properties.iter().map(|property| {
                        format!("{conditional_source}:condition:{error}:{property}")
                    }),
                );
                tainted_properties.extend(properties);
                continue;
            }
        }
        let factor_milli = match conditional.scale {
            EstateConditionalScale::Fixed => Some(1_000),
            EstateConditionalScale::Influence => state.and_then(|state| state.influence_milli),
            EstateConditionalScale::Loyalty => state.and_then(|state| state.loyalty_milli),
        };
        let scale = match conditional.scale {
            EstateConditionalScale::Fixed => Some(1_000_000),
            EstateConditionalScale::Influence | EstateConditionalScale::Loyalty => factor_milli
                .map(|value| (value / 100).checked_mul(1_000).ok_or(Error::Overflow))
                .transpose()?,
        };
        let Some(scale) = scale else {
            let properties = conditional_properties(conditional, property);
            result.unresolved.extend(
                properties
                    .iter()
                    .map(|property| format!("{conditional_source}:unknown_scale:{property}")),
            );
            tainted_properties.extend(properties);
            continue;
        };
        tainted_properties.extend(
            conditional
                .unresolved_properties
                .iter()
                .filter(|name| property.is_none_or(|selected| selected == name.as_str()))
                .cloned(),
        );
        let conditional_effect = EstateEffect {
            modifier: conditional.modifier.clone(),
            unresolved_properties: conditional.unresolved_properties.clone(),
            ..Default::default()
        };
        append(
            result,
            conditional_source,
            &conditional_effect,
            Some(scale),
            property,
        )?;
    }
    for property in tainted_properties {
        result.contributions.retain(|contribution| {
            contribution.property != property
                || (contribution.source != source
                    && !contribution.source.starts_with(&conditional_prefix))
        });
        rebuild_total(result, &property)?;
    }
    Ok(())
}

fn conditional_properties(
    conditional: &EstateConditionalModifier,
    property: Option<&str>,
) -> BTreeSet<String> {
    conditional
        .modifier
        .values_micros
        .keys()
        .chain(conditional.modifier.flags.keys())
        .chain(conditional.unresolved_properties.iter())
        .filter(|name| property.is_none_or(|selected| selected == name.as_str()))
        .cloned()
        .collect()
}

fn rebuild_total(result: &mut EstateEvaluation, property: &str) -> Result<(), Error> {
    let mut total = 0_i64;
    let mut found = false;
    for contribution in result
        .contributions
        .iter()
        .filter(|contribution| contribution.property == property)
    {
        found = true;
        total = total
            .checked_add(contribution.micros)
            .ok_or(Error::Overflow)?;
    }
    if found {
        result.known_totals_micros.insert(property.into(), total);
    } else {
        result.known_totals_micros.remove(property);
    }
    Ok(())
}

fn append(
    result: &mut EstateEvaluation,
    source: String,
    effect: &EstateEffect,
    scale: Option<i64>,
    property: Option<&str>,
) -> Result<(), Error> {
    if !relevant(effect, property) {
        return Ok(());
    }
    result.unresolved.extend(
        effect
            .unresolved_effects
            .iter()
            .map(|e| format!("{source}:unexecuted:{e}")),
    );
    result.unresolved.extend(
        effect
            .unresolved_properties
            .iter()
            .filter(|p| property.is_none_or(|q| q == *p))
            .map(|p| format!("{source}:unresolved:{p}")),
    );
    let Some(scale) = scale else {
        result.unresolved.insert(format!("{source}:unknown_scale"));
        return Ok(());
    };
    if property.is_some_and(|p| effect.modifier.flags.contains_key(p)) {
        return Err(Error::Invalid(
            "estate attribute requires numeric property".into(),
        ));
    }
    for (p, value) in effect
        .modifier
        .values_micros
        .iter()
        .filter(|(p, _)| property.is_none_or(|q| q == *p))
    {
        let micros = i64::try_from(i128::from(*value) * i128::from(scale) / 1_000_000)
            .map_err(|_| Error::Overflow)?;
        let total = result.known_totals_micros.entry(p.clone()).or_default();
        *total = total.checked_add(micros).ok_or(Error::Overflow)?;
        result.contributions.push(EstateContribution {
            source: source.clone(),
            property: p.clone(),
            micros,
        });
    }
    Ok(())
}

fn validate_effect(effect: &EstateEffect, world: &World) -> Result<(), Error> {
    validate_effect_contents(effect)?;
    for conditional in &effect.conditional_modifiers {
        crate::attributes::validate_attribute_condition(&conditional.condition)?;
        world.validate_condition(&conditional.condition)?;
        let modifier = EstateEffect {
            modifier: conditional.modifier.clone(),
            unresolved_properties: conditional.unresolved_properties.clone(),
            ..Default::default()
        };
        validate_effect_contents(&modifier)?;
    }
    Ok(())
}

fn validate_effect_contents(effect: &EstateEffect) -> Result<(), Error> {
    let m = &effect.modifier;
    if m.values_micros
        .keys()
        .chain(m.flags.keys())
        .any(String::is_empty)
        || m.values_micros.keys().any(|p| m.flags.contains_key(p))
        || effect
            .unresolved_properties
            .iter()
            .any(|p| p.is_empty() || m.values_micros.contains_key(p) || m.flags.contains_key(p))
        || effect.unresolved_effects.iter().any(String::is_empty)
    {
        return Err(Error::Invalid("invalid estate effect".into()));
    }
    Ok(())
}
