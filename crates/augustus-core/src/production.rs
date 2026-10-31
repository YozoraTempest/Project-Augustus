// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

use crate::{
    EntityId, EntityKind, Error, MonthlySystem, TaxSettlement, VanillaProductionBaseValueInput,
    VanillaProductionInputState, World, calculate_vanilla_production,
    calculate_vanilla_production_base_value,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProductionProvinceAccount {
    pub pre_query_value_milli: Option<i64>,
    pub annual_value_milli: Option<i64>,
    pub monthly_value_milli: Option<i64>,
    pub unresolved: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProductionCountryAccount {
    pub known_annual_value_milli: i64,
    pub known_monthly_value_milli: i64,
    pub annual_value_milli: Option<i64>,
    pub monthly_value_milli: Option<i64>,
    pub treasury_before_milli: Option<i64>,
    pub treasury_after_milli: Option<i64>,
    pub credited_milli: Option<i64>,
    pub provinces: BTreeMap<EntityId, ProductionProvinceAccount>,
    pub unresolved: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProductionSettlement {
    pub day: u64,
    pub accounts: BTreeMap<EntityId, ProductionCountryAccount>,
    pub unresolved_provinces: BTreeMap<EntityId, BTreeSet<String>>,
}

/// Monthly ordinary production with explicitly supplied current inputs.
///
/// This component owns settlement and receipts. The compatibility layer owns
/// the producers and refresh timing of each input row.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MonthlyProduction {
    pub inputs: BTreeMap<EntityId, VanillaProductionInputState>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub price_inputs: BTreeMap<EntityId, VanillaProductionBaseValueInput>,
    pub settlement_count: u64,
    pub last_settlement: Option<ProductionSettlement>,
}

impl MonthlyProduction {
    pub fn new(inputs: BTreeMap<EntityId, VanillaProductionInputState>) -> Self {
        Self {
            inputs,
            price_inputs: BTreeMap::new(),
            settlement_count: 0,
            last_settlement: None,
        }
    }

    pub fn with_price_inputs(
        mut self,
        inputs: BTreeMap<EntityId, VanillaProductionBaseValueInput>,
    ) -> Self {
        self.price_inputs = inputs;
        self
    }

    pub(crate) fn refreshed_prices(
        &self,
        world: &World,
        refreshed_prices: &BTreeMap<String, Option<i64>>,
    ) -> Result<Self, Error> {
        let mut refreshed = self.clone();
        for (&province, source) in &self.price_inputs {
            let trade_good = world
                .economy()
                .provinces
                .get(&province)
                .and_then(|state| state.trade_good.as_deref())
                .ok_or_else(|| Error::Invalid("price input province has no trade good".into()))?;
            let price = refreshed_prices
                .get(trade_good)
                .copied()
                .flatten()
                .or_else(|| {
                    world
                        .economy()
                        .current_trade_good_price_milli(trade_good)
                        .flatten()
                })
                .ok_or_else(|| {
                    Error::Invalid("price input has no current trade-good price".into())
                })?;
            let price = i32::try_from(price)
                .map_err(|_| Error::Invalid("trade-good price exceeds production range".into()))?;
            let value = calculate_vanilla_production_base_value(VanillaProductionBaseValueInput {
                trade_good_price_milli: price,
                ..*source
            });
            let state = refreshed
                .inputs
                .get_mut(&province)
                .ok_or_else(|| Error::Invalid("price input has no production input".into()))?;
            let input = state.input.as_mut().ok_or_else(|| {
                Error::Invalid("price input production state is unresolved".into())
            })?;
            input.base_value_milli = value.value_milli;
        }
        Ok(refreshed)
    }

    pub(crate) fn prepare(
        &self,
        world: &World,
        taxation: Option<&TaxSettlement>,
    ) -> Result<ProductionSettlement, Error> {
        self.settlement_count
            .checked_add(1)
            .ok_or(Error::Overflow)?;

        let mut provinces_by_country: BTreeMap<
            EntityId,
            BTreeMap<EntityId, ProductionProvinceAccount>,
        > = BTreeMap::new();
        let mut unresolved_provinces = BTreeMap::new();

        for (&province, state) in &self.inputs {
            let entity = world.entity(province)?;
            if entity.kind != EntityKind::Province {
                return Err(Error::Invalid(
                    "monthly production inputs require province entities".into(),
                ));
            }
            let Some(owner) = entity.owner else {
                unresolved_provinces.insert(province, ["unknown_owner".into()].into());
                continue;
            };
            if world.entity(owner)?.kind != EntityKind::Country {
                return Err(Error::Invalid(
                    "monthly production province owner must be a country".into(),
                ));
            }

            let mut unresolved = BTreeSet::new();
            let result = match (&state.input, state.unresolved.is_empty()) {
                (Some(input), true) => Some(calculate_vanilla_production(*input)),
                (None, true) => {
                    unresolved.insert("missing_production_input".into());
                    None
                }
                (_, false) => {
                    unresolved.extend(
                        state
                            .unresolved
                            .iter()
                            .filter(|reason| !reason.is_empty())
                            .cloned(),
                    );
                    if unresolved.is_empty() {
                        unresolved.insert("unknown_production_input".into());
                    }
                    None
                }
            };
            let (pre_query, annual, monthly) = result
                .map(|value| {
                    (
                        Some(i64::from(value.pre_query_value_milli)),
                        Some(i64::from(value.annual_value_milli)),
                        Some(i64::from(value.monthly_value_milli)),
                    )
                })
                .unwrap_or((None, None, None));
            provinces_by_country.entry(owner).or_default().insert(
                province,
                ProductionProvinceAccount {
                    pre_query_value_milli: pre_query,
                    annual_value_milli: annual,
                    monthly_value_milli: monthly,
                    unresolved,
                },
            );
        }

        let mut accounts = BTreeMap::new();
        for (country, provinces) in provinces_by_country {
            let mut known_annual = 0_i64;
            let mut known_monthly = 0_i64;
            let mut input_unresolved = BTreeSet::new();
            for account in provinces.values() {
                if let Some(value) = account.annual_value_milli {
                    known_annual = known_annual.checked_add(value).ok_or(Error::Overflow)?;
                }
                if let Some(value) = account.monthly_value_milli {
                    known_monthly = known_monthly.checked_add(value).ok_or(Error::Overflow)?;
                }
                input_unresolved.extend(
                    account
                        .unresolved
                        .iter()
                        .map(|reason| format!("province_input:{reason}")),
                );
            }
            let annual = input_unresolved.is_empty().then_some(known_annual);
            let monthly = input_unresolved.is_empty().then_some(known_monthly);

            let (treasury_before, mut unresolved) = if let Some(taxation) = taxation {
                match taxation.accounts.get(&country) {
                    Some(account) => (
                        account.treasury_after_milli,
                        if account.treasury_after_milli.is_none() {
                            ["unknown_treasury_after_tax".into()].into()
                        } else {
                            BTreeSet::new()
                        },
                    ),
                    None => (
                        world
                            .country_resources()
                            .get(&country)
                            .and_then(|state| state.treasury_milli),
                        BTreeSet::new(),
                    ),
                }
            } else {
                (
                    world
                        .country_resources()
                        .get(&country)
                        .and_then(|state| state.treasury_milli),
                    BTreeSet::new(),
                )
            };
            unresolved.extend(input_unresolved);
            if treasury_before.is_none() {
                unresolved.insert("unknown_treasury".into());
            }

            let (treasury_after, credited) = if unresolved.is_empty() {
                let before = treasury_before.expect("known treasury");
                let income = monthly.expect("known monthly production");
                let after = crate::resources::treasury_after_adjustment(before, income)?;
                (Some(after), Some(income))
            } else {
                (None, None)
            };
            accounts.insert(
                country,
                ProductionCountryAccount {
                    known_annual_value_milli: known_annual,
                    known_monthly_value_milli: known_monthly,
                    annual_value_milli: annual,
                    monthly_value_milli: monthly,
                    treasury_before_milli: treasury_before,
                    treasury_after_milli: treasury_after,
                    credited_milli: credited,
                    provinces,
                    unresolved,
                },
            );
        }

        Ok(ProductionSettlement {
            day: world.day(),
            accounts,
            unresolved_provinces,
        })
    }

    pub(crate) fn commit(&mut self, world: &mut World, settlement: ProductionSettlement) {
        for (&country, account) in &settlement.accounts {
            if let Some(after) = account.treasury_after_milli {
                world
                    .country_resources
                    .entry(country)
                    .or_default()
                    .treasury_milli = Some(after);
            }
        }
        self.settlement_count += 1;
        self.last_settlement = Some(settlement);
    }

    pub(crate) fn validate(&self, world: &World) -> Result<(), Error> {
        let calendar = world
            .calendar()
            .ok_or_else(|| Error::Invalid("monthly production requires a calendar".into()))?;
        if self.settlement_count != calendar.elapsed_months(world.day())? {
            return Err(Error::Invalid("invalid production settlement count".into()));
        }
        if world
            .monthly_stocks
            .as_ref()
            .is_some_and(|stocks| stocks.conflicts_with_production())
        {
            return Err(Error::Invalid("duplicate monthly treasury executor".into()));
        }
        for (&province, state) in &self.inputs {
            if world.entity(province)?.kind != EntityKind::Province
                || (state.input.is_none() && state.unresolved.is_empty())
                || state.unresolved.iter().any(String::is_empty)
            {
                return Err(Error::Invalid("invalid monthly production input".into()));
            }
        }
        if self.price_inputs.iter().any(|(province, _)| {
            self.inputs
                .get(province)
                .is_none_or(|state| state.input.is_none() || !state.unresolved.is_empty())
        }) {
            return Err(Error::Invalid(
                "invalid monthly production price input".into(),
            ));
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
                    return Err(Error::Invalid("invalid saved production settlement".into()));
                }
                for reasons in last.unresolved_provinces.values() {
                    if reasons.iter().any(String::is_empty) {
                        return Err(Error::Invalid("invalid production unknown reason".into()));
                    }
                }
                for account in last.accounts.values() {
                    let mut annual = 0_i64;
                    let mut monthly = 0_i64;
                    let mut unresolved = BTreeSet::new();
                    for province in account.provinces.values() {
                        if let Some(value) = province.annual_value_milli {
                            annual = annual.checked_add(value).ok_or(Error::Overflow)?;
                        }
                        if let Some(value) = province.monthly_value_milli {
                            monthly = monthly.checked_add(value).ok_or(Error::Overflow)?;
                        }
                        unresolved.extend(
                            province
                                .unresolved
                                .iter()
                                .map(|reason| format!("province_input:{reason}")),
                        );
                    }
                    if annual != account.known_annual_value_milli
                        || monthly != account.known_monthly_value_milli
                        || account.annual_value_milli != unresolved.is_empty().then_some(annual)
                        || account.monthly_value_milli != unresolved.is_empty().then_some(monthly)
                        || account.unresolved.is_empty() && !unresolved.is_empty()
                    {
                        return Err(Error::Invalid("invalid saved production account".into()));
                    }
                    let expected_after = account
                        .treasury_before_milli
                        .zip(account.credited_milli)
                        .map(|(before, credited)| {
                            crate::resources::treasury_after_adjustment(before, credited)
                        })
                        .transpose()?;
                    if account.treasury_after_milli != expected_after
                        || account.credited_milli
                            != account
                                .monthly_value_milli
                                .filter(|_| account.unresolved.is_empty())
                    {
                        return Err(Error::Invalid("invalid saved production treasury".into()));
                    }
                }
                Ok(())
            }
            _ => Err(Error::Invalid("invalid production settlement count".into())),
        }
    }
}

