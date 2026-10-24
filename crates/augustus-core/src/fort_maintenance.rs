// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use serde::{Deserialize, Serialize};

use crate::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaFortMaintenanceInput {
    pub base_cost_milli: i32,
    pub country_modifier_milli: i32,
    pub province_modifier_milli: i32,
    pub conversion_factor_micros: i32,
    pub local_multiplier_milli: i32,
    pub relationship_modifier_milli: i32,
    /// Whether the resolved related-country condition applies its modifier.
    pub apply_relationship_modifier: bool,
    /// Whether the caller has resolved the native branch that halves the result.
    pub halve_for_special_case: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaFortMaintenanceResult {
    pub base_after_country_and_province_modifiers_milli: i32,
    pub converted_base_milli: i32,
    pub after_local_multiplier_milli: i32,
    pub amount_milli: i64,
}

/// Calculates the observed fort-maintenance stages using already resolved inputs.
/// Each multiplication divides immediately and truncates toward zero.
pub fn calculate_vanilla_fort_maintenance(
    input: VanillaFortMaintenanceInput,
) -> Result<VanillaFortMaintenanceResult, Error> {
    let base_factor_milli = (1_000_i64
        + i64::from(input.country_modifier_milli)
        + i64::from(input.province_modifier_milli))
    .max(100);
    let base_after_country_and_province_modifiers_milli = i32::try_from(
        i64::from(input.base_cost_milli)
            .checked_mul(base_factor_milli)
            .ok_or(Error::Overflow)?
            / 1_000,
    )
    .map_err(|_| Error::Overflow)?;
    let converted_input = i64::from(base_after_country_and_province_modifiers_milli)
        .checked_mul(i64::from(input.conversion_factor_micros))
        .ok_or(Error::Overflow)?
        / 1_000;
    let converted_input = i32::try_from(converted_input).map_err(|_| Error::Overflow)?;
    let converted_base_milli = i32::try_from((i64::from(converted_input) * 1_000) / 1_000_000)
        .map_err(|_| Error::Overflow)?;
    let after_local_multiplier_milli = i32::try_from(
        (i64::from(converted_base_milli) * i64::from(input.local_multiplier_milli)) / 1_000,
    )
    .map_err(|_| Error::Overflow)?;
    let relation_adjusted_milli = if after_local_multiplier_milli > 0
        && input.apply_relationship_modifier
        && input.relationship_modifier_milli != 0
    {
        let relationship_factor_milli =
            1_000 + input.relationship_modifier_milli.clamp(-1_000, 1_000);
        i32::try_from(
            (i64::from(after_local_multiplier_milli) * i64::from(relationship_factor_milli))
                / 1_000,
        )
        .map_err(|_| Error::Overflow)?
    } else {
        after_local_multiplier_milli
    };
    let amount_milli = if input.halve_for_special_case {
        i64::from(relation_adjusted_milli / 2)
    } else {
        i64::from(relation_adjusted_milli)
    };

    Ok(VanillaFortMaintenanceResult {
        base_after_country_and_province_modifiers_milli,
        converted_base_milli,
        after_local_multiplier_milli,
        amount_milli,
    })
}
