// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::{collections::BTreeMap, path::Path};

use augustus_core::{CountryLedger, CountryResources};
use augustus_eu4::VanillaWorld;

use crate::ledger_view::country_ledger_view;

const DATE: &str = "1444.11.11";

fn fixture() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world")
}

fn seeded_world() -> VanillaWorld {
    VanillaWorld::load_with_seed(fixture(), DATE, 41).unwrap()
}

fn ledger(report: &serde_json::Value) -> &serde_json::Value {
    report.get("ledger").unwrap()
}

#[test]
fn absent_and_explicit_default_ledgers_keep_their_distinct_shapes() {
    let mut loaded = seeded_world();
    let absent = country_ledger_view(&loaded, "AAA").unwrap();

    assert_eq!(absent["country"], "AAA");
    assert_eq!(absent["day"], "0");
    assert_eq!(absent["date"]["year"], 1444);
    assert_eq!(absent["date"]["month"], 11);
    assert_eq!(absent["date"]["day"], 11);
    assert!(ledger(&absent).is_null());
    assert_eq!(absent["income_keys"].as_array().unwrap().len(), 19);
    assert_eq!(absent["expense_keys"].as_array().unwrap().len(), 38);
    assert_eq!(absent["income_keys"][0], "INCOMETAX");
    assert_eq!(absent["expense_keys"][8], "EXPENSEFORTM");

    loaded
        .set_country_ledgers(BTreeMap::from([("AAA".into(), CountryLedger::default())]))
        .unwrap();
    let explicit = country_ledger_view(&loaded, "AAA").unwrap();
    let explicit_ledger = ledger(&explicit);
    assert!(explicit_ledger.is_object());
    assert_eq!(explicit_ledger["pending_milli"], "0");
    assert_eq!(
        explicit_ledger["income_sources_milli"],
        serde_json::json!([])
    );
    assert_eq!(
        explicit_ledger["expense_sources_milli"],
        serde_json::json!([])
    );
    assert!(explicit_ledger.get("categories").is_none());
}

#[test]
fn live_records_preserve_category_identity_and_queries_are_read_only() {
    let resources = BTreeMap::from([(
        "AAA".into(),
        CountryResources {
            treasury_milli: Some(10_000),
            ..Default::default()
        },
    )]);
    let mut loaded =
        VanillaWorld::load_with_country_resources(fixture(), DATE, &resources).unwrap();
    loaded.record_country_income("AAA", 0, 300).unwrap();
    loaded.record_country_expense("AAA", 8, 100).unwrap();
    loaded
        .record_delayed_country_expense("AAA", 12, 50)
        .unwrap();

    let snapshot = loaded.save().unwrap();
    let random_state = loaded.initial.world().eu4_random_state();
    let report = country_ledger_view(&loaded, "AAA").unwrap();
    let current_ledger = ledger(&report);

    assert_eq!(current_ledger["pending_milli"], "-50");
    assert_eq!(current_ledger["categories"]["0"]["income_milli"], "300");
    assert_eq!(current_ledger["categories"]["0"]["expense_milli"], "0");
    assert_eq!(current_ledger["categories"]["8"]["income_milli"], "0");
    assert_eq!(current_ledger["categories"]["8"]["expense_milli"], "100");
    assert_eq!(current_ledger["categories"]["12"]["income_milli"], "0");
    assert_eq!(current_ledger["categories"]["12"]["expense_milli"], "50");
    assert_eq!(current_ledger["income_sources_milli"][0], "300");
    assert_eq!(current_ledger["expense_sources_milli"][8], "100");
    assert_eq!(current_ledger["expense_sources_milli"][12], "50");
    assert_eq!(loaded.save().unwrap(), snapshot);
    assert_eq!(loaded.initial.world().eu4_random_state(), random_state);
    assert!(country_ledger_view(&loaded, "ZZZ").is_err());
    assert_eq!(loaded.save().unwrap(), snapshot);
    assert_eq!(loaded.initial.world().eu4_random_state(), random_state);
}

#[test]
fn exact_string_conversion_preserves_legal_i64_boundaries() {
    let mut loaded = seeded_world();
    let mut income_sources = vec![0; 19];
    income_sources[4] = i64::MAX;
    loaded
        .set_country_ledgers(BTreeMap::from([(
            "AAA".into(),
            CountryLedger {
                pending_milli: i64::MIN,
                income_sources_milli: income_sources,
                ..Default::default()
            },
        )]))
        .unwrap();

    let report = country_ledger_view(&loaded, "AAA").unwrap();
    let current_ledger = ledger(&report);
    assert_eq!(current_ledger["pending_milli"], i64::MIN.to_string());
    assert_eq!(
        current_ledger["income_sources_milli"][4],
        i64::MAX.to_string()
    );
    assert_eq!(
        current_ledger["income_sources_milli"]
            .as_array()
            .unwrap()
            .len(),
        19
    );
}

