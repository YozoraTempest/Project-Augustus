// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::BTreeSet;

use augustus_core::{World, WorldDate};
use serde::Serialize;

use crate::{Error, VanillaWorld};

/// Current balances and the latest completed monthly receipts, not a forecast.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VanillaEconomyReport {
    pub country: String,
    pub day: u64,
    pub date: Option<WorldDate>,
    pub treasury_milli: Option<i64>,
    /// Absent when the country has no loaded ledger row. This balance has
    /// not been flushed into the current treasury.
    pub ledger_pending_milli: Option<i64>,
    pub tax: Option<VanillaEconomyStage>,
    pub production: Option<VanillaEconomyStage>,
    pub cashflow: Option<VanillaEconomyStage>,
}

/// An installed stage. The counter covers the entire component, and a missing
/// receipt means that no latest account for this country is recorded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VanillaEconomyStage {
    pub settlement_count: u64,
    pub last_receipt: Option<VanillaEconomyReceipt>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VanillaEconomyReceipt {
    pub day: u64,
    pub date: Option<WorldDate>,
    /// Monthly known portions for tax and ordinary production; cashflows do
    /// not expose a partial subtotal.
    pub known_subtotal_milli: Option<i64>,
    /// Tax income, ordinary production income, or the signed cashflow amount.
    pub amount_milli: Option<i64>,
    /// Signed confirmed adjustment; a cashflow expense can be negative.
    pub credited_milli: Option<i64>,
    /// Historical balances recorded by this stage. Later stages, ledger
    /// flushing and immediate transactions can change the current treasury.
    pub treasury_before_milli: Option<i64>,
    pub treasury_after_milli: Option<i64>,
    pub unresolved: BTreeSet<String>,
}

fn stage<T>(
    world: &World,
    settlement_count: u64,
    last: Option<(u64, &T)>,
    receipt: impl FnOnce(u64, Option<WorldDate>, &T) -> VanillaEconomyReceipt,
) -> Result<VanillaEconomyStage, Error> {
    let last_receipt = last
        .map(|(day, account)| {
            let date = world
                .calendar()
                .map(|calendar| calendar.date(day))
                .transpose()
                .map_err(|error| Error::caused_by("economy report", error))?;
            Ok(receipt(day, date, account))
        })
        .transpose()?;
    Ok(VanillaEconomyStage {
        settlement_count,
        last_receipt,
    })
}

impl VanillaWorld {
    /// Reads balances and existing receipts without settling, refreshing
    /// inputs, flushing the ledger or consuming RNG. Missing stages are not
    /// installed, and missing balances remain unknown.
    pub fn economy_report(&self, country: &str) -> Result<VanillaEconomyReport, Error> {
        let entity = self
            .initial
            .country_entity(country)
            .ok_or_else(|| Error::message("economy report", "unknown economy country"))?;
        let world = self.initial.world();
        let tax = world
            .taxation()
            .map(|state| {
                let last = state.last_settlement.as_ref().and_then(|last| {
                    last.accounts
                        .get(&entity)
                        .map(|account| (last.day, account))
                });
                stage(world, state.settlement_count, last, |day, date, account| {
                    VanillaEconomyReceipt {
                        day,
                        date,
                        known_subtotal_milli: Some(account.known_subtotal_milli),
                        amount_milli: account.income_milli,
                        credited_milli: account.credited_milli,
                        treasury_before_milli: account.treasury_before_milli,
                        treasury_after_milli: account.treasury_after_milli,
                        unresolved: account.unresolved.clone(),
                    }
                })
            })
            .transpose()?;
        let production = world
            .monthly_production()
            .map(|state| {
                let last = state.last_settlement.as_ref().and_then(|last| {
                    last.accounts
                        .get(&entity)
                        .map(|account| (last.day, account))
                });
                stage(world, state.settlement_count, last, |day, date, account| {
                    VanillaEconomyReceipt {
                        day,
                        date,
                        known_subtotal_milli: Some(account.known_monthly_value_milli),
                        amount_milli: account.monthly_value_milli,
                        credited_milli: account.credited_milli,
                        treasury_before_milli: account.treasury_before_milli,
                        treasury_after_milli: account.treasury_after_milli,
                        unresolved: account.unresolved.clone(),
                    }
                })
            })
            .transpose()?;
        let cashflow = world
            .monthly_cashflows()
            .map(|state| {
                let last = state.last_settlement.as_ref().and_then(|last| {
                    last.accounts
                        .get(&entity)
                        .map(|account| (last.day, account))
                });
                stage(world, state.settlement_count, last, |day, date, account| {
                    VanillaEconomyReceipt {
                        day,
                        date,
                        known_subtotal_milli: None,
                        amount_milli: account.amount_milli,
                        credited_milli: account.credited_milli,
                        treasury_before_milli: account.treasury_before_milli,
                        treasury_after_milli: account.treasury_after_milli,
                        unresolved: account.unresolved.clone(),
                    }
                })
            })
            .transpose()?;
        Ok(VanillaEconomyReport {
            country: country.into(),
            day: world.day(),
            date: world
                .date()
                .map_err(|error| Error::caused_by("economy report", error))?,
            treasury_milli: world
                .country_resources()
                .get(&entity)
                .and_then(|resources| resources.treasury_milli),
            ledger_pending_milli: world
                .country_ledgers()
                .get(&entity)
                .map(|ledger| ledger.pending_milli),
            tax,
            production,
            cashflow,
        })
    }
}
