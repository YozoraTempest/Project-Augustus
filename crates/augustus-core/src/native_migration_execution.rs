// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{EntityId, EntityKind, Error, InitialWorld};

const POWER_NAMES: [&str; 3] = ["adm_power", "dip_power", "mil_power"];

/// Costs and the explicit post-step value already resolved by the caller.
///
/// This stage rechecks and debits the three power metrics and updates the
/// persisted migration step. Capital, ownership, controller and tribal-owner
/// effects remain separate stages because the core model does not represent
/// all of those relationships.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaNativeMigrationExecutionInput {
    pub source_country: EntityId,
    pub expected_migration_steps: u32,
    pub next_migration_steps: u32,
    pub adm_cost: i32,
    pub dip_cost: i32,
    pub mil_cost: i32,
}

/// The state before and after an accepted execution-stage debit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct VanillaNativeMigrationExecutionResult {
    pub source_country: EntityId,
    pub costs: [i32; 3],
    pub before: [i64; 3],
    pub after: [i64; 3],
    pub migration_steps_before: u32,
    pub migration_steps_after: u32,
}

impl InitialWorld {
    /// Rechecks current power and state, then atomically applies the
    /// execution-stage debit and explicit post-step value.
    ///
    /// The input must come from a previously resolved migration gate, but the
    /// current metric values are read again here. Unknown or insufficient
    /// power returns an error without changing any metric.
    pub fn execute_vanilla_native_migration(
        &mut self,
        input: VanillaNativeMigrationExecutionInput,
    ) -> Result<VanillaNativeMigrationExecutionResult, Error> {
        if input.adm_cost < 0 || input.dip_cost < 0 || input.mil_cost < 0 {
            return Err(Error::Invalid(
                "native migration execution costs must be nonnegative".into(),
            ));
        }
        if self.world().entity(input.source_country)?.kind != EntityKind::Country {
            return Err(Error::Invalid(
                "native migration execution source must be a country".into(),
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
                "native migration steps changed before execution".into(),
            ));
        }

        let costs = [input.adm_cost, input.dip_cost, input.mil_cost];
        let before = POWER_NAMES.map(|name| {
            self.world()
                .metrics()
                .value(self.world(), input.source_country, name)
        });
        let before: [i64; 3] = before
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
                "native migration execution power is insufficient".into(),
            ));
        }
        let after = before.map(|available| available);
        let after = [
            after[0]
                .checked_sub(i64::from(input.adm_cost))
                .ok_or(Error::Overflow)?,
            after[1]
                .checked_sub(i64::from(input.dip_cost))
                .ok_or(Error::Overflow)?,
            after[2]
                .checked_sub(i64::from(input.mil_cost))
                .ok_or(Error::Overflow)?,
        ];

        let mut staged = self.clone();
        let metric_values = POWER_NAMES
            .into_iter()
            .zip(after)
            .map(|(name, value)| (name.to_owned(), Some(value)))
            .collect();
        staged.set_metric_values(BTreeMap::from([(input.source_country, metric_values)]))?;
        let mut states = staged.world().native_migration_states().clone();
        states
            .countries
            .get_mut(&input.source_country)
            .expect("validated native migration country")
            .migration_steps = Some(input.next_migration_steps);
        staged.set_native_migration_states(states)?;
        *self = staged;
        Ok(VanillaNativeMigrationExecutionResult {
            source_country: input.source_country,
            costs,
            before,
            after,
            migration_steps_before,
            migration_steps_after: input.next_migration_steps,
        })
    }
}
