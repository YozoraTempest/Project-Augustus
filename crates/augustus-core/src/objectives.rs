// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{Condition, EntityId, Error, World};

/// Content-defined objective bundles. Higher priorities reserve claims first;
/// bundles at the same priority may share claims and remain selected together.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObjectiveGroup {
    pub id: String,
    pub condition: Condition,
    pub objectives: BTreeSet<String>,
    pub claims: BTreeSet<String>,
    pub priority: u32,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Objectives {
    pub groups: Vec<ObjectiveGroup>,
    /// Missing entities have unknown availability; empty sets are known empty.
    pub entities: BTreeMap<EntityId, BTreeSet<String>>,
}

impl Objectives {
    pub fn is_empty(&self) -> bool {
        self.groups.is_empty() && self.entities.is_empty()
    }

    pub(crate) fn validate(&self, world: &World) -> Result<(), Error> {
        let mut names = BTreeSet::new();
        for group in &self.groups {
            if group.id.is_empty() || !names.insert(&group.id) || group.claims.contains("") {
                return Err(Error::Invalid(
                    "empty or duplicate objective group or claim".into(),
                ));
            }
            for id in &group.objectives {
                world.completions().identifier(id)?;
            }
            crate::world::validate_condition(
                &group.condition,
                world.schema(),
                world.completions(),
                world.modifiers(),
                world.governments(),
                world.metrics(),
                world.justifications(),
            )?;
        }
        let declared: BTreeSet<_> = self.groups.iter().flat_map(|g| &g.objectives).collect();
        for (entity, ids) in &self.entities {
            world.entity(*entity)?;
            if ids.iter().any(|id| !declared.contains(id)) {
                return Err(Error::Invalid(
                    "availability contains an undeclared objective".into(),
                ));
            }
        }
        Ok(())
    }
}

// A conclusive false/true branch can settle eligibility even when another
// branch is unavailable. Unresolved errors never become a false predicate.
fn eligibility(condition: &Condition, world: &World, entity: EntityId) -> Result<bool, Error> {
    match condition {
        Condition::All(children) | Condition::Any(children) => {
            let all = matches!(condition, Condition::All(_));
            let mut pending = None;
            for child in children {
                match eligibility(child, world, entity) {
                    Ok(value) if value != all => return Ok(!all),
                    Ok(_) => (),
                    Err(error) => {
                        pending.get_or_insert(error);
                    }
                }
            }
            pending.map_or(Ok(all), Err)
        }
        Condition::Not(child) => eligibility(child, world, entity).map(|v| !v),
        _ => condition.evaluate(world, entity),
    }
}

impl World {
    pub fn objectives(&self) -> &Objectives {
        &self.objectives
    }

    pub fn set_objectives(&mut self, objectives: Objectives) -> Result<(), Error> {
        self.require_running()?;
        objectives.validate(self)?;
        self.objectives = objectives;
        Ok(())
    }

    pub fn objective_available(&self, entity: EntityId, id: &str) -> Result<bool, Error> {
        self.entity(entity)?;
        self.completions().identifier(id)?;
        let ids = self.objectives.entities.get(&entity).ok_or_else(|| {
            Error::Invalid(format!(
                "objective availability is unknown for entity {}",
                entity.0
            ))
        })?;
        Ok(ids.contains(id))
    }

    /// Explicitly replace availability from live eligibility. No completion
    /// records or rewards are changed; failed selection leaves the old set intact.
    pub fn refresh_objectives(&mut self, entity: EntityId) -> Result<(), Error> {
        self.require_running()?;
        self.entity(entity)?;
        let mut eligible: BTreeMap<u32, Vec<&ObjectiveGroup>> = BTreeMap::new();
        for group in &self.objectives.groups {
            if eligibility(&group.condition, self, entity)? {
                eligible.entry(group.priority).or_default().push(group);
            }
        }
        let mut claims = BTreeSet::new();
        let mut selected = BTreeSet::new();
        for (_, groups) in eligible.into_iter().rev() {
            let winners: Vec<_> = groups
                .into_iter()
                .filter(|group| group.claims.is_disjoint(&claims))
                .collect();
            for group in winners {
                claims.extend(group.claims.iter().cloned());
                selected.extend(group.objectives.iter().cloned());
            }
        }
        self.objectives.entities.insert(entity, selected);
        Ok(())
    }
}
