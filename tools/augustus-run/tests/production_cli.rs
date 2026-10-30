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

fn production_input(
    base_value_milli: i32,
    country_modifier_milli: i32,
    legacy_province_modifier_milli: i32,
    queried_province_modifier_milli: i32,
) -> serde_json::Value {
    serde_json::json!({
        "input": {
            "base_value_milli": base_value_milli,
            "country_modifier_milli": country_modifier_milli,
            "legacy_province_modifier_milli": legacy_province_modifier_milli,
            "queried_province_modifier_milli": queried_province_modifier_milli
        },
        "unresolved": []
    })
}

fn gold_production_input(
    gold_size_milli: i32,
    mine_base_milli: i32,
    external_gold_size_milli: i32,
) -> serde_json::Value {
    serde_json::json!({
        "input": {
            "gold_size_milli": gold_size_milli,
            "mine_base_milli": mine_base_milli,
            "external_gold_size_milli": external_gold_size_milli
        },
        "unresolved": []
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
fn vanilla_production_queries_preserve_known_subtotals_and_snapshot_bytes() {
    let temp = tempfile::tempdir().unwrap();
    let (game, source) = fixture(temp.path());
    let queries = temp.path().join("production-queries.json");
    fs::write(
        &queries,
        serde_json::to_vec(&serde_json::json!([
            {
                "country": "AAA",
                "inputs": {
                    "2": production_input(2000, 20, 0, -20),
                    "4": production_input(500, 20, 0, 0)
                }
            },
            {
                "country": "AAA",
                "inputs": {
                    "2": production_input(2000, 20, 0, -20)
                }
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
    let output = report(
        command(&game, &source)
            .arg("--vanilla-production-queries")
            .arg(&queries)
            .arg("--save")
            .arg(&queried_path)
            .output()
            .unwrap(),
    );
    let reports = output["vanilla_production_queries"].as_array().unwrap();
    assert_eq!(reports[0]["country"], "AAA");
    assert_eq!(reports[0]["evaluation"]["known_annual_value_milli"], 2509);
    assert_eq!(reports[0]["evaluation"]["annual_value_milli"], 2509);
    assert_eq!(reports[1]["evaluation"]["known_annual_value_milli"], 1999);
    assert_eq!(
        reports[1]["evaluation"]["annual_value_milli"],
        serde_json::Value::Null
    );
    assert!(
        reports[1]["evaluation"]["unresolved"]
            .as_array()
            .unwrap()
            .iter()
            .any(|reason| reason == "province:4:missing_production_input")
    );
    assert_eq!(
        fs::read(baseline_path).unwrap(),
        fs::read(queried_path).unwrap()
    );
}

#[test]
fn vanilla_gold_production_queries_filter_gold_and_preserve_snapshot_bytes() {
    let temp = tempfile::tempdir().unwrap();
    let (game, source) = fixture(temp.path());
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
    fs::write(
        game.join("history/provinces/4 - Highland.txt"),
        "owner = AAA\ntrade_goods = gold",
    )
    .unwrap();
    let queries = temp.path().join("gold-production-queries.json");
    fs::write(
        &queries,
        serde_json::to_vec(&serde_json::json!([
            {
                "country": "AAA",
                "inputs": {
                    "2": gold_production_input(800, 39640, 0),
                    "4": gold_production_input(500, 39640, 0)
                }
            },
            {
                "country": "AAA",
                "inputs": {
                    "2": gold_production_input(800, 39640, 0)
                }
            }
        ]))
        .unwrap(),
    )
    .unwrap();

    let baseline_path = temp.path().join("gold-baseline.json");
    report(
        command(&game, &source)
            .arg("--save")
            .arg(&baseline_path)
            .output()
            .unwrap(),
    );
    let queried_path = temp.path().join("gold-queried.json");
    let output = report(
        command(&game, &source)
            .arg("--vanilla-gold-production-queries")
            .arg(&queries)
            .arg("--save")
            .arg(&queried_path)
            .output()
            .unwrap(),
    );
    let reports = output["vanilla_gold_production_queries"]
        .as_array()
        .unwrap();
    assert_eq!(reports[0]["evaluation"]["known_value_milli"], 51532);
    assert_eq!(reports[0]["evaluation"]["value_milli"], 51532);
    assert_eq!(reports[1]["evaluation"]["known_value_milli"], 31712);
    assert_eq!(
        reports[1]["evaluation"]["value_milli"],
        serde_json::Value::Null
    );
    assert!(
        reports[1]["evaluation"]["unresolved"]
            .as_array()
            .unwrap()
            .iter()
            .any(|reason| reason == "province:4:missing_gold_production_input")
    );
    assert_eq!(
        fs::read(baseline_path).unwrap(),
        fs::read(queried_path).unwrap()
    );
}

#[test]
fn vanilla_production_query_json_and_country_errors_prevent_snapshot_creation() {
    let temp = tempfile::tempdir().unwrap();
    let (game, source) = fixture(temp.path());
    let cases = [
        (
            "unknown-country",
            serde_json::json!([{
                "country": "ZZZ",
                "inputs": {}
            }]),
            "unknown country",
        ),
        (
            "unknown-field",
            serde_json::json!([{
                "country": "AAA",
                "inputs": {},
                "unexpected": true
            }]),
            "unknown field",
        ),
    ];
    for (name, input, message) in cases {
        let query = temp.path().join(format!("{name}.json"));
        fs::write(&query, serde_json::to_vec(&input).unwrap()).unwrap();
        let snapshot = temp.path().join(format!("{name}.save"));
        let output = command(&game, &source)
            .arg("--vanilla-production-queries")
            .arg(&query)
            .arg("--save")
            .arg(&snapshot)
            .output()
            .unwrap();
        assert!(!output.status.success(), "{name}");
        assert!(String::from_utf8_lossy(&output.stderr).contains(message));
        assert!(!snapshot.exists(), "{name}");
    }
}
