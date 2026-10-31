// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_eu4::{VanillaTradeGoodPriceTableQuery, VanillaWorld};
use std::{fs, path::Path};

fn fixture() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world")
}

fn copy_fixture() -> tempfile::TempDir {
    let source = fixture();
    let temp = tempfile::tempdir().unwrap();
    for entry in walkdir::WalkDir::new(&source) {
        let entry = entry.unwrap();
        let target = temp
            .path()
            .join(entry.path().strip_prefix(&source).unwrap());
        if entry.file_type().is_dir() {
            fs::create_dir_all(target).unwrap();
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
    temp
}

#[test]
fn trade_price_table_query_reports_ordered_content_base_stage_without_mutation() {
    let loaded = VanillaWorld::load(fixture(), "1444.11.11").unwrap();
    let snapshot = loaded.save().unwrap();
    let report = loaded
        .query_vanilla_trade_good_price_table(&VanillaTradeGoodPriceTableQuery {
            trade_good: "cloth".into(),
            price_impacts_milli: vec![100, -40],
        })
        .unwrap();
    assert_eq!(report.base_price_milli, Some(3_000));
    let evaluation = report.evaluation.unwrap();
    assert_eq!(evaluation.combined_impact_milli, 60);
    assert_eq!(evaluation.factor_milli, 1_060);
    assert_eq!(evaluation.effective_price_milli, 3_180);
    assert!(report.unresolved.is_empty());
    assert_eq!(loaded.save().unwrap(), snapshot);
}

#[test]
fn trade_price_table_query_keeps_missing_and_unknown_goods_strict() {
    let temp = copy_fixture();
    let goods_path = temp.path().join("common/tradegoods/test_goods.txt");
    let goods = fs::read_to_string(&goods_path).unwrap();
    fs::write(
        goods_path,
        format!("{goods}\nunpriced = {{ color = {{ 0.1 0.1 0.1 }} }}\n"),
    )
    .unwrap();
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let missing = loaded
        .query_vanilla_trade_good_price_table(&VanillaTradeGoodPriceTableQuery {
            trade_good: "unpriced".into(),
            price_impacts_milli: Vec::new(),
        })
        .unwrap();
    assert_eq!(missing.base_price_milli, None);
    assert_eq!(missing.evaluation, None);
    assert_eq!(
        missing.unresolved,
        vec!["trade_good_price:missing_base_price"]
    );

    assert!(
        loaded
            .query_vanilla_trade_good_price_table(&VanillaTradeGoodPriceTableQuery {
                trade_good: "missing".into(),
                price_impacts_milli: Vec::new(),
            })
            .is_err()
    );

    let mut value = serde_json::json!({
        "trade_good": "cloth",
        "price_impacts_milli": [],
        "unexpected": true
    });
    assert!(serde_json::from_value::<VanillaTradeGoodPriceTableQuery>(value.take()).is_err());
}
