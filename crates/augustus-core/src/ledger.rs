// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::resources::treasury_after_adjustment;
use crate::{
    CashflowSettlement, EntityId, EntityKind, Error, ProductionSettlement, TaxSettlement, World,
};

/// Native country-ledger income categories occupy slots 0 through 18.
pub const COUNTRY_LEDGER_CATEGORY_COUNT: u32 = 19;
/// Native country-ledger expense categories occupy slots 0 through 37.
pub const COUNTRY_LEDGER_EXPENSE_CATEGORY_COUNT: u32 = 38;
/// State-maintenance expense slot in the native expense vector.
pub const COUNTRY_LEDGER_EXPENSE_STATE_MAINTENANCE_CATEGORY: u32 = 2;
/// Fort-maintenance expense slot in the native expense vector.
pub const COUNTRY_LEDGER_EXPENSE_FORT_MAINTENANCE_CATEGORY: u32 = 8;
/// Monthly tax receipt slot in the native income vector.
pub const COUNTRY_LEDGER_INCOME_TAX_CATEGORY: u32 = 0;
/// Ordinary production receipt slot in the native income vector.
pub const COUNTRY_LEDGER_INCOME_PRODUCTION_CATEGORY: u32 = 1;
/// Trade receipt slot in the native income vector.
pub const COUNTRY_LEDGER_INCOME_TRADE_CATEGORY: u32 = 2;
/// Gold receipt slot in the native income vector.
pub const COUNTRY_LEDGER_INCOME_GOLD_CATEGORY: u32 = 3;
/// Income vector indices excluded from the native filtered monthly total.
pub const COUNTRY_LEDGER_NATIVE_FILTERED_INCOME_CATEGORIES: [u32; 5] = [9, 10, 11, 13, 14];
/// Native internal keys for the 19 income slots. These are content/runtime
/// identifiers, not player-facing localization strings.
pub const COUNTRY_LEDGER_INCOME_KEYS: [&str; COUNTRY_LEDGER_CATEGORY_COUNT as usize] = [
    "INCOMETAX",
    "INCOMEPROD",
    "INCOMETRADE",
    "INCOMEGOLD",
    "INCOMETARRIFFS",
    "INCOMEVASSAL",
    "INCOMEHARBORFEES",
    "INCOMESUBS",
    "INCOME_WAR_REPARATION",
    "INCOMELOAN",
    "INCOMEGIFT",
    "INCOMEEVENT",
    "SPOILS_OF_WAR_ECOVIEW",
    "INCOME_TREASURE_FLEET",
    "SIPHON_INCOME",
    "CONDOTTIERI",
    "KNOWLEDGE_SHARING",
    "INC_BLOCKADES",
    "INC_LOOTING",
];
/// Native internal keys for the 38 expense slots. These are content/runtime
/// identifiers, not player-facing localization strings.
pub const COUNTRY_LEDGER_EXPENSE_KEYS: [&str; COUNTRY_LEDGER_EXPENSE_CATEGORY_COUNT as usize] = [
    "EXPENSECOURTMAINTENANCE",
    "EXPENSEINTEREST",
    "EXPENSE_STATE_MAINTENANCE",
    "EXPENSEHARBORFEES",
    "EXPENSESUBS",
    "EXPENSE_WAR_REPARATIONS",
    "EXPENSEAM",
    "EXPENSEFM",
    "EXPENSEFORTM",
    "COLONIAL_MAINTENANCE",
    "EXPENSEMISSIONARY",
    "EXPENSEARMY",
    "EXPENSEFLEET",
    "EXPENSEFORT",
    "EXPENSEMAN",
    "EXPENSESUPPORTREBELS",
    "EXPENSELOAN",
    "EXPENSEGIFT",
    "EXPENSECOURT",
    "EXPENSEEVENTS",
    "EXPENSEPEACE",
    "EXPENSEVASSAL",
    "EXPENSETARIFFS",
    "SUPPORT_LOYALISTS",
    "SEND_OFFICERS",
    "EXPENSE_SIPHON_INCOME",
    "CONDOTTIERI",
    "EXPENSE_CORRUPTION",
    "EXPENSE_TECHNOLOGY",
    "EXPENSE_GREAT_POWER",
    "KNOWLEDGE_SHARING",
    "TRADE_COMPANY_INVESTMENTS",
    "NONE",
    "EXP_BLOCKADES",
    "EXP_LOOTING",
    "EXPENSEGREATPROJECTS",
    "CENTRE_OF_TRADE_UPGRADE",
    "COLONY_CHANGES",
];
pub const COUNTRY_LEDGER_SUPPORT_LOYALISTS_CATEGORY: u32 = 23;
pub const COUNTRY_LEDGER_SEND_OFFICERS_CATEGORY: u32 = 24;
/// Expense vector indices excluded from the native filtered monthly total.
/// The set is recovered from the native consolidation switch after translating
/// its control value (`vector_index - 11`) back to the vector index.
pub const COUNTRY_LEDGER_NATIVE_FILTERED_EXPENSE_CATEGORIES: [u32; 13] =
    [11, 12, 13, 14, 16, 17, 18, 19, 20, 28, 32, 35, 36];

