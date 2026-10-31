// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::{fs, path::Path, process::Command};

fn game_fixture() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world")
}

fn external_state_fixture() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/economy/external_state.json")
}

fn command() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_augustus-world"));
    command
        .arg("--game-root")
        .arg(game_fixture())
        .arg("--external-state")
        .arg(external_state_fixture())
        .arg("--omit-load-report");
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
fn country_finances_is_opt_in_and_keeps_the_existing_economy_directory_output() {
    let default = report(command().output().unwrap());
    assert!(default.get("country_finances").is_none());
    assert!(default.get("economy").is_none());

    let missing_country = command().arg("--country-finances").output().unwrap();
    assert!(!missing_country.status.success());
    assert!(String::from_utf8_lossy(&missing_country.stderr).contains("--country"));

    let legacy = report(command().arg("--economy").output().unwrap());
    assert!(legacy["economy"]["buildings"].is_object());
    assert!(legacy["economy"]["trade_goods"].is_object());
    assert!(legacy["economy"]["trade_nodes"].is_array());
    assert!(legacy.get("country_finances").is_none());
}

#[test]
fn country_finances_runs_after_advance_restores_receipts_and_does_not_change_the_snapshot() {
    let temp = tempfile::tempdir().unwrap();
    let plain_snapshot = temp.path().join("plain.json");
    let queried_snapshot = temp.path().join("queried.json");
    let restored_snapshot = temp.path().join("restored.json");

    report(
        command()
            .args(["--days", "25", "--save"])
            .arg(&plain_snapshot)
            .output()
            .unwrap(),
    );
    let advanced = report(
        command()
            .args([
                "--days",
                "25",
                "--country",
                "AAA",
                "--country-finances",
                "--save",
            ])
            .arg(&queried_snapshot)
            .output()
            .unwrap(),
    );
    let finance = &advanced["country_finances"];
    assert_eq!(finance["country"], "AAA");
    assert_eq!(finance["day"], 25);
    assert_eq!(
        finance["date"],
        serde_json::json!({"year": 1444, "month": 12, "day": 6})
    );
    assert_eq!(finance["treasury_milli"], 11_250);
    assert_eq!(finance["tax"]["settlement_count"], 1);
    assert_eq!(finance["tax"]["last_receipt"]["day"], 20);
    assert_eq!(
        finance["tax"]["last_receipt"]["date"],
        serde_json::json!({"year": 1444, "month": 12, "day": 1})
    );
    assert_eq!(finance["tax"]["last_receipt"]["amount_milli"], 1_000);
    assert_eq!(finance["production"]["last_receipt"]["amount_milli"], 500);
    assert_eq!(finance["cashflow"]["last_receipt"]["amount_milli"], -250);
    assert_eq!(
        fs::read(&plain_snapshot).unwrap(),
        fs::read(&queried_snapshot).unwrap()
    );

    let restored = report(
        command()
            .arg("--load")
            .arg(&queried_snapshot)
            .args(["--country", "AAA", "--country-finances", "--save"])
            .arg(&restored_snapshot)
            .output()
            .unwrap(),
    );
    assert_eq!(restored["country_finances"], *finance);
    assert_eq!(
        fs::read(&queried_snapshot).unwrap(),
        fs::read(&restored_snapshot).unwrap()
    );
}

#[test]
fn invalid_country_finances_query_does_not_replace_an_existing_save_file() {
    let temp = tempfile::tempdir().unwrap();
    let sentinel = temp.path().join("existing-snapshot.json");
    let contents = b"keep this snapshot unchanged";
    fs::write(&sentinel, contents).unwrap();

    let output = command()
        .args(["--country", "ZZZ", "--country-finances", "--save"])
        .arg(&sentinel)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unknown economy country"));
    assert_eq!(fs::read(sentinel).unwrap(), contents);
}
