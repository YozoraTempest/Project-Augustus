// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{
    COUNTRY_LEDGER_EXPENSE_CATEGORY_COUNT, CountryLedger, CountryResource, CountryResources,
    EntityId, EntityKind, Error, NumericOperation, NumericSlot, World,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct NavalConstructionId(pub u64);

#[derive(Debug, Clone, PartialEq, Eq)]
/// Construction timing supplied as an absolute core day.
/// Ordinary enqueue methods treat each input independently; explicit queue
/// imports can postpone later orders. This input does not quote time or cost,
/// or infer pause and cancellation rules.
pub struct NavalConstructionInput {
    pub owner: EntityId,
    pub location: EntityId,
    pub unit_type: String,
    pub completion_day: u64,
}

/// One independent construction request with an explicit treasury deduction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaidNavalConstructionInput {
    pub construction: NavalConstructionInput,
    pub cost_milli: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// A pending order and its current absolute completion day.
pub struct NavalConstructionOrder {
    pub id: NavalConstructionId,
    pub owner: EntityId,
    pub location: EntityId,
    pub unit_type: String,
    pub completion_day: u64,
}

/// A pending order with its current position among FIFO orders at its location.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NavalConstructionOrderView<'a> {
    pub order: &'a NavalConstructionOrder,
    /// One-based position for an explicitly imported FIFO order. Independent
    /// orders have no position, even when they share a FIFO location.
    pub queue_position: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NavalConstructions {
    next_id: u64,
    orders: Vec<NavalConstructionOrder>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    fifo_order_ids: BTreeSet<NavalConstructionId>,
}

impl NavalConstructions {
    pub(crate) fn is_initial_empty(&self) -> bool {
        self.next_id == 0 && self.orders.is_empty()
    }

    pub(crate) fn next_due_day(&self, current_day: u64) -> Option<u64> {
        self.orders
            .iter()
            .map(|order| order.completion_day)
            .chain(
                (!self.fifo_order_ids.is_empty())
                    .then(|| current_day.checked_add(1))
                    .flatten(),
            )
            .min()
    }

    pub(crate) fn enqueue_validated(
        &mut self,
        inputs: &[NavalConstructionInput],
    ) -> Result<Vec<NavalConstructionId>, Error> {
        let count = u64::try_from(inputs.len()).map_err(|_| Error::Overflow)?;
        let next_id = self.next_id.checked_add(count).ok_or(Error::Overflow)?;
        let ids = (self.next_id..next_id)
            .map(NavalConstructionId)
            .collect::<Vec<_>>();
        self.orders
            .extend(inputs.iter().zip(ids.iter().copied()).map(|(input, id)| {
                NavalConstructionOrder {
                    id,
                    owner: input.owner,
                    location: input.location,
                    unit_type: input.unit_type.clone(),
                    completion_day: input.completion_day,
                }
            }));
        self.next_id = next_id;
        Ok(ids)
    }

    fn take_due(
        &mut self,
        day: u64,
        advance_fifo: bool,
        calendar: Option<&crate::Calendar>,
    ) -> Result<Vec<NavalConstructionOrder>, Error> {
        let mut blocked = BTreeSet::new();
        let mut due = Vec::new();
        for order in &mut self.orders {
            if self.fifo_order_ids.contains(&order.id) {
                if blocked.contains(&order.location) {
                    if advance_fifo {
                        order.completion_day =
                            order.completion_day.checked_add(1).ok_or(Error::Overflow)?;
                        if let Some(calendar) = calendar {
                            calendar.date(order.completion_day)?;
                        }
                    }
                    continue;
                }
                if order.completion_day > day {
                    blocked.insert(order.location);
                    continue;
                }
            }
            if order.completion_day <= day {
                due.push(order.clone());
            }
        }
        let completed_ids = due.iter().map(|order| order.id).collect::<BTreeSet<_>>();
        self.orders
            .retain(|order| !completed_ids.contains(&order.id));
        self.fifo_order_ids.retain(|id| !completed_ids.contains(id));
        Ok(due)
    }

