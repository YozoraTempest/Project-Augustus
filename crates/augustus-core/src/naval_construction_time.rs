// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use serde::{Deserialize, Serialize};

use crate::Error;

const SCALE_MILLI: i64 = 1_000;

/// Fully resolved inputs for the ordinary naval construction time branch.
///
/// Callers must resolve all three modifiers and use this calculator only for
/// the normal flags-zero path where country, province owner, and controller
/// match. This does not infer missing modifiers or handle alternate branches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaNavalConstructionTimeInput {
    pub base_days: i32,
    pub minimum_modifier_milli: i32,
    pub country_modifier_milli: i32,
    pub province_modifier_milli: i32,
    pub additional_province_modifier_milli: i32,
}

/// The resolved ordinary-branch modifier factor and whole construction days.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct VanillaNavalConstructionTimeResult {
    pub modifier_factor_milli: i64,
    pub duration_days: u32,
}

/// Calculates the ordinary naval construction duration from explicit inputs.
///
/// The country and province factors are multiplied and truncated to thousandths,
/// then clamped to the configured minimum. The adjusted duration is truncated
/// first to milli-days and then to whole days, with a one-day minimum. This
/// calculation does not quote cost or submit a construction order.
pub fn calculate_vanilla_naval_construction_time(
    input: VanillaNavalConstructionTimeInput,
) -> Result<VanillaNavalConstructionTimeResult, Error> {
    if input.base_days <= 0 {
        return Err(Error::Invalid(
            "naval construction base time must be positive".into(),
        ));
    }
    if input.minimum_modifier_milli < 0 {
        return Err(Error::Invalid(
            "naval construction minimum modifier must be nonnegative".into(),
        ));
    }

    let country_factor = SCALE_MILLI
        .checked_add(i64::from(input.country_modifier_milli))
        .ok_or(Error::Overflow)?;
    let province_factor = SCALE_MILLI
        .checked_add(i64::from(input.province_modifier_milli))
        .and_then(|value| value.checked_add(i64::from(input.additional_province_modifier_milli)))
        .ok_or(Error::Overflow)?;
    let modifier_factor_milli = country_factor
        .checked_mul(province_factor)
        .ok_or(Error::Overflow)?
        .checked_div(SCALE_MILLI)
        .ok_or(Error::Overflow)?
        .max(i64::from(input.minimum_modifier_milli));

    let adjusted_milli_days = i64::from(input.base_days)
        .checked_mul(SCALE_MILLI)
        .and_then(|value| value.checked_mul(modifier_factor_milli))
        .ok_or(Error::Overflow)?
        .checked_div(SCALE_MILLI)
        .ok_or(Error::Overflow)?;
    let duration_days = adjusted_milli_days
        .checked_div(SCALE_MILLI)
        .ok_or(Error::Overflow)?
        .max(1);

    Ok(VanillaNavalConstructionTimeResult {
        modifier_factor_milli,
        duration_days: u32::try_from(duration_days).map_err(|_| Error::Overflow)?,
    })
}