pub fn country_ledger_income_key(category: u32) -> Option<&'static str> {
    usize::try_from(category)
        .ok()
        .and_then(|index| COUNTRY_LEDGER_INCOME_KEYS.get(index).copied())
}

pub fn country_ledger_expense_key(category: u32) -> Option<&'static str> {
    usize::try_from(category)
        .ok()
        .and_then(|index| COUNTRY_LEDGER_EXPENSE_KEYS.get(index).copied())
}

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
    /// Income source vector consumed by monthly consolidation.
    #[serde(default)]
    pub income_sources_milli: Vec<i64>,
    /// Expense source vector consumed by monthly consolidation.
    #[serde(default)]
    pub expense_sources_milli: Vec<i64>,
    /// Income vector used by annual rollover.
    #[serde(default)]
    pub income_accumulator_milli: Vec<i64>,
    /// Expense vector used by annual rollover.
    #[serde(default)]
    pub expense_accumulator_milli: Vec<i64>,
    /// Auxiliary expense vector persisted by the native container.
    #[serde(default)]
    pub expense_auxiliary_milli: Vec<i64>,
    /// Current monthly snapshots produced by consolidation.
    #[serde(default)]
    pub income_snapshot_milli: Vec<i64>,
    #[serde(default)]
    pub expense_snapshot_milli: Vec<i64>,
    /// Annual rollover snapshots and totals.
    #[serde(default)]
    pub income_rollover_milli: Vec<i64>,
    #[serde(default)]
    pub expense_rollover_milli: Vec<i64>,
    #[serde(default)]
    pub income_total_milli: i64,
    #[serde(default)]
    pub expense_total_milli: i64,
    #[serde(default)]
    pub income_filtered_total_milli: i64,
    #[serde(default)]
    pub expense_filtered_total_milli: i64,
    #[serde(default)]
    pub income_rollover_total_milli: i64,
    #[serde(default)]
    pub expense_rollover_total_milli: i64,
}

fn default_pending() -> i64 {
    0
}

