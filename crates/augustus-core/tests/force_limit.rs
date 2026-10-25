// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{VanillaForceLimitInput, calculate_vanilla_force_limit};

#[test]
fn force_limit_adds_flat_contributions_before_percentage_scaling() {
    let result = calculate_vanilla_force_limit(VanillaForceLimitInput {
        base_milli: 10_999,
        flat_modifier_milli: 2_001,
        percentage_modifier_milli: 250,
    });

    assert_eq!(result.base_with_flat_modifier_milli, 13_000);
    assert_eq!(result.scale_factor_milli, 1_250);
    assert_eq!(result.scaled_limit_milli, 16_250);
    assert_eq!(result.whole_unit_limit_milli, 16_000);
    assert_eq!(result.force_limit_milli, 16_000);
}

#[test]
fn force_limit_truncates_signed_scaling_to_whole_units() {
    let result = calculate_vanilla_force_limit(VanillaForceLimitInput {
        base_milli: 1_000,
        flat_modifier_milli: -6_999,
        percentage_modifier_milli: -500,
    });

    assert_eq!(result.base_with_flat_modifier_milli, -5_999);
    assert_eq!(result.scale_factor_milli, 500);
    assert_eq!(result.scaled_limit_milli, -2_999);
    assert_eq!(result.whole_unit_limit_milli, -2_000);
    assert_eq!(result.force_limit_milli, 1_000);
}

#[test]
fn force_limit_wraps_the_base_sum_at_32_bits_before_scaling() {
    let result = calculate_vanilla_force_limit(VanillaForceLimitInput {
        base_milli: i32::MAX,
        flat_modifier_milli: 1,
        percentage_modifier_milli: 0,
    });

    assert_eq!(result.base_with_flat_modifier_milli, i32::MIN);
    assert_eq!(result.scaled_limit_milli, i32::MIN);
    assert_eq!(result.whole_unit_limit_milli, -2_147_483_000);
    assert_eq!(result.force_limit_milli, 1_000);
}
