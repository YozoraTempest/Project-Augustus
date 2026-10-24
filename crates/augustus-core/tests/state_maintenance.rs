// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    VanillaStateMaintenanceDistanceInput, VanillaStateMaintenanceInput,
    calculate_vanilla_state_maintenance,
};

#[test]
fn province_state_maintenance_preserves_distance_and_modifier_truncation() {
    let result = calculate_vanilla_state_maintenance(VanillaStateMaintenanceInput {
        base_tax_milli: 2_000,
        base_production_milli: 3_000,
        base_manpower_milli: 1_000,
        development_scale_milli: 7,
        special_development_scale_milli: None,
        distance: Some(VanillaStateMaintenanceDistanceInput {
            value: 81,
            square_root_mode: true,
            modifier_milli: 10,
        }),
        region_difference_modifier_milli: 100,
        owner_relation_modifier_milli: 250,
        country_key61_modifier_milli: 50,
        province_key62_modifier_milli: -100,
        caller_extra_modifier_milli: 25,
    })
    .unwrap();

    assert_eq!(result.base_sum_milli, 6_000);
    assert_eq!(result.scaled_base_milli, 42);
    assert_eq!(result.distance_term_milli, 90);
    assert_eq!(result.final_factor_milli, 1_415);
    assert_eq!(result.maintenance_milli, 59);
}

#[test]
fn special_scale_integer_distance_and_negative_floor_are_separate_stages() {
    let scaled = calculate_vanilla_state_maintenance(VanillaStateMaintenanceInput {
        base_tax_milli: 1,
        base_production_milli: 1,
        base_manpower_milli: 1,
        development_scale_milli: 500,
        special_development_scale_milli: Some(50),
        distance: Some(VanillaStateMaintenanceDistanceInput {
            value: 10,
            square_root_mode: false,
            modifier_milli: -20,
        }),
        region_difference_modifier_milli: 0,
        owner_relation_modifier_milli: 0,
        country_key61_modifier_milli: 0,
        province_key62_modifier_milli: 0,
        caller_extra_modifier_milli: 0,
    })
    .unwrap();

    assert_eq!(scaled.base_sum_milli, 3);
    assert_eq!(scaled.scaled_base_milli, 1);
    assert_eq!(scaled.distance_term_milli, -200);
    assert_eq!(scaled.final_factor_milli, 800);
    assert_eq!(scaled.maintenance_milli, 0);

    let floored = calculate_vanilla_state_maintenance(VanillaStateMaintenanceInput {
        base_tax_milli: 1_000,
        base_production_milli: 0,
        base_manpower_milli: 0,
        development_scale_milli: 1_000,
        special_development_scale_milli: None,
        distance: None,
        region_difference_modifier_milli: 0,
        owner_relation_modifier_milli: 0,
        country_key61_modifier_milli: -2_000,
        province_key62_modifier_milli: 0,
        caller_extra_modifier_milli: 0,
    })
    .unwrap();
    assert_eq!(floored.final_factor_milli, -1_000);
    assert_eq!(floored.maintenance_milli, 0);
}

#[test]
fn square_root_distance_rejects_negative_service_results() {
    let result = calculate_vanilla_state_maintenance(VanillaStateMaintenanceInput {
        base_tax_milli: 0,
        base_production_milli: 0,
        base_manpower_milli: 0,
        development_scale_milli: 1_000,
        special_development_scale_milli: None,
        distance: Some(VanillaStateMaintenanceDistanceInput {
            value: -1,
            square_root_mode: true,
            modifier_milli: 1_000,
        }),
        region_difference_modifier_milli: 0,
        owner_relation_modifier_milli: 0,
        country_key61_modifier_milli: 0,
        province_key62_modifier_milli: 0,
        caller_extra_modifier_milli: 0,
    });

    assert!(result.is_err());
}