impl CountryLedger {
    fn ensure_layout(&mut self) {
        if self.income_sources_milli.is_empty() {
            self.income_sources_milli = vec![0; COUNTRY_LEDGER_CATEGORY_COUNT as usize];
        }
        if self.expense_sources_milli.is_empty() {
            self.expense_sources_milli = vec![0; COUNTRY_LEDGER_EXPENSE_CATEGORY_COUNT as usize];
        }
        if self.income_accumulator_milli.is_empty() {
            self.income_accumulator_milli = vec![0; COUNTRY_LEDGER_CATEGORY_COUNT as usize];
        }
        if self.expense_accumulator_milli.is_empty() {
            self.expense_accumulator_milli =
                vec![0; COUNTRY_LEDGER_EXPENSE_CATEGORY_COUNT as usize];
        }
        if self.expense_auxiliary_milli.is_empty() {
            self.expense_auxiliary_milli = vec![0; COUNTRY_LEDGER_EXPENSE_CATEGORY_COUNT as usize];
        }
        if self.income_snapshot_milli.is_empty() {
            self.income_snapshot_milli = vec![0; COUNTRY_LEDGER_CATEGORY_COUNT as usize];
        }
        if self.expense_snapshot_milli.is_empty() {
            self.expense_snapshot_milli = vec![0; COUNTRY_LEDGER_EXPENSE_CATEGORY_COUNT as usize];
        }
        if self.income_rollover_milli.is_empty() {
            self.income_rollover_milli = vec![0; COUNTRY_LEDGER_CATEGORY_COUNT as usize];
        }
        if self.expense_rollover_milli.is_empty() {
            self.expense_rollover_milli = vec![0; COUNTRY_LEDGER_EXPENSE_CATEGORY_COUNT as usize];
        }
    }

    fn validate_vector(vector: &[i64], expected: usize, name: &str) -> Result<(), Error> {
        if !vector.is_empty() && vector.len() != expected {
            return Err(Error::Invalid(format!(
                "country ledger {name} has invalid length"
            )));
        }
        if vector.iter().any(|value| *value < 0) {
            return Err(Error::Invalid(format!(
                "country ledger {name} has a negative value"
            )));
        }
        Ok(())
    }

    pub fn validate(&self) -> Result<(), Error> {
        for (&category, entry) in &self.categories {
            if category >= COUNTRY_LEDGER_EXPENSE_CATEGORY_COUNT {
                return Err(Error::Invalid(
                    "country ledger category is outside 0..38".into(),
                ));
            }
            if (category >= COUNTRY_LEDGER_CATEGORY_COUNT && entry.income_milli != 0)
                || entry.income_milli < 0
                || entry.expense_milli < 0
            {
                return Err(Error::Invalid(
                    "country ledger totals must be nonnegative".into(),
                ));
            }
        }
        Self::validate_vector(
            &self.income_sources_milli,
            COUNTRY_LEDGER_CATEGORY_COUNT as usize,
            "income source vector",
        )?;
        Self::validate_vector(
            &self.expense_sources_milli,
            COUNTRY_LEDGER_EXPENSE_CATEGORY_COUNT as usize,
            "expense source vector",
        )?;
        Self::validate_vector(
            &self.income_accumulator_milli,
            COUNTRY_LEDGER_CATEGORY_COUNT as usize,
            "income accumulator",
        )?;
        Self::validate_vector(
            &self.expense_accumulator_milli,
            COUNTRY_LEDGER_EXPENSE_CATEGORY_COUNT as usize,
            "expense accumulator",
        )?;
        Self::validate_vector(
            &self.expense_auxiliary_milli,
            COUNTRY_LEDGER_EXPENSE_CATEGORY_COUNT as usize,
            "expense auxiliary vector",
        )?;
        Self::validate_vector(
            &self.income_snapshot_milli,
            COUNTRY_LEDGER_CATEGORY_COUNT as usize,
            "income snapshot",
        )?;
        Self::validate_vector(
            &self.expense_snapshot_milli,
            COUNTRY_LEDGER_EXPENSE_CATEGORY_COUNT as usize,
            "expense snapshot",
        )?;
        Self::validate_vector(
            &self.income_rollover_milli,
            COUNTRY_LEDGER_CATEGORY_COUNT as usize,
            "income rollover",
        )?;
        Self::validate_vector(
            &self.expense_rollover_milli,
            COUNTRY_LEDGER_EXPENSE_CATEGORY_COUNT as usize,
            "expense rollover",
        )?;
        if self.income_total_milli < 0
            || self.expense_total_milli < 0
            || self.income_filtered_total_milli < 0
            || self.expense_filtered_total_milli < 0
            || self.income_rollover_total_milli < 0
            || self.expense_rollover_total_milli < 0
        {
            return Err(Error::Invalid(
                "country ledger totals must be nonnegative".into(),
            ));
        }
        Ok(())
    }

