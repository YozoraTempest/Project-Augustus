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

fn check_fifo_submission(classified: bool) {
    let temp = tempfile::tempdir().unwrap();
    let game = temp.path().join("game");
    copy_directory(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world"),
        &game,
    );
    let map = game.join("map/default.map");
    fs::write(
        &map,
        format!(
            "{}\nforce_coastal={{1 2}}\n",
            fs::read_to_string(&map).unwrap()
        ),
    )
    .unwrap();
    let light = fs::read_to_string(game.join("common/units/sample_light_ship.txt")).unwrap();
    fs::write(
        game.join("common/units/sample_heavy_ship.txt"),
        light.replace("type = light_ship", "type = heavy_ship"),
    )
    .unwrap();
    let source = temp.path().join("baseline.eu4");
    fs::write(&source, "EU4txt\ndate=1444.11.11\nplayer=AAA\ncountries={AAA={treasury=20.000} BBB={}}\nprovinces={-1={owner=AAA controller=AAA} -2={owner=AAA controller=AAA}}\n").unwrap();
    let requests = temp.path().join("requests.json");
    fs::write(&requests, serde_json::to_vec(&serde_json::json!([
        {"country":"AAA","province_id":1,"unit_type":"sample_light_ship","completion_day":3,"cost_milli":5000},
        {"country":"AAA","province_id":2,"unit_type":"sample_light_ship","completion_day":2,"cost_milli":1000},
        {"country":"AAA","province_id":1,"unit_type":"sample_heavy_ship","completion_day":2,"cost_milli":2000}
    ])).unwrap()).unwrap();
    let command = || {
        let mut command = Command::new(env!("CARGO_BIN_EXE_augustus-world"));
        command
            .arg("--game-root")
            .arg(&game)
            .arg("--eu4-save")
            .arg(&source)
            .args(["--country", "AAA", "--show-naval-constructions"]);
        command
    };
    let run = |days: u32, submit: bool, load: Option<&Path>, save: Option<&Path>| {
        let mut command = command();
        command.arg("--days").arg(days.to_string());
        if submit {
            command.arg("--naval-construction-queues").arg(&requests);
            if classified {
                command.arg("--naval-construction-ledger");
            }
        }
        if let Some(path) = load {
            command.arg("--load").arg(path);
        }
        if let Some(path) = save {
            command.arg("--save").arg(path);
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap()
    };
    let baseline = run(0, false, None, None);
    let pending_path = temp.path().join("pending.json");
    let pending = run(2, true, None, Some(&pending_path));
    assert_eq!(
        pending["submitted_naval_constructions"],
        serde_json::json!([0, 1, 2])
    );
    assert_eq!(pending["country"]["resources"]["treasury_milli"], 12_000);
    assert_eq!(pending["simulation"]["naval_units"], 1);
    assert_eq!(
        pending["naval_construction_orders"],
        serde_json::json!([
            {"id":0,"country":"AAA","province_id":1,"unit_type":"sample_light_ship",
             "completion_day":3,"completion_date":{"year":1444,"month":11,"day":14},"queue_position":1},
            {"id":2,"country":"AAA","province_id":1,"unit_type":"sample_heavy_ship",
             "completion_day":4,"completion_date":{"year":1444,"month":11,"day":15},"queue_position":2}
        ])
    );
    if classified {
        for key in [
            "expense_sources_milli",
            "expense_accumulator_milli",
            "expense_auxiliary_milli",
        ] {
            assert_eq!(pending["country"]["ledger"][key][12], 8_000);
        }
        assert_eq!(pending["country"]["ledger"]["pending_milli"], 0);
    } else {
        assert_eq!(
            pending["country"].get("ledger"),
            baseline["country"].get("ledger")
        );
    }

    let pending_bytes = fs::read(&pending_path).unwrap();
    let rejected_path = temp.path().join("rejected.json");
    let mut rejected = command();
    rejected
        .arg("--load")
        .arg(&pending_path)
        .arg("--naval-construction-queues")
        .arg(&requests)
        .arg("--save")
        .arg(&rejected_path);
    if classified {
        rejected.arg("--naval-construction-ledger");
    }
    let rejected = rejected.output().unwrap();
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("pending naval order"));
    assert!(!rejected_path.exists());
    assert_eq!(fs::read(&pending_path).unwrap(), pending_bytes);

    let head_path = temp.path().join("head.json");
    let head = run(1, false, Some(&pending_path), Some(&head_path));
    assert!(head.get("submitted_naval_constructions").is_none());
    assert_eq!(head["simulation"]["naval_units"], 2);
    assert_eq!(head["naval_construction_orders"][0]["id"], 2);
    assert_eq!(head["naval_construction_orders"][0]["queue_position"], 1);
    assert_eq!(
        head["naval_construction_orders"].as_array().unwrap().len(),
        1
    );

    let final_path = temp.path().join("final.json");
    let complete = run(1, false, Some(&head_path), Some(&final_path));
    assert_eq!(complete["simulation"]["naval_units"], 3);
    assert_eq!(complete["naval_construction_orders"], serde_json::json!([]));
    assert_eq!(complete["country"]["resources"]["treasury_milli"], 12_000);
    if classified {
        assert_eq!(complete["country"]["ledger"], pending["country"]["ledger"]);
    }
    let continuous_path = temp.path().join("continuous.json");
    run(4, true, None, Some(&continuous_path));
    assert_eq!(
        fs::read(&final_path).unwrap(),
        fs::read(&continuous_path).unwrap()
    );
    let later = run(2, false, Some(&final_path), None);
    assert_eq!(later["simulation"]["naval_units"], 3);
    assert_eq!(later["country"]["resources"]["treasury_milli"], 12_000);

    let broken_requests = temp.path().join("broken.json");
    let mut broken: serde_json::Value =
        serde_json::from_slice(&fs::read(&requests).unwrap()).unwrap();
    broken[2]["cost_milli"] = serde_json::json!(-1);
    fs::write(&broken_requests, serde_json::to_vec(&broken).unwrap()).unwrap();
    let failed_path = temp.path().join("failed.json");
    let mut failed = command();
    failed
        .arg("--naval-construction-queues")
        .arg(&broken_requests)
        .arg("--save")
        .arg(&failed_path);
    if classified {
        failed.arg("--naval-construction-ledger");
    }
    let failed = failed.output().unwrap();
    assert!(!failed.status.success());
    assert!(String::from_utf8_lossy(&failed.stderr).contains("nonnegative"));
    assert!(!failed_path.exists());
}