#[test]
fn monthly_and_annual_reports_reflect_state_changes_and_restore() {
    let resources = BTreeMap::from([(
        "AAA".into(),
        CountryResources {
            treasury_milli: Some(10_000),
            ..Default::default()
        },
    )]);
    let mut loaded =
        VanillaWorld::load_with_country_resources(fixture(), DATE, &resources).unwrap();
    loaded.record_country_income("AAA", 2, 300).unwrap();
    loaded.record_country_expense("AAA", 8, 100).unwrap();
    let original_snapshot = loaded.save().unwrap();

    loaded
        .consolidate_country_ledgers_monthly(BTreeMap::new())
        .unwrap();
    let monthly_snapshot = loaded.save().unwrap();
    let monthly = country_ledger_view(&loaded, "AAA").unwrap();
    let monthly_ledger = ledger(&monthly);
    assert_eq!(monthly_ledger["income_sources_milli"][2], "0");
    assert_eq!(monthly_ledger["expense_sources_milli"][8], "0");
    assert_eq!(monthly_ledger["income_snapshot_milli"][2], "300");
    assert_eq!(monthly_ledger["expense_snapshot_milli"][8], "100");
    assert_eq!(monthly_ledger["income_total_milli"], "300");
    assert_eq!(monthly_ledger["income_filtered_total_milli"], "300");
    assert_eq!(monthly_ledger["expense_total_milli"], "100");
    assert_eq!(monthly_ledger["expense_filtered_total_milli"], "100");
    assert_eq!(monthly_ledger["income_accumulator_milli"][2], "300");
    assert_eq!(monthly_ledger["expense_accumulator_milli"][8], "100");
    assert_eq!(monthly_ledger["expense_auxiliary_milli"][8], "100");
    assert_eq!(monthly_ledger["categories"]["2"]["income_milli"], "300");
    assert_eq!(monthly_ledger["categories"]["8"]["expense_milli"], "100");
    assert_eq!(loaded.save().unwrap(), monthly_snapshot);

    loaded.rollover_country_ledgers_annual().unwrap();
    let annual_snapshot = loaded.save().unwrap();
    let annual = country_ledger_view(&loaded, "AAA").unwrap();
    let annual_ledger = ledger(&annual);
    assert_eq!(annual_ledger["income_rollover_milli"][2], "300");
    assert_eq!(annual_ledger["expense_rollover_milli"][8], "100");
    assert_eq!(annual_ledger["income_rollover_total_milli"], "300");
    assert_eq!(annual_ledger["expense_rollover_total_milli"], "100");
    assert_eq!(annual_ledger["income_accumulator_milli"][2], "0");
    assert_eq!(annual_ledger["expense_accumulator_milli"][8], "0");
    assert_eq!(annual_ledger["income_snapshot_milli"][2], "300");
    assert_eq!(annual_ledger["expense_snapshot_milli"][8], "100");
    assert_eq!(annual_ledger["categories"]["2"]["income_milli"], "300");
    assert_eq!(annual_ledger["categories"]["8"]["expense_milli"], "100");
    assert_eq!(loaded.save().unwrap(), annual_snapshot);

    loaded.restore(&original_snapshot).unwrap();
    let restored = country_ledger_view(&loaded, "AAA").unwrap();
    let restored_ledger = ledger(&restored);
    assert_eq!(restored_ledger["income_sources_milli"][2], "300");
    assert_eq!(restored_ledger["expense_sources_milli"][8], "100");
    assert_eq!(
        restored_ledger["income_snapshot_milli"],
        serde_json::json!(vec!["0"; 19])
    );
    assert_eq!(
        restored_ledger["expense_snapshot_milli"],
        serde_json::json!(vec!["0"; 38])
    );
    assert_eq!(restored_ledger["income_accumulator_milli"][2], "300");
    assert_eq!(restored_ledger["expense_accumulator_milli"][8], "100");
    assert_eq!(
        restored_ledger["income_rollover_milli"],
        serde_json::json!(vec!["0"; 19])
    );
    assert_eq!(
        restored_ledger["expense_rollover_milli"],
        serde_json::json!(vec!["0"; 38])
    );
    assert_eq!(restored_ledger["categories"]["2"]["income_milli"], "300");
    assert_eq!(restored_ledger["categories"]["8"]["expense_milli"], "100");
    assert_eq!(loaded.save().unwrap(), original_snapshot);
}
