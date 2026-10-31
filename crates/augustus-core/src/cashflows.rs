// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use crate::{
    EntityId, EntityKind, Error, MonthlySystem, ProductionSettlement, TaxSettlement,
    VanillaTradeIncomePassInput, World,
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
    /// Selected trade-income records for core-side calculation. If supplied,
    /// `trade_milli` may be omitted or must match the calculated total.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trade_income_pass: Option<VanillaTradeIncomePassInput>,
    pub gold_milli: Option<i64>,
    pub expenses_milli: Option<i64>,
    /// Known portions of aggregate expenses, keyed by native ledger category.
    /// Omitted categories remain unclassified rather than being inferred as zero.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub expense_categories_milli: BTreeMap<u32, i64>,
}

impl CashflowBreakdown {
    pub(crate) fn resolved_trade_milli(&self) -> Result<Option<i64>, Error> {
        let Some(input) = &self.trade_income_pass else {
            return Ok(self.trade_milli);
        };
        let calculated = i64::from(
            crate::calculate_vanilla_trade_income_pass(input.clone())?.total_income_milli,
        );
        if self.trade_milli.is_some_and(|value| value != calculated) {
            return Err(Error::Invalid(
                "cashflow trade amount disagrees with trade-income records".into(),
            ));
        }
        Ok(Some(calculated))
    }

    pub fn try_net_milli(&self) -> Result<Option<i64>, Error> {
        let Some(((trade, gold), expenses)) = self
            .resolved_trade_milli()?
            .zip(self.gold_milli)
            .zip(self.expenses_milli)
        else {
            return Ok(None);
        };
        let net = trade
            .checked_add(gold)
            .and_then(|value| value.checked_sub(expenses))
            .ok_or(Error::Overflow)?;
        Ok(Some(net))
    }

    pub fn net_milli(&self) -> Option<i64> {
        self.try_net_milli().ok().flatten()
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vanilla_gold_production_inputs:
        Option<BTreeMap<EntityId, crate::VanillaGoldProductionInputState>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vanilla_state_maintenance_inputs:
        Option<BTreeMap<EntityId, crate::VanillaStateMaintenanceProvinceInputState>>,
    pub settlement_count: u64,
    pub last_settlement: Option<CashflowSettlement>,
}

struct ResolvedCashflowInputs {
    amounts_milli: BTreeMap<EntityId, Option<i64>>,
    breakdowns: BTreeMap<EntityId, CashflowBreakdown>,
}

impl MonthlyExternalCashflows {
    pub fn new(amounts_milli: BTreeMap<EntityId, Option<i64>>) -> Self {
        Self {
            amounts_milli,
            breakdowns: BTreeMap::new(),
            vanilla_gold_production_inputs: None,
            vanilla_state_maintenance_inputs: None,
            settlement_count: 0,
            last_settlement: None,
        }
    }

    pub fn with_breakdowns(mut self, breakdowns: BTreeMap<EntityId, CashflowBreakdown>) -> Self {
        self.breakdowns = breakdowns;
        self
    }

    pub fn with_vanilla_gold_production_inputs(
        mut self,
        inputs: BTreeMap<EntityId, crate::VanillaGoldProductionInputState>,
    ) -> Self {
        self.vanilla_gold_production_inputs = Some(inputs);
        self
    }

    pub fn with_vanilla_state_maintenance_inputs(
        mut self,
        inputs: BTreeMap<EntityId, crate::VanillaStateMaintenanceProvinceInputState>,
    ) -> Self {
        self.vanilla_state_maintenance_inputs = Some(inputs);
        self
    }

    fn validate_breakdowns(&self, world: &World) -> Result<BTreeMap<EntityId, Option<i64>>, Error> {
        Self::validate_breakdown_inputs(world, &self.amounts_milli, &self.breakdowns)
    }

    fn validate_breakdown_inputs(
        world: &World,
        amounts_milli: &BTreeMap<EntityId, Option<i64>>,
        breakdowns: &BTreeMap<EntityId, CashflowBreakdown>,
    ) -> Result<BTreeMap<EntityId, Option<i64>>, Error> {
        let mut net_amounts = BTreeMap::new();
        for (&country, breakdown) in breakdowns {
            if world.entity(country)?.kind != EntityKind::Country
                || !amounts_milli.contains_key(&country)
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
            let net = breakdown.try_net_milli()?;
            if let (Some(amount), Some(net)) = (amounts_milli[&country], net)
                && amount != net
            {
                return Err(Error::Invalid(
                    "cashflow amount disagrees with its category breakdown".into(),
                ));
            }
            net_amounts.insert(country, net);
        }
        Ok(net_amounts)
    }

