// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::CountryLedger;
use augustus_eu4::{VanillaAnnualLedgerRolloverQuery, VanillaWorld};
use std::path::Path;

fn fixture() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world")
}

#[test]
fn annual_ledger_rollover_query_reports_cleared_sources_without_mutation() {
    let mut loaded = VanillaWorld::load(fixture(), "1444.11.11").unwrap();
    loaded
        .set_country_ledgers(std::collections::BTreeMap::from([(
            "AAA".into(),
            CountryLedger {
                income_accumulator_milli: {
                    let mut values = vec![0; 19];
                    values[2] = 400;
                    values
                },
                expense_accumulator_milli: {
                    let mut values = vec![0; 38];
                    values[5] = 120;
                    values
                },
                ..Default::default()
            },
        )]))
        .unwrap();
    let snapshot = loaded.save().unwrap();
    let report = loaded
        .query_vanilla_annual_ledger_rollover(&VanillaAnnualLedgerRolloverQuery {
            country: "AAA".into(),
        })
        .unwrap();
    let ledger = report.evaluation.unwrap();
    assert_eq!(ledger.income_rollover_milli[2], 400);
    assert_eq!(ledger.expense_rollover_milli[5], 120);
    assert_eq!(ledger.income_rollover_total_milli, 400);
    assert_eq!(ledger.expense_rollover_total_milli, 120);
    assert!(
        ledger
            .income_accumulator_milli
            .iter()
            .all(|value| *value == 0)
    );
    assert!(
        ledger
            .expense_accumulator_milli
            .iter()
            .all(|value| *value == 0)
    );
    assert_eq!(loaded.save().unwrap(), snapshot);
}

#[test]
fn annual_ledger_rollover_query_rejects_unknown_country_and_fields() {
    let loaded = VanillaWorld::load(fixture(), "1444.11.11").unwrap();
    assert!(
        loaded
            .query_vanilla_annual_ledger_rollover(&VanillaAnnualLedgerRolloverQuery {
                country: "ZZZ".into(),
            })
            .is_err()
    );
    let report = loaded
        .query_vanilla_annual_ledger_rollover(&VanillaAnnualLedgerRolloverQuery {
            country: "AAA".into(),
        })
        .unwrap();
    assert!(report.evaluation.is_none());
    let value = serde_json::json!({"country": "AAA", "unexpected": true});
    assert!(serde_json::from_value::<VanillaAnnualLedgerRolloverQuery>(value).is_err());
}
