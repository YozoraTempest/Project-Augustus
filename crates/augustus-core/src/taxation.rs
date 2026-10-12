// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

use crate::{EntityId, EntityKind, Error, World};

/// A selected monthly executor. Its identifier includes the rule version.
/// Providers own their persistent state in World; changing the provider is a
/// pre-campaign decision. No previous executor runs alongside the replacement.
pub trait MonthlySystem {
    fn id(&self) -> &str;
    fn validate(&self, world: &World) -> Result<(), Error>;
    fn settle(&self, world: &mut World) -> Result<(), Error>;
}

pub struct MonthlyTax;

impl MonthlySystem for MonthlyTax {
    fn id(&self) -> &str {
        "monthly-tax:1"
    }

    fn validate(&self, world: &World) -> Result<(), Error> {
        world
            .taxation()
            .ok_or_else(|| Error::Invalid("missing taxation state".into()))?
            .validate(world)
    }

    fn settle(&self, world: &mut World) -> Result<(), Error> {
        let mut taxation = world
            .taxation
            .take()
            .ok_or_else(|| Error::Invalid("missing taxation state".into()))?;
        let result = taxation.settle(world);
        world.taxation = Some(taxation);
        result
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MonthlyTaxAccount {
    pub known_subtotal_milli: i64,
    pub income_milli: Option<i64>,
    pub credited_milli: Option<i64>,
    pub treasury_before_milli: Option<i64>,
    pub treasury_after_milli: Option<i64>,
    pub unresolved: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaxSettlement {
    pub day: u64,
    pub accounts: BTreeMap<EntityId, MonthlyTaxAccount>,
}

/// Monthly tax is derived from current province ownership and an annual
/// attribute. This component owns tax receipts only; other cash flows need
/// their own systems. The latest receipt is historical, not a value cache.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Taxation {
    pub annual_attribute: String,
    pub countries: BTreeSet<EntityId>,
    pub settlement_count: u64,
    pub last_settlement: Option<TaxSettlement>,
}

impl Taxation {
    pub fn new(annual_attribute: impl Into<String>, countries: BTreeSet<EntityId>) -> Self {
        Self {
            annual_attribute: annual_attribute.into(),
            countries,
            settlement_count: 0,
            last_settlement: None,
        }
    }

    pub(crate) fn settle(&mut self, world: &mut World) -> Result<(), Error> {
        let settlement = self.prepare(world)?;
        self.commit(world, settlement);
        Ok(())
    }

    pub(crate) fn prepare(&self, world: &World) -> Result<TaxSettlement, Error> {
        self.settlement_count
            .checked_add(1)
            .ok_or(Error::Overflow)?;
        let mut accounts = BTreeMap::new();
        // Prepare all receipts before committing any balance. An overflow in a
        // later country cannot leave an earlier country partially settled.
        for &country in &self.countries {
            let amount = world
                .attributes()
                .sum_owned(world, country, &self.annual_attribute)?;
            let income_milli = amount.value_micros.map(|v| v / 12_000);
            let before = world
                .country_resources()
                .get(&country)
                .and_then(|r| r.treasury_milli);
            let mut unresolved = amount.unresolved;
            if before.is_none() {
                unresolved.insert("unknown_treasury".into());
            }
            let after = before
                .zip(income_milli)
                .map(|(balance, income)| balance.checked_add(income).ok_or(Error::Overflow))
                .transpose()?;
            accounts.insert(
                country,
                MonthlyTaxAccount {
                    known_subtotal_milli: amount.known_subtotal_micros / 12_000,
                    income_milli,
                    credited_milli: after.and(income_milli),
                    treasury_before_milli: before,
                    treasury_after_milli: after,
                    unresolved,
                },
            );
        }
        Ok(TaxSettlement {
            day: world.day(),
            accounts,
        })
    }

    pub(crate) fn commit(&mut self, world: &mut World, settlement: TaxSettlement) {
        for (&country, account) in &settlement.accounts {
            world
                .country_resources
                .entry(country)
                .or_default()
                .treasury_milli = account.treasury_after_milli;
        }
        self.last_settlement = Some(settlement);
        self.settlement_count += 1;
    }

    pub(crate) fn validate(&self, world: &World) -> Result<(), Error> {
        let calendar = world
            .calendar()
            .ok_or_else(|| Error::Invalid("taxation requires a calendar".into()))?;
        if self.annual_attribute.is_empty()
            || self.settlement_count != calendar.elapsed_months(world.day())?
        {
            return Err(Error::Invalid(
                "invalid taxation configuration or month count".into(),
            ));
        }
        for &country in &self.countries {
            if world.entity(country)?.kind != EntityKind::Country {
                return Err(Error::Invalid("taxation requires country entities".into()));
            }
        }
        match &self.last_settlement {
            None if self.settlement_count == 0 => Ok(()),
            Some(last) if self.settlement_count > 0 => {
                let current = calendar.date(world.day())?;
                let last_date = calendar.date(last.day)?;
                if last.day == 0
                    || last.day > world.day()
                    || last_date.day != 1
                    || last_date.year != current.year
                    || last_date.month != current.month
                    || last.accounts.keys().copied().collect::<BTreeSet<_>>() != self.countries
                {
                    return Err(Error::Invalid("invalid saved tax settlement".into()));
                }
                for account in last.accounts.values() {
                    let expected = account
                        .treasury_before_milli
                        .zip(account.income_milli)
                        .map(|(before, income)| before.checked_add(income).ok_or(Error::Overflow))
                        .transpose()?;
                    if account.treasury_after_milli != expected
                        || account.credited_milli != expected.and(account.income_milli)
                        || account
                            .income_milli
                            .is_some_and(|v| v != account.known_subtotal_milli)
                        || (account.income_milli.is_none() && account.unresolved.is_empty())
                    {
                        return Err(Error::Invalid("invalid saved tax account".into()));
                    }
                }
                Ok(())
            }
            _ => Err(Error::Invalid("invalid saved tax settlement count".into())),
        }
    }
}
