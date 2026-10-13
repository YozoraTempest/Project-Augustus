// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use crate::{EntityId, Error, MonthlySystem, NumericOperation, NumericSlot, World};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Credit a live attribute to an existing metric in the chosen integer units.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccrualRule {
    pub attribute: String,
    pub divisor: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccrualAccount {
    pub income: Option<i64>,
    pub quantity_before: Option<i64>,
    pub quantity_after: Option<i64>,
    pub credited: Option<i64>,
    pub growth_ceiling: Option<i64>,
    pub unresolved: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccrualSettlement {
    pub day: u64,
    pub accounts: BTreeMap<EntityId, BTreeMap<String, AccrualAccount>>,
}

/// Rules are content; receipts are historical. Income is never a saved cache.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MonthlyAccruals {
    pub rules: BTreeMap<EntityId, BTreeMap<String, AccrualRule>>,
    pub settlement_count: u64,
    pub last_settlement: Option<AccrualSettlement>,
}

impl MonthlyAccruals {
    pub(crate) fn prepare(&self, world: &World) -> Result<AccrualSettlement, Error> {
        self.settlement_count
            .checked_add(1)
            .ok_or(Error::Overflow)?;
        let mut accounts = BTreeMap::new();
        for (&entity, rules) in &self.rules {
            let mut rows = BTreeMap::new();
            for (name, rule) in rules {
                let evaluation = world
                    .attributes()
                    .evaluate(world, entity, &rule.attribute)?;
                let mut unresolved = evaluation.unresolved;
                let income = evaluation.value_micros.map(|v| v / rule.divisor);
                let before = world
                    .metrics()
                    .entities
                    .get(&entity)
                    .and_then(|values| values.get(name))
                    .copied()
                    .flatten();
                let definition = world.metrics().definition(name)?;
                let ceiling = world
                    .metrics()
                    .ceilings
                    .get(&entity)
                    .and_then(|values| values.get(name))
                    .copied()
                    .flatten();
                if before.is_none() {
                    unresolved.insert("unknown_quantity".into());
                }
                if income.is_none() {
                    unresolved.insert("unknown_income".into());
                }
                let ceiling_needed = definition.growth_ceiling
                    && before
                        .zip(income)
                        .is_some_and(|(before, income)| income > 0 && before < definition.maximum);
                if ceiling_needed && ceiling.is_none() {
                    unresolved.insert("unknown_growth_ceiling".into());
                }
                let after = if let Some((_, income)) = before.zip(income)
                    && !(ceiling_needed && ceiling.is_none())
                {
                    Some(world.preview_numeric_update(
                        entity,
                        &NumericSlot::Metric(name.clone()),
                        NumericOperation::Add,
                        income,
                    )?)
                } else {
                    None
                };
                let credited = before
                    .zip(after)
                    .map(|(before, after)| after.checked_sub(before).ok_or(Error::Overflow))
                    .transpose()?;
                rows.insert(
                    name.clone(),
                    AccrualAccount {
                        income,
                        quantity_before: before,
                        quantity_after: after,
                        credited,
                        growth_ceiling: ceiling,
                        unresolved,
                    },
                );
            }
            accounts.insert(entity, rows);
        }
        Ok(AccrualSettlement {
            day: world.day(),
            accounts,
        })
    }

