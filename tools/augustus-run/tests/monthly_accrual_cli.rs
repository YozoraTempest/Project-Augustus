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

fn command(game: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_augustus-world"));
    command
        .arg("--game-root")
        .arg(game)
        .args(["--omit-load-report", "--days", "0"]);
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
fn monthly_accrual_queries_are_read_only_and_support_metric_filtering() {
    let temp = tempfile::tempdir().unwrap();
    let game = fixture(temp.path());
    let queries = temp.path().join("monthly-accrual-queries.json");
    fs::write(
        &queries,
        serde_json::to_vec(&serde_json::json!([
            {"country": "AAA"},
            {"country": "AAA", "metric": "adm_power"}
        ]))
        .unwrap(),
    )
    .unwrap();
    let baseline_path = temp.path().join("baseline.json");
    report(
        command(&game)
            .arg("--save")
            .arg(&baseline_path)
            .output()
            .unwrap(),
    );
    let queried_path = temp.path().join("queried.json");
    let output = report(
        command(&game)
            .arg("--vanilla-monthly-accrual-queries")
            .arg(&queries)
            .arg("--save")
            .arg(&queried_path)
            .output()
            .unwrap(),
    );
    let reports = &output["vanilla_monthly_accrual_queries"];
    assert!(reports[0]["accounts"].get("adm_power").is_some());
    assert!(reports[1]["accounts"].get("adm_power").is_some());
    assert_eq!(reports[1]["accounts"].as_object().unwrap().len(), 1);
    assert_eq!(
        fs::read(baseline_path).unwrap(),
        fs::read(queried_path).unwrap()
    );
}

#[test]
fn monthly_accrual_query_unknown_fields_fail_before_snapshot_creation() {
    let temp = tempfile::tempdir().unwrap();
    let game = fixture(temp.path());
    let queries = temp.path().join("invalid-monthly-accrual-queries.json");
    fs::write(
        &queries,
        serde_json::to_vec(&serde_json::json!([
            {"country": "AAA", "unexpected": true}
        ]))
        .unwrap(),
    )
    .unwrap();
    let snapshot = temp.path().join("invalid.json");
    let output = command(&game)
        .arg("--vanilla-monthly-accrual-queries")
        .arg(&queries)
        .arg("--save")
        .arg(&snapshot)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unknown field"));
    assert!(!snapshot.exists());
}
