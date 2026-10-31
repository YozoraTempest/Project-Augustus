// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::{fs, path::Path, process::Command};

fn copy_directory(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let target = destination.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_directory(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn fixture(root: &Path) -> std::path::PathBuf {
    let game = root.join("game");
    copy_directory(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world"),
        &game,
    );
    game
}

fn command(game: &Path, external: Option<&Path>) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_augustus-world"));
    command
        .arg("--game-root")
        .arg(game)
        .args(["--omit-load-report", "--days", "0"]);
    if let Some(external) = external {
        command.arg("--external-state").arg(external);
    }
    command
}

fn report(output: std::process::Output) -> serde_json::Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn monthly_cashflow_queries_are_read_only_and_preview_external_inputs() {
    let temp = tempfile::tempdir().unwrap();
    let game = fixture(temp.path());
    let external = temp.path().join("external-state.json");
    fs::write(
        &external,
        serde_json::json!({
            "country_resources": {"AAA": {"treasury_milli": 1000}},
            "country_monthly_cashflows_milli": {"AAA": 250},
            "country_annual_tax_milli": {"AAA": 0}
        })
        .to_string(),
    )
    .unwrap();
    let queries = temp.path().join("monthly-cashflow-queries.json");
    fs::write(
        &queries,
        serde_json::to_vec(&serde_json::json!([{"country": "AAA"}])).unwrap(),
    )
    .unwrap();
    let baseline_path = temp.path().join("baseline.json");
    report(
        command(&game, Some(&external))
            .arg("--save")
            .arg(&baseline_path)
            .output()
            .unwrap(),
    );
    let queried_path = temp.path().join("queried.json");
    let output = report(
        command(&game, Some(&external))
            .arg("--vanilla-monthly-cashflow-queries")
            .arg(&queries)
            .arg("--save")
            .arg(&queried_path)
            .output()
            .unwrap(),
    );
    let evaluation = &output["vanilla_monthly_cashflow_queries"][0]["evaluation"];
    assert_eq!(evaluation["amount_milli"], 250);
    assert!(evaluation["treasury_after_milli"].is_null());
    assert_eq!(
        fs::read(baseline_path).unwrap(),
        fs::read(queried_path).unwrap()
    );
}

#[test]
fn monthly_cashflow_query_requires_executor_and_strict_fields() {
    let temp = tempfile::tempdir().unwrap();
    let game = fixture(temp.path());
    let queries = temp.path().join("invalid-monthly-cashflow.json");
    fs::write(
        &queries,
        serde_json::to_vec(&serde_json::json!([
            {"country": "AAA", "unexpected": true}
        ]))
        .unwrap(),
    )
    .unwrap();
    let snapshot = temp.path().join("invalid.json");
    let output = command(&game, None)
        .arg("--vanilla-monthly-cashflow-queries")
        .arg(&queries)
        .arg("--save")
        .arg(&snapshot)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unknown field"));
    assert!(!snapshot.exists());
}
