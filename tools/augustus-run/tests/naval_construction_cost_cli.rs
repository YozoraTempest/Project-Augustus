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
    let unit = game.join("common/units/sample_light_ship.txt");
    fs::write(
        game.join("common/units/sample_galley.txt"),
        fs::read_to_string(unit)
            .unwrap()
            .replace("type = light_ship", "type = galley"),
    )
    .unwrap();
    let defines = game.join("common/defines.lua");
    fs::write(
        &defines,
        format!(
            "{}\nNDefines.NMilitary.GALLEY_COST=10\n",
            fs::read_to_string(&defines).unwrap()
        ),
    )
    .unwrap();
    let map = game.join("map/default.map");
    fs::write(
        &map,
        format!(
            "{}\nforce_coastal={{1}}\n",
            fs::read_to_string(&map).unwrap()
        ),
    )
    .unwrap();
    let source = root.join("baseline.eu4");
    fs::write(&source, "EU4txt\ndate=1444.11.11\nplayer=AAA\nmultiplayer_random_seed=7\nmultiplayer_random_count=0\ncountries={AAA={treasury=20.000} BBB={}}\nprovinces={-1={owner=AAA controller=AAA}}\n").unwrap();
    (game, source)
}

fn query() -> serde_json::Value {
    serde_json::json!({
        "country":"AAA", "province_id":1, "unit_type":"sample_galley",
        "country_modifier_milli":0, "additional_country_modifier_milli":0,
        "province_modifier_milli":0, "naval_quantity_milli":10000, "naval_limit_milli":17000
    })
}

fn command(game: &Path, source: &Path, days: u32) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_augustus-world"));
    command
        .arg("--game-root")
        .arg(game)
        .arg("--eu4-save")
        .arg(source)
        .args(["--country", "AAA", "--omit-load-report", "--days"])
        .arg(days.to_string());
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
fn cost_queries_preserve_restored_snapshots_and_report_unsupported_contexts() {
    let temp = tempfile::tempdir().unwrap();
    let (game, source) = fixture(temp.path());
    let mut modified = query();
    modified["country_modifier_milli"] = 100.into();
    modified["additional_country_modifier_milli"] = 200.into();
    modified["province_modifier_milli"] = (-50).into();
    let mut over_limit = query();
    over_limit["naval_quantity_milli"] = 18000.into();
    let mut missing = query();
    missing
        .as_object_mut()
        .unwrap()
        .remove("country_modifier_milli");
    let mut zero_limit = query();
    zero_limit["naval_limit_milli"] = 0.into();
    let mut unsupported = query();
    unsupported["unit_type"] = "sample_light_ship".into();
    let queries = temp.path().join("queries.json");
    fs::write(
        &queries,
        serde_json::to_vec(&serde_json::json!([
            query(),
            modified,
            over_limit,
            missing,
            zero_limit,
            unsupported
        ]))
        .unwrap(),
    )
    .unwrap();
    let baseline = temp.path().join("baseline.json");
    report(
        command(&game, &source, 0)
            .arg("--save")
            .arg(&baseline)
            .output()
            .unwrap(),
    );
    let plain = temp.path().join("plain.json");
    report(
        command(&game, &source, 1)
            .arg("--load")
            .arg(&baseline)
            .arg("--save")
            .arg(&plain)
            .output()
            .unwrap(),
    );
    let queried = temp.path().join("queried.json");
    let output = report(
        command(&game, &source, 1)
            .arg("--load")
            .arg(&baseline)
            .arg("--naval-construction-cost-queries")
            .arg(&queries)
            .arg("--save")
            .arg(&queried)
            .output()
            .unwrap(),
    );
    assert_eq!(output["simulation"]["day"], 1);
    let evaluations = output["naval_construction_cost_queries"]
        .as_array()
        .unwrap();
    assert_eq!(evaluations[0]["evaluation"]["cost_milli"], 10000);
    assert_eq!(evaluations[1]["evaluation"]["cost_milli"], 12500);
    for evaluation in &evaluations[2..] {
        assert!(evaluation["evaluation"].is_null());
        assert!(!evaluation["unresolved"].as_array().unwrap().is_empty());
    }
    assert_eq!(fs::read(queried).unwrap(), fs::read(plain).unwrap());
}

