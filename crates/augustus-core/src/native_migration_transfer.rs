// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use serde::{Deserialize, Serialize};

use crate::{EntityId, EntityKind, Error, InitialWorld, Target};

const POWER_NAMES: [&str; 3] = ["adm_power", "dip_power", "mil_power"];

/// A caller-resolved native migration transfer across the represented owner,
/// capital, controller and step fields. The core records the old controller
/// as previous-controller history when it is known; tribal-owner relations
/// and raw native province lists remain outside this transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaNativeMigrationTransferInput {
    pub source_country: EntityId,
    pub expected_current_capital: EntityId,
    pub destination_province: EntityId,
    pub expected_migration_steps: u32,
    pub next_migration_steps: u32,
    pub adm_cost: i32,
    pub dip_cost: i32,
    pub mil_cost: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct VanillaNativeMigrationTransferResult {
    pub source_country: EntityId,
    pub old_capital: EntityId,
    pub destination_province: EntityId,
    pub costs: [i32; 3],
    pub power_before: [i64; 3],
    pub power_after: [i64; 3],
    pub migration_steps_before: u32,
    pub migration_steps_after: u32,
}

impl InitialWorld {
    /// Atomically applies the represented portion of a native migration:
    /// power debit, destination ownership, capital move, old-owner absence,
    /// and an explicit post-migration step. Live inputs are rechecked before
    /// the staged clone is committed, and projection-only ambiguity rejects
    /// the staged transaction.
    pub fn execute_vanilla_native_migration_transfer(
        &mut self,
        input: VanillaNativeMigrationTransferInput,
    ) -> Result<VanillaNativeMigrationTransferResult, Error> {
        if input.adm_cost < 0 || input.dip_cost < 0 || input.mil_cost < 0 {
            return Err(Error::Invalid(
                "native migration transfer costs must be nonnegative".into(),
            ));
        }
        if self.world().entity(input.source_country)?.kind != EntityKind::Country {
            return Err(Error::Invalid(
                "native migration transfer source must be a country".into(),
            ));
        }
        if self.world().entity(input.expected_current_capital)?.kind != EntityKind::Province
            || self.world().entity(input.destination_province)?.kind != EntityKind::Province
        {
            return Err(Error::Invalid(
                "native migration transfer capitals must be provinces".into(),
            ));
        }
        if input.expected_current_capital == input.destination_province {
            return Err(Error::Invalid(
                "native migration transfer destination must differ from current capital".into(),
            ));
        }
        let current_capital = self
            .world()
            .entity(input.source_country)?
            .capital
            .ok_or_else(|| Error::Invalid("native migration current capital is unknown".into()))?;
        if current_capital != input.expected_current_capital {
            return Err(Error::Invalid(
                "native migration current capital changed before transfer".into(),
            ));
        }
        if self.world().entity(input.expected_current_capital)?.owner != Some(input.source_country)
        {
            return Err(Error::Invalid(
                "native migration current capital is not source-owned".into(),
            ));
        }
        let destination = self.world().entity(input.destination_province)?;
        if destination.owner.is_some()
            || !self
                .world()
                .known_absent_targets()
                .get(&input.destination_province)
                .is_some_and(|targets| targets.contains(&Target::Owner))
        {
            return Err(Error::Invalid(
                "native migration destination owner is not known absent".into(),
            ));
        }
        if self.world().entities().iter().any(|entity| {
            entity.kind == EntityKind::Country && entity.capital == Some(input.destination_province)
        }) {
            return Err(Error::Invalid(
                "native migration destination is another country's capital".into(),
            ));
        }

        let current_state = self
            .world()
            .native_migration_states()
            .countries
            .get(&input.source_country)
            .ok_or_else(|| Error::Invalid("native migration state is unknown".into()))?;
        if current_state.provinces.is_none() {
            return Err(Error::Invalid(
                "native migration province list is unknown".into(),
            ));
        }
        let migration_steps_before = current_state
            .migration_steps
            .ok_or_else(|| Error::Invalid("native migration steps are unknown".into()))?;
        if migration_steps_before != input.expected_migration_steps {
            return Err(Error::Invalid(
                "native migration steps changed before transfer".into(),
            ));
        }

        let costs = [input.adm_cost, input.dip_cost, input.mil_cost];
        let before: [i64; 3] = POWER_NAMES
            .map(|name| {
                self.world()
                    .metrics()
                    .value(self.world(), input.source_country, name)
            })
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?
            .try_into()
            .map_err(|_| {
                Error::Invalid("native migration power vector has invalid length".into())
            })?;
        if costs
            .iter()
            .zip(before)
            .any(|(&cost, available)| i64::from(cost) > available)
        {
            return Err(Error::Invalid(
                "native migration transfer power is insufficient".into(),
            ));
        }
        let after = [
            before[0]
                .checked_sub(i64::from(input.adm_cost))
                .ok_or(Error::Overflow)?,
            before[1]
                .checked_sub(i64::from(input.dip_cost))
                .ok_or(Error::Overflow)?,
            before[2]
                .checked_sub(i64::from(input.mil_cost))
                .ok_or(Error::Overflow)?,
        ];

        let mut staged = self.clone();
        staged.commit_native_migration_transfer(
            input.source_country,
            input.expected_current_capital,
            input.destination_province,
            after,
            input.next_migration_steps,
        )?;
        *self = staged;
        Ok(VanillaNativeMigrationTransferResult {
            source_country: input.source_country,
            old_capital: input.expected_current_capital,
            destination_province: input.destination_province,
            costs,
            power_before: before,
            power_after: after,
            migration_steps_before,
            migration_steps_after: input.next_migration_steps,
        })
    }
}
