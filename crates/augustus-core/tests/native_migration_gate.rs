// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    VanillaNativeMigrationCostInput, VanillaNativeMigrationGateInput,
    calculate_vanilla_native_migration_gate,
};

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

fn input() -> VanillaNativeMigrationGateInput {
    VanillaNativeMigrationGateInput {
        source_country: Some(augustus_core::EntityId(0)),
        current_capital: Some(100),
        destination_province: Some(200),
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
fn all_explicit_contexts_produce_a_passing_resolved_gate() {
    let result = calculate_vanilla_native_migration_gate(input()).unwrap();
    assert_eq!(result.cost_eligible, Some(true));
    assert_eq!(result.resolved_gate, Some(true));
    assert!(result.unresolved.is_empty());
    assert!(result.rejected.is_empty());
    assert_eq!(result.destination_province, Some(200));
}

#[test]
fn unknown_contexts_remain_indeterminate_in_stable_order() {
    let mut input = input();
    input.source_country = None;
    input.current_capital = None;
    input.destination_province = None;
    input.native_membership_known = false;
    input.native_mode_eligible = None;
    input.owner_eligible = None;
    input.controller_eligible = None;
    input.tribal_owner_eligible = None;
    input.additional_gate_eligible = None;

    let result = calculate_vanilla_native_migration_gate(input).unwrap();
    assert_eq!(result.resolved_gate, None);
    assert_eq!(
        result.unresolved,
        vec![
            "source_country_unknown".to_string(),
            "current_capital_unknown".to_string(),
            "destination_province_unknown".to_string(),
            "native_membership_unknown".to_string(),
            "native_mode_unknown".to_string(),
            "owner_unknown".to_string(),
            "controller_unknown".to_string(),
            "tribal_owner_unknown".to_string(),
            "additional_gate_unknown".to_string(),
        ]
    );
    assert!(result.rejected.is_empty());
}

#[test]
fn explicit_false_context_rejects_even_when_cost_passes() {
    let mut input = input();
    input.owner_eligible = Some(false);
    input.tribal_owner_eligible = Some(false);

    let result = calculate_vanilla_native_migration_gate(input).unwrap();
    assert_eq!(result.resolved_gate, Some(false));
    assert_eq!(
        result.rejected,
        vec![
            "owner_ineligible".to_string(),
            "tribal_owner_ineligible".to_string(),
        ]
    );
    assert!(result.unresolved.is_empty());
}

#[test]
fn cost_failure_rejects_before_unknown_context_can_be_treated_as_pass() {
    let mut input = input();
    input.cost.available_mil_power = 49;
    input.native_mode_eligible = None;

    let result = calculate_vanilla_native_migration_gate(input).unwrap();
    assert_eq!(result.resolved_gate, Some(false));
    assert_eq!(result.rejected, vec!["cost_ineligible".to_string()]);
    assert_eq!(result.unresolved, vec!["native_mode_unknown".to_string()]);
}

#[test]
fn nonmember_cost_gate_distinguishes_insufficient_and_equal_military_power() {
    let mut input = input();
    input.cost.destination_in_native_membership = false;
    input.cost.migration_steps = 2;
    input.cost.available_mil_power = 126;

    let result = calculate_vanilla_native_migration_gate(input.clone()).unwrap();
    assert_eq!(result.cost.unwrap().mil_cost, 150);
    assert_eq!(result.cost_eligible, Some(false));
    assert_eq!(result.resolved_gate, Some(false));
    assert_eq!(result.rejected, vec!["cost_ineligible".to_string()]);

    input.cost.available_mil_power = 150;
    let result = calculate_vanilla_native_migration_gate(input).unwrap();
    assert_eq!(result.cost.unwrap().mil_cost, 150);
    assert_eq!(result.cost_eligible, Some(true));
    assert_eq!(result.resolved_gate, Some(true));
}

#[test]
fn discounted_cost_uses_positive_rounding_and_equal_resource_gate() {
    let mut input = input();
    input.cost.all_power_cost_modifier_milli = -50;
    input.cost.available_mil_power = 48;

    let result = calculate_vanilla_native_migration_gate(input).unwrap();
    assert_eq!(result.cost.unwrap().modifier_factor_milli, 950);
    assert_eq!(result.cost.unwrap().mil_cost, 48);
    assert_eq!(result.resolved_gate, Some(true));
}

#[test]
fn invalid_cost_input_is_propagated() {
    let mut input = input();
    input.cost.available_mil_power = -1;
    assert!(calculate_vanilla_native_migration_gate(input).is_err());
}

#[test]
fn unknown_membership_never_turns_a_placeholder_cost_into_rejection() {
    for membership in [false, true] {
        let mut input = input();
        input.native_membership_known = false;
        input.cost.destination_in_native_membership = membership;
        input.cost.migration_steps = 2;
        input.cost.available_mil_power = 49;
        let result = calculate_vanilla_native_migration_gate(input).unwrap();
        assert_eq!(result.cost, None);
        assert_eq!(result.cost_eligible, None);
        assert_eq!(result.resolved_gate, None);
        assert!(result.rejected.is_empty());
        assert_eq!(result.unresolved, ["native_membership_unknown"]);
    }
}