    fn validate_vanilla_gold_production_inputs(&self, world: &World) -> Result<(), Error> {
        let Some(inputs) = &self.vanilla_gold_production_inputs else {
            return Ok(());
        };
        for (&province, state) in inputs {
            if world.entity(province)?.kind != EntityKind::Province
                || state.unresolved.iter().any(String::is_empty)
            {
                return Err(Error::Invalid(
                    "invalid vanilla gold-production input state".into(),
                ));
            }
        }
        Ok(())
    }

    fn validate_vanilla_state_maintenance_inputs(&self, world: &World) -> Result<(), Error> {
        let Some(inputs) = &self.vanilla_state_maintenance_inputs else {
            return Ok(());
        };
        for (&province, state) in inputs {
            if world.entity(province)?.kind != EntityKind::Province
                || state.unresolved.iter().any(String::is_empty)
            {
                return Err(Error::Invalid(
                    "invalid vanilla state-maintenance input state".into(),
                ));
            }
        }
        Ok(())
    }

    fn resolve_vanilla_gold_production_inputs(
        &self,
        world: &World,
    ) -> Result<ResolvedCashflowInputs, Error> {
        let mut amounts_milli = self.amounts_milli.clone();
        let mut breakdowns = self.breakdowns.clone();
        let Some(inputs) = &self.vanilla_gold_production_inputs else {
            return Ok(ResolvedCashflowInputs {
                amounts_milli,
                breakdowns,
            });
        };

        self.validate_vanilla_gold_production_inputs(world)?;
        for &province in inputs.keys() {
            if let Some(owner) = world.entity(province)?.owner {
                if world.entity(owner)?.kind != EntityKind::Country {
                    return Err(Error::Invalid(
                        "gold-production province owner must be a country".into(),
                    ));
                }
                amounts_milli.entry(owner).or_insert(None);
            }
        }

        let countries: Vec<_> = amounts_milli.keys().copied().collect();
        for country in countries {
            let mut known_monthly_value = 0_i64;
            let mut has_gold = false;
            let mut unresolved = false;
            for &province in world.owned_provinces(country)? {
                let Some(state) = inputs.get(&province) else {
                    continue;
                };
                has_gold = true;
                let Some(input) = state.input.filter(|_| state.unresolved.is_empty()) else {
                    unresolved = true;
                    continue;
                };
                let monthly =
                    i64::from(crate::calculate_vanilla_gold_production(input).monthly_value_milli);
                known_monthly_value = known_monthly_value
                    .checked_add(monthly)
                    .ok_or(Error::Overflow)?;
            }

            if !has_gold && !breakdowns.contains_key(&country) {
                continue;
            }
            let breakdown = breakdowns
                .entry(country)
                .or_insert_with(|| CashflowBreakdown {
                    trade_milli: None,
                    trade_income_pass: None,
                    gold_milli: None,
                    expenses_milli: None,
                    expense_categories_milli: BTreeMap::new(),
                });
            breakdown.gold_milli = (!unresolved).then_some(known_monthly_value);

            let trade_is_known = breakdown.resolved_trade_milli()?.is_some();
            if trade_is_known && breakdown.expenses_milli.is_some() {
                amounts_milli.insert(country, breakdown.try_net_milli()?);
            }
        }
        Ok(ResolvedCashflowInputs {
            amounts_milli,
            breakdowns,
        })
    }

