// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{AttributeTerm, EntityId, EntityKind, Error, ModifierDefinition, Target, World};

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdeaEffect {
    pub modifier: ModifierDefinition,
    pub unresolved_properties: BTreeSet<String>,
    /// Unexecuted effects can change arbitrary properties, unlike a single
    /// unresolved direct modifier. They taint every query while active.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub unresolved_effects: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdeaDefinition {
    pub id: String,
    pub effect: IdeaEffect,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdeaGroup {
    pub traditions: IdeaEffect,
    pub ideas: Vec<IdeaDefinition>,
    pub completion: IdeaEffect,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdeaState {
    /// None means selection is unknown. A selected group can have unknown progress.
    pub groups: Option<BTreeMap<String, Option<u16>>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IdeaContribution {
    pub source: String,
    pub property: String,
    pub micros: i64,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
pub struct IdeaEvaluation {
    pub known_totals_micros: BTreeMap<String, i64>,
    pub contributions: Vec<IdeaContribution>,
    pub unresolved: BTreeSet<String>,
}

/// Ordered progression content shared by native and translated worlds.
/// Traditions apply on selection, steps apply in order, and completion applies
/// only after every step. No EU4 selection syntax or fixed group size lives here.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ideas {
    pub groups: BTreeMap<String, IdeaGroup>,
    pub countries: BTreeMap<EntityId, IdeaState>,
}

impl Ideas {
    pub fn is_empty(&self) -> bool {
        self.groups.is_empty() && self.countries.is_empty()
    }

    pub fn declared_effects(&self, entity: EntityId) -> Result<IdeaEvaluation, Error> {
        self.evaluate(entity, None)
    }

    pub fn property_effects(
        &self,
        entity: EntityId,
        property: &str,
    ) -> Result<IdeaEvaluation, Error> {
        self.evaluate(entity, Some(property))
    }

    fn evaluate(&self, entity: EntityId, property: Option<&str>) -> Result<IdeaEvaluation, Error> {
        let mut result = IdeaEvaluation::default();
        let Some(groups) = self.countries.get(&entity).and_then(|s| s.groups.as_ref()) else {
            result.unresolved.insert("ideas:unknown_selection".into());
            return Ok(result);
        };
        for (name, count) in groups {
            let Some(group) = self.groups.get(name) else {
                result
                    .unresolved
                    .insert(format!("ideas:{name}:missing_definition"));
                continue;
            };
            append(
                &mut result,
                format!("ideas:traditions:{name}"),
                &group.traditions,
                property,
            )?;
            if let Some(count) = count {
                if usize::from(*count) > group.ideas.len() {
                    return Err(Error::Invalid("idea progress exceeds group length".into()));
                }
                for idea in group.ideas.iter().take(usize::from(*count)) {
                    append(
                        &mut result,
                        format!("ideas:idea:{name}:{}", idea.id),
                        &idea.effect,
                        property,
                    )?;
                }
                if usize::from(*count) == group.ideas.len() {
                    append(
                        &mut result,
                        format!("ideas:completion:{name}"),
                        &group.completion,
                        property,
                    )?;
                }
            } else if property.is_none_or(|p| {
                group
                    .ideas
                    .iter()
                    .map(|i| &i.effect)
                    .chain([&group.completion])
                    .any(|e| {
                        e.modifier.values_micros.contains_key(p)
                            || e.modifier.flags.contains_key(p)
                            || e.unresolved_properties.contains(p)
                            || !e.unresolved_effects.is_empty()
                    })
            }) {
                result
                    .unresolved
                    .insert(format!("ideas:{name}:unknown_progress"));
            }
        }
        Ok(result)
    }

    pub(crate) fn validate(&self, world: &World) -> Result<(), Error> {
        for (name, group) in &self.groups {
            let mut ids = BTreeSet::new();
            if name.is_empty()
                || group.ideas.is_empty()
                || group.ideas.len() > usize::from(u16::MAX)
                || group
                    .ideas
                    .iter()
                    .any(|i| i.id.is_empty() || !ids.insert(&i.id))
            {
                return Err(Error::Invalid("invalid idea group".into()));
            }
            for effect in group
                .ideas
                .iter()
                .map(|i| &i.effect)
                .chain([&group.traditions, &group.completion])
            {
                let m = &effect.modifier;
                if effect.unresolved_effects.iter().any(String::is_empty)
                    || m.values_micros
                        .keys()
                        .chain(m.flags.keys())
                        .any(String::is_empty)
                    || m.values_micros.keys().any(|p| m.flags.contains_key(p))
                    || effect.unresolved_properties.iter().any(|p| {
                        p.is_empty() || m.values_micros.contains_key(p) || m.flags.contains_key(p)
                    })
                {
                    return Err(Error::Invalid("invalid idea effect".into()));
                }
            }
        }
        for (&entity, state) in &self.countries {
            if world.entity(entity)?.kind != EntityKind::Country
                || state
                    .groups
                    .as_ref()
                    .is_some_and(|g| g.keys().any(String::is_empty))
            {
                return Err(Error::Invalid("invalid idea country state".into()));
            }
            self.declared_effects(entity)?;
        }
        for (&entity, formulas) in &world.attributes().formulas {
            for term in formulas
                .values()
                .flat_map(|f| &f.channels)
                .flat_map(|c| c.flat.iter().chain(&c.rates))
            {
                if let AttributeTerm::Ideas { target, property } = term {
                    let target = match target {
                        Target::Current => Some(entity),
                        Target::Owner => world.entity(entity)?.owner,
                        Target::Capital => world.entity(entity)?.capital,
                    };
                    if let Some(target) = target {
                        self.property_effects(target, property)?;
                    }
                }
            }
        }
        Ok(())
    }
}

fn append(
    result: &mut IdeaEvaluation,
    source: String,
    effect: &IdeaEffect,
    property: Option<&str>,
) -> Result<(), Error> {
    result.unresolved.extend(
        effect
            .unresolved_effects
            .iter()
            .map(|reason| format!("{source}:unexecuted:{reason}")),
    );
    if property.is_some_and(|p| effect.modifier.flags.contains_key(p)) {
        return Err(Error::Invalid(
            "idea attribute requires numeric property".into(),
        ));
    }
    let selected = |p: &str| property.is_none_or(|name| name == p);
    for (p, &micros) in effect
        .modifier
        .values_micros
        .iter()
        .filter(|(p, _)| selected(p))
    {
        let total = result.known_totals_micros.entry(p.clone()).or_default();
        *total = total.checked_add(micros).ok_or(Error::Overflow)?;
        result.contributions.push(IdeaContribution {
            source: source.clone(),
            property: p.clone(),
            micros,
        });
    }
    result.unresolved.extend(
        effect
            .unresolved_properties
            .iter()
            .filter(|p| selected(p))
            .map(|p| format!("{source}:unresolved:{p}")),
    );
    Ok(())
}
