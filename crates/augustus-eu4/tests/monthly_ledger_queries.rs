// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    COUNTRY_LEDGER_INCOME_GOLD_CATEGORY, COUNTRY_LEDGER_INCOME_TRADE_CATEGORY, CountryResources,
};
use augustus_eu4::{
    CashflowBreakdown, VanillaExternalState, VanillaMonthlyLedgerQuery, VanillaWorld,
};
use std::{collections::BTreeMap, path::Path};

fn fixture() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world")
}

#[test]
fn monthly_ledger_query_reports_pending_category_updates_without_mutation() {
    let state = VanillaExternalState {
        country_resources: BTreeMap::from([(
            "AAA".into(),
            CountryResources {
                treasury_milli: Some(1_000),
                ..Default::default()
            },
        )]),
        country_monthly_cashflows_milli: BTreeMap::from([("AAA".into(), Some(250))]),
        country_monthly_cashflow_breakdowns: BTreeMap::from([(
            "AAA".into(),
            CashflowBreakdown {
                trade_milli: Some(200),
                trade_income_pass: None,
                gold_milli: Some(100),
                expenses_milli: Some(50),
                expense_categories_milli: BTreeMap::new(),
            },
        )]),
        ..Default::default()
    };
    let loaded = VanillaWorld::load_with_external_state(fixture(), "1444.11.11", &state).unwrap();
    let snapshot = loaded.save().unwrap();
    let report = loaded
        .query_vanilla_monthly_ledger(&VanillaMonthlyLedgerQuery {
            country: "AAA".into(),
        })
        .unwrap();
    let evaluation = report.evaluation.unwrap();
    assert_eq!(
        evaluation.income_accumulator_milli[COUNTRY_LEDGER_INCOME_TRADE_CATEGORY as usize],
        200
    );
    assert_eq!(
        evaluation.income_accumulator_milli[COUNTRY_LEDGER_INCOME_GOLD_CATEGORY as usize],
        100
    );
    assert_eq!(loaded.save().unwrap(), snapshot);
}

#[test]
fn monthly_ledger_query_rejects_unknown_country_and_fields() {
    let loaded = VanillaWorld::load(fixture(), "1444.11.11").unwrap();
    assert!(
        loaded
            .query_vanilla_monthly_ledger(&VanillaMonthlyLedgerQuery {
                country: "ZZZ".into(),
            })
            .is_err()
    );
    let value = serde_json::json!({"country": "AAA", "unexpected": true});
    assert!(serde_json::from_value::<VanillaMonthlyLedgerQuery>(value).is_err());
}
