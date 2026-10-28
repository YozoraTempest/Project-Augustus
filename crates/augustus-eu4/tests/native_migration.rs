// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::VanillaNativeMigrationCostInput;
use augustus_eu4::VanillaWorld;
use std::path::Path;

fn fixture() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world")
}

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
fn vanilla_world_native_migration_method_delegates_explicit_inputs() {
    let world = VanillaWorld::load(fixture(), "1444.11.11").unwrap();
    let result = world
        .calculate_vanilla_native_migration_cost(1, input())
        .unwrap();

    assert_eq!(result.adm_cost, 0);
    assert_eq!(result.dip_cost, 0);
    assert_eq!(result.mil_cost, 50);
    assert_eq!(result.step_factor, 1);
    assert_eq!(result.modifier_factor_milli, 1_000);
    assert!(result.eligible);
}

#[test]
fn vanilla_world_native_migration_method_preserves_nonmember_scaling() {
    let world = VanillaWorld::load(fixture(), "1444.11.11").unwrap();
    let mut input = input();
    input.destination_in_native_membership = false;
    input.migration_steps = 2;
    input.available_mil_power = 150;
    let result = world
        .calculate_vanilla_native_migration_cost(1, input)
        .unwrap();

    assert_eq!(result.step_factor, 3);
    assert_eq!(result.mil_cost, 150);
    assert!(result.eligible);
}

#[test]
fn vanilla_world_native_migration_method_rejects_unknown_destination() {
    let world = VanillaWorld::load(fixture(), "1444.11.11").unwrap();
    let error = world
        .calculate_vanilla_native_migration_cost(999, input())
        .unwrap_err()
        .to_string();
    assert!(error.contains("unknown native-migration destination province"));
}
