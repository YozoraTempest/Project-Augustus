// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use crate::{Condition, CountryResource, EntityId, Error, MonthlySystem, World};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StockRule {
    pub income_attribute: String,
    pub divisor: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ceiling_attribute: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition: Option<Condition>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StockAccount {
    pub applies: Option<bool>,
    pub income: Option<i64>,
    pub growth_ceiling: Option<i64>,
    pub quantity_before: Option<i64>,
    pub quantity_after: Option<i64>,
    pub credited: Option<i64>,
    pub unresolved: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StockSettlement {
    pub day: u64,
    pub accounts: BTreeMap<EntityId, BTreeMap<CountryResource, StockAccount>>,
}

/// Content-selected monthly flows into the existing country resource stocks.
/// A growth ceiling preserves excess; event operations remain independent.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MonthlyStocks {
    pub rules: BTreeMap<EntityId, BTreeMap<CountryResource, StockRule>>,
    pub settlement_count: u64,
    pub last_settlement: Option<StockSettlement>,
}

impl StockRule {
    fn after(
        &self,
        resource: CountryResource,
        account: &StockAccount,
    ) -> Result<Option<i64>, Error> {
        match account.applies {
            Some(false) => return Ok(account.quantity_before),
            None => return Ok(None),
            Some(true) => (),
        }
        let Some((before, income)) = account.quantity_before.zip(account.income) else {
            return Ok(None);
        };
        let raw = resource.bounded(i128::from(before) + i128::from(income));
        let after = if raw > i128::from(before) && self.ceiling_attribute.is_some() {
            let Some(ceiling) = account.growth_ceiling else {
                return Ok(None);
            };
            raw.min(i128::from(before.max(ceiling)))
        } else {
            raw
        };
        i64::try_from(after).map(Some).map_err(|_| Error::Overflow)
    }
}

impl MonthlyStocks {
    pub(crate) fn conflicts_with_taxation(&self, taxation: &crate::Taxation) -> bool {
        self.rules.iter().any(|(entity, rules)| {
            taxation.countries.contains(entity) && rules.contains_key(&CountryResource::Treasury)
        })
    }

    pub(crate) fn validate_attributes(&self, attributes: &crate::Attributes) -> Result<(), Error> {
        for (entity, rules) in &self.rules {
            for rule in rules.values() {
                for name in
                    std::iter::once(&rule.income_attribute).chain(rule.ceiling_attribute.iter())
                {
                    if name.is_empty()
                        || !attributes
                            .formulas
                            .get(entity)
                            .is_some_and(|formulas| formulas.contains_key(name))
                    {
                        return Err(Error::Invalid("missing monthly stock calculation".into()));
                    }
                }
            }
        }
        Ok(())
    }

    pub(crate) fn prepare(&self, world: &World) -> Result<StockSettlement, Error> {
        self.settlement_count
            .checked_add(1)
            .ok_or(Error::Overflow)?;
        let mut accounts = BTreeMap::new();
        for (&entity, rules) in &self.rules {
            let mut rows = BTreeMap::new();
            for (&resource, rule) in rules {
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
                    .country_resources()
                    .get(&entity)
                    .and_then(|state| resource.get(state));
                let (income, ceiling) = if applies == Some(false) {
                    (Some(0), None)
                } else {
                    let evaluation =
                        world
                            .attributes()
                            .evaluate(world, entity, &rule.income_attribute)?;
                    unresolved.extend(evaluation.unresolved);
                    let income = evaluation.value_micros.map(|value| value / rule.divisor);
                    let ceiling = if let Some(name) = &rule.ceiling_attribute {
                        let evaluation = world.attributes().evaluate(world, entity, name)?;
                        unresolved.extend(evaluation.unresolved);
                        evaluation.value_micros.map(|value| value / rule.divisor)
                    } else {
                        None
                    };
                    if before.is_none() {
                        unresolved.insert("unknown_quantity".into());
                    }
                    if income.is_none() {
                        unresolved.insert("unknown_income".into());
                    }
                    if rule.ceiling_attribute.is_some() && ceiling.is_none() {
                        unresolved.insert("unknown_growth_ceiling".into());
                    }
                    (income, ceiling)
                };
                if ceiling
                    .is_some_and(|value| resource.bounded(i128::from(value)) != i128::from(value))
                {
                    return Err(Error::Invalid(
                        "monthly stock ceiling is outside resource bounds".into(),
                    ));
                }
                let mut account = StockAccount {
                    applies,
                    income,
                    growth_ceiling: ceiling,
                    quantity_before: before,
                    quantity_after: None,
                    credited: None,
                    unresolved,
                };
                account.quantity_after = rule.after(resource, &account)?;
                account.credited = before
                    .zip(account.quantity_after)
                    .map(|(before, after)| after.checked_sub(before).ok_or(Error::Overflow))
                    .transpose()?;
                rows.insert(resource, account);
            }
            accounts.insert(entity, rows);
        }
        Ok(StockSettlement {
            day: world.day(),
            accounts,
        })
    }

