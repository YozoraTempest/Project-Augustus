// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{VanillaNativeMigrationCostInput, calculate_vanilla_native_migration_cost};

fn input() -> VanillaNativeMigrationCostInput {
    VanillaNativeMigrationCostInput {
        define_adm_power_milli: 0,
        define_dip_power_milli: 0,
        define_mil_power_milli: 50_000,
        migration_steps: 0,
        destination_in_native_membership: true,
        all_power_cost_modifier_milli: 0,
        country_migration_cost_modifier_milli: 0,
        province_migration_cost_modifier_milli: 0,
        available_adm_power: 140,
        available_dip_power: 126,
        available_mil_power: 50,
    }
}

#[test]
fn member_destination_costs_fifty_and_equal_resource_is_eligible() {
    let result = calculate_vanilla_native_migration_cost(input()).unwrap();
    assert_eq!(result.adm_cost, 0);
    assert_eq!(result.dip_cost, 0);
    assert_eq!(result.mil_cost, 50);
    assert_eq!(result.step_factor, 1);
    assert_eq!(result.modifier_factor_milli, 1_000);
    assert!(result.adm_eligible);
    assert!(result.dip_eligible);
    assert!(result.mil_eligible);
    assert!(result.eligible);

    let mut insufficient = input();
    insufficient.available_mil_power = 49;
    let result = calculate_vanilla_native_migration_cost(insufficient).unwrap();
    assert!(!result.mil_eligible);
    assert!(!result.eligible);
}

#[test]
fn positive_fraction_rounds_up_after_modifier_scaling() {
    let mut input = input();
    input.all_power_cost_modifier_milli = -50;
    input.available_mil_power = 48;
    let result = calculate_vanilla_native_migration_cost(input).unwrap();
    assert_eq!(result.modifier_factor_milli, 950);
    assert_eq!(result.mil_cost, 48);
    assert!(result.eligible);
}

#[test]
fn nonmember_destination_uses_steps_plus_one() {
    let mut input = input();
    input.destination_in_native_membership = false;
    input.migration_steps = 2;
    input.available_mil_power = 150;
    let result = calculate_vanilla_native_migration_cost(input).unwrap();
    assert_eq!(result.step_factor, 3);
    assert_eq!(result.mil_cost, 150);
    assert!(result.eligible);

    input.available_mil_power = 149;
    let result = calculate_vanilla_native_migration_cost(input).unwrap();
    assert!(!result.mil_eligible);
    assert!(!result.eligible);
}

#[test]
fn zero_cost_resources_are_always_eligible() {
    let mut input = input();
    input.available_adm_power = 0;
    input.available_dip_power = 0;
    input.available_mil_power = 0;
    let result = calculate_vanilla_native_migration_cost(input).unwrap();
    assert_eq!(result.adm_cost, 0);
    assert_eq!(result.dip_cost, 0);
    assert_eq!(result.mil_cost, 50);
    assert!(result.adm_eligible);
    assert!(result.dip_eligible);
    assert!(!result.mil_eligible);
    assert!(!result.eligible);
}

#[test]
fn invalid_inputs_and_checked_overflow_are_rejected() {
    for mutate in [
        |input: &mut VanillaNativeMigrationCostInput| input.define_adm_power_milli = -1,
        |input: &mut VanillaNativeMigrationCostInput| input.available_mil_power = -1,
        |input: &mut VanillaNativeMigrationCostInput| input.all_power_cost_modifier_milli = -1_000,
    ] {
        let mut input = input();
        mutate(&mut input);
        assert!(calculate_vanilla_native_migration_cost(input).is_err());
    }

    let mut overflow = input();
    overflow.destination_in_native_membership = false;
    overflow.migration_steps = u32::MAX;
    assert!(calculate_vanilla_native_migration_cost(overflow).is_err());

    let mut overflow = input();
    overflow.destination_in_native_membership = false;
    overflow.migration_steps = u32::MAX - 1;
    overflow.define_mil_power_milli = i32::MAX;
    overflow.all_power_cost_modifier_milli = i32::MAX;
    assert!(calculate_vanilla_native_migration_cost(overflow).is_err());
}
