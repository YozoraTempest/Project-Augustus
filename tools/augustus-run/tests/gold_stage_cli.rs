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
    fs::write(
        game.join("common/tradegoods/test_goods.txt"),
        "grain = { color = { 0.9 0.8 0.3 } }\ncloth = { color = { 0.2 0.3 0.8 } }\ngold = { color = { 1.0 0.9 0.0 } goldtype = yes }\n",
    )
    .unwrap();
    fs::write(
        game.join("history/provinces/2 - East.txt"),
        "owner = AAA\ntrade_goods = gold",
    )
    .unwrap();
    game
}

fn command(game: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_augustus-world"));
    command
        .arg("--game-root")
        .arg(game)
        .args(["--omit-load-report", "--days", "1"]);
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
fn gold_stage_queries_preview_base_and_production_without_mutation() {
    let temp = tempfile::tempdir().unwrap();
    let game = fixture(temp.path());
    let queries = temp.path().join("gold-stage-queries.json");
    fs::write(
        &queries,
        serde_json::to_vec(&serde_json::json!([
            {
                "mode": "base",
                "query": {
                    "province": 2,
                    "input": {
                        "province_field_2df0": 0,
                        "province_field_428": 900,
                        "province_field_2df4": 100000,
                        "global_base_milli": 40000
                    }
                }
            },
            {
                "mode": "production",
                "query": {
                    "province": 2,
                    "input": {
                        "gold_size_milli": 800,
                        "mine_base_milli": 39640,
                        "external_gold_size_milli": 0
                    }
                }
            },
            {
                "mode": "production",
                "query": {
                    "province": 1,
                    "input": {
                        "gold_size_milli": 800,
                        "mine_base_milli": 39640,
                        "external_gold_size_milli": 0
                    }
                }
            }
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
            .arg("--vanilla-gold-stage-queries")
            .arg(&queries)
            .arg("--save")
            .arg(&queried_path)
            .output()
            .unwrap(),
    );
    let reports = output["vanilla_gold_stage_queries"].as_array().unwrap();
    assert_eq!(reports[0]["trade_good"], "gold");
    assert_eq!(reports[0]["gold_type"], true);
    assert_eq!(reports[0]["evaluation"]["base_value_milli"], 39640);
    assert_eq!(reports[1]["evaluation"]["value_milli"], 31712);
    assert_eq!(reports[1]["evaluation"]["monthly_value_milli"], 2642);
    assert_eq!(reports[2]["evaluation"], serde_json::Value::Null);
    assert_eq!(reports[2]["gold_type"], false);
    assert_eq!(
        reports[2]["unresolved"],
        serde_json::json!(["gold_stage:not_gold"])
    );
    assert_eq!(
        fs::read(baseline_path).unwrap(),
        fs::read(queried_path).unwrap()
    );
}

#[test]
fn gold_stage_query_unknown_fields_fail_before_snapshot_creation() {
    let temp = tempfile::tempdir().unwrap();
    let game = fixture(temp.path());
    let queries = temp.path().join("invalid-gold-stage-queries.json");
    fs::write(
        &queries,
        serde_json::to_vec(&serde_json::json!([
            {
                "mode": "base",
                "query": {
                    "province": 2,
                    "input": {
                        "province_field_2df0": 0,
                        "province_field_428": 900,
                        "province_field_2df4": 100000,
                        "global_base_milli": 40000,
                        "unexpected": true
                    }
                }
            }
        ]))
        .unwrap(),
    )
    .unwrap();
    let snapshot = temp.path().join("invalid.json");
    let output = command(&game)
        .arg("--vanilla-gold-stage-queries")
        .arg(&queries)
        .arg("--save")
        .arg(&snapshot)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unknown field"));
    assert!(!snapshot.exists());
}