    fn category_mut(&mut self, category: u32) -> Result<&mut CountryLedgerCategory, Error> {
        if category >= COUNTRY_LEDGER_EXPENSE_CATEGORY_COUNT {
            return Err(Error::Invalid(
                "country ledger category is outside 0..38".into(),
            ));
        }
        Ok(self.categories.entry(category).or_default())
    }

    fn add_income(&mut self, category: u32, amount: i64) -> Result<(), Error> {
        if category >= COUNTRY_LEDGER_CATEGORY_COUNT {
            return Err(Error::Invalid(
                "country ledger income category is outside 0..19".into(),
            ));
        }
        if amount < 0 {
            return Err(Error::Invalid(
                "country ledger income must be nonnegative".into(),
            ));
        }
        self.ensure_layout();
        let index = category as usize;
        self.income_sources_milli[index] = self.income_sources_milli[index]
            .checked_add(amount)
            .ok_or(Error::Overflow)?;
        self.income_accumulator_milli[index] = self.income_accumulator_milli[index]
            .checked_add(amount)
            .ok_or(Error::Overflow)?;
        let entry = self.category_mut(category)?;
        entry.income_milli = entry
            .income_milli
            .checked_add(amount)
            .ok_or(Error::Overflow)?;
        Ok(())
    }

    fn add_expense(&mut self, category: u32, amount: i64) -> Result<(), Error> {
        if category >= COUNTRY_LEDGER_EXPENSE_CATEGORY_COUNT {
            return Err(Error::Invalid(
                "country ledger expense category is outside 0..38".into(),
            ));
        }
        if amount < 0 {
            return Err(Error::Invalid(
                "country ledger expense must be nonnegative".into(),
            ));
        }
        self.ensure_layout();
        let index = category as usize;
        self.expense_sources_milli[index] = self.expense_sources_milli[index]
            .checked_add(amount)
            .ok_or(Error::Overflow)?;
        self.expense_accumulator_milli[index] = self.expense_accumulator_milli[index]
            .checked_add(amount)
            .ok_or(Error::Overflow)?;
        self.expense_auxiliary_milli[index] = self.expense_auxiliary_milli[index]
            .checked_add(amount)
            .ok_or(Error::Overflow)?;
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
        self.income_sources_milli.clear();
        self.expense_sources_milli.clear();
        self.income_accumulator_milli.clear();
        self.expense_accumulator_milli.clear();
        self.expense_auxiliary_milli.clear();
        self.income_snapshot_milli.clear();
        self.expense_snapshot_milli.clear();
        self.income_rollover_milli.clear();
        self.expense_rollover_milli.clear();
        self.income_total_milli = 0;
        self.expense_total_milli = 0;
        self.income_filtered_total_milli = 0;
        self.expense_filtered_total_milli = 0;
        self.income_rollover_total_milli = 0;
        self.expense_rollover_total_milli = 0;
    }

