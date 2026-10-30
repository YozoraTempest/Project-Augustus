// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    Error, VanillaNavalConstructionCostInput, calculate_vanilla_naval_construction_cost,
};

fn input() -> VanillaNavalConstructionCostInput {
    VanillaNavalConstructionCostInput {
        base_cost_milli: 10_000,
        country_modifier_milli: 0,
        additional_country_modifier_milli: 0,
        province_modifier_milli: 0,
        naval_quantity_milli: 10_000,
        naval_limit_milli: 17_000,
    }
}

#[test]
fn observed_ordinary_galley_inputs_produce_ten_currency_units() {
    let result = calculate_vanilla_naval_construction_cost(input()).unwrap();
    assert_eq!(result.modifier_factor_milli, 1_000);
    assert_eq!(result.cost_milli, 10_000);
}

#[test]
fn modifiers_sum_and_cost_truncates_to_milli_units() {
    let mut input = input();
    input.base_cost_milli = 10_001;
    input.country_modifier_milli = 100;
    input.additional_country_modifier_milli = -50;
    input.province_modifier_milli = -51;

    let result = calculate_vanilla_naval_construction_cost(input).unwrap();
    assert_eq!(result.modifier_factor_milli, 999);
    assert_eq!(result.cost_milli, 9_990);
}

#[test]
fn minimum_factor_and_cost_are_applied() {
    let mut input = input();
    input.base_cost_milli = 1;
    input.country_modifier_milli = -1_000;

    let result = calculate_vanilla_naval_construction_cost(input).unwrap();
    assert_eq!(result.modifier_factor_milli, 200);
    assert_eq!(result.cost_milli, 1_000);
}

#[test]
fn quantity_at_limit_is_supported() {
    let mut input = input();
    input.naval_quantity_milli = input.naval_limit_milli;

    assert!(calculate_vanilla_naval_construction_cost(input).is_ok());
}

#[test]
fn invalid_inputs_and_overflow_are_rejected() {
    let mut invalid = input();
    invalid.base_cost_milli = -1;
    assert!(matches!(
        calculate_vanilla_naval_construction_cost(invalid),
        Err(Error::Invalid(_))
    ));

    let mut invalid = input();
    invalid.naval_quantity_milli = -1;
    assert!(matches!(
        calculate_vanilla_naval_construction_cost(invalid),
        Err(Error::Invalid(_))
    ));

    let mut invalid = input();
    invalid.naval_limit_milli = 0;
    assert!(matches!(
        calculate_vanilla_naval_construction_cost(invalid),
        Err(Error::Invalid(_))
    ));

    let mut invalid = input();
    invalid.naval_quantity_milli = input().naval_limit_milli + 1;
    assert!(matches!(
        calculate_vanilla_naval_construction_cost(invalid),
        Err(Error::Invalid(_))
    ));

    let mut overflow = input();
    overflow.base_cost_milli = i32::MAX;
    overflow.country_modifier_milli = i32::MAX;
    overflow.additional_country_modifier_milli = i32::MAX;
    overflow.province_modifier_milli = i32::MAX;
    overflow.naval_quantity_milli = 1;
    overflow.naval_limit_milli = 1;
    assert!(matches!(
        calculate_vanilla_naval_construction_cost(overflow),
        Err(Error::Overflow)
    ));
}