    pub(crate) fn commit(&mut self, world: &mut World, settlement: AccrualSettlement) {
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
        let calendar = world
            .calendar()
            .ok_or_else(|| Error::Invalid("accruals require a calendar".into()))?;
        if self.rules.is_empty() || self.settlement_count != calendar.elapsed_months(world.day())? {
            return Err(Error::Invalid(
                "invalid accrual rules or month count".into(),
            ));
        }
        for (&entity, rules) in &self.rules {
            if rules.is_empty() {
                return Err(Error::Invalid("empty accrual entity rules".into()));
            }
            for (name, rule) in rules {
                if world.entity(entity)?.kind != world.metrics().definition(name)?.kind
                    || rule.divisor <= 0
                    || !world
                        .attributes()
                        .formulas
                        .get(&entity)
                        .is_some_and(|formulas| formulas.contains_key(&rule.attribute))
                {
                    return Err(Error::Invalid("invalid accrual target or attribute".into()));
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
                    return Err(Error::Invalid("invalid saved accrual settlement".into()));
                }
                for (&entity, accounts) in &last.accounts {
                    if !accounts.keys().eq(self.rules[&entity].keys()) {
                        return Err(Error::Invalid("invalid saved accrual accounts".into()));
                    }
                    for (name, account) in accounts {
                        let definition = world.metrics().definition(name)?;
                        if !definition.growth_ceiling && account.growth_ceiling.is_some() {
                            return Err(Error::Invalid("unexpected saved growth ceiling".into()));
                        }
                        for value in [
                            account.quantity_before,
                            account.quantity_after,
                            account.growth_ceiling,
                        ]
                        .into_iter()
                        .flatten()
                        {
                            if !(definition.minimum..=definition.maximum).contains(&value) {
                                return Err(Error::Invalid(
                                    "saved accrual value outside bounds".into(),
                                ));
                            }
                        }
                        let expected = account.quantity_before.zip(account.income).and_then(
                            |(before, income)| {
                                let raw = (i128::from(before) + i128::from(income)).clamp(
                                    i128::from(definition.minimum),
                                    i128::from(definition.maximum),
                                );
                                if definition.growth_ceiling && raw > i128::from(before) {
                                    account.growth_ceiling.map(|ceiling| {
                                        raw.min(i128::from(before.max(ceiling))) as i64
                                    })
                                } else {
                                    Some(raw as i64)
                                }
                            },
                        );
                        let credited = account
                            .quantity_before
                            .zip(expected)
                            .map(|(before, after)| after.checked_sub(before).ok_or(Error::Overflow))
                            .transpose()?;
                        if account.quantity_after != expected
                            || account.credited != credited
                            || account.unresolved.iter().any(String::is_empty)
                            || (expected.is_none() && account.unresolved.is_empty())
                        {
                            return Err(Error::Invalid("invalid saved accrual receipt".into()));
                        }
                    }
                }
                Ok(())
            }
            _ => Err(Error::Invalid("invalid saved accrual count".into())),
        }
    }
}

/// The standard executor prepares all changes before committing monthly state.
pub struct MonthlyEconomy;

impl MonthlySystem for MonthlyEconomy {
    fn id(&self) -> &str {
        "monthly-economy:1"
    }

    fn validate(&self, world: &World) -> Result<(), Error> {
        if world.monthly_accruals().is_none()
            && world.monthly_convergence().is_none()
            && world.monthly_stocks().is_none()
        {
            return Err(Error::Invalid("missing monthly metric state".into()));
        }
        if let Some(accruals) = world.monthly_accruals() {
            accruals.validate(world)?;
        }
        if let Some(convergence) = world.monthly_convergence() {
            convergence.validate(world)?;
        }
        if let Some(stocks) = world.monthly_stocks() {
            stocks.validate(world)?;
        }
        if let Some(taxation) = world.taxation() {
            taxation.validate(world)?;
        }
        Ok(())
    }

    fn settle(&self, world: &mut World) -> Result<(), Error> {
        let taxes = world
            .taxation()
            .map(|taxation| taxation.prepare(world))
            .transpose()?;
        let receipts = world
            .monthly_accruals()
            .map(|accruals| accruals.prepare(world))
            .transpose()?;
        let convergence_receipts = world
            .monthly_convergence()
            .map(|convergence| convergence.prepare(world))
            .transpose()?;
        let stock_receipts = world
            .monthly_stocks()
            .map(|stocks| stocks.prepare(world))
            .transpose()?;
        if let Some(taxes) = taxes {
            let mut taxation = world.taxation.take().expect("prepared taxation");
            taxation.commit(world, taxes);
            world.taxation = Some(taxation);
        }
        if let Some(receipts) = receipts {
            let mut accruals = world.monthly_accruals.take().expect("prepared accruals");
            accruals.commit(world, receipts);
            world.monthly_accruals = Some(accruals);
        }
        if let Some(receipts) = convergence_receipts {
            let mut convergence = world
                .monthly_convergence
                .take()
                .expect("prepared convergence");
            convergence.commit(world, receipts);
            world.monthly_convergence = Some(convergence);
        }
        if let Some(receipts) = stock_receipts {
            let mut stocks = world
                .monthly_stocks
                .take()
                .expect("prepared monthly stocks");
            stocks.commit(world, receipts);
            world.monthly_stocks = Some(stocks);
        }
        Ok(())
    }
}

impl World {
    pub fn monthly_accruals(&self) -> Option<&MonthlyAccruals> {
        self.monthly_accruals.as_ref()
    }

    pub fn set_monthly_accruals(&mut self, accruals: MonthlyAccruals) -> Result<(), Error> {
        self.require_running()?;
        if self.day() != 0
            || self
                .monthly_system_id()
                .is_some_and(|id| id != MonthlyEconomy.id() && id != crate::MonthlyTax.id())
        {
            return Err(Error::Invalid(
                "accruals must be selected before advancing".into(),
            ));
        }
        accruals.validate(self)?;
        if let Some(convergence) = &self.monthly_convergence {
            convergence.validate_accruals(&accruals)?;
        }
        self.monthly_accruals = Some(accruals);
        self.monthly_system_id = Some(MonthlyEconomy.id().into());
        Ok(())
    }
}