    /// Consolidates source vectors into the current monthly snapshots and
    /// clears the consumed sources. The native income filter is fixed; the
    /// compatibility layer may supply an expense filter for its source.
    pub fn consolidate_monthly(
        &mut self,
        expense_filtered_categories: &std::collections::BTreeSet<u32>,
    ) -> Result<(), Error> {
        self.ensure_layout();
        self.income_snapshot_milli
            .clone_from(&self.income_sources_milli);
        self.expense_snapshot_milli
            .clone_from(&self.expense_sources_milli);
        self.income_total_milli = self
            .income_snapshot_milli
            .iter()
            .try_fold(0_i64, |total, value| {
                total.checked_add(*value).ok_or(Error::Overflow)
            })?;
        self.expense_total_milli = self
            .expense_snapshot_milli
            .iter()
            .try_fold(0_i64, |total, value| {
                total.checked_add(*value).ok_or(Error::Overflow)
            })?;
        self.income_filtered_total_milli = self
            .income_snapshot_milli
            .iter()
            .enumerate()
            .filter(|(index, _)| {
                !COUNTRY_LEDGER_NATIVE_FILTERED_INCOME_CATEGORIES.contains(&(*index as u32))
            })
            .try_fold(0_i64, |total, (_, value)| {
                total.checked_add(*value).ok_or(Error::Overflow)
            })?;
        self.expense_filtered_total_milli = self
            .expense_snapshot_milli
            .iter()
            .enumerate()
            .filter(|(index, _)| !expense_filtered_categories.contains(&(*index as u32)))
            .try_fold(0_i64, |total, (_, value)| {
                total.checked_add(*value).ok_or(Error::Overflow)
            })?;
        self.income_sources_milli.fill(0);
        self.expense_sources_milli.fill(0);
        Ok(())
    }

    /// Copies accumulator vectors into rollover snapshots and clears the
    /// consumed monthly/annual source vectors.
    pub fn rollover_annual(&mut self) -> Result<(), Error> {
        self.ensure_layout();
        self.income_rollover_milli
            .clone_from(&self.income_accumulator_milli);
        self.expense_rollover_milli
            .clone_from(&self.expense_accumulator_milli);
        self.income_rollover_total_milli = self
            .income_rollover_milli
            .iter()
            .try_fold(0_i64, |total, value| {
                total.checked_add(*value).ok_or(Error::Overflow)
            })?;
        self.expense_rollover_total_milli = self
            .expense_rollover_milli
            .iter()
            .try_fold(0_i64, |total, value| {
                total.checked_add(*value).ok_or(Error::Overflow)
            })?;
        self.income_accumulator_milli.fill(0);
        self.expense_accumulator_milli.fill(0);
        self.income_sources_milli.fill(0);
        self.expense_sources_milli.fill(0);
        Ok(())
    }
}

/// Rolls every loaded country ledger into its annual snapshot vectors.
///
/// The caller places this provider in the selected yearly pipeline where the
/// translated ruleset requires it; this type does not infer a calendar order.
pub struct CountryLedgerAnnualRollover;

impl crate::YearlySystem for CountryLedgerAnnualRollover {
    fn id(&self) -> &str {
        "country-ledger-annual-rollover:1"
    }

    fn validate(&self, _world: &World) -> Result<(), Error> {
        Ok(())
    }

    fn settle(&self, world: &mut World) -> Result<(), Error> {
        world.rollover_country_ledgers_annual()
    }
}

/// Consolidates the loaded country ledgers into their monthly snapshots.
///
/// Keep this as a separate provider so a pipeline can place it after its
/// monthly cash-flow producers. An empty expense-filter map uses the native
/// category set; the compatibility layer may override it per country. A custom
/// filter mapping must use an ID that changes when that mapping changes.
#[derive(Debug, Clone)]
pub struct CountryLedgerMonthlyConsolidation {
    id: String,
    expense_filtered_categories: BTreeMap<EntityId, BTreeSet<u32>>,
}

impl CountryLedgerMonthlyConsolidation {
    pub fn new(
        id: impl Into<String>,
        expense_filtered_categories: BTreeMap<EntityId, BTreeSet<u32>>,
    ) -> Result<Self, Error> {
        let system = Self {
            id: id.into(),
            expense_filtered_categories,
        };
        if system.id.trim().is_empty() {
            return Err(Error::Invalid(
                "monthly ledger consolidation requires an id".into(),
            ));
        }
        Ok(system)
    }
}

impl Default for CountryLedgerMonthlyConsolidation {
    fn default() -> Self {
        Self {
            id: "country-ledger-monthly-consolidation:1".into(),
            expense_filtered_categories: BTreeMap::new(),
        }
    }
}

