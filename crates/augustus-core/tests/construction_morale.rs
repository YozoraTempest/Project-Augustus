// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{VanillaConstructionMoraleInput, calculate_vanilla_construction_morale};

fn calculate(
    regular_base: i32,
    alternate_base: i32,
    alternate_selected: bool,
    factor: i32,
    override_with_regular: bool,
) -> i32 {
    calculate_vanilla_construction_morale(VanillaConstructionMoraleInput {
        regular_base_morale_milli: regular_base,
        alternate_base_morale_milli: alternate_base,
        alternate_base_selected: alternate_selected,
        construction_factor_milli: factor,
        override_with_regular_base: override_with_regular,
    })
    .morale_milli
}

#[test]
fn selects_the_unit_base_and_caps_only_the_factor_upper_bound() {
    let result = calculate_vanilla_construction_morale(VanillaConstructionMoraleInput {
        regular_base_morale_milli: 800,
        alternate_base_morale_milli: 1_200,
        alternate_base_selected: true,
        construction_factor_milli: 1_500,
        override_with_regular_base: false,
    });

    assert_eq!(result.selected_base_morale_milli, 1_200);
    assert_eq!(result.capped_factor_milli, 1_000);
    assert_eq!(result.scaled_morale_milli, 1_200);
    assert_eq!(result.morale_milli, 1_200);
}

#[test]
fn signed_division_truncates_toward_zero_before_zero_clamp() {
    assert_eq!(calculate(50, 50, true, -50, false), 0);
    assert_eq!(calculate(-50, -50, true, -50, false), 2);
}

#[test]
fn zero_and_negative_factors_are_not_lower_bounded() {
    assert_eq!(calculate(1_000, 1_000, false, 0, false), 0);
    assert_eq!(calculate(1_000, 1_000, false, -1_000, false), 0);
}

#[test]
fn special_owner_override_uses_regular_base_without_the_factor() {
    assert_eq!(calculate(800, 1_200, true, 500, true), 800);
    assert_eq!(calculate(-20, 1_200, true, 500, true), 0);
}

#[test]
fn ordinary_path_clamps_negative_scaled_morale_to_zero() {
    assert_eq!(calculate(800, -1_200, true, 500, false), 0);
}
