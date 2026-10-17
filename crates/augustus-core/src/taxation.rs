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
    /// A country source is added once, after summing its provinces.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub country_annual_attribute: Option<String>,
    /// Explicit annual country tax supplied by a compatibility provider.
    ///
    /// An entry takes precedence over `country_annual_attribute` for the same
    /// country. This lets a compatibility layer provide a verified native
    /// source while retaining content-derived taxation for countries without
    /// an override. Values are annual milli-units; `None` keeps the country
    /// unresolved while preserving the known provincial subtotal.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub external_country_annual_milli: BTreeMap<EntityId, Option<i64>>,
    /// Explicit annual province tax supplied by a compatibility provider.
    ///
    /// An entry replaces the content-derived annual attribute for that
    /// province. `None` keeps that province unresolved while retaining the
    /// known values from the other provinces. Values are annual milli-units.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub external_province_annual_milli: BTreeMap<EntityId, Option<i64>>,
    pub countries: BTreeSet<EntityId>,
    pub settlement_count: u64,
    pub last_settlement: Option<TaxSettlement>,
}

impl Taxation {
    pub fn new(annual_attribute: impl Into<String>, countries: BTreeSet<EntityId>) -> Self {
        Self {
            annual_attribute: annual_attribute.into(),
            country_annual_attribute: None,
            external_country_annual_milli: BTreeMap::new(),
            external_province_annual_milli: BTreeMap::new(),
            countries,
            settlement_count: 0,
            last_settlement: None,
        }
    }

    pub fn with_country_annual_attribute(mut self, attribute: impl Into<String>) -> Self {
        self.country_annual_attribute = Some(attribute.into());
        self
    }

    pub fn with_external_country_annual_milli(
        mut self,
        values: BTreeMap<EntityId, Option<i64>>,
    ) -> Self {
        self.external_country_annual_milli = values;
        self
    }

    pub fn with_external_province_annual_milli(
        mut self,
        values: BTreeMap<EntityId, Option<i64>>,
    ) -> Self {
        self.external_province_annual_milli = values;
        self
    }

    fn annual_amount(
        &self,
        world: &World,
        country: EntityId,
    ) -> Result<crate::AttributeAggregate, Error> {
        let mut subtotal = 0_i128;
        let mut unresolved = BTreeSet::new();
        let mut count = 0;
        if world.ownership.has_unknown() {
            unresolved.insert("unknown_province_ownership".into());
        }
        let mut members = world.ownership.known_members(country).to_vec();
        members.sort_unstable();
        for entity in members {
            let province = world.entity(entity)?;
            count += 1;
            if let Some(value) = self.external_province_annual_milli.get(&entity) {
                match value {
                    Some(value) => {
                        subtotal = subtotal
                            .checked_add(i128::from(*value) * 1_000)
                            .ok_or(Error::Overflow)?;
                    }
                    None => {
                        unresolved.insert(format!(
                            "province:{}:unknown_external_annual_tax",
                            province.name
                        ));
                    }
                }
                continue;
            }
            if !world
                .attributes()
                .formulas
                .get(&entity)
                .is_some_and(|formulas| formulas.contains_key(&self.annual_attribute))
            {
                unresolved.insert(format!(
                    "province:{}:missing_attribute:{}",
                    province.name, self.annual_attribute
                ));
                continue;
            }
            let result = world
                .attributes()
                .evaluate(world, entity, &self.annual_attribute)?;
            if let Some(value) = result.known_subtotal_micros {
                subtotal = subtotal
                    .checked_add(i128::from(value))
                    .ok_or(Error::Overflow)?;
            } else {
                let missing = if result.base_micros.is_none() {
                    "unknown_base"
                } else {
                    "unknown_calculation"
                };
                unresolved.insert(format!("province:{}:{missing}", province.name));
            }
            unresolved.extend(result.unresolved);
        }
        let subtotal = i64::try_from(subtotal).map_err(|_| Error::Overflow)?;
        Ok(crate::AttributeAggregate {
            province_count: count,
            known_subtotal_micros: subtotal,
            value_micros: unresolved.is_empty().then_some(subtotal),
            unresolved,
        })
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
            let mut amount = self.annual_amount(world, country)?;
            if let Some(value) = self.external_country_annual_milli.get(&country) {
                match value {
                    Some(value) => {
                        let micros = value.checked_mul(1_000).ok_or(Error::Overflow)?;
                        amount.known_subtotal_micros = amount
                            .known_subtotal_micros
                            .checked_add(micros)
                            .ok_or(Error::Overflow)?;
                        amount.value_micros = amount
                            .value_micros
                            .map(|provinces| provinces.checked_add(micros).ok_or(Error::Overflow))
                            .transpose()?;
                    }
                    None => {
                        amount
                            .unresolved
                            .insert("unknown_external_country_annual_tax".into());
                        amount.value_micros = None;
                    }
                }
            } else if let Some(attribute) = &self.country_annual_attribute {
                let country_amount = world.attributes().evaluate(world, country, attribute)?;
                amount.known_subtotal_micros = amount
                    .known_subtotal_micros
                    .checked_add(country_amount.known_subtotal_micros.unwrap_or(0))
                    .ok_or(Error::Overflow)?;
                amount.value_micros = amount
                    .value_micros
                    .zip(country_amount.value_micros)
                    .map(|(provinces, country)| {
                        provinces.checked_add(country).ok_or(Error::Overflow)
                    })
                    .transpose()?;
                amount.unresolved.extend(country_amount.unresolved);
            }
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
            || self
                .country_annual_attribute
                .as_ref()
                .is_some_and(String::is_empty)
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
            if let Some(attribute) = &self.country_annual_attribute
                && !world
                    .attributes()
                    .formulas
                    .get(&country)
                    .is_some_and(|formulas| formulas.contains_key(attribute))
            {
                return Err(Error::Invalid("missing country tax attribute".into()));
            }
        }
        if self
            .external_country_annual_milli
            .keys()
            .any(|country| !self.countries.contains(country))
        {
            return Err(Error::Invalid(
                "external country tax requires a configured taxation country".into(),
            ));
        }
        if self
            .external_province_annual_milli
            .keys()
            .any(|province| world.entity(*province).is_err())
        {
            return Err(Error::Invalid(
                "external province tax requires valid province entities".into(),
            ));
        }
        for province in self.external_province_annual_milli.keys() {
            if world.entity(*province)?.kind != EntityKind::Province {
                return Err(Error::Invalid(
                    "external province tax requires province entities".into(),
                ));
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
