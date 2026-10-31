// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{Condition, EntityId, Error, MonthlySystem, NumericOperation, World};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConvergenceRule {
    pub target_value: i64,
    /// None denotes an unknown rate; zero disables movement explicitly.
    pub step: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition: Option<Condition>,
    /// Assign the target when the condition is false instead of retaining stock.
    #[serde(default, skip_serializing_if = "is_false")]
    pub reset_when_inactive: bool,
}

fn is_false(value: &bool) -> bool {
    !value
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConvergenceAccount {
    pub applies: Option<bool>,
    pub quantity_before: Option<i64>,
    pub quantity_after: Option<i64>,
    pub changed: Option<i64>,
    pub unresolved: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConvergenceSettlement {
    pub day: u64,
    pub accounts: BTreeMap<EntityId, BTreeMap<String, ConvergenceAccount>>,
}

/// Current quantities live in Metrics; this component stores policies and receipts.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MonthlyConvergence {
    pub rules: BTreeMap<EntityId, BTreeMap<String, ConvergenceRule>>,
    pub settlement_count: u64,
    pub last_settlement: Option<ConvergenceSettlement>,
}

impl ConvergenceRule {
    fn after(&self, applies: Option<bool>, before: Option<i64>) -> Result<Option<i64>, Error> {
        if before == Some(self.target_value) {
            return Ok(before);
        }
        match (applies, before, self.step) {
            (Some(false), _, _) if self.reset_when_inactive => Ok(Some(self.target_value)),
            (Some(false), before, _) => Ok(before),
            (Some(true), Some(before), Some(step)) => NumericOperation::Approach { step }
                .calculate(before, self.target_value)
                .and_then(|value| i64::try_from(value).map(Some).map_err(|_| Error::Overflow)),
            _ => Ok(None),
        }
    }
}

impl MonthlyConvergence {
    pub(crate) fn prepare(&self, world: &World) -> Result<ConvergenceSettlement, Error> {
        self.settlement_count
            .checked_add(1)
            .ok_or(Error::Overflow)?;
        let mut accounts = BTreeMap::new();
        for (&entity, rules) in &self.rules {
            let mut rows = BTreeMap::new();
            for (name, rule) in rules {
                let mut unresolved = BTreeSet::new();
                let applies = match rule
                    .condition
                    .as_ref()
                    .map_or(Ok(true), |condition| condition.evaluate(world, entity))
                {
                    Ok(value) => Some(value),
                    Err(error) => {
                        unresolved.insert(format!("unknown_condition:{error}"));
                        None
                    }
                };
                let before = world
                    .metrics()
                    .entities
                    .get(&entity)
                    .and_then(|values| values.get(name))
                    .copied()
                    .flatten();
                if applies != Some(false) {
                    if before.is_none() {
                        unresolved.insert("unknown_quantity".into());
                    }
                    if rule.step.is_none() && before != Some(rule.target_value) {
                        unresolved.insert("unknown_step".into());
                    }
                }
                let after = rule.after(applies, before)?;
                world.metrics().definition(name)?.validate_value(after)?;
                let changed = before
                    .zip(after)
                    .map(|(before, after)| after.checked_sub(before).ok_or(Error::Overflow))
                    .transpose()?;
                rows.insert(
                    name.clone(),
                    ConvergenceAccount {
                        applies,
                        quantity_before: before,
                        quantity_after: after,
                        changed,
                        unresolved,
                    },
                );
            }
            accounts.insert(entity, rows);
        }
        Ok(ConvergenceSettlement {
            day: world.day(),
            accounts,
        })
    }

    pub(crate) fn commit(&mut self, world: &mut World, settlement: ConvergenceSettlement) {
        for (&entity, accounts) in &settlement.accounts {
            for (name, account) in accounts {
                world
                    .metrics
                    .entities
                    .entry(entity)
                    .or_default()
                    .insert(name.clone(), account.quantity_after);
            }
        }
        self.settlement_count += 1;
        self.last_settlement = Some(settlement);
    }

    pub(crate) fn validate(&self, world: &World) -> Result<(), Error> {
        if let Some(accruals) = world.monthly_accruals() {
            self.validate_accruals(accruals)?;
        }
        let calendar = world
            .calendar()
            .ok_or_else(|| Error::Invalid("convergence requires a calendar".into()))?;
        if self.rules.is_empty() || self.settlement_count != calendar.elapsed_months(world.day())? {
            return Err(Error::Invalid(
                "invalid convergence rules or month count".into(),
            ));
        }
        for (&entity, rules) in &self.rules {
            if rules.is_empty() {
                return Err(Error::Invalid("empty convergence entity rules".into()));
            }
            for (name, rule) in rules {
                let definition = world.metrics().definition(name)?;
                if world.entity(entity)?.kind != definition.kind
                    || !(definition.minimum..=definition.maximum).contains(&rule.target_value)
                    || rule.step.is_some_and(|step| step < 0)
                    || definition.growth_ceiling
                {
                    return Err(Error::Invalid("invalid convergence target or step".into()));
                }
                if let Some(condition) = &rule.condition {
                    world.validate_condition(condition)?;
                }
            }
        }
        match &self.last_settlement {
            None if self.settlement_count == 0 => Ok(()),
            Some(last) if self.settlement_count > 0 => {
                let date = calendar.date(last.day)?;
                let current = calendar.date(world.day())?;
                if last.day == 0
                    || last.day > world.day()
                    || date.day != 1
                    || date.year != current.year
                    || date.month != current.month
                    || !last.accounts.keys().eq(self.rules.keys())
                {
                    return Err(Error::Invalid(
                        "invalid saved convergence settlement".into(),
                    ));
                }
                for (&entity, accounts) in &last.accounts {
                    if !accounts.keys().eq(self.rules[&entity].keys()) {
                        return Err(Error::Invalid("invalid saved convergence accounts".into()));
                    }
                    for (name, account) in accounts {
                        let definition = world.metrics().definition(name)?;
                        definition.validate_value(account.quantity_after)?;
                        let rule = &self.rules[&entity][name];
                        let expected = rule.after(account.applies, account.quantity_before)?;
                        let changed = account
                            .quantity_before
                            .zip(expected)
                            .map(|(before, after)| after.checked_sub(before).ok_or(Error::Overflow))
                            .transpose()?;
                        if [account.quantity_before, account.quantity_after]
                            .into_iter()
                            .flatten()
                            .any(|value| {
                                !(definition.minimum..=definition.maximum).contains(&value)
                            })
                            || account.quantity_after != expected
                            || account.changed != changed
                            || account.unresolved.iter().any(String::is_empty)
                            || (rule.condition.is_none() && account.applies != Some(true))
                            || (account.applies.is_none()
                                && !account
                                    .unresolved
                                    .iter()
                                    .any(|reason| reason.starts_with("unknown_condition:")))
                            || (account.applies != Some(false)
                                && expected.is_none()
                                && account.unresolved.is_empty())
                        {
                            return Err(Error::Invalid("invalid saved convergence receipt".into()));
                        }
                    }
                }
                Ok(())
            }
            _ => Err(Error::Invalid("invalid saved convergence count".into())),
        }
    }

    pub(crate) fn validate_accruals(&self, accruals: &crate::MonthlyAccruals) -> Result<(), Error> {
        for (entity, rules) in &self.rules {
            if accruals
                .rules
                .get(entity)
                .is_some_and(|other| rules.keys().any(|name| other.contains_key(name)))
            {
                return Err(Error::Invalid(
                    "a monthly metric cannot accrue and converge simultaneously".into(),
                ));
            }
        }
        Ok(())
    }
}

impl World {
    /// Prepares the next monthly convergence settlement without committing
    /// metric values, receipts, or settlement counters. The staged world
    /// applies the same monthly-autonomy phase used by `MonthlyEconomy`.
    pub fn preview_monthly_convergence(&self) -> Result<ConvergenceSettlement, Error> {
        let mut staged = self.clone();
        if let Some(administration) = staged.prepare_monthly_autonomy()? {
            staged.replace_administration(administration)?;
        }
        staged
            .monthly_convergence
            .as_ref()
            .ok_or_else(|| Error::Invalid("missing monthly convergence".into()))?
            .prepare(&staged)
    }

    pub fn monthly_convergence(&self) -> Option<&MonthlyConvergence> {
        self.monthly_convergence.as_ref()
    }

    pub fn set_monthly_convergence(
        &mut self,
        convergence: Option<MonthlyConvergence>,
    ) -> Result<(), Error> {
        self.require_running()?;
        if self.day() != 0 {
            return Err(Error::Invalid(
                "convergence must be selected before advancing".into(),
            ));
        }
        let custom_system = self.monthly_system_is_custom();
        if let Some(state) = &convergence {
            state.validate(self)?;
        }
        self.monthly_convergence = convergence;
        if !custom_system {
            if self.monthly_convergence.is_some()
                || self.monthly_accruals.is_some()
                || self.monthly_stocks.is_some()
                || self.monthly_production.is_some()
                || self.monthly_cashflows.is_some()
            {
                self.monthly_system_id = Some(crate::MonthlyEconomy.id().into());
            } else {
                self.monthly_system_id = self
                    .taxation
                    .as_ref()
                    .map(|_| crate::MonthlyTax.id().into());
            }
        }
        Ok(())
    }
}
