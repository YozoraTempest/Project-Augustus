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
        fs::read_to_string(&unit)
            .unwrap()
            .replace("type = light_ship", "type = galley"),
    )
    .unwrap();
    let defines = game.join("common/defines.lua");
    fs::write(
        &defines,
        format!(
            "{}\nNDefines.NMilitary.GALLEY_TIME=365\nNDefines.NMilitary.MIN_RECRUIT_TIME_MODIFIER=0.2\n",
            fs::read_to_string(&defines).unwrap()
        ),
    )
    .unwrap();
    let source = root.join("baseline.eu4");
    fs::write(&source, "EU4txt\ndate=1444.11.11\nplayer=AAA\nmultiplayer_random_seed=7\nmultiplayer_random_count=0\ncountries={AAA={} BBB={}}\nprovinces={-1={owner=AAA controller=AAA}}\n").unwrap();
    (game, source)
}

fn command(game: &Path, source: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_augustus-world"));
    command
        .arg("--game-root")
        .arg(game)
        .arg("--eu4-save")
        .arg(source)
        .args(["--days", "1", "--omit-load-report"]);
    command
}

#[test]
fn duration_queries_report_known_and_unknown_inputs_without_mutating_restored_state() {
    let temp = tempfile::tempdir().unwrap();
    let (game, source) = fixture(temp.path());
    let queries = temp.path().join("queries.json");
    let known = serde_json::json!({
        "country":"AAA", "province_id":1, "unit_type":"sample_galley",
        "country_modifier_milli":0, "province_modifier_milli":-30,
        "additional_province_modifier_milli":0
    });
    let mut changed = known.clone();
    changed["province_modifier_milli"] = (-40).into();
    let mut unknown = known.clone();
    unknown["country_modifier_milli"] = serde_json::Value::Null;
    let mut unsupported = known.clone();
    unsupported["unit_type"] = "sample_light_ship".into();
    fs::write(
        &queries,
        serde_json::to_vec(&serde_json::json!([known, changed, unknown, unsupported])).unwrap(),
    )
    .unwrap();

    let baseline = temp.path().join("baseline.json");
    let output = command(&game, &source)
        .arg("--save")
        .arg(&baseline)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let plain = temp.path().join("plain.json");
    let output = command(&game, &source)
        .arg("--load")
        .arg(&baseline)
        .arg("--save")
        .arg(&plain)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let queried = temp.path().join("queried.json");
    let output = command(&game, &source)
        .arg("--load")
        .arg(&baseline)
        .arg("--naval-construction-time-queries")
        .arg(&queries)
        .arg("--save")
        .arg(&queried)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let reports = &report["naval_construction_time_queries"];
    assert_eq!(report["simulation"]["day"], 2);
    assert_eq!(reports[0]["evaluation"]["duration_days"], 354);
    assert_eq!(reports[1]["evaluation"]["duration_days"], 350);
    assert!(reports[2]["evaluation"].is_null());
    assert!(!reports[2]["unresolved"].as_array().unwrap().is_empty());
    assert!(reports[3]["evaluation"].is_null());
    assert!(!reports[3]["unresolved"].as_array().unwrap().is_empty());
    assert_eq!(fs::read(queried).unwrap(), fs::read(plain).unwrap());
}

#[test]
fn invalid_duration_query_inputs_fail_before_creating_snapshot() {
    let temp = tempfile::tempdir().unwrap();
    let (game, source) = fixture(temp.path());
    let valid = serde_json::json!({
        "country":"AAA", "province_id":1, "unit_type":"sample_galley",
        "country_modifier_milli":0, "province_modifier_milli":-30,
        "additional_province_modifier_milli":0
    });
    let mut unknown_country = valid.clone();
    unknown_country["country"] = "ZZZ".into();
    let mut extra = valid.clone();
    extra["base_days"] = 1.into();
    for (name, input) in [
        (
            "country",
            serde_json::to_vec(&serde_json::json!([unknown_country])).unwrap(),
        ),
        (
            "override",
            serde_json::to_vec(&serde_json::json!([extra])).unwrap(),
        ),
        ("malformed", b"[".to_vec()),
    ] {
        let query = temp.path().join(format!("{name}.json"));
        let snapshot = temp.path().join(format!("{name}.save"));
        fs::write(&query, input).unwrap();
        let output = command(&game, &source)
            .arg("--naval-construction-time-queries")
            .arg(&query)
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
            "{}\nNDefines.NMilitary.GALLEY_TIME=365.5\n",
            fs::read_to_string(&defines).unwrap()
        ),
    )
    .unwrap();
    let output = command(&game, &source).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let query = temp.path().join("invalid-define.json");
    fs::write(
        &query,
        serde_json::to_vec(&serde_json::json!([valid])).unwrap(),
    )
    .unwrap();
    let snapshot = temp.path().join("invalid-define.save");
    let output = command(&game, &source)
        .arg("--naval-construction-time-queries")
        .arg(&query)
        .arg("--save")
        .arg(&snapshot)
        .output()
        .unwrap();
    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("GALLEY_TIME"), "{error}");
    assert!(error.contains("defines.lua"), "{error}");
    assert!(!snapshot.exists());
}
