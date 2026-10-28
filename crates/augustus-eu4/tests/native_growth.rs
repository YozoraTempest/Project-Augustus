// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::VanillaNativeGrowthCacheInput;
use augustus_eu4::VanillaWorld;
use std::path::Path;

fn fixture() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world")
}

fn input() -> VanillaNativeGrowthCacheInput {
    VanillaNativeGrowthCacheInput {
        base_progress_milli: 20,
        territory_bonus_milli: 0,
        current_devastation_milli: 99_999,
        net_monthly_devastation_milli: -265,
        devastation_burn_milli: 235,
        selected_native_development_milli: 1_275,
    }
}

#[test]
fn vanilla_world_native_growth_method_delegates_resolved_inputs() {
    let world = VanillaWorld::load(fixture(), "1444.11.11").unwrap();
    let result = world
        .calculate_vanilla_province_native_growth_cache(1, input())
        .unwrap();

    assert_eq!(result.combined_base_progress_milli, 20);
    assert_eq!(result.devastation_difference_milli, 100_264);
    assert!(result.high_branch);
    assert_eq!(result.ratio_milli, Some(426_655));
    assert_eq!(result.progress_milli, 8_533);
    assert_eq!(result.cache_milli, 6_692);
}

#[test]
fn vanilla_world_native_growth_method_rejects_unknown_province() {
    let world = VanillaWorld::load(fixture(), "1444.11.11").unwrap();
    let error = world
        .calculate_vanilla_province_native_growth_cache(999, input())
        .unwrap_err()
        .to_string();
    assert!(error.contains("unknown native-growth province"));
}