#[test]
fn priced_fifo_requests_resume_without_duplicate_ships_or_charges() {
    check_fifo_submission(false);
}

#[test]
fn classified_fifo_requests_resume_without_duplicate_ledger_expenses() {
    check_fifo_submission(true);
}

#[test]
fn submission_modes_are_exclusive_and_ledger_requires_a_request_file() {
    let game = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world");
    let temp = tempfile::tempdir().unwrap();
    for (name, arguments, expected) in [
        (
            "both",
            vec![
                "--naval-constructions",
                "missing-single.json",
                "--naval-construction-queues",
                "missing-queue.json",
            ],
            "cannot be used with",
        ),
        (
            "ledger-alone",
            vec!["--naval-construction-ledger"],
            "required arguments were not provided",
        ),
    ] {
        let path = temp.path().join(format!("{name}.json"));
        let output = Command::new(env!("CARGO_BIN_EXE_augustus-world"))
            .arg("--game-root")
            .arg(&game)
            .args(arguments)
            .arg("--save")
            .arg(&path)
            .output()
            .unwrap();
        assert!(!output.status.success());
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(error.contains(expected), "{error}");
        assert!(error.contains("--naval-constructions"), "{error}");
        assert!(error.contains("--naval-construction-queues"), "{error}");
        assert!(!path.exists());
    }
}
