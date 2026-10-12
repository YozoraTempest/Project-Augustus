// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use rand_chacha::rand_core::Rng;
use serde::{Deserialize, Serialize};

use crate::{EntityId, Error, FieldId, RuleId, World};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Target {
    Current,
    Owner,
    Capital,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Condition {
    With {
        target: Target,
        kind: crate::EntityKind,
        condition: Box<Condition>,
    },
    InScope {
        selector: crate::ScopeSelector,
        kind: crate::EntityKind,
        condition: Box<Condition>,
    },
    GeographicMember {
        target: Target,
        level: crate::GeographicLevel,
        name: String,
    },
    CountryControlled {
        target: Target,
        controller: crate::CountryController,
    },
    AffiliationPresent {
        target: Target,
        dimension: String,
    },
    /// Compare an entity's live membership in a content-defined dimension.
    AffiliationMember {
        target: Target,
        dimension: String,
        member: String,
    },
    /// Compare the live membership's group; unknown membership or grouping is an error.
    AffiliationGroup {
        target: Target,
        dimension: String,
        group: String,
    },
    GovernmentMetricActive {
        target: Target,
        name: String,
    },
    HasMarker {
        target: Target,
        name: String,
    },
    HasModifier {
        target: Target,
        name: String,
    },
    NumericCompare {
        target: Target,
        slot: crate::NumericSlot,
        comparison: crate::NumericComparison,
        value: i64,
    },
    CompletionRecorded {
        target: Target,
        id: String,
    },
    /// Compare a target's stable entity name with a content identifier.
    EntityNamed {
        target: Target,
        name: String,
    },
    /// Read the current simulation calendar; a calendar is required.
    YearAtLeast(i16),
    /// Compare the current entity with a live office or global reference.
    ScopeIsCurrent(crate::ScopeSelector),
    Always(bool),
    AtLeast {
        target: Target,
        field: FieldId,
        value: i64,
    },
    All(Vec<Condition>),
    Any(Vec<Condition>),
    Not(Box<Condition>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Action {
    /// Localisation content for effect descriptions; no simulation side effects.
    Description {
        key: String,
        #[serde(default, skip_serializing_if = "is_false")]
        hidden: bool,
    },
    /// Describe effects without executing the body, resolving scopes, or drawing randomness.
    Preview {
        plan: Plan,
    },
    SetMarker {
        target: Target,
        name: String,
        present: bool,
    },
    AttachModifier {
        target: Target,
        name: String,
        duration: Option<u64>,
        hidden: Option<bool>,
        renewal: crate::ModifierRenewal,
        /// Localisation key for the effect's duration text; not attachment state.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        duration_label: Option<String>,
    },
    RemoveModifier {
        target: Target,
        name: String,
    },
    UpdateNumeric {
        target: Target,
        slot: crate::NumericSlot,
        operation: crate::NumericOperation,
        value: i64,
    },
    RecordCompletion {
        id: String,
    },
    /// Filter the entire list before executing any selected event.
    EventCandidates {
        events: Vec<String>,
    },
    /// Check the event's condition against the current state, then dispatch.
    TryEvent {
        event: String,
    },
    /// Execute in a live named scope; an absent scope skips the body.
    InScope {
        selector: crate::ScopeSelector,
        kind: crate::EntityKind,
        plan: Plan,
    },
    BindGlobalScope {
        name: String,
    },
    ClearGlobalScope {
        name: String,
    },
    /// Dispatch an event in the current scope, within the caller's execution limits.
    TriggerEvent {
        event: String,
    },
    ChangeEstateMetric {
        estate: String,
        metric: crate::EstateMetric,
        amount: i64,
        minimum: Option<i64>,
    },
    SetEstatePrivilege {
        estate: String,
        privilege: String,
        granted: bool,
    },
    ResolveEstatePrivilegeEffect {
        privilege: String,
        effect: String,
    },
    /// Two equally likely branches; exactly one u64 draw, in instruction order.
    RandomBranch {
        then: Plan,
        otherwise: Plan,
    },
    Accumulate {
        target: Target,
        field: FieldId,
        source: FieldId,
    },
    Run(RuleId),
    Add {
        target: Target,
        field: FieldId,
        value: i64,
    },
    Set {
        target: Target,
        field: FieldId,
        value: i64,
    },
    If {
        condition: Condition,
        then: Plan,
        otherwise: Plan,
    },
    With {
        target: Target,
        plan: Plan,
    },
    Schedule {
        delay: u64,
        plan: Plan,
    },
    Emit(String),
}

pub type Plan = Vec<Action>;

impl Condition {
    pub fn evaluate(&self, world: &World, current: EntityId) -> Result<bool, Error> {
        match self {
            Self::With {
                target,
                kind,
                condition,
            } => {
                let Some(selected) = world.condition_target(current, *target)? else {
                    return Ok(false);
                };
                if world.entity(selected)?.kind != *kind {
                    return Err(Error::Invalid(
                        "condition scope has the wrong entity type".into(),
                    ));
                }
                condition.evaluate(world, selected)
            }
            Self::InScope {
                selector,
                kind,
                condition,
            } => {
                let Some(selected) = world.resolve_scope(selector)? else {
                    return Ok(false);
                };
                if world.entity(selected)?.kind != *kind {
                    return Err(Error::Invalid(
                        "condition scope has the wrong entity type".into(),
                    ));
                }
                condition.evaluate(world, selected)
            }
            Self::GeographicMember {
                target,
                level,
                name,
            } => {
                let province = world.target(current, *target)?;
                if world.entity(province)?.kind != crate::EntityKind::Province {
                    return Err(Error::Invalid(
                        "geographic condition requires a province".into(),
                    ));
                }
                world
                    .administration()
                    .geographic_member(province, *level, name)
                    .ok_or_else(|| {
                        Error::Invalid(format!(
                            "unknown geographic membership {name} for entity {}",
                            province.0
                        ))
                    })
            }
            Self::CountryControlled { target, controller } => {
                let country = world.target(current, *target)?;
                let actual = world.country_controller(country)?.ok_or_else(|| {
                    Error::Invalid(format!(
                        "unknown country controller for entity {}",
                        country.0
                    ))
                })?;
                Ok(actual == *controller)
            }
            Self::AffiliationPresent { target, dimension } => {
                crate::affiliations::validate_condition(dimension, None)?;
                let entity = world.target(current, *target)?;
                world
                    .affiliations()
                    .present(dimension, entity)
                    .ok_or_else(|| {
                        Error::Invalid(format!(
                            "unknown affiliation presence {dimension} for entity {}",
                            entity.0
                        ))
                    })
            }
            Self::AffiliationMember {
                target,
                dimension,
                member,
            } => {
                crate::affiliations::validate_condition(dimension, Some(member))?;
                let entity = world.target(current, *target)?;
                if world.affiliations().present(dimension, entity) == Some(false) {
                    return Ok(false);
                }
                let actual = world
                    .affiliations()
                    .member(dimension, entity)
                    .ok_or_else(|| {
                        Error::Invalid(format!(
                            "unknown affiliation member {dimension} for entity {}",
                            entity.0
                        ))
                    })?;
                Ok(actual == member)
            }
            Self::AffiliationGroup {
                target,
                dimension,
                group,
            } => {
                crate::affiliations::validate_condition(dimension, Some(group))?;
                let entity = world.target(current, *target)?;
                if world.affiliations().present(dimension, entity) == Some(false) {
                    return Ok(false);
                }
                let actual = world
                    .affiliations()
                    .group(dimension, entity)
                    .ok_or_else(|| {
                        Error::Invalid(format!(
                            "unknown affiliation group {dimension} for entity {}",
                            entity.0
                        ))
                    })?;
                Ok(actual == group)
            }
            Self::GovernmentMetricActive { target, name } => {
                world.government_metric_active(world.target(current, *target)?, name)
            }
            Self::HasMarker { target, name } => {
                world.has_marker(world.target(current, *target)?, name)
            }
            Self::HasModifier { target, name } => {
                world.has_modifier(world.target(current, *target)?, name)
            }
            Self::NumericCompare {
                target,
                slot,
                comparison,
                value,
            } => {
                let actual = world.numeric_value(world.target(current, *target)?, slot)?;
                Ok(match comparison {
                    crate::NumericComparison::AtLeast => actual >= *value,
                    crate::NumericComparison::Equal => actual == *value,
                })
            }
            Self::CompletionRecorded { target, id } => {
                world.completion_recorded(world.target(current, *target)?, id)
            }
            Self::EntityNamed { target, name } => {
                if name.is_empty() {
                    return Err(Error::Invalid("empty entity condition name".into()));
                }
                Ok(world.entity(world.target(current, *target)?)?.name == *name)
            }
            Self::YearAtLeast(year) => {
                if *year < 1 {
                    return Err(Error::Invalid(
                        "year condition requires a positive year".into(),
                    ));
                }
                let date = world
                    .date()?
                    .ok_or_else(|| Error::Invalid("year condition requires a calendar".into()))?;
                Ok(date.year >= *year)
            }
            Self::ScopeIsCurrent(selector) => {
                world.entity(current)?;
                Ok(world.resolve_scope(selector)? == Some(current))
            }
            Self::Always(value) => Ok(*value),
            Self::AtLeast {
                target,
                field,
                value,
            } => Ok(world.value(world.target(current, *target)?, *field)? >= *value),
            Self::All(conditions) => {
                for condition in conditions {
                    if !condition.evaluate(world, current)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            Self::Any(conditions) => {
                for condition in conditions {
                    if condition.evaluate(world, current)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            Self::Not(condition) => Ok(!condition.evaluate(world, current)?),
        }
    }
}

impl World {
    /// Writes are visible to the next instruction, including condition evaluation.
    /// A runtime error preserves preceding writes and stops execution and saving.
    pub fn execute(&mut self, current: EntityId, plan: &[Action]) -> Result<(), Error> {
        self.execute_phases(current, &[plan])
    }

    pub(crate) fn execute_phases(
        &mut self,
        current: EntityId,
        phases: &[&[Action]],
    ) -> Result<(), Error> {
        self.require_running()?;
        let mut budget = 100_000;
        let result = phases
            .iter()
            .try_for_each(|plan| self.execute_inner(current, plan, &mut budget, 0));
        if result.is_err() {
            self.failed = true;
        }
        result
    }

    pub(crate) fn execute_inner(
        &mut self,
        current: EntityId,
        plan: &[Action],
        budget: &mut usize,
        depth: usize,
    ) -> Result<(), Error> {
        if depth >= 64 {
            return Err(Error::Invalid("plan call depth exceeded".into()));
        }
        self.entity(current)?;
        for action in plan {
            if *budget == 0 {
                return Err(Error::Invalid("plan instruction limit exceeded".into()));
            }
            *budget -= 1;
            match action {
                Action::Description { key, .. } => validate_description_key(key)?,
                Action::Preview { .. } => (),
                Action::SetMarker {
                    target,
                    name,
                    present,
                } => {
                    let entity = self.target(current, *target)?;
                    self.set_marker(entity, name, *present)?;
                }
                Action::AttachModifier {
                    target,
                    name,
                    duration,
                    hidden,
                    renewal,
                    duration_label,
                } => {
                    crate::modifiers::validate_duration_label(duration_label.as_deref())?;
                    let entity = self.target(current, *target)?;
                    self.attach_modifier(entity, name, *duration, *hidden, *renewal)?;
                }
                Action::RemoveModifier { target, name } => {
                    let entity = self.target(current, *target)?;
                    self.remove_modifier(entity, name)?;
                }
                Action::UpdateNumeric {
                    target,
                    slot,
                    operation,
                    value,
                } => {
                    let entity = self.target(current, *target)?;
                    self.update_numeric(entity, slot, *operation, *value)?;
                }
                Action::RecordCompletion { id } => {
                    self.record_completion(current, id)?;
                }
                Action::EventCandidates { events } => {
                    self.event_candidates_inner(events, current, budget, depth + 1)?;
                }
                Action::TryEvent { event } => {
                    self.try_event_inner(event, current, budget, depth + 1)?;
                }
                Action::InScope {
                    selector,
                    kind,
                    plan,
                } => {
                    if let Some(selected) = self.resolve_scope(selector)? {
                        if self.entity(selected)?.kind != *kind {
                            return Err(Error::Invalid(
                                "named scope entity type does not match".into(),
                            ));
                        }
                        self.execute_inner(selected, plan, budget, depth + 1)?;
                    }
                }
                Action::BindGlobalScope { name } => self.bind_global_scope(name, current)?,
                Action::ClearGlobalScope { name } => self.clear_global_scope(name)?,
                Action::TriggerEvent { event } => {
                    self.trigger_event_inner(event, current, budget, depth + 1)?;
                }
                Action::ChangeEstateMetric {
                    estate,
                    metric,
                    amount,
                    minimum,
                } => self.change_estate_metric(current, estate, *metric, *amount, *minimum)?,
                Action::SetEstatePrivilege {
                    estate,
                    privilege,
                    granted,
                } => self.set_estate_privilege(current, estate, privilege, *granted)?,
                Action::ResolveEstatePrivilegeEffect { privilege, effect } => {
                    self.resolve_estate_privilege_effect(current, privilege, effect)?
                }
                Action::RandomBranch { then, otherwise } => {
                    let branch = if self.random.next_u64() & 1 == 0 {
                        then
                    } else {
                        otherwise
                    };
                    self.execute_inner(current, branch, budget, depth + 1)?;
                }
                Action::Accumulate {
                    target,
                    field,
                    source,
                } => {
                    let entity = self.target(current, *target)?;
                    let value = self
                        .value(entity, *field)?
                        .checked_add(self.value(entity, *source)?)
                        .ok_or(Error::Overflow)?;
                    self.set(entity, *field, value)?;
                }
                Action::Run(rule) => {
                    let plan = self
                        .rules
                        .get(rule.0)
                        .ok_or_else(|| Error::Invalid(format!("unknown rule {}", rule.0)))?
                        .clone();
                    self.execute_inner(current, &plan, budget, depth + 1)?;
                }
                Action::Add {
                    target,
                    field,
                    value,
                } => {
                    let entity = self.target(current, *target)?;
                    let result = self
                        .value(entity, *field)?
                        .checked_add(*value)
                        .ok_or(Error::Overflow)?;
                    self.set(entity, *field, result)?;
                }
                Action::Set {
                    target,
                    field,
                    value,
                } => self.set(self.target(current, *target)?, *field, *value)?,
                Action::If {
                    condition,
                    then,
                    otherwise,
                } => {
                    let branch = if condition.evaluate(self, current)? {
                        then
                    } else {
                        otherwise
                    };
                    self.execute_inner(current, branch, budget, depth + 1)?;
                }
                Action::With { target, plan } => {
                    self.execute_inner(self.target(current, *target)?, plan, budget, depth + 1)?
                }
                Action::Schedule { delay, plan } => self.schedule(current, *delay, plan.clone())?,
                Action::Emit(message) => self.emit(current, message.clone()),
            }
        }
        Ok(())
    }
}

fn is_false(value: &bool) -> bool {
    !value
}

pub(crate) fn validate_description_key(key: &str) -> Result<(), Error> {
    if key.trim().is_empty() {
        return Err(Error::Invalid("empty description key".into()));
    }
    Ok(())
}