impl crate::MonthlySystem for CountryLedgerMonthlyConsolidation {
    fn id(&self) -> &str {
        &self.id
    }

    fn validate(&self, world: &World) -> Result<(), Error> {
        world.validate_country_ledger_expense_filters(&self.expense_filtered_categories)?;
        for (&country, ledger) in world.country_ledgers() {
            world.require_country(country)?;
            ledger.validate()?;
        }
        Ok(())
    }

    fn settle(&self, world: &mut World) -> Result<(), Error> {
        world.consolidate_country_ledgers_monthly(&self.expense_filtered_categories)
    }
}

/// Applies pending country-ledger balances at an explicit monthly stage.
///
/// Pipelines place this provider after ordinary monthly producers have
/// recorded delayed flows and before later monthly producers and consolidation.
pub struct CountryLedgerPendingFlush;

impl crate::MonthlySystem for CountryLedgerPendingFlush {
    fn id(&self) -> &str {
        "country-ledger-pending-flush:1"
    }

    fn validate(&self, world: &World) -> Result<(), Error> {
        for (&country, ledger) in world.country_ledgers() {
            world.require_country(country)?;
            ledger.validate()?;
            if world
                .country_resources()
                .get(&country)
                .and_then(|resources| resources.treasury_milli)
                .is_none()
            {
                return Err(Error::Invalid("country treasury is unknown".into()));
            }
        }
        Ok(())
    }

    fn settle(&self, world: &mut World) -> Result<(), Error> {
        world.flush_country_ledgers_pending().map(|_| ())
    }
}

