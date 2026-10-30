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

fn command(game: &Path, source: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_augustus-world"));
    command
        .arg("--game-root")
        .arg(game)
        .arg("--eu4-save")
        .arg(source)
        .args(["--country", "AAA", "--omit-load-report", "--days", "0"]);
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
fn land_force_limit_queries_use_explicit_inputs_and_preserve_snapshot_bytes() {
    let temp = tempfile::tempdir().unwrap();
    let game = temp.path().join("game");
    copy_directory(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world"),
        &game,
    );
    let source = temp.path().join("baseline.eu4");
    fs::write(
        &source,
        "EU4txt\ndate=1444.11.11\nplayer=AAA\ncountries={AAA={} BBB={}}\nprovinces={-1={owner=AAA controller=AAA}}\n",
    )
    .unwrap();
    let queries = temp.path().join("land-force-limit-queries.json");
    fs::write(
        &queries,
        serde_json::to_vec(&serde_json::json!([
            {
                "country": "AAA",
                "base_milli": 17000,
                "flat_modifier_milli": 746,
                "percentage_modifier_milli": 0
            },
            {
                "country": "AAA",
                "base_milli": 17000,
                "flat_modifier_milli": null,
                "percentage_modifier_milli": 0
            }
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
    let queried = report(
        command(&game, &source)
            .arg("--land-force-limit-queries")
            .arg(&queries)
            .arg("--save")
            .arg(&queried_path)
            .output()
            .unwrap(),
    );
    assert_eq!(
        queried["land_force_limit_queries"][0]["evaluation"]["force_limit_milli"],
        17_000
    );
    assert_eq!(
        queried["land_force_limit_queries"][1]["evaluation"],
        serde_json::Value::Null
    );
    assert_eq!(
        queried["land_force_limit_queries"][1]["unresolved"][0],
        "land_force_limit:missing_input:flat_modifier_milli"
    );
    assert_eq!(
        fs::read(baseline_path).unwrap(),
        fs::read(queried_path).unwrap()
    );
}
