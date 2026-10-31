// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{CountryResources, VanillaProductionInput, VanillaProductionInputState};
use augustus_eu4::{VanillaExternalState, VanillaMonthlyProductionQuery, VanillaWorld};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

fn fixture() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world")
}

fn input() -> VanillaProductionInputState {
    VanillaProductionInputState {
        input: Some(VanillaProductionInput {
            base_value_milli: 1_000,
            country_modifier_milli: 20,
            legacy_province_modifier_milli: 0,
            queried_province_modifier_milli: -10,
        }),
        unresolved: BTreeSet::new(),
    }
}

#[test]
fn monthly_production_query_reports_loaded_account_without_mutation() {
    let state = VanillaExternalState {
        country_resources: BTreeMap::from([(
            "BBB".into(),
            CountryResources {
                treasury_milli: Some(10_000),
                ..Default::default()
            },
        )]),
        production_inputs: BTreeMap::from([(1, input())]),
        ..Default::default()
    };
    let loaded = VanillaWorld::load_with_external_state(fixture(), "1444.11.11", &state).unwrap();
    let snapshot = loaded.save().unwrap();
    let report = loaded
        .query_vanilla_monthly_production(&VanillaMonthlyProductionQuery {
            country: "BBB".into(),
        })
        .unwrap();
    let evaluation = report.evaluation.unwrap();
    assert_eq!(report.country, "BBB");
    assert_eq!(report.day, 0);
    assert_eq!(evaluation.annual_value_milli, Some(1009));
    assert_eq!(evaluation.monthly_value_milli, Some(84));
    assert_eq!(evaluation.provinces.len(), 1);
    assert_eq!(loaded.save().unwrap(), snapshot);
}

#[test]
fn monthly_production_query_keeps_missing_executor_and_fields_strict() {
    let loaded = VanillaWorld::load(fixture(), "1444.11.11").unwrap();
    assert!(
        loaded
            .query_vanilla_monthly_production(&VanillaMonthlyProductionQuery {
                country: "BBB".into(),
            })
            .is_err()
    );
    assert!(
        loaded
            .query_vanilla_monthly_production(&VanillaMonthlyProductionQuery {
                country: "ZZZ".into(),
            })
            .is_err()
    );
    let value = serde_json::json!({"country": "AAA", "unexpected": true});
    assert!(serde_json::from_value::<VanillaMonthlyProductionQuery>(value).is_err());
}
