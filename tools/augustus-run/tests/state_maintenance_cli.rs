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

fn input(development_scale_milli: i32) -> serde_json::Value {
    serde_json::json!({
        "development_scale_milli": development_scale_milli,
        "special_development_scale_milli": null,
        "distance": null,
        "region_difference_modifier_milli": 0,
        "owner_relation_modifier_milli": 0,
        "country_key61_modifier_milli": 0,
        "province_key62_modifier_milli": 0,
        "caller_extra_modifier_milli": 0
    })
}

fn content_input() -> serde_json::Value {
    serde_json::json!({
        "special_development_scale_milli": null,
        "distance": null,
        "region_difference_modifier_milli": 0,
        "owner_relation_modifier_milli": 0,
        "country_key61_modifier_milli": 0,
        "province_key62_modifier_milli": 0,
        "caller_extra_modifier_milli": 0
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
fn state_maintenance_queries_cover_direct_content_and_preserve_snapshot_bytes() {
    let temp = tempfile::tempdir().unwrap();
    let (game, source) = fixture(temp.path());
    let queries = temp.path().join("state-maintenance-queries.json");
    fs::write(
        &queries,
        serde_json::to_vec(&serde_json::json!([
            {"mode": "direct", "query": {"province": 1, "input": input(1000)}},
            {"mode": "content", "query": {"province": 1, "input": content_input()}},
            {"mode": "direct", "query": {"province": 999, "input": input(1000)}}
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
            .arg("--state-maintenance-queries")
            .arg(&queries)
            .arg("--save")
            .arg(&queried_path)
            .output()
            .unwrap(),
    );
    let reports = output["state_maintenance_queries"].as_array().unwrap();
    assert_eq!(reports[0]["mode"], "direct");
    assert_eq!(reports[0]["evaluation"]["maintenance_milli"], 7625);
    assert_eq!(reports[1]["mode"], "content");
    assert_eq!(reports[1]["evaluation"]["maintenance_milli"], 53);
    assert_eq!(reports[2]["evaluation"], serde_json::Value::Null);
    assert_eq!(
        reports[2]["unresolved"][0],
        "state_maintenance:unknown_province:999"
    );
    assert_eq!(
        fs::read(baseline_path).unwrap(),
        fs::read(queried_path).unwrap()
    );
}

#[test]
fn state_maintenance_queries_report_missing_base_and_content_define() {
    let temp = tempfile::tempdir().unwrap();
    let (game, source) = fixture(temp.path());
    fs::write(
        game.join("history/provinces/1 - North.txt"),
        "owner = AAA\ncontroller = AAA",
    )
    .unwrap();
    let queries = temp.path().join("missing-base-queries.json");
    fs::write(
        &queries,
        serde_json::to_vec(&serde_json::json!([
            {"mode": "content", "query": {"province": 1, "input": content_input()}}
        ]))
        .unwrap(),
    )
    .unwrap();
    let missing_base = report(
        command(&game, &source)
            .arg("--state-maintenance-queries")
            .arg(&queries)
            .output()
            .unwrap(),
    );
    assert_eq!(
        missing_base["state_maintenance_queries"][0]["evaluation"],
        serde_json::Value::Null
    );
    assert_eq!(
        missing_base["state_maintenance_queries"][0]["unresolved"][0],
        "state_maintenance:missing_input:base_tax_milli"
    );

    let missing_define = tempfile::tempdir().unwrap();
    let (define_game, define_source) = fixture(missing_define.path());
    let defines = define_game.join("common/defines.lua");
    let text = fs::read_to_string(&defines).unwrap();
    fs::write(
        &defines,
        text.replace("STATE_MAINTENANCE_DEV_FACTOR = 0.007,", ""),
    )
    .unwrap();
    let define_queries = missing_define.path().join("missing-define-queries.json");
    fs::write(
        &define_queries,
        serde_json::to_vec(&serde_json::json!([
            {"mode": "content", "query": {"province": 1, "input": content_input()}}
        ]))
        .unwrap(),
    )
    .unwrap();
    let missing_define = report(
        command(&define_game, &define_source)
            .arg("--state-maintenance-queries")
            .arg(&define_queries)
            .output()
            .unwrap(),
    );
    assert_eq!(
        missing_define["state_maintenance_queries"][0]["evaluation"],
        serde_json::Value::Null
    );
    assert_eq!(
        missing_define["state_maintenance_queries"][0]["unresolved"][0],
        "state_maintenance:missing_define:STATE_MAINTENANCE_DEV_FACTOR"
    );
}

#[test]
fn state_maintenance_query_unknown_fields_fail_before_snapshot_creation() {
    let temp = tempfile::tempdir().unwrap();
    let (game, source) = fixture(temp.path());
    let queries = temp.path().join("invalid-state-maintenance-queries.json");
    fs::write(
        &queries,
        serde_json::to_vec(&serde_json::json!([
            {
                "mode": "direct",
                "query": {"province": 1, "input": input(1000), "unexpected": true}
            }
        ]))
        .unwrap(),
    )
    .unwrap();
    let snapshot = temp.path().join("invalid.json");
    let output = command(&game, &source)
        .arg("--state-maintenance-queries")
        .arg(&queries)
        .arg("--save")
        .arg(&snapshot)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unknown field"));
    assert!(!snapshot.exists());
}