    fn resolve_vanilla_state_maintenance_inputs(
        &self,
        world: &World,
        resolved: &mut ResolvedCashflowInputs,
    ) -> Result<(), Error> {
        let Some(inputs) = &self.vanilla_state_maintenance_inputs else {
            return Ok(());
        };
        self.validate_vanilla_state_maintenance_inputs(world)?;

        for breakdown in resolved.breakdowns.values_mut() {
            breakdown
                .expense_categories_milli
                .remove(&crate::COUNTRY_LEDGER_EXPENSE_STATE_MAINTENANCE_CATEGORY);
        }

        for (index, entity) in world.entities().iter().enumerate() {
            if entity.kind != EntityKind::Country {
                continue;
            }
            let country = EntityId(index);
            let Some(amount) =
                crate::calculate_vanilla_state_maintenance_for_country(world, country, inputs)?
            else {
                continue;
            };
            resolved.amounts_milli.entry(country).or_insert(None);
            let breakdown =
                resolved
                    .breakdowns
                    .entry(country)
                    .or_insert_with(|| CashflowBreakdown {
                        trade_milli: None,
                        trade_income_pass: None,
                        gold_milli: None,
                        expenses_milli: None,
                        expense_categories_milli: BTreeMap::new(),
                    });
            breakdown.expense_categories_milli.insert(
                crate::COUNTRY_LEDGER_EXPENSE_STATE_MAINTENANCE_CATEGORY,
                amount,
            );
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
        let mut resolved = self.resolve_vanilla_gold_production_inputs(world)?;
        self.resolve_vanilla_state_maintenance_inputs(world, &mut resolved)?;
        let breakdown_nets =
            Self::validate_breakdown_inputs(world, &resolved.amounts_milli, &resolved.breakdowns)?;
        let mut accounts = BTreeMap::new();
        for (&country, &amount) in &resolved.amounts_milli {
            if world.entity(country)?.kind != EntityKind::Country {
                return Err(Error::Invalid(
                    "monthly cashflows require country entities".into(),
                ));
            }
            let mut unresolved = BTreeSet::new();
            let breakdown = resolved.breakdowns.get(&country).cloned();
            let net = breakdown_nets.get(&country).copied().flatten();
            let amount = match (amount, net) {
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
        if self.vanilla_gold_production_inputs.is_some()
            || self.vanilla_state_maintenance_inputs.is_some()
        {
            for (&country, account) in &settlement.accounts {
                self.amounts_milli.insert(country, account.amount_milli);
                if let Some(breakdown) = &account.breakdown {
                    self.breakdowns.insert(country, breakdown.clone());
                }
            }
        }
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
        self.validate_vanilla_gold_production_inputs(world)?;
        self.validate_vanilla_state_maintenance_inputs(world)?;
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
                        && let Some(net) = breakdown.try_net_milli()?
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
    /// Prepares the next monthly cashflow settlement without committing
    /// refreshed prices, inputs, treasury, ledger entries, receipts, or
    /// settlement counters. The staged calculation follows the complete
    /// monthly executor order through taxation and ordinary production.
    pub fn preview_monthly_cashflows(&self) -> Result<CashflowSettlement, Error> {
        if self.monthly_cashflows.is_none() {
            return Err(Error::Invalid("missing monthly cashflows".into()));
        }
        let mut staged = self.clone();
        let refreshed_prices = staged.economy().refreshed_trade_good_prices()?;
        let refreshed_production = staged
            .monthly_production
            .as_ref()
            .map(|production| production.refreshed_prices(&staged, &refreshed_prices))
            .transpose()?;
        if let Some(administration) = staged.prepare_monthly_autonomy()? {
            staged.replace_administration(administration)?;
        }
        let taxes = staged
            .taxation()
            .map(|taxation| taxation.prepare(&staged))
            .transpose()?;
        staged
            .monthly_accruals
            .as_ref()
            .map(|accruals| accruals.prepare(&staged))
            .transpose()?;
        staged
            .monthly_convergence
            .as_ref()
            .map(|convergence| convergence.prepare(&staged))
            .transpose()?;
        staged
            .monthly_stocks
            .as_ref()
            .map(|stocks| stocks.prepare(&staged))
            .transpose()?;
        let production = match staged.monthly_production.as_ref() {
            Some(_) => Some(
                refreshed_production
                    .as_ref()
                    .expect("monthly production was checked before staging")
                    .prepare(&staged, taxes.as_ref())?,
            ),
            None => None,
        };
        staged
            .monthly_cashflows
            .as_ref()
            .expect("monthly cashflows was checked before staging")
            .prepare(&staged, taxes.as_ref(), production.as_ref())
    }

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

    pub fn set_monthly_vanilla_gold_production_inputs(
        &mut self,
        inputs: Option<BTreeMap<EntityId, crate::VanillaGoldProductionInputState>>,
    ) -> Result<(), Error> {
        self.require_running()?;
        let Some(current) = self.monthly_cashflows.as_ref() else {
            return Err(Error::Invalid("monthly cashflows are not selected".into()));
        };
        let mut replacement = current.clone();
        replacement.vanilla_gold_production_inputs = inputs;
        replacement.validate_vanilla_gold_production_inputs(self)?;
        self.monthly_cashflows = Some(replacement);
        Ok(())
    }

    pub fn set_monthly_vanilla_state_maintenance_inputs(
        &mut self,
        inputs: Option<BTreeMap<EntityId, crate::VanillaStateMaintenanceProvinceInputState>>,
    ) -> Result<(), Error> {
        self.require_running()?;
        let Some(current) = self.monthly_cashflows.as_ref() else {
            return Err(Error::Invalid("monthly cashflows are not selected".into()));
        };
        let mut replacement = current.clone();
        if current.vanilla_state_maintenance_inputs.is_some() && inputs.is_none() {
            for breakdown in replacement.breakdowns.values_mut() {
                breakdown
                    .expense_categories_milli
                    .remove(&crate::COUNTRY_LEDGER_EXPENSE_STATE_MAINTENANCE_CATEGORY);
            }
        }
        replacement.vanilla_state_maintenance_inputs = inputs;
        replacement.validate_vanilla_state_maintenance_inputs(self)?;
        self.monthly_cashflows = Some(replacement);
        Ok(())
    }
}
