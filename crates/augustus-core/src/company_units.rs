// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use serde::{Deserialize, Serialize};

/// Resolved company definition values used by the composition calculation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaCompanyCompositionDefinition {
    pub total_count_limit: i32,
    pub minimum_total_count: i32,
    pub size_factor_milli: i32,
    pub first_group_ratio_milli: i32,
    pub second_group_ratio_milli: i32,
    pub third_group_ratio_milli: i32,
    pub second_group_count_limit: i32,
    pub duration_override: i32,
    pub ignore_capacity_excess_for_duration: bool,
}

/// Inputs for the verified company composition and duration calculation.
///
/// The compatibility layer resolves the effective definition, fallback
/// limits, size divisors, category availability, and duration modifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaCompanyCompositionInput {
    pub country_capacity_milli: i32,
    pub global_capacity_addition_milli: i32,
    pub definition: VanillaCompanyCompositionDefinition,
    pub default_total_count_limit: i32,
    pub default_minimum_total_count: i32,
    pub size_scaling_divisors: [Option<i32>; 2],
    pub country_count_ceiling_milli: i32,
    pub global_duration_scale_milli: i32,
    pub resolved_duration_modifier_milli: i32,
    pub second_group_available: bool,
    pub third_group_available: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VanillaCompanyCompositionResult {
    pub first_group_count: i32,
    pub second_group_count: i32,
    pub third_group_count: i32,
    pub total_count: i32,
    pub duration: i32,
}

/// Calculates the five values consumed by company creation and construction.
///
/// Intermediate values retain native signed 32-bit wrapping and truncation.
/// The minimum total count is applied after the size ceiling and can therefore
/// exceed that ceiling.
pub fn calculate_vanilla_company_composition(
    input: VanillaCompanyCompositionInput,
) -> VanillaCompanyCompositionResult {
    let definition = input.definition;
    let capacity_milli = input
        .country_capacity_milli
        .wrapping_add(input.global_capacity_addition_milli)
        .max(2_000);
    let capacity_count_ceiling = (capacity_milli / 1_000).wrapping_mul(2);
    let effective_total_count_limit = if definition.total_count_limit < 0 {
        input.default_total_count_limit
    } else {
        definition.total_count_limit
    };
    let size_count_ceiling = capacity_count_ceiling.min(effective_total_count_limit);
    let capacity_excess = capacity_count_ceiling.wrapping_sub(effective_total_count_limit);
    let duration_excess_count =
        if !definition.ignore_capacity_excess_for_duration && capacity_excess > 0 {
            capacity_excess
        } else {
            0
        };

    let selected_size_divisor = input.size_scaling_divisors.into_iter().flatten().max();
    let scaled_size = match selected_size_divisor {
        Some(divisor) if divisor > 0 => scale_count_by_divisor(size_count_ceiling, divisor),
        _ => (size_count_ceiling as u32).wrapping_mul(10_000) as i32,
    };
    let rounded_size = (scaled_size / 1_000).wrapping_mul(1_000);
    let budgeted_size = rounded_size.min(input.country_count_ceiling_milli);
    let candidate_total_count =
        (i64::from(budgeted_size) * i64::from(definition.size_factor_milli) / 1_000) as i32 / 1_000;
    let effective_minimum_total_count = if definition.minimum_total_count < 0 {
        input.default_minimum_total_count
    } else {
        definition.minimum_total_count
    };
    let total_count = if effective_minimum_total_count <= candidate_total_count {
        candidate_total_count.min(size_count_ceiling)
    } else {
        effective_minimum_total_count
    };

    let category_count_ceiling = total_count.max(0).min(capacity_milli / 1_000);
    let third_group_count = if input.third_group_available {
        let desired = calculate_group_count(total_count, definition.third_group_ratio_milli);
        let ratio_base = definition
            .first_group_ratio_milli
            .wrapping_add(definition.second_group_ratio_milli);
        if ratio_base > 0 {
            let ratio =
                calculate_third_group_limit_ratio(definition.third_group_ratio_milli, ratio_base);
            desired.min(calculate_group_count(category_count_ceiling, ratio))
        } else {
            desired
        }
    } else {
        0
    };
    let mut second_group_count = if input.second_group_available {
        calculate_group_count(total_count, definition.second_group_ratio_milli)
    } else {
        0
    };
    if definition.second_group_count_limit > 0
        && definition.second_group_count_limit < second_group_count
    {
        second_group_count = definition.second_group_count_limit;
    }
    let first_group_count = total_count
        .wrapping_sub(third_group_count)
        .wrapping_sub(second_group_count);

    let unadjusted_duration = if definition.duration_override == 0 {
        let count_with_excess = (duration_excess_count as u32).wrapping_add(total_count as u32);
        let scaled_count = count_with_excess.wrapping_mul(1_000) as i32;
        (i64::from(scaled_count) * i64::from(input.global_duration_scale_milli) / 1_000) as i32
    } else {
        definition.duration_override.max(0)
    };
    let duration_factor = input.resolved_duration_modifier_milli.wrapping_add(1_000);
    let duration = (i64::from(unadjusted_duration) * i64::from(duration_factor) / 1_000) as i32;

    VanillaCompanyCompositionResult {
        first_group_count,
        second_group_count,
        third_group_count,
        total_count,
        duration,
    }
}

fn scale_count_by_divisor(count: i32, divisor: i32) -> i32 {
    let count_bits = count as u32;
    let use_narrow_product = count_bits.wrapping_mul(1_000).wrapping_add(0x189373) < 0x39580e;
    if use_narrow_product {
        count_bits.wrapping_mul(1_000_000) as i32 / divisor
    } else {
        let scaled_count = count_bits.wrapping_mul(1_000) as i32;
        (i64::from(scaled_count) * 1_000 / i64::from(divisor)) as i32
    }
}

fn calculate_group_count(total_count: i32, ratio_milli: i32) -> i32 {
    let scaled_count = (total_count as u32).wrapping_mul(1_000) as i32;
    ((i64::from(scaled_count) * i64::from(ratio_milli) / 1_000) as i32) / 1_000
}

fn calculate_third_group_limit_ratio(third_ratio_milli: i32, other_ratio_milli: i32) -> i32 {
    let use_narrow_product = (third_ratio_milli as u32).wrapping_add(0x189373) < 0x39580e;
    if use_narrow_product {
        third_ratio_milli.wrapping_mul(1_000) / other_ratio_milli
    } else {
        (i64::from(third_ratio_milli) * 1_000 / i64::from(other_ratio_milli)) as i32
    }
}