#[test]
fn resolved_cost_can_be_consumed_by_an_explicit_order_without_duplicate_charge() {
    let temp = tempfile::tempdir().unwrap();
    let (game, source) = fixture(temp.path());
    let queries = temp.path().join("query.json");
    fs::write(
        &queries,
        serde_json::to_vec(&serde_json::json!([query()])).unwrap(),
    )
    .unwrap();
    let quoted = report(
        command(&game, &source, 0)
            .arg("--naval-construction-cost-queries")
            .arg(&queries)
            .output()
            .unwrap(),
    );
    assert_eq!(quoted["country"]["resources"]["treasury_milli"], 20000);
    let requests = temp.path().join("requests.json");
    fs::write(
        &requests,
        serde_json::to_vec(&serde_json::json!([{
            "country":"AAA", "province_id":1, "unit_type":"sample_galley", "completion_day":3,
            "cost_milli":quoted["naval_construction_cost_queries"][0]["evaluation"]["cost_milli"]
        }]))
        .unwrap(),
    )
    .unwrap();
    let pending = temp.path().join("pending.json");
    let submitted = report(
        command(&game, &source, 1)
            .arg("--naval-constructions")
            .arg(&requests)
            .arg("--save")
            .arg(&pending)
            .output()
            .unwrap(),
    );
    assert_eq!(submitted["country"]["resources"]["treasury_milli"], 10000);
    assert_eq!(submitted["simulation"]["pending_naval_constructions"], 1);
    let completed = report(
        command(&game, &source, 2)
            .arg("--load")
            .arg(&pending)
            .output()
            .unwrap(),
    );
    assert_eq!(completed["country"]["resources"]["treasury_milli"], 10000);
    assert_eq!(completed["simulation"]["pending_naval_constructions"], 0);
    assert_eq!(completed["simulation"]["naval_units"], 1);
}

#[test]
fn invalid_cost_queries_and_defines_fail_before_snapshot_creation() {
    let temp = tempfile::tempdir().unwrap();
    let (game, source) = fixture(temp.path());
    let mut unknown = query();
    unknown["country"] = "ZZZ".into();
    let mut negative = query();
    negative["naval_quantity_milli"] = (-1).into();
    let mut extra = query();
    extra["base_cost_milli"] = 1.into();
    for (name, input) in [
        (
            "unknown",
            serde_json::to_vec(&serde_json::json!([unknown])).unwrap(),
        ),
        (
            "negative",
            serde_json::to_vec(&serde_json::json!([negative])).unwrap(),
        ),
        (
            "override",
            serde_json::to_vec(&serde_json::json!([extra])).unwrap(),
        ),
        ("malformed", b"[".to_vec()),
    ] {
        let path = temp.path().join(format!("{name}.json"));
        let snapshot = temp.path().join(format!("{name}.save"));
        fs::write(&path, input).unwrap();
        let output = command(&game, &source, 0)
            .arg("--naval-construction-cost-queries")
            .arg(&path)
            .arg("--save")
            .arg(&snapshot)
            .output()
            .unwrap();
        assert!(!output.status.success(), "{name}");
        assert!(!snapshot.exists(), "{name}");
    }
    let defines = game.join("common/defines.lua");
    fs::write(
        &defines,
        format!(
            "{}\nNDefines.NMilitary.GALLEY_COST=-0.0000001\n",
            fs::read_to_string(&defines).unwrap()
        ),
    )
    .unwrap();
    report(command(&game, &source, 0).output().unwrap());
    let path = temp.path().join("invalid-define.json");
    fs::write(
        &path,
        serde_json::to_vec(&serde_json::json!([query()])).unwrap(),
    )
    .unwrap();
    let snapshot = temp.path().join("invalid-define.save");
    let output = command(&game, &source, 0)
        .arg("--naval-construction-cost-queries")
        .arg(&path)
        .arg("--save")
        .arg(&snapshot)
        .output()
        .unwrap();
    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("GALLEY_COST"), "{error}");
    assert!(error.contains("defines.lua"), "{error}");
    assert!(!snapshot.exists());
}
