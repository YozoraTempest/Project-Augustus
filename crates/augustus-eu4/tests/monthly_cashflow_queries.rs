// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::CountryResources;
use augustus_eu4::{VanillaExternalState, VanillaMonthlyCashflowQuery, VanillaWorld};
use std::{collections::BTreeMap, path::Path};

fn fixture() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world")
}

#[test]
fn monthly_cashflow_query_reports_external_account_without_mutation() {
    let state = VanillaExternalState {
        country_resources: BTreeMap::from([(
            "AAA".into(),
            CountryResources {
                treasury_milli: Some(1_000),
                ..Default::default()
            },
        )]),
        country_monthly_cashflows_milli: BTreeMap::from([("AAA".into(), Some(250))]),
        country_annual_tax_milli: BTreeMap::from([("AAA".into(), Some(0))]),
        ..Default::default()
    };
    let loaded = VanillaWorld::load_with_external_state(fixture(), "1444.11.11", &state).unwrap();
    let snapshot = loaded.save().unwrap();
    let report = loaded
        .query_vanilla_monthly_cashflow(&VanillaMonthlyCashflowQuery {
            country: "AAA".into(),
        })
        .unwrap();
    let evaluation = report.evaluation.unwrap();
    assert_eq!(report.country, "AAA");
    assert_eq!(report.day, 0);
    assert_eq!(evaluation.amount_milli, Some(250));
    assert_eq!(evaluation.treasury_before_milli, None);
    assert_eq!(evaluation.treasury_after_milli, None);
    assert_eq!(evaluation.credited_milli, None);
    assert!(evaluation.unresolved.contains("unknown_treasury_after_tax"));
    assert_eq!(loaded.save().unwrap(), snapshot);
}

#[test]
fn monthly_cashflow_query_requires_executor_and_fields_are_strict() {
    let loaded = VanillaWorld::load(fixture(), "1444.11.11").unwrap();
    assert!(
        loaded
            .query_vanilla_monthly_cashflow(&VanillaMonthlyCashflowQuery {
                country: "AAA".into(),
            })
            .is_err()
    );
    assert!(
        loaded
            .query_vanilla_monthly_cashflow(&VanillaMonthlyCashflowQuery {
                country: "ZZZ".into(),
            })
            .is_err()
    );
    let value = serde_json::json!({"country": "AAA", "unexpected": true});
    assert!(serde_json::from_value::<VanillaMonthlyCashflowQuery>(value).is_err());
}
