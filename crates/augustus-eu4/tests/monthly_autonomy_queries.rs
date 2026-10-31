// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{AttributeBase, AttributeFormula, MONTHLY_AUTONOMY_DELTA_ATTRIBUTE};
use augustus_eu4::{VanillaExternalState, VanillaMonthlyAutonomyQuery, VanillaWorld};
use std::{collections::BTreeMap, path::Path};

fn fixture() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world")
}

#[test]
fn monthly_autonomy_query_matches_capital_clamp_without_mutation() {
    let state = VanillaExternalState {
        province_raw_autonomy_milli: BTreeMap::from([(1, Some(50_000))]),
        ..Default::default()
    };
    let mut loaded =
        VanillaWorld::load_with_external_state(fixture(), "1444.11.11", &state).unwrap();
    let province = loaded.initial.province_entity(1).unwrap();
    let mut attributes = loaded.initial.world().attributes().clone();
    attributes.formulas.get_mut(&province).unwrap().insert(
        MONTHLY_AUTONOMY_DELTA_ATTRIBUTE.into(),
        AttributeFormula {
            base: AttributeBase::Constant { micros: -1_000 },
            channels: Vec::new(),
            minimum_micros: None,
            maximum_micros: None,
            result_quantum_micros: None,
            unresolved: Default::default(),
        },
    );
    loaded.initial.set_attributes(attributes).unwrap();
    let snapshot = loaded.save().unwrap();
    let report = loaded
        .query_vanilla_monthly_autonomy(&VanillaMonthlyAutonomyQuery { province: 1 })
        .unwrap();
    assert_eq!(report.evaluation.raw_autonomy_micros, Some(500_000));
    assert_eq!(report.evaluation.monthly_delta_micros, Some(-1_000));
    assert_eq!(report.evaluation.minimum_autonomy_micros, Some(0));
    assert_eq!(report.evaluation.maximum_autonomy_micros, Some(0));
    assert_eq!(report.evaluation.next_raw_autonomy_micros, Some(0));
    assert!(report.evaluation.changed);
    assert!(report.evaluation.unresolved.is_empty());
    assert_eq!(loaded.save().unwrap(), snapshot);
}

#[test]
fn monthly_autonomy_query_keeps_unknown_province_and_fields_strict() {
    let loaded = VanillaWorld::load(fixture(), "1444.11.11").unwrap();
    assert!(
        loaded
            .query_vanilla_monthly_autonomy(&VanillaMonthlyAutonomyQuery { province: 999 })
            .is_err()
    );
    let mut value = serde_json::json!({"province": 1, "unexpected": true});
    assert!(serde_json::from_value::<VanillaMonthlyAutonomyQuery>(value.take()).is_err());
}
