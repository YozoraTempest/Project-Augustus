// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::CountryResources;
use augustus_eu4::{VanillaExternalState, VanillaMonthlyTaxQuery, VanillaWorld};
use std::{collections::BTreeMap, path::Path};

fn fixture() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world")
}

#[test]
fn monthly_tax_query_reports_external_tax_account_without_mutation() {
    let state = VanillaExternalState {
        country_resources: BTreeMap::from([(
            "AAA".into(),
            CountryResources {
                treasury_milli: Some(10_000),
                ..Default::default()
            },
        )]),
        province_annual_tax_milli: BTreeMap::from([(1, Some(12_000)), (2, Some(8_000))]),
        ..Default::default()
    };
    let loaded = VanillaWorld::load_with_external_state(fixture(), "1444.11.11", &state).unwrap();
    let snapshot = loaded.save().unwrap();
    let report = loaded
        .query_vanilla_monthly_tax(&VanillaMonthlyTaxQuery {
            country: "AAA".into(),
        })
        .unwrap();
    let account = report.evaluation.unwrap();
    assert_eq!(report.day, 0);
    assert_eq!(
        (
            account.known_subtotal_milli,
            account.income_milli,
            account.credited_milli,
            account.treasury_before_milli,
            account.treasury_after_milli,
        ),
        (666, Some(666), Some(666), Some(10_000), Some(10_666))
    );
    assert!(account.unresolved.is_empty());
    assert_eq!(loaded.save().unwrap(), snapshot);
}

#[test]
fn monthly_tax_query_rejects_unknown_country_and_fields() {
    let loaded = VanillaWorld::load(fixture(), "1444.11.11").unwrap();
    assert!(
        loaded
            .query_vanilla_monthly_tax(&VanillaMonthlyTaxQuery {
                country: "ZZZ".into(),
            })
            .is_err()
    );
    let value = serde_json::json!({"country": "AAA", "unexpected": true});
    assert!(serde_json::from_value::<VanillaMonthlyTaxQuery>(value).is_err());
}
