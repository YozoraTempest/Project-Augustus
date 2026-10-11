// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{EntityId, EntityKind, Error, ModifierDefinition, World};

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EstateEffect {
    pub modifier: ModifierDefinition,
    pub unresolved_properties: BTreeSet<String>,
    pub unresolved_effects: BTreeSet<String>,
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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EstateInfluenceBand {
    pub minimum_milli: i64,
    pub scale_micros: i64,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EstateRules {
    pub angry_below_milli: Option<i64>,
    pub happy_from_milli: Option<i64>,
    /// None means that the influence-to-effect rule has not been established.
    pub influence_bands: Option<Vec<EstateInfluenceBand>>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EstateState {
    pub active: Option<bool>,
    pub loyalty_milli: Option<i64>,
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
        self.evaluate(entity, None)
    }

    pub fn property_effects(
        &self,
        entity: EntityId,
        property: &str,
    ) -> Result<EstateEvaluation, Error> {
        self.evaluate(entity, Some(property))
    }

    fn evaluate(
        &self,
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
                append(
                    &mut result,
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
            append(
                &mut result,
                format!("{prefix}:land"),
                &definition.land_ownership,
                state.land_share_micros,
                property,
            )?;
            if let Some(selected) = &state.privileges {
                for id in selected {
                    let source = format!("{prefix}:privilege:{id}");
                    if let Some(privilege) = self.privileges.get(id) {
                        append(
                            &mut result,
                            source.clone(),
                            &self.resolved_effect(entity, id, &privilege.effect),
                            Some(1_000_000),
                            property,
                        )?;
                        append(
                            &mut result,
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
                validate_effect(e)?;
            }
        }
        for (id, privilege) in &self.privileges {
            if id.is_empty() {
                return Err(Error::Invalid("empty estate privilege".into()));
            }
            validate_effect(&privilege.effect)?;
            validate_effect(&privilege.land_ownership)?;
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
            self.declared_effects(entity)?;
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

fn validate_effect(effect: &EstateEffect) -> Result<(), Error> {
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
