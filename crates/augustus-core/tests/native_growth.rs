// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    VanillaNativeGrowthApplicationInput, VanillaNativeGrowthCacheInput,
    calculate_vanilla_native_growth_application, calculate_vanilla_native_growth_cache,
};

fn input() -> VanillaNativeGrowthCacheInput {
    VanillaNativeGrowthCacheInput {
        base_progress_milli: 20,
        territory_bonus_milli: 0,
        current_devastation_milli: 99_000,
        net_monthly_devastation_milli: 0,
        devastation_burn_milli: 235,
        selected_native_development_milli: 1_275,
    }
}

#[test]
fn ordinary_growth_uses_zero_or_eighty_territory_bonus() {
    let result = calculate_vanilla_native_growth_cache(input()).unwrap();
    assert_eq!(result.combined_base_progress_milli, 20);
    assert_eq!(result.devastation_difference_milli, 99_000);
    assert!(!result.high_branch);
    assert_eq!(result.ratio_milli, None);
    assert_eq!(result.progress_milli, 20);
    assert_eq!(result.cache_milli, 15);

    let mut with_bonus = input();
    with_bonus.territory_bonus_milli = 80;
    let result = calculate_vanilla_native_growth_cache(with_bonus).unwrap();
    assert_eq!(result.combined_base_progress_milli, 100);
    assert_eq!(result.progress_milli, 100);
    assert_eq!(result.cache_milli, 78);
}

#[test]
fn devastation_cap_blocks_cache_production_before_high_branch() {
    let mut input = input();
    input.current_devastation_milli = 100_000;
    input.net_monthly_devastation_milli = -265;
    let result = calculate_vanilla_native_growth_cache(input).unwrap();
    assert_eq!(result.combined_base_progress_milli, 20);
    assert_eq!(result.devastation_difference_milli, 100_265);
    assert!(!result.high_branch);
    assert_eq!(result.ratio_milli, None);
    assert_eq!(result.progress_milli, 0);
    assert_eq!(result.cache_milli, 0);
}

#[test]
fn high_branch_scales_progress_and_selected_native_component() {
    let mut input = input();
    input.current_devastation_milli = 99_999;
    input.net_monthly_devastation_milli = -265;
    let result = calculate_vanilla_native_growth_cache(input).unwrap();
    assert_eq!(result.devastation_difference_milli, 100_264);
    assert!(result.high_branch);
    assert_eq!(result.ratio_milli, Some(426_655));
    assert_eq!(result.progress_milli, 8_533);
    assert_eq!(result.cache_milli, 6_692);
}

#[test]
fn zero_burn_uses_negative_one_ratio_and_truncates_to_zero() {
    let mut input = input();
    input.current_devastation_milli = 99_500;
    input.net_monthly_devastation_milli = -500;
    input.devastation_burn_milli = 0;
    input.selected_native_development_milli = 1;
    let result = calculate_vanilla_native_growth_cache(input).unwrap();
    assert!(result.high_branch);
    assert_eq!(result.ratio_milli, Some(-1));
    assert_eq!(result.progress_milli, 0);
    assert_eq!(result.cache_milli, 0);
}

#[test]
fn zero_burn_with_large_base_progress_rejects_negative_growth() {
    let mut input = input();
    input.base_progress_milli = 1_000;
    input.current_devastation_milli = 99_500;
    input.net_monthly_devastation_milli = -500;
    input.devastation_burn_milli = 0;
    input.selected_native_development_milli = 1;
    let error = calculate_vanilla_native_growth_cache(input)
        .unwrap_err()
        .to_string();
    assert!(error.contains("negative native growth progress"));
}

#[test]
fn selected_component_one_passes_progress_through_and_exact_thousand_normalizes() {
    let mut input = input();
    input.selected_native_development_milli = 1;
    let result = calculate_vanilla_native_growth_cache(input).unwrap();
    assert_eq!(result.cache_milli, result.progress_milli);

    input.selected_native_development_milli = 1_000;
    let result = calculate_vanilla_native_growth_cache(input).unwrap();
    assert_eq!(result.cache_milli, result.progress_milli);
}

