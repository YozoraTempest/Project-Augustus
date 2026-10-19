// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use crate::{
    EntityId, EntityKind, Error, MonthlySystem, ProductionSettlement, TaxSettlement, World,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// The externally supplied monthly income categories observed in EU4's
/// treasury ledger. `expenses_milli` is a positive amount and is subtracted
/// when the net cashflow is derived.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CashflowBreakdown {
    pub trade_milli: Option<i64>,
    pub gold_milli: Option<i64>,
    pub expenses_milli: Option<i64>,
    /// Known portions of aggregate expenses, keyed by native ledger category.
    /// Omitted categories remain unclassified rather than being inferred as zero.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub expense_categories_milli: BTreeMap<u32, i64>,
}

impl CashflowBreakdown {
    pub fn net_milli(&self) -> Option<i64> {
        self.trade_milli
            .zip(self.gold_milli)
            .zip(self.expenses_milli)
            .and_then(|((trade, gold), expenses)| trade.checked_add(gold)?.checked_sub(expenses))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CashflowAccount {
    pub amount_milli: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub breakdown: Option<CashflowBreakdown>,
    pub treasury_before_milli: Option<i64>,
    pub treasury_after_milli: Option<i64>,
    pub credited_milli: Option<i64>,
    pub unresolved: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CashflowSettlement {
    pub day: u64,
    pub accounts: BTreeMap<EntityId, CashflowAccount>,
}

/// Monthly signed cashflows supplied by a compatibility provider.
///
/// Positive values are receipts and negative values are expenses. The
/// provider owns the upstream rule translation; the core only applies the
/// supplied amount after tax and ordinary production have settled.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MonthlyExternalCashflows {
    pub amounts_milli: BTreeMap<EntityId, Option<i64>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub breakdowns: BTreeMap<EntityId, CashflowBreakdown>,
    pub settlement_count: u64,
    pub last_settlement: Option<CashflowSettlement>,
}

impl MonthlyExternalCashflows {
    pub fn new(amounts_milli: BTreeMap<EntityId, Option<i64>>) -> Self {
        Self {
            amounts_milli,
            breakdowns: BTreeMap::new(),
            settlement_count: 0,
            last_settlement: None,
        }
    }

    pub fn with_breakdowns(mut self, breakdowns: BTreeMap<EntityId, CashflowBreakdown>) -> Self {
        self.breakdowns = breakdowns;
        self
    }

    fn validate_breakdowns(&self, world: &World) -> Result<(), Error> {
        for (&country, breakdown) in &self.breakdowns {
            if world.entity(country)?.kind != EntityKind::Country
                || !self.amounts_milli.contains_key(&country)
            {
                return Err(Error::Invalid(
                    "cashflow breakdown requires a matching country amount".into(),
                ));
            }
            if breakdown.expenses_milli.is_some_and(|value| value < 0) {
                return Err(Error::Invalid(
                    "cashflow expenses must be nonnegative".into(),
                ));
            }
            let mut categorized_expenses = 0_i64;
            for (&category, &amount) in &breakdown.expense_categories_milli {
                if category >= crate::ledger::COUNTRY_LEDGER_EXPENSE_CATEGORY_COUNT || amount < 0 {
                    return Err(Error::Invalid(
                        "invalid cashflow expense category or amount".into(),
                    ));
                }
                categorized_expenses = categorized_expenses
                    .checked_add(amount)
                    .ok_or(Error::Overflow)?;
            }
            if breakdown
                .expenses_milli
                .is_some_and(|expenses| categorized_expenses > expenses)
            {
                return Err(Error::Invalid(
                    "categorized expenses exceed the cashflow expense total".into(),
                ));
            }
            if let (Some(amount), Some(net)) = (self.amounts_milli[&country], breakdown.net_milli())
                && amount != net
            {
                return Err(Error::Invalid(
                    "cashflow amount disagrees with its category breakdown".into(),
                ));
            }
        }
        Ok(())
    }

    pub(crate) fn prepare(
        &self,
        world: &World,
        taxation: Option<&TaxSettlement>,
        production: Option<&ProductionSettlement>,
    ) -> Result<CashflowSettlement, Error> {
        self.settlement_count
            .checked_add(1)
            .ok_or(Error::Overflow)?;
        self.validate_breakdowns(world)?;
        let mut accounts = BTreeMap::new();
        for (&country, &amount) in &self.amounts_milli {
            if world.entity(country)?.kind != EntityKind::Country {
                return Err(Error::Invalid(
                    "monthly cashflows require country entities".into(),
                ));
            }
            let mut unresolved = BTreeSet::new();
            let breakdown = self.breakdowns.get(&country).cloned();
            let amount = match (
                amount,
                breakdown.as_ref().and_then(CashflowBreakdown::net_milli),
            ) {
                (Some(amount), Some(net)) if amount == net => Some(amount),
                (Some(amount), _) => Some(amount),
                (None, Some(net)) => Some(net),
                (None, None) => None,
            };
            let (before, dependency_reason) = if let Some(account) =
                production.and_then(|settlement| settlement.accounts.get(&country))
            {
                (
                    account.treasury_after_milli,
                    "unknown_treasury_after_production",
                )
            } else if let Some(account) =
                taxation.and_then(|settlement| settlement.accounts.get(&country))
            {
                (account.treasury_after_milli, "unknown_treasury_after_tax")
            } else {
                (
                    world
                        .country_resources()
                        .get(&country)
                        .and_then(|resources| resources.treasury_milli),
                    "unknown_treasury",
                )
            };
            if before.is_none() {
                unresolved.insert(dependency_reason.into());
            }
            if amount.is_none() {
                unresolved.insert("unknown_cashflow".into());
            }
            let after = before
                .zip(amount)
                .map(|(before, amount)| crate::resources::treasury_after_adjustment(before, amount))
                .transpose()?;
            let credited = unresolved.is_empty().then_some(amount).flatten();
            accounts.insert(
                country,
                CashflowAccount {
                    amount_milli: amount,
                    breakdown,
                    treasury_before_milli: before,
                    treasury_after_milli: after.filter(|_| unresolved.is_empty()),
                    credited_milli: credited,
                    unresolved,
                },
            );
        }
        Ok(CashflowSettlement {
            day: world.day(),
            accounts,
        })
    }

    pub(crate) fn commit(&mut self, world: &mut World, settlement: CashflowSettlement) {
        for (&country, account) in &settlement.accounts {
            world
                .country_resources
                .entry(country)
                .or_default()
                .treasury_milli = account.treasury_after_milli;
        }
        self.settlement_count += 1;
        self.last_settlement = Some(settlement);
    }

    pub(crate) fn validate(&self, world: &World) -> Result<(), Error> {
        let calendar = world
            .calendar()
            .ok_or_else(|| Error::Invalid("monthly cashflows require a calendar".into()))?;
        if self.amounts_milli.is_empty()
            || self.settlement_count != calendar.elapsed_months(world.day())?
        {
            return Err(Error::Invalid(
                "invalid monthly cashflow rules or settlement count".into(),
            ));
        }
        self.validate_breakdowns(world)?;
        for &country in self.amounts_milli.keys() {
            if world.entity(country)?.kind != EntityKind::Country {
                return Err(Error::Invalid(
                    "monthly cashflows require country entities".into(),
                ));
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
                {
                    return Err(Error::Invalid("invalid saved cashflow settlement".into()));
                }
                for (&country, account) in &last.accounts {
                    if world.entity(country)?.kind != EntityKind::Country
                        || account.unresolved.iter().any(String::is_empty)
                    {
                        return Err(Error::Invalid("invalid saved cashflow account".into()));
                    }
                    let expected = account
                        .treasury_before_milli
                        .zip(account.amount_milli)
                        .map(|(before, amount)| {
                            crate::resources::treasury_after_adjustment(before, amount)
                        })
                        .transpose()?;
                    if account.unresolved.is_empty()
                        && (account.amount_milli.is_none()
                            || account.treasury_before_milli.is_none()
                            || account.treasury_after_milli != expected
                            || account.credited_milli != account.amount_milli)
                    {
                        return Err(Error::Invalid("invalid saved cashflow receipt".into()));
                    }
                    if let Some(breakdown) = &account.breakdown
                        && let Some(net) = breakdown.net_milli()
                        && account.amount_milli != Some(net)
                    {
                        return Err(Error::Invalid(
                            "saved cashflow breakdown disagrees with amount".into(),
                        ));
                    }
                    if !account.unresolved.is_empty()
                        && (account.treasury_after_milli.is_some()
                            || account.credited_milli.is_some())
                    {
                        return Err(Error::Invalid("invalid unresolved cashflow receipt".into()));
                    }
                }
                Ok(())
            }
            _ => Err(Error::Invalid(
                "invalid saved cashflow settlement count".into(),
            )),
        }
    }
}

impl World {
    pub fn monthly_cashflows(&self) -> Option<&MonthlyExternalCashflows> {
        self.monthly_cashflows.as_ref()
    }

    pub fn set_monthly_cashflows(
        &mut self,
        cashflows: Option<MonthlyExternalCashflows>,
    ) -> Result<(), Error> {
        self.require_running()?;
        if self.day() != 0 {
            return Err(Error::Invalid(
                "monthly cashflows must be selected before advancing".into(),
            ));
        }
        let custom_system = self.monthly_system_is_custom();
        if let Some(cashflows) = &cashflows {
            cashflows.validate(self)?;
        }
        self.monthly_cashflows = cashflows;
        if !custom_system {
            self.monthly_system_id = if self.monthly_cashflows.is_some()
                || self.monthly_accruals.is_some()
                || self.monthly_convergence.is_some()
                || self.monthly_stocks.is_some()
                || self.monthly_production.is_some()
            {
                Some(crate::MonthlyEconomy.id().into())
            } else {
                self.taxation
                    .as_ref()
                    .map(|_| crate::MonthlyTax.id().into())
            };
        }
        Ok(())
    }

    pub fn set_monthly_cashflow_inputs(
        &mut self,
        amounts_milli: BTreeMap<EntityId, Option<i64>>,
    ) -> Result<(), Error> {
        self.require_running()?;
        let Some(current) = self.monthly_cashflows.as_ref() else {
            return Err(Error::Invalid("monthly cashflows are not selected".into()));
        };
        let mut replacement = current.clone();
        replacement.amounts_milli = amounts_milli;
        replacement.validate(self)?;
        self.monthly_cashflows = Some(replacement);
        Ok(())
    }

    pub fn set_monthly_cashflow_breakdowns(
        &mut self,
        breakdowns: BTreeMap<EntityId, CashflowBreakdown>,
    ) -> Result<(), Error> {
        self.require_running()?;
        let Some(current) = self.monthly_cashflows.as_ref() else {
            return Err(Error::Invalid("monthly cashflows are not selected".into()));
        };
        let mut replacement = current.clone();
        replacement.breakdowns = breakdowns;
        replacement.validate(self)?;
        self.monthly_cashflows = Some(replacement);
        Ok(())
    }
}
