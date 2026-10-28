// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use serde::{Deserialize, Serialize};

use crate::Error;

const SCALE_MILLI: i64 = 1_000;

/// Explicit inputs for the observed native migration cost and resource gate.
///
/// The compatibility layer supplies the three define values, the destination
/// membership result, the resolved modifier values, and the current power
/// resources. This calculator does not inspect native state or mutate a
/// treasury.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaNativeMigrationCostInput {
    pub define_adm_power_milli: i32,
    pub define_dip_power_milli: i32,
    pub define_mil_power_milli: i32,
    pub migration_steps: u32,
    pub destination_in_native_membership: bool,
    pub all_power_cost_modifier_milli: i32,
    pub country_migration_cost_modifier_milli: i32,
    pub province_migration_cost_modifier_milli: i32,
    pub available_adm_power: i32,
    pub available_dip_power: i32,
    pub available_mil_power: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VanillaNativeMigrationCostResult {
    pub adm_cost: i32,
    pub dip_cost: i32,
    pub mil_cost: i32,
    pub step_factor: u32,
    pub modifier_factor_milli: i64,
    pub adm_eligible: bool,
    pub dip_eligible: bool,
    pub mil_eligible: bool,
    pub eligible: bool,
}

fn cost_for_define(
    define_power_milli: i32,
    step_factor: u32,
    modifier_factor_milli: i64,
) -> Result<i32, Error> {
    // Defines are already thousandths of a complete power point. The first
    // stage applies the integer step factor, then the modifier stage divides
    // by 1000 before the final conversion to complete power.
    let stage1_milli = i64::from(define_power_milli)
        .checked_mul(i64::from(step_factor))
        .ok_or(Error::Overflow)?;
    let stage2_milli = stage1_milli
        .checked_mul(modifier_factor_milli)
        .ok_or(Error::Overflow)?
        / SCALE_MILLI;
    let mut cost = stage2_milli / SCALE_MILLI;
    if stage2_milli > 0 && stage2_milli % SCALE_MILLI != 0 {
        cost = cost.checked_add(1).ok_or(Error::Overflow)?;
    }
    i32::try_from(cost).map_err(|_| Error::Overflow)
}

fn has_power(cost: i32, available: i32) -> bool {
    cost < 1 || available >= cost
}

/// Calculates the explicit native migration costs and resource qualification.
///
/// Each define is scaled by `S`, where `S` is one for a destination in the
/// native membership list and otherwise `migration_steps + 1`. The combined
/// modifier factor is `1000 + all + country + province`; each multiplication
/// truncates toward zero. Positive fractional complete-power results round up,
/// while zero does not receive a minimum cost. No command or resource state is
/// changed by this function.
pub fn calculate_vanilla_native_migration_cost(
    input: VanillaNativeMigrationCostInput,
) -> Result<VanillaNativeMigrationCostResult, Error> {
    if input.define_adm_power_milli < 0
        || input.define_dip_power_milli < 0
        || input.define_mil_power_milli < 0
    {
        return Err(Error::Invalid(
            "native migration defines must be nonnegative".into(),
        ));
    }
    if input.available_adm_power < 0
        || input.available_dip_power < 0
        || input.available_mil_power < 0
    {
        return Err(Error::Invalid(
            "native migration available power must be nonnegative".into(),
        ));
    }

    let step_factor = if input.destination_in_native_membership {
        1
    } else {
        input
            .migration_steps
            .checked_add(1)
            .ok_or(Error::Overflow)?
    };
    let modifier_factor_milli = 1_000_i64
        .checked_add(i64::from(input.all_power_cost_modifier_milli))
        .and_then(|value| value.checked_add(i64::from(input.country_migration_cost_modifier_milli)))
        .and_then(|value| {
            value.checked_add(i64::from(input.province_migration_cost_modifier_milli))
        })
        .ok_or(Error::Overflow)?;
    if modifier_factor_milli <= 0 {
        return Err(Error::Invalid(
            "native migration modifier factor must be positive".into(),
        ));
    }

    let adm_cost = cost_for_define(
        input.define_adm_power_milli,
        step_factor,
        modifier_factor_milli,
    )?;
    let dip_cost = cost_for_define(
        input.define_dip_power_milli,
        step_factor,
        modifier_factor_milli,
    )?;
    let mil_cost = cost_for_define(
        input.define_mil_power_milli,
        step_factor,
        modifier_factor_milli,
    )?;
    let adm_eligible = has_power(adm_cost, input.available_adm_power);
    let dip_eligible = has_power(dip_cost, input.available_dip_power);
    let mil_eligible = has_power(mil_cost, input.available_mil_power);

    Ok(VanillaNativeMigrationCostResult {
        adm_cost,
        dip_cost,
        mil_cost,
        step_factor,
        modifier_factor_milli,
        adm_eligible,
        dip_eligible,
        mil_eligible,
        eligible: adm_eligible && dip_eligible && mil_eligible,
    })
}
