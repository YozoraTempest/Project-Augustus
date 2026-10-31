// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_eu4::{VanillaExternalState, VanillaProvinceCenterOfTradeQuery, VanillaWorld};
use std::{collections::BTreeMap, path::Path};

fn fixture() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world")
}

#[test]
fn province_center_of_trade_query_preserves_known_zero_and_unknown_state() {
    let loaded = VanillaWorld::load(fixture(), "1444.11.11").unwrap();
    let before = loaded.save().unwrap();

    let zero = loaded
        .query_vanilla_province_center_of_trade(&VanillaProvinceCenterOfTradeQuery { province: 1 })
        .unwrap();
    assert_eq!(zero.level, Some(0));

    let active = loaded
        .query_vanilla_province_center_of_trade(&VanillaProvinceCenterOfTradeQuery { province: 2 })
        .unwrap();
    assert_eq!(active.level, Some(2));

    let unknown = loaded
        .query_vanilla_province_center_of_trade(&VanillaProvinceCenterOfTradeQuery { province: 3 })
        .unwrap();
    assert_eq!(unknown.level, None);
    assert_eq!(loaded.save().unwrap(), before);
}

#[test]
fn province_center_of_trade_query_reads_external_level_and_rejects_invalid_inputs() {
    let loaded = VanillaWorld::load_with_external_state(
        fixture(),
        "1444.11.11",
        &VanillaExternalState {
            province_center_of_trade_levels: BTreeMap::from([(1, Some(3))]),
            ..Default::default()
        },
    )
    .unwrap();
    let report = loaded
        .query_vanilla_province_center_of_trade(&VanillaProvinceCenterOfTradeQuery { province: 1 })
        .unwrap();
    assert_eq!(report.level, Some(3));

    let error = loaded
        .query_vanilla_province_center_of_trade(&VanillaProvinceCenterOfTradeQuery {
            province: 999,
        })
        .unwrap_err()
        .to_string();
    assert!(error.contains("unknown EU4 center-of-trade province 999"));

    let error = serde_json::from_str::<VanillaProvinceCenterOfTradeQuery>(
        r#"{"province":1,"unexpected":true}"#,
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("unknown field"));
}
