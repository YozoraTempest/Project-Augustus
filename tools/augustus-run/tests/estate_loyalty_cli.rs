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
    let defines = fs::read_to_string(game.join("common/defines.lua")).unwrap();
    fs::write(
        game.join("common/defines.lua"),
        defines.replace(
            "NCountry = {",
            "NCountry = {\n        ESTATE_ANGRY_THRESHOLD = 30,\n        ESTATE_HAPPY_THRESHOLD = 60,\n        ESTATE_LOYALTY_DECAY_BASE_MIN = 2.0,\n        ESTATE_LOYALTY_DECAY_BASE_MAX = 5.0,",
        ),
    )
    .unwrap();
    fs::create_dir_all(game.join("common/estates")).unwrap();
    fs::write(
        game.join("common/estates/test.txt"),
        "estate_guild = { trigger = { tag = AAA } base_influence = 12 }",
    )
    .unwrap();
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
fn estate_loyalty_queries_preview_the_next_daily_step_without_mutation() {
    let temp = tempfile::tempdir().unwrap();
    let game = fixture(temp.path());
    let state = temp.path().join("state.json");
    fs::write(
        &state,
        serde_json::to_vec(&serde_json::json!({
            "estate_states": {
                "AAA": {
                    "estate_guild": {
                        "loyalty_milli": 49615,
                        "land_share_micros": 150000
                    }
                }
            },
            "estate_loyalty_targets": {
                "AAA": {"estate_guild": 44600}
            }
        }))
        .unwrap(),
    )
    .unwrap();
    let queries = temp.path().join("estate-loyalty-queries.json");
    fs::write(
        &queries,
        serde_json::to_vec(&serde_json::json!([
            {"country": "AAA", "estate": "estate_guild"},
            {"country": "AAA"}
        ]))
        .unwrap(),
    )
    .unwrap();

    let baseline_path = temp.path().join("baseline.json");
    report(
        command(&game)
            .arg("--external-state")
            .arg(&state)
            .arg("--save")
            .arg(&baseline_path)
            .output()
            .unwrap(),
    );
    let queried_path = temp.path().join("queried.json");
    let output = report(
        command(&game)
            .arg("--external-state")
            .arg(&state)
            .arg("--estate-loyalty-queries")
            .arg(&queries)
            .arg("--save")
            .arg(&queried_path)
            .output()
            .unwrap(),
    );
    let reports = output["estate_loyalty_queries"].as_array().unwrap();
    assert_eq!(reports[0]["entries"][0]["active"], true);
    assert_eq!(
        reports[0]["entries"][0]["evaluation"]["current_loyalty_milli"],
        49615
    );
    assert_eq!(
        reports[0]["entries"][0]["evaluation"]["target_loyalty_milli"],
        44600
    );
    assert_eq!(
        reports[0]["entries"][0]["evaluation"]["next_loyalty_milli"],
        49424
    );
    assert_eq!(reports[1]["entries"].as_array().unwrap().len(), 1);
    assert_eq!(
        fs::read(baseline_path).unwrap(),
        fs::read(queried_path).unwrap()
    );
}

#[test]
fn estate_loyalty_query_unknown_fields_fail_before_snapshot_creation() {
    let temp = tempfile::tempdir().unwrap();
    let game = fixture(temp.path());
    let queries = temp.path().join("invalid-estate-loyalty-queries.json");
    fs::write(
        &queries,
        serde_json::to_vec(&serde_json::json!([
            {"country": "AAA", "estate": "estate_guild", "unexpected": true}
        ]))
        .unwrap(),
    )
    .unwrap();
    let snapshot = temp.path().join("invalid.json");
    let output = command(&game)
        .arg("--estate-loyalty-queries")
        .arg(&queries)
        .arg("--save")
        .arg(&snapshot)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unknown field"));
    assert!(!snapshot.exists());
}
