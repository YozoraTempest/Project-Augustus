// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::{path::Path, process::Command};

#[test]
fn world_cli_reports_state_sources_and_query_failures() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world");
    let output = Command::new(env!("CARGO_BIN_EXE_augustus-world"))
        .arg("--game-root")
        .arg(&fixture)
        .args(["--country", "AAA", "--province", "1"])
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
