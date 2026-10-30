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

fn fixture(root: &Path) -> (std::path::PathBuf, std::path::PathBuf) {
    let game = root.join("game");
    copy_directory(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world"),
        &game,
    );
    let source = root.join("baseline.eu4");
    fs::write(
        &source,
        "EU4txt\ndate=1444.11.11\nplayer=AAA\nmultiplayer_random_seed=7\nmultiplayer_random_count=0\ncountries={AAA={} BBB={}}\nprovinces={-1={owner=BBB controller=BBB} -2={owner=AAA controller=AAA} -4={owner=AAA controller=AAA}}\n",
    )
    .unwrap();
    (game, source)
}

fn command(game: &Path, source: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_augustus-world"));
    command
        .arg("--game-root")
        .arg(game)
        .arg("--eu4-save")
        .arg(source)
        .args(["--omit-load-report", "--days", "1"]);
    command
}

fn input() -> serde_json::Value {
    serde_json::json!({
        "goods_size_input": {
            "province_base_milli": 800,
            "trade_good_base_milli": 0,
            "external_base_milli": 0,
            "province_modifier_milli": 44,
            "trade_good_modifier_milli": 0,
            "external_modifier_milli": 0
        },
        "flat_modifier_milli": 0,
        "external_flat_modifier_milli": 0,
        "rate_modifier_milli": 0,
        "external_rate_modifier_milli": 0,
        "legacy_province_modifier_milli": 0,
        "queried_province_modifier_milli": 0
    })
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
fn content_production_queries_preview_binding_without_installing_inputs() {
    let temp = tempfile::tempdir().unwrap();
    let (game, source) = fixture(temp.path());
    let queries = temp.path().join("content-production-queries.json");
    fs::write(
        &queries,
        serde_json::to_vec(&serde_json::json!([
            {"province": 2, "input": input()},
            {"province": 4, "input": input()}
        ]))
        .unwrap(),
    )
    .unwrap();

    let baseline_path = temp.path().join("baseline.json");
    report(
        command(&game, &source)
            .arg("--save")
            .arg(&baseline_path)
            .output()
            .unwrap(),
    );
    let queried_path = temp.path().join("queried.json");
    let output = report(
        command(&game, &source)
            .arg("--vanilla-content-production-queries")
            .arg(&queries)
            .arg("--save")
            .arg(&queried_path)
            .output()
            .unwrap(),
    );
    let reports = output["vanilla_content_production_queries"]
        .as_array()
        .unwrap();
    assert_eq!(reports[0]["goods_size"]["quantity_milli"], 835);
    assert_eq!(reports[0]["value"]["value_milli"], 2_505);
    assert_eq!(
        reports[0]["production_input"]["input"]["base_value_milli"],
        2_505
    );
    assert_eq!(reports[0]["unresolved"], serde_json::json!([]));
    assert_eq!(reports[1]["value"], serde_json::Value::Null);
    assert_eq!(
        reports[1]["production_input"]["input"],
        serde_json::Value::Null
    );
    assert_eq!(
        reports[1]["unresolved"],
        serde_json::json!(["trade_good_price:unknown"])
    );
    assert_eq!(
        fs::read(baseline_path).unwrap(),
        fs::read(queried_path).unwrap()
    );
}

#[test]
fn content_production_query_unknown_fields_fail_before_snapshot_creation() {
    let temp = tempfile::tempdir().unwrap();
    let (game, source) = fixture(temp.path());
    let queries = temp.path().join("invalid-content-production-queries.json");
    let mut value = input();
    value["unexpected"] = serde_json::json!(true);
    fs::write(
        &queries,
        serde_json::to_vec(&serde_json::json!([
            {"province": 2, "input": value}
        ]))
        .unwrap(),
    )
    .unwrap();
    let snapshot = temp.path().join("invalid.json");
    let output = command(&game, &source)
        .arg("--vanilla-content-production-queries")
        .arg(&queries)
        .arg("--save")
        .arg(&snapshot)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unknown field"));
    assert!(!snapshot.exists());
}