impl World {
    /// Prepares the next monthly production settlement without committing
    /// prices, production inputs, treasury values, receipts, or settlement
    /// counters. The staged calculation follows `MonthlyEconomy::settle`:
    /// refresh prices and production value inputs, apply monthly autonomy,
    /// prepare taxation, then evaluate production.
    pub fn preview_monthly_production(&self) -> Result<ProductionSettlement, Error> {
        if self.monthly_production.is_none() {
            return Err(Error::Invalid("missing monthly production".into()));
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
        refreshed_production
            .as_ref()
            .expect("monthly production was checked before staging")
            .prepare(&staged, taxes.as_ref())
    }

    pub fn monthly_production(&self) -> Option<&MonthlyProduction> {
        self.monthly_production.as_ref()
    }

    pub fn set_monthly_production(&mut self, production: MonthlyProduction) -> Result<(), Error> {
        self.require_running()?;
        if self.day() != 0 {
            return Err(Error::Invalid(
                "production must be selected before advancing".into(),
            ));
        }
        let custom_system = self.monthly_system_is_custom();
        if self
            .monthly_stocks
            .as_ref()
            .is_some_and(|stocks| stocks.conflicts_with_production())
        {
            return Err(Error::Invalid("duplicate monthly treasury executor".into()));
        }
        production.validate(self)?;
        self.monthly_production = Some(production);
        if !custom_system {
            self.monthly_system_id = Some(crate::MonthlyEconomy.id().into());
        }
        Ok(())
    }

    /// Replace the provider-supplied inputs after the monthly executor has
    /// already been installed. Settlement receipts and the executor identity
    /// remain unchanged; the new inputs are used by the next settlement.
    pub fn set_monthly_production_inputs(
        &mut self,
        inputs: BTreeMap<EntityId, VanillaProductionInputState>,
    ) -> Result<(), Error> {
        self.require_running()?;
        if self
            .monthly_system_id()
            .is_some_and(|id| id != crate::MonthlyEconomy.id() && id != crate::MonthlyTax.id())
        {
            return Err(Error::Invalid(
                "production inputs cannot replace another monthly executor".into(),
            ));
        }
        let Some(current) = self.monthly_production.as_ref() else {
            return Err(Error::Invalid("monthly production is not selected".into()));
        };
        let mut replacement = current.clone();
        replacement.inputs = inputs;
        replacement.validate(self)?;
        self.monthly_production = Some(replacement);
        Ok(())
    }
}