    pub(crate) fn validate_calendar(&self, calendar: &crate::Calendar) -> Result<(), Error> {
        for order in &self.orders {
            calendar.date(order.completion_day)?;
        }
        Ok(())
    }

    pub(crate) fn validate(&self, world: &World) -> Result<(), Error> {
        let mut ids = BTreeSet::new();
        for order in &self.orders {
            if order.id.0 >= self.next_id || !ids.insert(order.id) {
                return Err(Error::Invalid(
                    "invalid naval construction identifier".into(),
                ));
            }
            if order.completion_day <= world.day() {
                return Err(Error::Invalid(
                    "invalid saved naval construction day".into(),
                ));
            }
            if let Some(calendar) = world.calendar() {
                calendar.date(order.completion_day)?;
            }
            validate_construction_location(world, order.owner, order.location, &order.unit_type)?;
        }
        if !self.fifo_order_ids.is_subset(&ids) {
            return Err(Error::Invalid(
                "naval construction queue references an unknown order".into(),
            ));
        }
        Ok(())
    }
}

fn validate_construction_location(
    world: &World,
    owner: EntityId,
    location: EntityId,
    unit_type: &str,
) -> Result<(), Error> {
    if world.entity(owner)?.kind != EntityKind::Country {
        return Err(Error::Invalid(
            "naval construction owner must be a country".into(),
        ));
    }
    if world.entity(location)?.kind != EntityKind::Province {
        return Err(Error::Invalid(
            "naval construction location must be a province".into(),
        ));
    }
    if !world.province_has_port(location)? {
        return Err(Error::Invalid(
            "naval construction requires a port province".into(),
        ));
    }
    crate::naval::NavalUnits::validate_type(unit_type)
}

impl World {
    /// Read the pending orders using explicit absolute simulation days.
    pub fn naval_constructions(&self) -> &[NavalConstructionOrder] {
        &self.naval_constructions.orders
    }