#[test]
fn invalid_negative_zero_and_overflow_inputs_are_rejected() {
    for mutate in [
        |input: &mut VanillaNativeGrowthCacheInput| input.base_progress_milli = -1,
        |input: &mut VanillaNativeGrowthCacheInput| input.territory_bonus_milli = -1,
        |input: &mut VanillaNativeGrowthCacheInput| input.current_devastation_milli = -1,
        |input: &mut VanillaNativeGrowthCacheInput| input.devastation_burn_milli = -1,
        |input: &mut VanillaNativeGrowthCacheInput| input.selected_native_development_milli = 0,
        |input: &mut VanillaNativeGrowthCacheInput| input.selected_native_development_milli = -1,
    ] {
        let mut input = input();
        mutate(&mut input);
        assert!(calculate_vanilla_native_growth_cache(input).is_err());
    }

    let mut overflow = input();
    overflow.base_progress_milli = i32::MAX;
    overflow.territory_bonus_milli = 1;
    assert!(calculate_vanilla_native_growth_cache(overflow).is_err());

    let mut overflow = input();
    overflow.current_devastation_milli = 99_999;
    overflow.net_monthly_devastation_milli = i32::MIN;
    overflow.devastation_burn_milli = 1;
    overflow.base_progress_milli = i32::MAX;
    assert!(calculate_vanilla_native_growth_cache(overflow).is_err());
}

#[test]
fn application_writes_province_before_scaling_country_growth() {
    let result = calculate_vanilla_native_growth_application(VanillaNativeGrowthApplicationInput {
        selected_native_development_milli: 1_275,
        province_cache_milli: 15,
        national_growth_milli: 20,
    })
    .unwrap();
    assert_eq!(result.province_write_milli, Some(15));
    assert_eq!(result.component_after_province_milli, 1_290);
    assert_eq!(result.country_write_milli, 15);
    assert_eq!(result.final_component_milli, 1_305);
}

#[test]
fn zero_cache_skips_province_write_and_small_component_is_not_normalized() {
    let result = calculate_vanilla_native_growth_application(VanillaNativeGrowthApplicationInput {
        selected_native_development_milli: 1_275,
        province_cache_milli: 0,
        national_growth_milli: 20,
    })
    .unwrap();
    assert_eq!(result.province_write_milli, None);
    assert_eq!(result.component_after_province_milli, 1_275);
    assert_eq!(result.country_write_milli, 15);
    assert_eq!(result.final_component_milli, 1_290);

    let result = calculate_vanilla_native_growth_application(VanillaNativeGrowthApplicationInput {
        selected_native_development_milli: 1,
        province_cache_milli: 0,
        national_growth_milli: 20,
    })
    .unwrap();
    assert_eq!(result.province_write_milli, None);
    assert_eq!(result.component_after_province_milli, 1);
    assert_eq!(result.country_write_milli, 20);
    assert_eq!(result.final_component_milli, 21);
}

#[test]
fn application_rejects_unverified_nonpositive_inputs_and_checked_overflow() {
    for input in [
        VanillaNativeGrowthApplicationInput {
            selected_native_development_milli: -1,
            province_cache_milli: 1,
            national_growth_milli: 1,
        },
        VanillaNativeGrowthApplicationInput {
            selected_native_development_milli: 0,
            province_cache_milli: 1,
            national_growth_milli: 1,
        },
        VanillaNativeGrowthApplicationInput {
            selected_native_development_milli: 1,
            province_cache_milli: -1,
            national_growth_milli: 1,
        },
        VanillaNativeGrowthApplicationInput {
            selected_native_development_milli: 1,
            province_cache_milli: 1,
            national_growth_milli: -1,
        },
    ] {
        assert!(calculate_vanilla_native_growth_application(input).is_err());
    }

    assert!(
        calculate_vanilla_native_growth_application(VanillaNativeGrowthApplicationInput {
            selected_native_development_milli: i32::MAX,
            province_cache_milli: 1,
            national_growth_milli: 1,
        },)
        .is_err()
    );
    assert!(
        calculate_vanilla_native_growth_application(VanillaNativeGrowthApplicationInput {
            selected_native_development_milli: i32::MAX,
            province_cache_milli: 0,
            national_growth_milli: i32::MAX,
        },)
        .is_err()
    );
}
