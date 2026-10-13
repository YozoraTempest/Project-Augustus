// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::{fs, path::Path, process::Command};

#[test]
fn world_cli_reports_state_sources_and_query_failures() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world");
    let output = Command::new(env!("CARGO_BIN_EXE_augustus-world"))
        .arg("--game-root")
        .arg(&fixture)
        .args([
            "--country",
            "AAA",
            "--province",
            "1",
            "--economy",
            "--technology",
            "administrative",
            "--modifier",
            "test_river",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["country"]["bound_capital"], 2);
    assert_eq!(value["province"]["owner"], "BBB");
    assert_eq!(value["province_sources"][0]["line"], 2);
    assert_eq!(value["country"]["politics"]["ruler"]["name"], "Iris");
    assert_eq!(value["country"]["diplomacy"].as_array().unwrap().len(), 5);
    assert_eq!(
        value["province"]["buildings"],
        serde_json::json!(["test_fort"])
    );
    assert_eq!(value["province"]["trade_node"], "river");
    assert_eq!(value["country"]["technology"]["administrative"], 3);
    assert_eq!(value["country"]["resources"]["mercantilism_milli"], 18500);
    assert_eq!(
        value["province"]["attributes"]["tax_basis"]["known_subtotal_micros"],
        2337500
    );
    assert!(value["province"]["attributes"]["tax_basis"]["value_micros"].is_null());
    assert_eq!(value["province"]["administration"]["area"], "north_area");
    assert_eq!(
        value["province"]["administration"]["history_autonomy_delta_micros"],
        0
    );
    assert_eq!(
        value["province"]["administration"]["raw_autonomy_micros"],
        300000
    );
    assert_eq!(
        value["country"]["administrative_areas"]["east_area"],
        "state"
    );
    assert_eq!(
        value["province"]["attributes"]["tax_after_autonomy"]["steps"][1]["rate_micros"],
        0
    );
    assert_eq!(
        value["country"]["province_attribute_totals"]["tax_basis"]["known_subtotal_micros"],
        5100000
    );
    assert_eq!(
        value["modifier"]["definition"]["values_micros"]["test_growth"],
        -2500
    );
    assert_eq!(
        value["technology"]["levels"][0]["values_micros"]["test_yield"],
        12500
    );
    assert_eq!(
        value["economy"]["buildings"]["test_market"]["cost_milli"],
        87125
    );
    assert_eq!(
        value["economy"]["trade_nodes"][1]["members"],
        serde_json::json!([1])
    );
    let all = Command::new(env!("CARGO_BIN_EXE_augustus-world"))
        .arg("--game-root")
        .arg(&fixture)
        .arg("--diplomacy")
        .output()
        .unwrap();
    assert!(all.status.success());
    let all: serde_json::Value = serde_json::from_slice(&all.stdout).unwrap();
    assert_eq!(all["political_offices"]["imperial_emperor"], "AAA");
    assert_eq!(all["diplomacy"].as_array().unwrap().len(), 5);
    let error = Command::new(env!("CARGO_BIN_EXE_augustus-world"))
        .arg("--game-root")
        .arg(fixture)
        .args(["--province", "999"])
        .output()
        .unwrap();
    assert!(!error.status.success());
    assert!(error.stdout.is_empty());
    assert!(String::from_utf8_lossy(&error.stderr).contains("unknown province"));
}

#[test]
fn world_cli_loads_a_directory_mod_through_eu4_source_resolution() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world");
    let temp = tempfile::tempdir().unwrap();
    let mod_root = temp.path().join("test-mod");
    fs::create_dir_all(mod_root.join("common/ideas")).unwrap();
    fs::write(mod_root.join("descriptor.mod"), "name=\"CLI Test Mod\"").unwrap();
    fs::write(
        mod_root.join("common/ideas/99_mod.txt"),
        "mod_group = { free = yes first_idea = {} }",
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_augustus-world"))
        .arg("--game-root")
        .arg(fixture)
        .arg("--mod")
        .arg(&mod_root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(
        value["report"]["ideas"]["definition_sources"]["mod_group"]["path"]
            .as_str()
            .is_some_and(|path| path.ends_with("99_mod.txt"))
    );
}
