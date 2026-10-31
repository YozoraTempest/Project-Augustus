// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{COUNTRY_LEDGER_INCOME_TAX_CATEGORY, CountryResources};
use augustus_eu4::{VanillaExternalState, VanillaMonthlySettlementQuery, VanillaWorld};
use std::{collections::BTreeMap, path::Path};

fn fixture() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world")
}

#[test]
fn monthly_settlement_query_reports_final_treasury_and_ledger_without_mutation() {
    let state = VanillaExternalState {
        country_resources: BTreeMap::from([(
            "AAA".into(),
            CountryResources {
                treasury_milli: Some(10_000),
                ..Default::default()
            },
        )]),
        province_annual_tax_milli: BTreeMap::from([(1, Some(12_000)), (2, Some(8_000))]),
        country_monthly_cashflows_milli: BTreeMap::from([("AAA".into(), Some(250))]),
        ..Default::default()
    };
    let loaded = VanillaWorld::load_with_external_state(fixture(), "1444.11.11", &state).unwrap();
    let snapshot = loaded.save().unwrap();
    let report = loaded
        .query_vanilla_monthly_settlement(&VanillaMonthlySettlementQuery {
            country: "AAA".into(),
        })
        .unwrap();
    let evaluation = report.evaluation.unwrap();
    assert_eq!(report.day, 0);
    assert_eq!(report.system_id, "monthly-economy:1");
    assert_eq!(evaluation.treasury_before_milli, Some(10_000));
    assert_eq!(evaluation.treasury_after_milli, Some(10_916));
    assert_eq!(evaluation.pending_before_milli, 0);
    assert_eq!(evaluation.pending_after_milli, 0);
    assert!(evaluation.unresolved.is_empty());
    assert_eq!(
        evaluation.ledger.as_ref().unwrap().income_accumulator_milli
            [COUNTRY_LEDGER_INCOME_TAX_CATEGORY as usize],
        666
    );
    assert_eq!(loaded.save().unwrap(), snapshot);
}

#[test]
fn monthly_settlement_query_rejects_unknown_country_and_fields() {
    let loaded = VanillaWorld::load(fixture(), "1444.11.11").unwrap();
    assert!(
        loaded
            .query_vanilla_monthly_settlement(&VanillaMonthlySettlementQuery {
                country: "ZZZ".into(),
            })
            .is_err()
    );
    let value = serde_json::json!({"country": "AAA", "unexpected": true});
    assert!(serde_json::from_value::<VanillaMonthlySettlementQuery>(value).is_err());
}
