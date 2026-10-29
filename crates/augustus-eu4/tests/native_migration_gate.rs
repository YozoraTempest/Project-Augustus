// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{VanillaNativeMigrationCostInput, VanillaNativeMigrationGateInput};
use augustus_eu4::VanillaWorld;
use std::path::Path;

fn fixture() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world")
}

fn cost() -> VanillaNativeMigrationCostInput {
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

fn input(world: &VanillaWorld) -> VanillaNativeMigrationGateInput {
    VanillaNativeMigrationGateInput {
        source_country: world.initial.country_entity("AAA"),
        current_capital: world
            .initial
            .provinces()
            .iter()
            .find(|province| {
                world.initial.province_entity(province.id)
                    == world
                        .initial
                        .world()
                        .entity(world.initial.country_entity("AAA").unwrap())
                        .unwrap()
                        .capital
            })
            .map(|province| province.id),
        destination_province: None,
        cost: cost(),
        native_membership_known: true,
        native_mode_eligible: Some(true),
        owner_eligible: Some(true),
        controller_eligible: Some(true),
        tribal_owner_eligible: Some(true),
        additional_gate_eligible: Some(true),
    }
}

#[test]
fn vanilla_world_gate_method_fills_explicit_destination_without_side_effects() {
    let world = VanillaWorld::load(fixture(), "1444.11.11").unwrap();
    let before = world.save().unwrap();
    let result = world
        .calculate_vanilla_native_migration_gate(1, input(&world))
        .unwrap();
    assert_eq!(result.destination_province, Some(1));
    assert_eq!(result.resolved_gate, Some(true));
    assert_eq!(world.save().unwrap(), before);
}

#[test]
fn vanilla_world_gate_method_rejects_conflicting_destination() {
    let world = VanillaWorld::load(fixture(), "1444.11.11").unwrap();
    let mut input = input(&world);
    input.destination_province = Some(2);
    let error = world
        .calculate_vanilla_native_migration_gate(1, input)
        .unwrap_err()
        .to_string();
    assert!(error.contains("destination does not match input"));
}

#[test]
fn vanilla_world_gate_method_rejects_unknown_destination() {
    let world = VanillaWorld::load(fixture(), "1444.11.11").unwrap();
    let error = world
        .calculate_vanilla_native_migration_gate(999, input(&world))
        .unwrap_err()
        .to_string();
    assert!(error.contains("unknown native-migration destination province"));
}

#[test]
fn adapter_rejects_invalid_country_and_capital_context_before_reporting_pass() {
    let world = VanillaWorld::load(fixture(), "1444.11.11").unwrap();
    let before = world.save().unwrap();
    for (country, capital) in [
        (augustus_core::EntityId(999), 3),
        (world.initial.province_entity(1).unwrap(), 3),
        (world.initial.country_entity("AAA").unwrap(), 999),
        (
            world.initial.country_entity("AAA").unwrap(),
            if input(&world).current_capital == Some(1) {
                2
            } else {
                1
            },
        ),
    ] {
        let mut request = input(&world);
        request.source_country = Some(country);
        request.current_capital = Some(capital);
        assert!(
            world
                .calculate_vanilla_native_migration_gate(1, request)
                .is_err()
        );
        assert_eq!(world.save().unwrap(), before);
    }
}

#[test]
fn unresolved_membership_and_explicit_rejection_leave_world_unchanged() {
    let world = VanillaWorld::load(fixture(), "1444.11.11").unwrap();
    let before = world.save().unwrap();
    let mut request = input(&world);
    request.native_membership_known = false;
    request.cost.available_mil_power = 49;
    let result = world
        .calculate_vanilla_native_migration_gate(1, request.clone())
        .unwrap();
    assert_eq!(result.cost, None);
    assert_eq!(result.resolved_gate, None);
    request.owner_eligible = Some(false);
    let result = world
        .calculate_vanilla_native_migration_gate(1, request)
        .unwrap();
    assert_eq!(result.resolved_gate, Some(false));
    assert_eq!(result.rejected, ["owner_ineligible"]);
    assert_eq!(world.save().unwrap(), before);
}
