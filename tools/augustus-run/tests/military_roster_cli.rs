// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::{fs, path::Path, process::Command};

use augustus_eu4::{VanillaExternalState, VanillaWorld};

fn example_world() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world")
}

fn external_state_path() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/military-roster/external_state.json")
}

fn external_state() -> VanillaExternalState {
    serde_json::from_slice(&fs::read(external_state_path()).unwrap()).unwrap()
}

fn command() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_augustus-world"));
    command
        .arg("--game-root")
        .arg(example_world())
        .arg("--external-state")
        .arg(external_state_path())
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
fn cli_military_output_matches_api_and_restored_snapshot_with_combined_filters() {
    let temp = tempfile::tempdir().unwrap();
    let baseline_path = temp.path().join("baseline.json");
    let baseline = report(
        command()
            .arg("--save")
            .arg(&baseline_path)
            .output()
            .unwrap(),
    );
    assert!(baseline.get("military").is_none());

    let queried_path = temp.path().join("queried.json");
    let queried = report(
        command()
            .arg("--military")
            .arg("--save")
            .arg(&queried_path)
            .output()
            .unwrap(),
    );
    let api =
        VanillaWorld::load_with_external_state(example_world(), "1444.11.11", &external_state())
            .unwrap();
    let expected = serde_json::to_value(api.military_roster(None, None).unwrap()).unwrap();
    assert_eq!(queried["military"], expected);
    assert_eq!(queried["simulation"], baseline["simulation"]);
    assert_eq!(
        fs::read(&baseline_path).unwrap(),
        fs::read(&queried_path).unwrap()
    );

    let restored = report(
        command()
            .arg("--load")
            .arg(&baseline_path)
            .arg("--military")
            .output()
            .unwrap(),
    );
    assert_eq!(restored["military"], expected);

    let filtered = report(
        command()
            .arg("--load")
            .arg(&baseline_path)
            .args(["--military", "--country", "AAA", "--province", "2"])
            .output()
            .unwrap(),
    );
    let filtered_api = api.military_roster(Some("AAA"), Some(2)).unwrap();
    assert_eq!(
        filtered["military"],
        serde_json::to_value(filtered_api).unwrap()
    );
}

#[test]
fn unknown_military_filters_fail_before_a_save_file_is_created() {
    let temp = tempfile::tempdir().unwrap();
    for (filter, value) in [("--country", "ZZZ"), ("--province", "999")] {
        let save = temp.path().join(format!("unknown-{filter}.json"));
        let output = command()
            .args(["--military", filter, value])
            .arg("--save")
            .arg(&save)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(!save.exists(), "invalid {filter} query wrote a snapshot");
    }
}
