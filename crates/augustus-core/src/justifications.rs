// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{Condition, EntityId, EntityKind, Error, World};

/// Conditions used when maintaining an existing directed grant. An absent
/// condition preserves an unknown rule, rather than assuming eligibility.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JustificationDefinition {
    pub issuer: Option<Condition>,
    pub target: Option<Condition>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GrantTerm {
    Permanent,
    Days(u64),
    /// Count calendar months from the grant date. Monthly terms are retired
    /// after due plans on the first of a month, once their deadline has passed.
    Months {
        count: u32,
        monthly: bool,
    },
}

impl GrantTerm {
    pub(crate) fn validate(self) -> Result<(), Error> {
        if matches!(
            self,
            Self::Days(0)
                | Self::Months {
                    count: 0,
                    monthly: false
                }
        ) {
            return Err(Error::Invalid("grant duration must be positive".into()));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Justification {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_on: Option<u64>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub monthly: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unresolved: Option<String>,
}

/// Received grants, indexed by issuer, target and content-defined type. This
/// inventory does not decide whether war may be declared or generate rights
/// from unimplemented automatic rules.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Justifications {
    pub definitions: BTreeMap<String, JustificationDefinition>,
    pub active: BTreeMap<EntityId, BTreeMap<EntityId, BTreeMap<String, Justification>>>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub unknown_countries: BTreeSet<EntityId>,
}

fn is_false(value: &bool) -> bool {
    !value
}

impl Justifications {
    pub fn is_empty(&self) -> bool {
        self.definitions.is_empty() && self.active.is_empty() && self.unknown_countries.is_empty()
    }

    pub(crate) fn identifier(&self, name: &str) -> Result<(), Error> {
        if name.is_empty() || !self.definitions.contains_key(name) {
            return Err(Error::Invalid(format!("unknown justification {name}")));
        }
        Ok(())
    }

    fn country(world: &World, id: EntityId) -> Result<(), Error> {
        if world.entity(id)?.kind != EntityKind::Country {
            return Err(Error::Invalid(
                "justification endpoint requires a country".into(),
            ));
        }
        Ok(())
    }

    pub(crate) fn validate_definitions(&self, world: &World) -> Result<(), Error> {
        for (name, definition) in &self.definitions {
            self.identifier(name)?;
            for condition in [&definition.issuer, &definition.target]
                .into_iter()
                .flatten()
            {
                crate::world::validate_condition(
                    condition,
                    world.schema(),
                    &world.completions,
                    &world.modifiers,
                    &world.governments,
                    &world.metrics,
                    self,
                    world.subjects(),
                )?;
            }
        }
        Ok(())
    }

    pub(crate) fn validate(&self, world: &World) -> Result<(), Error> {
        self.validate_definitions(world)?;
        for &id in &self.unknown_countries {
            Self::country(world, id)?;
        }
        for (&issuer, targets) in &self.active {
            Self::country(world, issuer)?;
            if targets.is_empty() || self.unknown_countries.contains(&issuer) {
                return Err(Error::Invalid("invalid justification inventory".into()));
            }
            for (&target, grants) in targets {
                Self::country(world, target)?;
                if issuer == target || grants.is_empty() {
                    return Err(Error::Invalid("invalid justification target".into()));
                }
                for (name, grant) in grants {
                    self.identifier(name)?;
                    if grant.unresolved.as_ref().is_some_and(String::is_empty) {
                        return Err(Error::Invalid("empty justification uncertainty".into()));
                    }
                    match (grant.expires_on, grant.monthly) {
                        (Some(deadline), true) => {
                            let calendar = world.calendar().ok_or_else(|| {
                                Error::Invalid("monthly justification requires a calendar".into())
                            })?;
                            if calendar.next_month(deadline)? <= world.day() {
                                return Err(Error::Invalid("expired monthly justification".into()));
                            }
                        }
                        (Some(deadline), false) if deadline <= world.day() => {
                            return Err(Error::Invalid("expired justification".into()));
                        }
                        (None, true) => {
                            return Err(Error::Invalid("undated monthly justification".into()));
                        }
                        _ => {}
                    }
                }
            }
        }
        Ok(())
    }
}

impl World {
    pub fn justifications(&self) -> &Justifications {
        &self.justifications
    }

    pub fn set_justifications(&mut self, state: Justifications) -> Result<(), Error> {
        self.require_running()?;
        state.validate(self)?;
        let previous = std::mem::replace(&mut self.justifications, state);
        if let Err(error) = self
            .validate_event_content(self.event_catalog())
            .and_then(|_| self.event_catalog().validate(self))
        {
            self.justifications = previous;
            return Err(error);
        }
        Ok(())
    }

    fn justification_endpoints(&self, issuer: EntityId, target: EntityId) -> Result<(), Error> {
        Justifications::country(self, issuer)?;
        Justifications::country(self, target)?;
        if self.justifications.unknown_countries.contains(&issuer) {
            return Err(Error::Invalid("justification inventory is unknown".into()));
        }
        Ok(())
    }

    pub fn has_justification(
        &self,
        issuer: EntityId,
        target: EntityId,
        name: Option<&str>,
    ) -> Result<bool, Error> {
        self.justification_endpoints(issuer, target)?;
        if let Some(name) = name {
            self.justifications.identifier(name)?;
        }
        let grants = self
            .justifications
            .active
            .get(&issuer)
            .and_then(|targets| targets.get(&target));
        if let Some(grants) = grants {
            let selected = grants
                .iter()
                .filter(|(id, _)| name.is_none_or(|name| name == id.as_str()));
            let mut found = false;
            for (_, grant) in selected {
                if let Some(reason) = &grant.unresolved {
                    return Err(Error::Invalid(format!(
                        "justification state is unknown: {reason}"
                    )));
                }
                found = true;
            }
            return Ok(found);
        }
        Ok(false)
    }

    /// Replaces the previous grant of this type, including its deadline.
    pub fn grant_justification(
        &mut self,
        issuer: EntityId,
        target: EntityId,
        name: &str,
        term: GrantTerm,
    ) -> Result<(), Error> {
        self.require_running()?;
        self.justification_endpoints(issuer, target)?;
        self.justifications.identifier(name)?;
        term.validate()?;
        let (expires_on, monthly) = match term {
            GrantTerm::Permanent => (None, false),
            GrantTerm::Days(days) => (
                Some(self.day().checked_add(days).ok_or(Error::Overflow)?),
                false,
            ),
            GrantTerm::Months { count, monthly } => {
                let calendar = self
                    .calendar()
                    .ok_or_else(|| Error::Invalid("calendar grant requires a calendar".into()))?;
                (Some(calendar.after_months(self.day(), count)?), monthly)
            }
        };
        if issuer != target {
            self.justifications
                .active
                .entry(issuer)
                .or_default()
                .entry(target)
                .or_default()
                .insert(
                    name.into(),
                    Justification {
                        expires_on,
                        monthly,
                        unresolved: None,
                    },
                );
        }
        Ok(())
    }

    pub fn remove_justification(
        &mut self,
        issuer: EntityId,
        target: EntityId,
        name: &str,
    ) -> Result<(), Error> {
        self.require_running()?;
        self.justification_endpoints(issuer, target)?;
        self.justifications.identifier(name)?;
        self.erase_justification(issuer, target, name);
        Ok(())
    }

    fn erase_justification(&mut self, issuer: EntityId, target: EntityId, name: &str) {
        if let Some(targets) = self.justifications.active.get_mut(&issuer) {
            if let Some(grants) = targets.get_mut(&target) {
                grants.remove(name);
                if grants.is_empty() {
                    targets.remove(&target);
                }
            }
            if targets.is_empty() {
                self.justifications.active.remove(&issuer);
            }
        }
    }

    pub(crate) fn maintain_justifications(&mut self, monthly: bool) -> Result<(), Error> {
        let keys: Vec<_> = self
            .justifications
            .active
            .iter()
            .flat_map(|(&issuer, targets)| {
                targets.iter().flat_map(move |(&target, grants)| {
                    grants
                        .keys()
                        .map(move |name| (issuer, target, name.clone()))
                })
            })
            .collect();
        for (issuer, target, name) in keys {
            let grant = &self.justifications.active[&issuer][&target][&name];
            let expired = if monthly {
                grant.monthly
                    && grant
                        .expires_on
                        .is_some_and(|deadline| deadline < self.day())
            } else {
                !grant.monthly
                    && grant
                        .expires_on
                        .is_some_and(|deadline| deadline <= self.day())
            };
            if expired {
                self.erase_justification(issuer, target, &name);
                continue;
            }
            if monthly {
                continue;
            }
            let definition = &self.justifications.definitions[&name];
            let mut uncertain = None;
            let mut eligible = true;
            for (condition, entity) in [(&definition.issuer, issuer), (&definition.target, target)]
            {
                match condition {
                    Some(condition) => match condition.evaluate(self, entity) {
                        Ok(false) => {
                            eligible = false;
                            break;
                        }
                        Ok(true) => (),
                        Err(Error::Invalid(reason)) => uncertain = Some(reason),
                        Err(error) => return Err(error),
                    },
                    None => uncertain = Some("uncompiled grant maintenance condition".into()),
                }
            }
            if !eligible {
                self.erase_justification(issuer, target, &name);
            } else {
                self.justifications
                    .active
                    .get_mut(&issuer)
                    .unwrap()
                    .get_mut(&target)
                    .unwrap()
                    .get_mut(&name)
                    .unwrap()
                    .unresolved = uncertain;
            }
        }
        Ok(())
    }
}
