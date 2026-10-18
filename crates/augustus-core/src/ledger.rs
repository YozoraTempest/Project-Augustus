// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{EntityId, EntityKind, Error, World};

/// Native country-ledger categories occupy slots 0 through 18.
pub const COUNTRY_LEDGER_CATEGORY_COUNT: u32 = 19;

/// One category's accumulated income and expense records.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CountryLedgerCategory {
    pub income_milli: i64,
    pub expense_milli: i64,
}

/// Country cash-flow records translated from the compatibility boundary.
///
/// `pending_milli` is the delayed cash balance used by the native income and
/// expense entry points. It is deliberately separate from the treasury: the
/// compatibility layer decides when the native flush boundary is reached.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CountryLedger {
    #[serde(default = "default_pending")]
    pub pending_milli: i64,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub categories: BTreeMap<u32, CountryLedgerCategory>,
}

fn default_pending() -> i64 {
    0
}

impl CountryLedger {
    pub fn validate(&self) -> Result<(), Error> {
        for (&category, entry) in &self.categories {
            if category >= COUNTRY_LEDGER_CATEGORY_COUNT {
                return Err(Error::Invalid(
                    "country ledger category is outside 0..19".into(),
                ));
            }
            if entry.income_milli < 0 || entry.expense_milli < 0 {
                return Err(Error::Invalid(
                    "country ledger totals must be nonnegative".into(),
                ));
            }
        }
        Ok(())
    }

    fn category_mut(&mut self, category: u32) -> Result<&mut CountryLedgerCategory, Error> {
        if category >= COUNTRY_LEDGER_CATEGORY_COUNT {
            return Err(Error::Invalid(
                "country ledger category is outside 0..19".into(),
            ));
        }
        Ok(self.categories.entry(category).or_default())
    }

    fn add_income(&mut self, category: u32, amount: i64) -> Result<(), Error> {
        if amount < 0 {
            return Err(Error::Invalid(
                "country ledger income must be nonnegative".into(),
            ));
        }
        let entry = self.category_mut(category)?;
        entry.income_milli = entry
            .income_milli
            .checked_add(amount)
            .ok_or(Error::Overflow)?;
        Ok(())
    }

    fn add_expense(&mut self, category: u32, amount: i64) -> Result<(), Error> {
        if amount < 0 {
            return Err(Error::Invalid(
                "country ledger expense must be nonnegative".into(),
            ));
        }
        let entry = self.category_mut(category)?;
        entry.expense_milli = entry
            .expense_milli
            .checked_add(amount)
            .ok_or(Error::Overflow)?;
        Ok(())
    }

    /// Clears category records while keeping the delayed cash balance.
    pub fn reset_categories(&mut self) {
        self.categories.clear();
    }
}

const TREASURY_MIN_MILLI: i64 = -1_000_000_000;
const TREASURY_MAX_MILLI: i64 = 1_000_000_000;

impl World {
    pub fn country_ledgers(&self) -> &BTreeMap<EntityId, CountryLedger> {
        &self.country_ledgers
    }

    pub fn set_country_ledgers(
        &mut self,
        ledgers: BTreeMap<EntityId, CountryLedger>,
    ) -> Result<(), Error> {
        self.require_running()?;
        for (&country, ledger) in &ledgers {
            self.require_country(country)?;
            ledger.validate()?;
        }
        self.country_ledgers = ledgers;
        Ok(())
    }

    /// Records income and changes the treasury immediately.
    pub fn record_country_income(
        &mut self,
        country: EntityId,
        category: u32,
        amount_milli: i64,
    ) -> Result<(), Error> {
        self.record_country_flow(country, category, amount_milli, true, false)
    }

    /// Records an expense and changes the treasury immediately.
    pub fn record_country_expense(
        &mut self,
        country: EntityId,
        category: u32,
        amount_milli: i64,
    ) -> Result<(), Error> {
        self.record_country_flow(country, category, amount_milli, false, false)
    }

