// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    Error, VanillaNavalConstructionTimeInput, calculate_vanilla_naval_construction_time,
};

fn input() -> VanillaNavalConstructionTimeInput {
    VanillaNavalConstructionTimeInput {
        base_days: 365,
        minimum_modifier_milli: 200,
        country_modifier_milli: 0,
        province_modifier_milli: 0,
        additional_province_modifier_milli: 0,
    }
}

#[test]
fn observed_galley_inputs_produce_354_and_350_days() {
    let mut input = input();
    input.province_modifier_milli = -30;
    let result = calculate_vanilla_naval_construction_time(input).unwrap();
    assert_eq!(result.modifier_factor_milli, 970);
    assert_eq!(result.duration_days, 354);

    input.province_modifier_milli = -40;
    let result = calculate_vanilla_naval_construction_time(input).unwrap();
    assert_eq!(result.modifier_factor_milli, 960);
    assert_eq!(result.duration_days, 350);
}

#[test]
fn factor_and_duration_truncate_at_each_stage() {
    let mut input = input();
    input.base_days = 1_001;
    input.minimum_modifier_milli = 0;
    input.country_modifier_milli = 1;
    input.province_modifier_milli = -1;

    let result = calculate_vanilla_naval_construction_time(input).unwrap();
    assert_eq!(result.modifier_factor_milli, 999);
    assert_eq!(result.duration_days, 999);
}

#[test]
fn minimum_modifier_and_one_day_floor_are_applied() {
    let mut input = input();
    input.country_modifier_milli = -900;
    let result = calculate_vanilla_naval_construction_time(input).unwrap();
    assert_eq!(result.modifier_factor_milli, 200);
    assert_eq!(result.duration_days, 73);

    input.base_days = 1;
    input.minimum_modifier_milli = 0;
    input.country_modifier_milli = -1_000;
    let result = calculate_vanilla_naval_construction_time(input).unwrap();
    assert_eq!(result.modifier_factor_milli, 0);
    assert_eq!(result.duration_days, 1);
}

#[test]
fn invalid_inputs_and_overflow_are_rejected() {
    let mut invalid = input();
    invalid.base_days = 0;
    assert!(matches!(
        calculate_vanilla_naval_construction_time(invalid),
        Err(Error::Invalid(_))
    ));

    let mut invalid = input();
    invalid.minimum_modifier_milli = -1;
    assert!(matches!(
        calculate_vanilla_naval_construction_time(invalid),
        Err(Error::Invalid(_))
    ));

    let mut overflow = input();
    overflow.country_modifier_milli = i32::MAX;
    overflow.province_modifier_milli = i32::MAX;
    overflow.additional_province_modifier_milli = i32::MAX;
    assert!(matches!(
        calculate_vanilla_naval_construction_time(overflow),
        Err(Error::Overflow)
    ));

    let mut overflow = input();
    overflow.base_days = i32::MAX;
    overflow.province_modifier_milli = i32::MAX;
    overflow.additional_province_modifier_milli = i32::MAX;
    assert!(matches!(
        calculate_vanilla_naval_construction_time(overflow),
        Err(Error::Overflow)
    ));

    let mut overflow = input();
    overflow.base_days = i32::MAX;
    overflow.country_modifier_milli = 1_001;
    assert!(matches!(
        calculate_vanilla_naval_construction_time(overflow),
        Err(Error::Overflow)
    ));
}
