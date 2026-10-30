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
    fs::write(
        game.join("common/buildings/test_buildings.txt"),
        "test_fort = { cost = 200 time = 18 influencing_fort = yes modifier = { fort_level = 2 } }\n",
    )
    .unwrap();
    let source = root.join("baseline.eu4");
    fs::write(
        &source,
        "EU4txt\ndate=1444.11.11\nplayer=AAA\nmultiplayer_random_seed=7\nmultiplayer_random_count=0\ncountries={AAA={} BBB={}}\nprovinces={-1={owner=BBB controller=BBB} -2={owner=AAA controller=AAA} -4={owner=AAA controller=AAA}}\n",
    )
    .unwrap();
    (game, source)
}

fn command(game: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_augustus-world"));
    command
        .arg("--game-root")
        .arg(game)
        .args(["--omit-load-report", "--days", "1"]);
    command
}

fn context() -> serde_json::Value {
    serde_json::json!({
        "country_modifier_milli": -200,
        "province_modifier_milli": 0,
        "relationship_modifier_milli": 0,
        "apply_relationship_modifier": true,
        "halve_for_special_case": false
    })
}

fn direct_input() -> serde_json::Value {
    serde_json::json!({
        "base_cost_milli": 500,
        "country_modifier_milli": -200,
        "province_modifier_milli": 0,
        "conversion_factor_micros": 1000000,
        "local_multiplier_milli": 2000,
        "relationship_modifier_milli": 0,
        "apply_relationship_modifier": true,
        "halve_for_special_case": false
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
fn fort_maintenance_queries_cover_direct_and_building_modes_without_mutation() {
    let temp = tempfile::tempdir().unwrap();
    let (game, _) = fixture(temp.path());
    let queries = temp.path().join("fort-maintenance-queries.json");
    fs::write(
        &queries,
        serde_json::to_vec(&serde_json::json!([
            {"mode": "direct", "query": {"province": 1, "input": direct_input()}},
            {"mode": "from_buildings", "query": {"province": 1, "context": context()}},
            {"mode": "from_buildings", "query": {"province": 2, "context": context()}}
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
            .arg("--fort-maintenance-queries")
            .arg(&queries)
            .arg("--save")
            .arg(&queried_path)
            .output()
            .unwrap(),
    );
    let reports = output["fort_maintenance_queries"].as_array().unwrap();
    assert_eq!(reports[0]["evaluation"]["amount_milli"], 800);
    assert_eq!(reports[1]["fort_level_milli"], 2_000);
    assert_eq!(reports[1]["evaluation"]["amount_milli"], 800);
    assert_eq!(reports[2]["evaluation"], serde_json::Value::Null);
    assert_eq!(
        reports[2]["unresolved"][0],
        "fort_maintenance:no_fortification_building"
    );
    assert_eq!(
        fs::read(baseline_path).unwrap(),
        fs::read(queried_path).unwrap()
    );
}

#[test]
fn fort_maintenance_query_unknown_fields_fail_before_snapshot_creation() {
    let temp = tempfile::tempdir().unwrap();
    let (game, _) = fixture(temp.path());
    let queries = temp.path().join("invalid-fort-maintenance-queries.json");
    fs::write(
        &queries,
        serde_json::to_vec(&serde_json::json!([
            {
                "mode": "direct",
                "query": {"province": 1, "input": direct_input(), "unexpected": true}
            }
        ]))
        .unwrap(),
    )
    .unwrap();
    let snapshot = temp.path().join("invalid.json");
    let output = command(&game)
        .arg("--fort-maintenance-queries")
        .arg(&queries)
        .arg("--save")
        .arg(&snapshot)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unknown field"));
    assert!(!snapshot.exists());
}