    /// Records delayed income without changing the treasury yet.
    pub fn record_delayed_country_income(
        &mut self,
        country: EntityId,
        category: u32,
        amount_milli: i64,
    ) -> Result<(), Error> {
        self.record_country_flow(country, category, amount_milli, true, true)
    }

    /// Records delayed expense without changing the treasury yet.
    pub fn record_delayed_country_expense(
        &mut self,
        country: EntityId,
        category: u32,
        amount_milli: i64,
    ) -> Result<(), Error> {
        self.record_country_flow(country, category, amount_milli, false, true)
    }

    /// Applies the delayed balance through the native treasury clamp.
    pub fn flush_country_pending(&mut self, country: EntityId) -> Result<i64, Error> {
        self.require_running()?;
        self.require_country(country)?;
        let pending = self
            .country_ledgers
            .get(&country)
            .ok_or_else(|| Error::Invalid("country ledger is unknown".into()))?
            .pending_milli;
        let treasury = self
            .country_resources
            .get(&country)
            .and_then(|resources| resources.treasury_milli)
            .ok_or_else(|| Error::Invalid("country treasury is unknown".into()))?;
        let next = treasury
            .checked_add(pending)
            .ok_or(Error::Overflow)?
            .clamp(TREASURY_MIN_MILLI, TREASURY_MAX_MILLI);
        self.country_resources
            .entry(country)
            .or_default()
            .treasury_milli = Some(next);
        self.country_ledgers
            .get_mut(&country)
            .expect("validated country ledger")
            .pending_milli = 0;
        Ok(next)
    }

    /// Rebuilds the category container without touching the delayed balance.
    pub fn reset_country_ledger(&mut self, country: EntityId) -> Result<(), Error> {
        self.require_running()?;
        self.require_country(country)?;
        self.country_ledgers
            .get_mut(&country)
            .ok_or_else(|| Error::Invalid("country ledger is unknown".into()))?
            .reset_categories();
        Ok(())
    }

    fn record_country_flow(
        &mut self,
        country: EntityId,
        category: u32,
        amount_milli: i64,
        income: bool,
        delayed: bool,
    ) -> Result<(), Error> {
        self.require_running()?;
        self.require_country(country)?;
        if amount_milli < 0 {
            return Err(Error::Invalid(
                "country ledger amount must be nonnegative".into(),
            ));
        }

        let mut ledger = self
            .country_ledgers
            .get(&country)
            .cloned()
            .unwrap_or_default();
        if income {
            ledger.add_income(category, amount_milli)?;
        } else {
            ledger.add_expense(category, amount_milli)?;
        }

        if delayed {
            ledger.pending_milli = if income {
                ledger
                    .pending_milli
                    .checked_add(amount_milli)
                    .ok_or(Error::Overflow)?
            } else {
                ledger
                    .pending_milli
                    .checked_sub(amount_milli)
                    .ok_or(Error::Overflow)?
            };
        } else {
            let treasury = self
                .country_resources
                .get(&country)
                .and_then(|resources| resources.treasury_milli)
                .ok_or_else(|| Error::Invalid("country treasury is unknown".into()))?;
            let delta = if income {
                amount_milli
            } else {
                amount_milli.checked_neg().ok_or(Error::Overflow)?
            };
            let next = treasury
                .checked_add(delta)
                .ok_or(Error::Overflow)?
                .clamp(TREASURY_MIN_MILLI, TREASURY_MAX_MILLI);
            self.country_resources
                .entry(country)
                .or_default()
                .treasury_milli = Some(next);
        }
        self.country_ledgers.insert(country, ledger);
        Ok(())
    }

    fn require_country(&self, country: EntityId) -> Result<(), Error> {
        if self.entity(country)?.kind != EntityKind::Country {
            return Err(Error::Invalid("country ledger requires a country".into()));
        }
        Ok(())
    }
}
