// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    VanillaCompanyCompositionDefinition, VanillaCompanyCompositionInput,
    calculate_vanilla_company_composition,
};

fn input() -> VanillaCompanyCompositionInput {
    VanillaCompanyCompositionInput {
        country_capacity_milli: 5_000,
        global_capacity_addition_milli: 0,
        definition: VanillaCompanyCompositionDefinition {
            total_count_limit: 10,
            minimum_total_count: 1,
            size_factor_milli: 1_000,
            first_group_ratio_milli: 600,
            second_group_ratio_milli: 200,
            third_group_ratio_milli: 200,
            second_group_count_limit: 0,
            duration_override: 0,
            ignore_capacity_excess_for_duration: false,
        },
        default_total_count_limit: 12,
        default_minimum_total_count: 1,
        size_scaling_divisors: [Some(1_000), Some(800)],
        country_count_ceiling_milli: 100_000,
        global_duration_scale_milli: 200,
        resolved_duration_modifier_milli: 0,
        second_group_available: true,
        third_group_available: true,
    }
}

#[test]
fn composition_keeps_the_size_ceiling_and_third_group_ratio_limit() {
    let result = calculate_vanilla_company_composition(input());

    assert_eq!(result.first_group_count, 7);
    assert_eq!(result.second_group_count, 2);
    assert_eq!(result.third_group_count, 1);
    assert_eq!(result.total_count, 10);
    assert_eq!(result.duration, 2_000);
}

#[test]
fn minimum_count_can_exceed_the_size_ceiling_and_capacity_excess_changes_duration() {
    let mut settings = input();
    settings.country_capacity_milli = 4_000;
    settings.definition.total_count_limit = 5;
    settings.definition.minimum_total_count = 7;
    settings.resolved_duration_modifier_milli = -500;
    settings.second_group_available = false;
    settings.third_group_available = false;

    let result = calculate_vanilla_company_composition(settings);
    assert_eq!(result.total_count, 7);
    assert_eq!(result.first_group_count, 7);
    assert_eq!(result.duration, 1_000);

    settings.definition.ignore_capacity_excess_for_duration = true;
    let without_excess = calculate_vanilla_company_composition(settings);
    assert_eq!(without_excess.duration, 700);
}

#[test]
fn negative_definition_limits_use_defaults_and_missing_divisors_use_the_fallback_path() {
    let mut settings = input();
    settings.country_capacity_milli = 3_000;
    settings.definition.total_count_limit = -1;
    settings.definition.minimum_total_count = -1;
    settings.default_total_count_limit = 4;
    settings.default_minimum_total_count = 3;
    settings.size_scaling_divisors = [None, None];
    settings.second_group_available = false;
    settings.third_group_available = false;

    let result = calculate_vanilla_company_composition(settings);
    assert_eq!(result.total_count, 4);
    assert_eq!(result.first_group_count, 4);
    assert_eq!(result.second_group_count, 0);
    assert_eq!(result.third_group_count, 0);
    assert_eq!(result.duration, 1_200);
}

#[test]
fn nonpositive_ratio_base_skips_the_third_group_limit() {
    let mut settings = input();
    settings.definition.first_group_ratio_milli = 0;
    settings.definition.second_group_ratio_milli = 0;
    settings.definition.third_group_ratio_milli = 200;
    settings.second_group_available = false;

    let result = calculate_vanilla_company_composition(settings);
    assert_eq!(result.total_count, 10);
    assert_eq!(result.third_group_count, 2);
    assert_eq!(result.first_group_count, 8);
}

#[test]
fn second_group_limit_and_explicit_duration_override_are_applied() {
    let mut settings = input();
    settings.definition.second_group_count_limit = 1;
    settings.definition.duration_override = 500;
    settings.resolved_duration_modifier_milli = 1_000;

    let result = calculate_vanilla_company_composition(settings);
    assert_eq!(result.first_group_count, 8);
    assert_eq!(result.second_group_count, 1);
    assert_eq!(result.third_group_count, 1);
    assert_eq!(result.total_count, 10);
    assert_eq!(result.duration, 1_000);

    settings.definition.duration_override = -500;
    let nonnegative_override = calculate_vanilla_company_composition(settings);
    assert_eq!(nonnegative_override.duration, 0);
}

#[test]
fn large_capacity_uses_the_wide_division_path() {
    let mut settings = input();
    settings.country_capacity_milli = 1_000_000_000;
    settings.definition.total_count_limit = 2_000_000;
    settings.definition.minimum_total_count = 1;
    settings.size_scaling_divisors = [Some(1_000), None];
    settings.country_count_ceiling_milli = 2_000_000_000;
    settings.second_group_available = false;
    settings.third_group_available = false;

    let result = calculate_vanilla_company_composition(settings);
    assert_eq!(result.total_count, 2_000_000);
    assert_eq!(result.duration, 400_000_000);
}
