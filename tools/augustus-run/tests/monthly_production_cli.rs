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

fn write_external_state(path: &Path) {
    fs::write(
        path,
        serde_json::json!({
            "country_resources": {
                "BBB": {"treasury_milli": 10000}
            },
            "production_inputs": {
                "1": {
                    "input": {
                        "base_value_milli": 1000,
                        "country_modifier_milli": 20,
                        "legacy_province_modifier_milli": 0,
                        "queried_province_modifier_milli": -10
                    },
                    "unresolved": []
                }
            }
        })
        .to_string(),
    )
    .unwrap();
}

#[test]
fn monthly_production_queries_are_read_only_and_preview_loaded_inputs() {
    let temp = tempfile::tempdir().unwrap();
    let game = fixture(temp.path());
    let external = temp.path().join("external-state.json");
    write_external_state(&external);
    let queries = temp.path().join("monthly-production-queries.json");
    fs::write(
        &queries,
        serde_json::to_vec(&serde_json::json!([{"country": "BBB"}])).unwrap(),
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
            .arg("--vanilla-monthly-production-queries")
            .arg(&queries)
            .arg("--save")
            .arg(&queried_path)
            .output()
            .unwrap(),
    );
    let evaluation = &output["vanilla_monthly_production_queries"][0]["evaluation"];
    assert_eq!(evaluation["annual_value_milli"], 1009);
    assert_eq!(evaluation["monthly_value_milli"], 84);
    assert_eq!(
        fs::read(baseline_path).unwrap(),
        fs::read(queried_path).unwrap()
    );
}

#[test]
fn monthly_production_query_requires_executor_and_strict_fields() {
    let temp = tempfile::tempdir().unwrap();
    let game = fixture(temp.path());
    let missing_queries = temp.path().join("missing-monthly-production.json");
    fs::write(
        &missing_queries,
        serde_json::to_vec(&serde_json::json!([{"country": "AAA"}])).unwrap(),
    )
    .unwrap();
    let missing_snapshot = temp.path().join("missing.json");
    let missing = command(&game, None)
        .arg("--vanilla-monthly-production-queries")
        .arg(&missing_queries)
        .arg("--save")
        .arg(&missing_snapshot)
        .output()
        .unwrap();
    assert!(!missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("missing monthly production"));
    assert!(!missing_snapshot.exists());

    let invalid_queries = temp.path().join("invalid-monthly-production.json");
    fs::write(
        &invalid_queries,
        serde_json::to_vec(&serde_json::json!([
            {"country": "AAA", "unexpected": true}
        ]))
        .unwrap(),
    )
    .unwrap();
    let invalid_snapshot = temp.path().join("invalid.json");
    let invalid = command(&game, None)
        .arg("--vanilla-monthly-production-queries")
        .arg(&invalid_queries)
        .arg("--save")
        .arg(&invalid_snapshot)
        .output()
        .unwrap();
    assert!(!invalid.status.success());
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("unknown field"));
    assert!(!invalid_snapshot.exists());
}
