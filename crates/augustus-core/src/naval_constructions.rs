// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::{EntityId, EntityKind, Error, World};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct NavalConstructionId(pub u64);

#[derive(Debug, Clone, PartialEq, Eq)]
/// One independent construction request with a caller-supplied absolute core day.
/// The core does not infer construction time, charge a cost, or apply EU4 FIFO,
/// pause, or cancellation rules.
pub struct NavalConstructionInput {
    pub owner: EntityId,
    pub location: EntityId,
    pub unit_type: String,
    pub completion_day: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// A pending independent order using its caller-supplied completion day.
pub struct NavalConstructionOrder {
    pub id: NavalConstructionId,
    pub owner: EntityId,
    pub location: EntityId,
    pub unit_type: String,
    pub completion_day: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NavalConstructions {
    next_id: u64,
    orders: Vec<NavalConstructionOrder>,
}

impl NavalConstructions {
    pub(crate) fn is_initial_empty(&self) -> bool {
        self.next_id == 0 && self.orders.is_empty()
    }

    pub(crate) fn next_due_day(&self) -> Option<u64> {
        self.orders.iter().map(|order| order.completion_day).min()
    }

    pub(crate) fn due_on_or_before(&self, day: u64) -> Vec<NavalConstructionOrder> {
        self.orders
            .iter()
            .filter(|order| order.completion_day <= day)
            .cloned()
            .collect()
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

    pub(crate) fn remove_due(&mut self, day: u64) {
        self.orders.retain(|order| order.completion_day > day);
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

    /// Atomically add independent orders with explicit absolute completion days.
    ///
    /// The core does not infer time or charge cost. Orders settle after the
    /// daily phase and expiration processing, before same-day scheduled tasks;
    /// EU4 FIFO and pause rules are not applied.
    pub fn enqueue_naval_constructions(
        &mut self,
        inputs: &[NavalConstructionInput],
    ) -> Result<Vec<NavalConstructionId>, Error> {
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
        self.naval_constructions.enqueue_validated(inputs)
    }

    pub(crate) fn complete_due_naval_constructions(&mut self, day: u64) -> Result<(), Error> {
        let due = self.naval_constructions.due_on_or_before(day);
        if due.is_empty() {
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
        let mut constructions = self.naval_constructions.clone();
        constructions.remove_due(day);

        self.naval_units = naval_units;
        self.fleets = fleets;
        self.naval_constructions = constructions;
        Ok(())
    }
}
