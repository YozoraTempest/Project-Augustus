// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use rand_chacha::rand_core::Rng;
use serde::{Deserialize, Serialize};

use crate::{EntityId, Error, FieldId, RuleId, World};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Target {
    Current,
    Owner,
    Capital,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Condition {
    Always(bool),
    AtLeast {
        target: Target,
        field: FieldId,
        value: i64,
    },
    All(Vec<Condition>),
    Not(Box<Condition>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Action {
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

    fn execute_inner(
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
