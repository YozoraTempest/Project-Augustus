// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use serde::{Deserialize, Serialize};

use crate::Error;

const SCALE_MILLI: i64 = 1_000;
const MINIMUM_MODIFIER_FACTOR_MILLI: i64 = 200;
const MINIMUM_COST_MILLI: i64 = 1_000;

/// Caller-resolved inputs for the ordinary, in-limit naval construction cost branch.
///
/// Callers must resolve the three modifier totals and naval quantity/limit, and
/// restrict use to the normal flags-zero path where country, province owner,
/// and controller match. This calculator does not infer those values or handle
/// over-limit and alternate branches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaNavalConstructionCostInput {
    pub base_cost_milli: i32,
    pub country_modifier_milli: i32,
    pub additional_country_modifier_milli: i32,
    pub province_modifier_milli: i32,
    pub naval_quantity_milli: i32,
    pub naval_limit_milli: i32,
}

/// The ordinary-branch modifier factor and final cost, both in thousandths.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct VanillaNavalConstructionCostResult {
    pub modifier_factor_milli: i64,
    pub cost_milli: i64,
}

/// Calculates ordinary naval construction cost from explicit caller-resolved inputs.
///
/// The three modifiers are added, the factor is clamped to 200 thousandths,
/// and cost is truncated to thousandths before applying the 1,000-milli minimum.
pub fn calculate_vanilla_naval_construction_cost(
    input: VanillaNavalConstructionCostInput,
) -> Result<VanillaNavalConstructionCostResult, Error> {
    if input.base_cost_milli < 0 {
        return Err(Error::Invalid(
            "naval construction base cost must be nonnegative".into(),
        ));
    }
    if input.naval_quantity_milli < 0 {
        return Err(Error::Invalid(
            "naval construction quantity must be nonnegative".into(),
        ));
    }
    if input.naval_limit_milli <= 0 {
        return Err(Error::Invalid(
            "naval construction limit must be positive".into(),
        ));
    }
    if input.naval_quantity_milli > input.naval_limit_milli {
        return Err(Error::Invalid(
            "naval construction quantity exceeds the limit".into(),
        ));
    }

    let modifier_factor_milli = SCALE_MILLI
        .checked_add(i64::from(input.country_modifier_milli))
        .and_then(|value| value.checked_add(i64::from(input.additional_country_modifier_milli)))
        .and_then(|value| value.checked_add(i64::from(input.province_modifier_milli)))
        .ok_or(Error::Overflow)?
        .max(MINIMUM_MODIFIER_FACTOR_MILLI);
    let cost_milli = i64::from(input.base_cost_milli)
        .checked_mul(modifier_factor_milli)
        .ok_or(Error::Overflow)?
        .checked_div(SCALE_MILLI)
        .ok_or(Error::Overflow)?
        .max(MINIMUM_COST_MILLI);

    Ok(VanillaNavalConstructionCostResult {
        modifier_factor_milli,
        cost_milli,
    })
}
