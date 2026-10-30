// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::path::Path;

use augustus_core::VanillaForceLimitResult;
use augustus_eu4::{
    ContentRoot, ContentSet, ContentSource, VanillaNavalForceLimitQuery, VanillaWorld,
};

fn world() -> VanillaWorld {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world");
    let content = ContentSet::resolve(vec![ContentSource::base(
        ContentRoot::open(fixture).unwrap(),
    )])
    .unwrap();
    VanillaWorld::load_from_content(&content, "1444.11.11").unwrap()
}

#[test]
fn caller_resolved_force_limit_query_delegates_to_core_and_accepts_zero() {
    let world = world();
    let before = world.save().unwrap();
    let report = world
        .query_naval_force_limit(&VanillaNavalForceLimitQuery {
            country: "AAA".into(),
            base_milli: Some(17_000),
            flat_modifier_milli: Some(746),
            percentage_modifier_milli: Some(0),
        })
        .unwrap();
    assert_eq!(
        report.evaluation,
        Some(VanillaForceLimitResult {
            base_with_flat_modifier_milli: 17_746,
            scale_factor_milli: 1_000,
            scaled_limit_milli: 17_746,
            whole_unit_limit_milli: 17_000,
            force_limit_milli: 17_000,
        })
    );
    assert!(report.unresolved.is_empty());
    assert_eq!(before, world.save().unwrap());
}

#[test]
fn caller_resolved_force_limit_query_accepts_all_zero_inputs_and_applies_minimum() {
    let world = world();
    let report = world
        .query_naval_force_limit(&VanillaNavalForceLimitQuery {
            country: "AAA".into(),
            base_milli: Some(0),
            flat_modifier_milli: Some(0),
            percentage_modifier_milli: Some(0),
        })
        .unwrap();

    assert_eq!(report.unresolved, Vec::<String>::new());
    assert_eq!(report.evaluation.unwrap().force_limit_milli, 1_000);
}

#[test]
fn caller_resolved_force_limit_query_preserves_missing_inputs_without_inference() {
    let world = world();
    let report = world
        .query_naval_force_limit(&VanillaNavalForceLimitQuery {
            country: "AAA".into(),
            base_milli: Some(17_000),
            flat_modifier_milli: None,
            percentage_modifier_milli: Some(0),
        })
        .unwrap();
    assert_eq!(report.evaluation, None);
    assert_eq!(
        report.unresolved,
        ["naval_force_limit:missing_input:flat_modifier_milli"]
    );
}

#[test]
fn caller_resolved_force_limit_query_rejects_unknown_country() {
    let error = world()
        .query_naval_force_limit(&VanillaNavalForceLimitQuery {
            country: "ZZZ".into(),
            base_milli: Some(1_000),
            flat_modifier_milli: Some(0),
            percentage_modifier_milli: Some(0),
        })
        .unwrap_err();
    assert!(error.to_string().contains("unknown country ZZZ"));
}