    /// Read pending orders in stored order without advancing or forecasting them.
    ///
    /// Positions are calculated from the remaining FIFO orders at each location.
    /// A waiting order's current completion day can still be postponed by future
    /// simulation days; this view does not predict its eventual completion.
    pub fn naval_construction_views(&self) -> impl Iterator<Item = NavalConstructionOrderView<'_>> {
        self.naval_constructions.orders.iter().scan(
            BTreeMap::<EntityId, usize>::new(),
            move |positions, order| {
                let queue_position = self
                    .naval_constructions
                    .fifo_order_ids
                    .contains(&order.id)
                    .then(|| {
                        let position = positions.entry(order.location).or_default();
                        *position += 1;
                        *position
                    });
                Some(NavalConstructionOrderView {
                    order,
                    queue_position,
                })
            },
        )
    }

    /// Atomically add independent orders with explicit absolute completion days.
    ///
    /// The core does not infer time or charge cost. Orders settle after the
    /// daily phase and expiration processing, before same-day scheduled tasks;
    /// EU4 FIFO and pause rules are not applied.
    pub fn enqueue_naval_constructions(
        &mut self,
        inputs: &[NavalConstructionInput],
    ) -> Result<Vec<NavalConstructionId>, Error> {
        self.validate_naval_construction_inputs(inputs)?;
        self.naval_constructions.enqueue_validated(inputs)
    }

    /// Atomically imports per-location FIFO queues in the supplied order.
    ///
    /// Each location must have no pending orders. While a queue head remains
    /// unfinished, later orders at that location have their completion days
    /// postponed by one on each simulated day. Completing a head lets the next
    /// order be checked on that same day. Single-order locations retain the
    /// independent completion-day representation. Costs, pauses and cancellation
    /// are not inferred.
    pub fn enqueue_naval_construction_queues(
        &mut self,
        inputs: &[NavalConstructionInput],
    ) -> Result<Vec<NavalConstructionId>, Error> {
        self.validate_naval_construction_inputs(inputs)?;
        let mut counts = BTreeMap::<EntityId, usize>::new();
        for input in inputs {
            *counts.entry(input.location).or_default() += 1;
        }
        if self
            .naval_constructions()
            .iter()
            .any(|order| counts.contains_key(&order.location))
        {
            return Err(Error::Invalid(
                "naval construction queue location already has pending orders".into(),
            ));
        }
        let mut staged = self.naval_constructions.clone();
        let ids = staged.enqueue_validated(inputs)?;
        for (input, id) in inputs.iter().zip(&ids) {
            if counts[&input.location] > 1 {
                staged.fifo_order_ids.insert(*id);
            }
        }
        self.naval_constructions = staged;
        Ok(ids)
    }

    /// Atomically subtract caller-supplied costs and enqueue the corresponding orders.
    ///
    /// Costs use the existing Treasury `Subtract` bounds and unknown-balance
    /// delta behavior. Costs are checked-summed per country and each total is
    /// applied in one subtraction; this need not match separately clamped
    /// deductions. No ledger category or price, eligibility, loan, or refund
    /// policy is inferred.
    pub fn enqueue_paid_naval_constructions(
        &mut self,
        inputs: &[PaidNavalConstructionInput],
    ) -> Result<Vec<NavalConstructionId>, Error> {
        let constructions = inputs
            .iter()
            .map(|input| input.construction.clone())
            .collect::<Vec<_>>();
        self.validate_naval_construction_inputs(&constructions)?;

        let mut expenses = BTreeMap::<EntityId, i64>::new();
        for input in inputs {
            if input.cost_milli < 0 {
                return Err(Error::Invalid(
                    "naval construction cost must be nonnegative".into(),
                ));
            }
            let total = expenses.entry(input.construction.owner).or_default();
            *total = total.checked_add(input.cost_milli).ok_or(Error::Overflow)?;
        }

        let mut constructions_state = self.naval_constructions.clone();
        let ids = constructions_state.enqueue_validated(&constructions)?;
        let treasury = NumericSlot::Resource(CountryResource::Treasury);
        let mut resource_updates = BTreeMap::<EntityId, CountryResources>::new();
        let mut unbased_updates =
            BTreeMap::<EntityId, Option<BTreeMap<CountryResource, i64>>>::new();

        for (&country, &cost_milli) in &expenses {
            let mut resources = self
                .country_resources
                .get(&country)
                .cloned()
                .unwrap_or_default();
            let mut deltas = self
                .unbased_resource_deltas
                .get(&country)
                .cloned()
                .unwrap_or_default();
            if resources.treasury_milli.is_some() {
                let next = self.preview_numeric_update(
                    country,
                    &treasury,
                    NumericOperation::Subtract,
                    cost_milli,
                )?;
                CountryResource::Treasury.set(&mut resources, next);
                deltas.remove(&CountryResource::Treasury);
                resource_updates.insert(country, resources);
            } else {
                // Match update_numeric's unknown-resource path while staging its delta.
                let adjustment = cost_milli.checked_neg().ok_or(Error::Overflow)?;
                let current = deltas.get(&CountryResource::Treasury).copied().unwrap_or(0);
                let next = current.checked_add(adjustment).ok_or(Error::Overflow)?;
                if next == 0 {
                    deltas.remove(&CountryResource::Treasury);
                } else {
                    deltas.insert(CountryResource::Treasury, next);
                }
            }
            unbased_updates.insert(country, (!deltas.is_empty()).then_some(deltas));
        }

        for (country, resources) in resource_updates {
            self.country_resources.insert(country, resources);
        }
        for (country, deltas) in unbased_updates {
            if let Some(deltas) = deltas {
                self.unbased_resource_deltas.insert(country, deltas);
            } else {
                self.unbased_resource_deltas.remove(&country);
            }
        }
        self.naval_constructions = constructions_state;
        Ok(ids)
    }

    /// Atomically subtracts caller-supplied costs, records a generic ledger
    /// expense category, and enqueues the corresponding orders.
    ///
    /// The category is a core expense-vector index in `0..38`; its meaning is
    /// supplied by the caller. Every affected country must have a known
    /// treasury, matching the immediate ledger expense API. Costs are summed
    /// per country and recorded once per country. The ledger's pending balance
    /// is left unchanged, and ledger overflow or a failed order enqueue leaves
    /// both the ledger and world resources untouched.
    pub fn enqueue_naval_constructions_with_expenses(
        &mut self,
        inputs: &[PaidNavalConstructionInput],
        expense_category: u32,
    ) -> Result<Vec<NavalConstructionId>, Error> {
        if expense_category >= COUNTRY_LEDGER_EXPENSE_CATEGORY_COUNT {
            return Err(Error::Invalid(
                "country ledger expense category is outside 0..38".into(),
            ));
        }

        let constructions = inputs
            .iter()
            .map(|input| input.construction.clone())
            .collect::<Vec<_>>();
        self.validate_naval_construction_inputs(&constructions)?;

        let mut expenses = BTreeMap::<EntityId, i64>::new();
        for input in inputs {
            if input.cost_milli < 0 {
                return Err(Error::Invalid(
                    "naval construction cost must be nonnegative".into(),
                ));
            }
            let total = expenses.entry(input.construction.owner).or_default();
            *total = total.checked_add(input.cost_milli).ok_or(Error::Overflow)?;
        }

        let mut staged_ledgers = BTreeMap::<EntityId, CountryLedger>::new();
        for (&country, &amount_milli) in &expenses {
            if self
                .country_resources
                .get(&country)
                .and_then(|resources| resources.treasury_milli)
                .is_none()
            {
                return Err(Error::Invalid("country treasury is unknown".into()));
            }
            let mut ledger = self
                .country_ledgers
                .get(&country)
                .cloned()
                .unwrap_or_default();
            ledger.add_expense(expense_category, amount_milli)?;
            staged_ledgers.insert(country, ledger);
        }

        let ids = self.enqueue_paid_naval_constructions(inputs)?;
        self.country_ledgers.extend(staged_ledgers);
        Ok(ids)
    }

    fn validate_naval_construction_inputs(
        &self,
        inputs: &[NavalConstructionInput],
    ) -> Result<(), Error> {
        self.require_running()?;
        for input in inputs {
            if input.completion_day <= self.day() {
                return Err(Error::Invalid(
                    "naval construction day must be in the future".into(),
                ));
            }
            if let Some(calendar) = self.calendar() {
                calendar.date(input.completion_day)?;
            }
            validate_construction_location(self, input.owner, input.location, &input.unit_type)?;
        }
        Ok(())
    }

    pub(crate) fn complete_due_naval_constructions(
        &mut self,
        day: u64,
        previous_day: u64,
    ) -> Result<(), Error> {
        if self.naval_constructions.orders.is_empty() {
            return Ok(());
        }
        let mut constructions = self.naval_constructions.clone();
        let due = constructions.take_due(day, day > previous_day, self.calendar())?;
        if due.is_empty() {
            self.naval_constructions = constructions;
            return Ok(());
        }

        for order in &due {
            validate_construction_location(self, order.owner, order.location, &order.unit_type)?;
        }

        let mut naval_units = self.naval_units.clone();
        let mut fleets = self.fleets.clone();
        for order in &due {
            crate::naval::insert_constructed_ship(
                &mut naval_units,
                &mut fleets,
                order.owner,
                order.location,
                &order.unit_type,
            )?;
        }
        self.naval_units = naval_units;
        self.fleets = fleets;
        self.naval_constructions = constructions;
        Ok(())
    }
}
