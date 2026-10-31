// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::COUNTRY_LEDGER_INCOME_TAX_CATEGORY;
use augustus_eu4::{VanillaExternalEu4RandomState, VanillaExternalState, VanillaWorld};
use std::{fs, path::Path};

fn world_fixture() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world")
}

fn external_state() -> VanillaExternalState {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/economy/external_state.json");
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

fn loaded_world() -> VanillaWorld {
    VanillaWorld::load_with_external_state(world_fixture(), "1444.11.11", &external_state())
        .unwrap()
}

#[test]
fn economy_report_distinguishes_uninstalled_stages_and_is_read_only() {
    let mut state = external_state();
    state.eu4_random_state = Some(VanillaExternalEu4RandomState { seed: 17, count: 3 });
    let world =
        VanillaWorld::load_with_external_state(world_fixture(), "1444.11.11", &state).unwrap();
    let before = world.save().unwrap();
    let random_before = world.initial.world().eu4_random_state();

    let report = world.economy_report("AAA").unwrap();
    assert_eq!(report.country, "AAA");
    assert_eq!(report.treasury_milli, Some(10_000));
    assert_eq!(report.ledger_pending_milli, None);
    assert_eq!(report.day, 0);
    assert_eq!(report.tax.as_ref().unwrap().settlement_count, 0);
    assert!(report.tax.as_ref().unwrap().last_receipt.is_none());
    assert_eq!(report.production.as_ref().unwrap().settlement_count, 0);
    assert!(report.production.as_ref().unwrap().last_receipt.is_none());
    assert_eq!(report.cashflow.as_ref().unwrap().settlement_count, 0);
    assert!(report.cashflow.as_ref().unwrap().last_receipt.is_none());
    assert_eq!(world.economy_report("AAA").unwrap(), report);
    assert!(world.economy_report("ZZZ").is_err());
    assert_eq!(world.save().unwrap(), before);
    assert_eq!(world.initial.world().eu4_random_state(), random_before);

    let no_external_stages = VanillaWorld::load(world_fixture(), "1444.11.11").unwrap();
    let uninstalled = no_external_stages.economy_report("AAA").unwrap();
    assert!(uninstalled.tax.is_some());
    assert!(uninstalled.production.is_none());
    assert!(uninstalled.cashflow.is_none());
}

#[test]
fn economy_report_tracks_monthly_boundaries_known_zero_and_partial_receipts() {
    let mut world = loaded_world();
    let initial = world.economy_report("AAA").unwrap();
    assert!(initial.tax.as_ref().unwrap().last_receipt.is_none());
    assert!(initial.production.as_ref().unwrap().last_receipt.is_none());
    assert!(initial.cashflow.as_ref().unwrap().last_receipt.is_none());

    world.advance(19).unwrap();
    let before_boundary = world.economy_report("AAA").unwrap();
    assert_eq!(before_boundary.date.unwrap().month, 11);
    assert!(before_boundary.tax.as_ref().unwrap().last_receipt.is_none());

    world.advance(1).unwrap();
    let aaa = world.economy_report("AAA").unwrap();
    assert_eq!(aaa.treasury_milli, Some(11_250));
    assert_eq!(aaa.ledger_pending_milli, Some(0));
    assert_eq!(aaa.day, 20);
    assert_eq!(
        aaa.date,
        Some(augustus_core::WorldDate {
            year: 1444,
            month: 12,
            day: 1,
        })
    );
    assert_eq!(aaa.tax.as_ref().unwrap().settlement_count, 1);
    for stage in [
        aaa.tax.as_ref(),
        aaa.production.as_ref(),
        aaa.cashflow.as_ref(),
    ] {
        let receipt = stage.unwrap().last_receipt.as_ref().unwrap();
        assert_eq!(receipt.day, 20);
        assert_eq!(receipt.date, aaa.date);
    }

    let tax = aaa.tax.as_ref().unwrap().last_receipt.as_ref().unwrap();
    assert_eq!(tax.known_subtotal_milli, Some(1_000));
    assert_eq!(tax.amount_milli, Some(1_000));
    assert_eq!(tax.credited_milli, Some(1_000));
    assert_eq!(tax.treasury_before_milli, Some(10_000));
    assert_eq!(tax.treasury_after_milli, Some(11_000));
    assert!(tax.unresolved.is_empty());
    let production = aaa
        .production
        .as_ref()
        .unwrap()
        .last_receipt
        .as_ref()
        .unwrap();
    assert_eq!(production.known_subtotal_milli, Some(500));
    assert_eq!(production.amount_milli, Some(500));
    assert_eq!(production.credited_milli, Some(500));
    assert_eq!(production.treasury_before_milli, Some(11_000));
    assert_eq!(production.treasury_after_milli, Some(11_500));
    assert!(production.unresolved.is_empty());
    let cashflow = aaa
        .cashflow
        .as_ref()
        .unwrap()
        .last_receipt
        .as_ref()
        .unwrap();
    assert_eq!(cashflow.known_subtotal_milli, None);
    assert_eq!(cashflow.amount_milli, Some(-250));
    assert_eq!(cashflow.credited_milli, Some(-250));
    assert_eq!(cashflow.treasury_before_milli, Some(11_500));
    assert_eq!(cashflow.treasury_after_milli, Some(11_250));
    assert!(cashflow.unresolved.is_empty());

    let bbb = world.economy_report("BBB").unwrap();
    assert_eq!(bbb.treasury_milli, None);
    assert_eq!(bbb.ledger_pending_milli, None);
    let production = bbb
        .production
        .as_ref()
        .unwrap()
        .last_receipt
        .as_ref()
        .unwrap();
    assert_eq!(production.known_subtotal_milli, Some(0));
    assert_eq!(production.amount_milli, None);
    assert!(
        production
            .unresolved
            .contains("province_input:self_authored_missing_production")
    );
    let cashflow = bbb
        .cashflow
        .as_ref()
        .unwrap()
        .last_receipt
        .as_ref()
        .unwrap();
    assert_eq!(cashflow.known_subtotal_milli, None);
    assert_eq!(cashflow.amount_milli, Some(0));
    assert_eq!(cashflow.credited_milli, None);

    let tax = bbb.tax.as_ref().unwrap().last_receipt.as_ref().unwrap();
    assert!(
        tax.known_subtotal_milli
            .is_some_and(|subtotal| subtotal > 0)
    );
    assert_eq!(tax.amount_milli, None);
    assert_eq!(tax.treasury_before_milli, None);
    assert_eq!(tax.credited_milli, None);
    assert!(!tax.unresolved.is_empty());
}

#[test]
fn economy_report_keeps_historical_receipts_when_balance_changes_and_restores() {
    let mut world = loaded_world();
    world.advance(20).unwrap();
    let historical = world.economy_report("AAA").unwrap();
    let original_receipt = historical.tax.as_ref().unwrap().last_receipt.clone();
    let original_treasury = historical.treasury_milli.unwrap();

    world
        .record_country_income("AAA", COUNTRY_LEDGER_INCOME_TAX_CATEGORY, 123)
        .unwrap();
    let changed = world.economy_report("AAA").unwrap();
    assert_eq!(changed.treasury_milli, Some(original_treasury + 123));
    assert_eq!(changed.tax.as_ref().unwrap().last_receipt, original_receipt);
    assert_eq!(changed.production, historical.production);
    assert_eq!(changed.cashflow, historical.cashflow);

    world
        .record_delayed_country_income("AAA", COUNTRY_LEDGER_INCOME_TAX_CATEGORY, 321)
        .unwrap();
    let pending = world.economy_report("AAA").unwrap();
    assert_eq!(pending.treasury_milli, changed.treasury_milli);
    assert_eq!(pending.ledger_pending_milli, Some(321));
    assert_eq!(pending.tax, historical.tax);

    let snapshot = world.save().unwrap();
    let mut restored = loaded_world();
    restored.restore(&snapshot).unwrap();
    assert_eq!(restored.economy_report("AAA").unwrap(), pending);
    assert_eq!(restored.economy_report("AAA").unwrap().tax, historical.tax);
}

#[test]
fn installed_stages_keep_global_counts_without_fabricating_a_country_receipt() {
    let mut state = external_state();
    state.production_inputs.remove(&1);
    state.country_monthly_cashflows_milli.remove("BBB");
    let mut world =
        VanillaWorld::load_with_external_state(world_fixture(), "1444.11.11", &state).unwrap();
    world.advance(20).unwrap();
    let bbb = world.economy_report("BBB").unwrap();
    for stage in [bbb.production, bbb.cashflow] {
        let stage = stage.expect("component is installed globally");
        assert_eq!(stage.settlement_count, 1);
        assert_eq!(stage.last_receipt, None);
    }
    let aaa = world.economy_report("AAA").unwrap();
    assert!(aaa.production.unwrap().last_receipt.is_some());
    assert!(aaa.cashflow.unwrap().last_receipt.is_some());
}