    pub(crate) fn commit(&mut self, world: &mut World, settlement: StockSettlement) {
        for (&entity, accounts) in &settlement.accounts {
            for (&resource, account) in accounts {
                resource.set_optional(
                    world.country_resources.entry(entity).or_default(),
                    account.quantity_after,
                );
            }
        }
        self.settlement_count += 1;
        self.last_settlement = Some(settlement);
    }

    pub(crate) fn validate(&self, world: &World) -> Result<(), Error> {
        let calendar = world
            .calendar()
            .ok_or_else(|| Error::Invalid("monthly stocks require a calendar".into()))?;
        if self.rules.is_empty()
            || self.settlement_count != calendar.elapsed_months(world.day())?
            || world
                .taxation()
                .is_some_and(|taxation| self.conflicts_with_taxation(taxation))
        {
            return Err(Error::Invalid(
                "invalid monthly stock rules, count or duplicate treasury executor".into(),
            ));
        }
        self.validate_attributes(world.attributes())?;
        for (&entity, rules) in &self.rules {
            world.resource_country(entity)?;
            if rules.is_empty() {
                return Err(Error::Invalid("empty monthly stock rules".into()));
            }
            for rule in rules.values() {
                if rule.divisor <= 0 {
                    return Err(Error::Invalid("invalid monthly stock divisor".into()));
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
                    return Err(Error::Invalid("invalid saved stock settlement".into()));
                }
                for (&entity, accounts) in &last.accounts {
                    if !accounts.keys().eq(self.rules[&entity].keys()) {
                        return Err(Error::Invalid("invalid saved stock accounts".into()));
                    }
                    for (&resource, account) in accounts {
                        let rule = &self.rules[&entity][&resource];
                        for value in [
                            account.quantity_before,
                            account.quantity_after,
                            account.growth_ceiling,
                        ]
                        .into_iter()
                        .flatten()
                        {
                            if resource.bounded(i128::from(value)) != i128::from(value) {
                                return Err(Error::Invalid(
                                    "saved stock outside resource bounds".into(),
                                ));
                            }
                        }
                        let after = rule.after(resource, account)?;
                        let credited = account
                            .quantity_before
                            .zip(after)
                            .map(|(before, after)| after.checked_sub(before).ok_or(Error::Overflow))
                            .transpose()?;
                        if after != account.quantity_after
                            || credited != account.credited
                            || account.unresolved.iter().any(String::is_empty)
                            || (rule.ceiling_attribute.is_none()
                                && account.growth_ceiling.is_some())
                            || (rule.condition.is_none() && account.applies != Some(true))
                            || (account.applies == Some(false)
                                && (account.income != Some(0) || account.growth_ceiling.is_some()))
                            || (account.applies.is_none()
                                && !account
                                    .unresolved
                                    .iter()
                                    .any(|reason| reason.starts_with("unknown_condition:")))
                            || (account.applies != Some(false)
                                && after.is_none()
                                && account.unresolved.is_empty())
                        {
                            return Err(Error::Invalid("invalid saved stock receipt".into()));
                        }
                    }
                }
                Ok(())
            }
            _ => Err(Error::Invalid("invalid saved stock count".into())),
        }
    }
}

impl World {
    pub fn monthly_stocks(&self) -> Option<&MonthlyStocks> {
        self.monthly_stocks.as_ref()
    }

    pub fn set_monthly_stocks(&mut self, state: Option<MonthlyStocks>) -> Result<(), Error> {
        self.require_running()?;
        if self.day() != 0
            || self
                .monthly_system_id()
                .is_some_and(|id| id != crate::MonthlyEconomy.id() && id != crate::MonthlyTax.id())
        {
            return Err(Error::Invalid(
                "monthly stocks must be selected before advancing".into(),
            ));
        }
        if let Some(state) = &state {
            state.validate(self)?;
        }
        self.monthly_stocks = state;
        self.monthly_system_id = if self.monthly_stocks.is_some()
            || self.monthly_accruals.is_some()
            || self.monthly_convergence.is_some()
        {
            Some(crate::MonthlyEconomy.id().into())
        } else {
            self.taxation
                .as_ref()
                .map(|_| crate::MonthlyTax.id().into())
        };
        Ok(())
    }
}