pub(crate) fn prepare_monthly_ledger_updates(
    world: &World,
    taxes: Option<&TaxSettlement>,
    production: Option<&ProductionSettlement>,
    cashflows: Option<&CashflowSettlement>,
) -> Result<BTreeMap<EntityId, CountryLedger>, Error> {
    fn append_income(
        world: &World,
        updates: &mut BTreeMap<EntityId, CountryLedger>,
        country: EntityId,
        category: u32,
        amount: Option<i64>,
    ) -> Result<(), Error> {
        let Some(amount) = amount.filter(|amount| *amount >= 0) else {
            // Signed losses need a separately verified expense category.
            return Ok(());
        };
        if world.entity(country)?.kind != EntityKind::Country {
            return Err(Error::Invalid(
                "monthly ledger income requires a country".into(),
            ));
        }
        let ledger = updates.entry(country).or_insert_with(|| {
            world
                .country_ledgers
                .get(&country)
                .cloned()
                .unwrap_or_default()
        });
        ledger.add_income(category, amount)
    }

    fn append_expense(
        world: &World,
        updates: &mut BTreeMap<EntityId, CountryLedger>,
        country: EntityId,
        category: u32,
        amount: i64,
    ) -> Result<(), Error> {
        if world.entity(country)?.kind != EntityKind::Country {
            return Err(Error::Invalid(
                "monthly ledger expense requires a country".into(),
            ));
        }
        let ledger = updates.entry(country).or_insert_with(|| {
            world
                .country_ledgers
                .get(&country)
                .cloned()
                .unwrap_or_default()
        });
        ledger.add_expense(category, amount)
    }

    let mut updates = BTreeMap::new();
    if let Some(taxes) = taxes {
        for (&country, account) in &taxes.accounts {
            append_income(
                world,
                &mut updates,
                country,
                COUNTRY_LEDGER_INCOME_TAX_CATEGORY,
                account.credited_milli,
            )?;
        }
    }
    if let Some(production) = production {
        for (&country, account) in &production.accounts {
            append_income(
                world,
                &mut updates,
                country,
                COUNTRY_LEDGER_INCOME_PRODUCTION_CATEGORY,
                account.credited_milli,
            )?;
        }
    }
    if let Some(cashflows) = cashflows {
        for (&country, account) in &cashflows.accounts {
            let Some(breakdown) = &account.breakdown else {
                continue;
            };
            append_income(
                world,
                &mut updates,
                country,
                COUNTRY_LEDGER_INCOME_TRADE_CATEGORY,
                breakdown.resolved_trade_milli()?,
            )?;
            append_income(
                world,
                &mut updates,
                country,
                COUNTRY_LEDGER_INCOME_GOLD_CATEGORY,
                breakdown.gold_milli,
            )?;
            for (&category, &amount) in &breakdown.expense_categories_milli {
                append_expense(world, &mut updates, country, category, amount)?;
            }
        }
    }
    Ok(updates)
}

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

    /// Consolidates every loaded country ledger at the monthly boundary.
    ///
    /// The compatibility layer supplies the expense categories that are
    /// excluded from the player-facing filtered total. The operation is
    /// staged on a clone so an overflow or invalid filter cannot leave only
    /// part of the country set consolidated.
    pub fn consolidate_country_ledgers_monthly(
        &mut self,
        expense_filtered_categories: &BTreeMap<EntityId, BTreeSet<u32>>,
    ) -> Result<(), Error> {
        self.require_running()?;
        self.validate_country_ledger_expense_filters(expense_filtered_categories)?;

        let mut ledgers = self.country_ledgers.clone();
        for (&country, ledger) in &mut ledgers {
            let categories = expense_filtered_categories
                .get(&country)
                .cloned()
                .unwrap_or_else(|| {
                    COUNTRY_LEDGER_NATIVE_FILTERED_EXPENSE_CATEGORIES
                        .into_iter()
                        .collect()
                });
            ledger.consolidate_monthly(&categories)?;
        }
        self.country_ledgers = ledgers;
        Ok(())
    }

    fn validate_country_ledger_expense_filters(
        &self,
        expense_filtered_categories: &BTreeMap<EntityId, BTreeSet<u32>>,
    ) -> Result<(), Error> {
        for (&country, categories) in expense_filtered_categories {
            self.require_country(country)?;
            if !self.country_ledgers.contains_key(&country) {
                return Err(Error::Invalid("country ledger is unknown".into()));
            }
            if categories
                .iter()
                .any(|category| *category >= COUNTRY_LEDGER_EXPENSE_CATEGORY_COUNT)
            {
                return Err(Error::Invalid(
                    "country ledger expense filter category is outside 0..38".into(),
                ));
            }
        }
        Ok(())
    }

    /// Performs the annual country-ledger rollover for every loaded country.
    /// The rollover is staged so every country observes the same boundary.
    pub fn rollover_country_ledgers_annual(&mut self) -> Result<(), Error> {
        self.require_running()?;
        let mut ledgers = self.country_ledgers.clone();
        for ledger in ledgers.values_mut() {
            ledger.rollover_annual()?;
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
        let next = treasury_after_adjustment(treasury, pending)?;
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

    /// Flushes every country's delayed balance at the shared monthly boundary.
    /// The balances and pending fields are staged together so one missing
    /// treasury cannot leave only part of the world advanced.
    pub fn flush_country_ledgers_pending(&mut self) -> Result<BTreeMap<EntityId, i64>, Error> {
        self.require_running()?;
        let ledgers = self.country_ledgers.clone();
        for &country in ledgers.keys() {
            self.require_country(country)?;
        }

        let mut resources = self.country_resources.clone();
        let mut updated_ledgers = ledgers;
        let mut balances = BTreeMap::new();
        for (&country, ledger) in &mut updated_ledgers {
            let treasury = resources
                .get(&country)
                .and_then(|resources| resources.treasury_milli)
                .ok_or_else(|| Error::Invalid("country treasury is unknown".into()))?;
            let next = treasury_after_adjustment(treasury, ledger.pending_milli)?;
            resources.entry(country).or_default().treasury_milli = Some(next);
            ledger.pending_milli = 0;
            balances.insert(country, next);
        }
        self.country_resources = resources;
        self.country_ledgers = updated_ledgers;
        Ok(balances)
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
            let next = treasury_after_adjustment(treasury, delta)?;
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
