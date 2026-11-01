// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::{ffi::OsStr, fs, path::Path, process::Command};

use augustus_core::{EntityId, EventChoice};
use augustus_demo::{EventDemo, Input};
use serde_json::Value;

fn content() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/live-occupation")
}

fn run_cli(content: &Path, arguments: &[&OsStr]) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_augustus-events"))
        .arg("--content")
        .arg(content)
        .arg("--input")
        .arg("native")
        .args(arguments)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn snapshot(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

fn tax_account(saved: &Value) -> &Value {
    &saved["taxation"]["last_settlement"]["accounts"]["0"]
}

fn has_log(view: &Value, message: &str) -> bool {
    view["log"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry[2] == message)
}

#[test]
fn occupation_changes_live_tax_and_save_resume_matches_continuous_execution() {
    let content = content();
    let temp = tempfile::tempdir().unwrap();
    let pending_path = temp.path().join("pending.json");
    let occupied_path = temp.path().join("occupied.json");
    let december_path = temp.path().join("december.json");
    let january_path = temp.path().join("january.json");

    let mut continuous = EventDemo::from_directory(&content, Input::Native).unwrap();
    let initial_snapshot = continuous.save().unwrap();
    assert_eq!(
        continuous
            .world()
            .province_controller_state(EntityId(2))
            .unwrap(),
        None
    );
    let initial_world = continuous.world();
    let unknown_tax = initial_world
        .attributes()
        .evaluate(initial_world, EntityId(2), "annual_tax")
        .unwrap();
    assert_eq!(unknown_tax.base_micros, Some(12_000_000));
    assert_eq!(unknown_tax.known_subtotal_micros, None);
    assert_eq!(unknown_tax.value_micros, None);
    assert!(!unknown_tax.unresolved.is_empty());
    let unknown = continuous.world().preview_monthly_tax().unwrap().accounts[&EntityId(0)].clone();
    assert_eq!(unknown.known_subtotal_milli, 0);
    assert!(unknown.income_milli.is_none());
    assert!(unknown.credited_milli.is_none());
    assert!(!unknown.unresolved.is_empty());
    assert_eq!(continuous.save().unwrap(), initial_snapshot);

    let pending_id = continuous.trigger("live_occupation.tax", 0).unwrap();
    continuous.advance(0).unwrap();
    assert_eq!(
        continuous
            .world()
            .province_controller_state(EntityId(2))
            .unwrap(),
        Some(Some(EntityId(0)))
    );
    let established =
        continuous.world().preview_monthly_tax().unwrap().accounts[&EntityId(0)].clone();
    assert_eq!(established.income_milli, Some(1_000));
    assert!(established.unresolved.is_empty());

    let pending_view = run_cli(
        &content,
        &[
            OsStr::new("--trigger"),
            OsStr::new("live_occupation.tax"),
            OsStr::new("--save"),
            pending_path.as_os_str(),
        ],
    );
    assert_eq!(
        pending_view["pending_events"][0]["options"],
        serde_json::json!(["0", "1"])
    );
    assert!(has_log(&pending_view, "controller-established-as-aaa"));
    assert_eq!(
        fs::read(&pending_path).unwrap(),
        continuous.save().unwrap().as_bytes()
    );

    let occupied_view = run_cli(
        &content,
        &[
            OsStr::new("--load"),
            pending_path.as_os_str(),
            OsStr::new("--choose"),
            OsStr::new("0:0"),
            OsStr::new("--save"),
            occupied_path.as_os_str(),
        ],
    );
    continuous
        .choose(&EventChoice {
            pending: pending_id,
            option: "0".into(),
        })
        .unwrap();
    assert!(has_log(&occupied_view, "province-occupied-by-bbb"));
    let occupied_snapshot = snapshot(&occupied_path);
    assert_eq!(occupied_snapshot["entities"][2]["owner"], 0);
    assert_eq!(occupied_snapshot["province_controllers"]["2"], 1);
    assert!(
        occupied_snapshot
            .get("province_previous_controllers")
            .is_none()
    );
    assert_eq!(
        fs::read(&occupied_path).unwrap(),
        continuous.save().unwrap().as_bytes()
    );

    let december_view = run_cli(
        &content,
        &[
            OsStr::new("--load"),
            occupied_path.as_os_str(),
            OsStr::new("--days"),
            OsStr::new("1"),
            OsStr::new("--save"),
            december_path.as_os_str(),
        ],
    );
    continuous.advance(1).unwrap();
    assert_eq!(
        december_view["country_resources"]["0"]["treasury_milli"],
        500
    );
    assert_eq!(december_view["country_resources"]["1"]["treasury_milli"], 0);
    assert!(!has_log(&december_view, "controller-returned-to-aaa"));
    let december_snapshot = snapshot(&december_path);
    assert_eq!(tax_account(&december_snapshot)["known_subtotal_milli"], 500);
    assert_eq!(tax_account(&december_snapshot)["income_milli"], 500);
    assert_eq!(tax_account(&december_snapshot)["credited_milli"], 500);
    assert_eq!(tax_account(&december_snapshot)["treasury_after_milli"], 500);
    assert!(
        tax_account(&december_snapshot)["unresolved"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        fs::read(&december_path).unwrap(),
        continuous.save().unwrap().as_bytes()
    );

    let january_view = run_cli(
        &content,
        &[
            OsStr::new("--load"),
            december_path.as_os_str(),
            OsStr::new("--days"),
            OsStr::new("31"),
            OsStr::new("--save"),
            january_path.as_os_str(),
        ],
    );
    continuous.advance(31).unwrap();
    assert_eq!(
        january_view["country_resources"]["0"]["treasury_milli"],
        1_500
    );
    assert_eq!(january_view["country_resources"]["1"]["treasury_milli"], 0);
    assert!(has_log(&january_view, "controller-returned-to-aaa"));
    assert_eq!(
        january_view,
        serde_json::to_value(continuous.view().unwrap()).unwrap()
    );
    let january_snapshot = snapshot(&january_path);
    assert_eq!(
        tax_account(&january_snapshot)["known_subtotal_milli"],
        1_000
    );
    assert_eq!(tax_account(&january_snapshot)["income_milli"], 1_000);
    assert_eq!(tax_account(&january_snapshot)["credited_milli"], 1_000);
    assert_eq!(tax_account(&january_snapshot)["treasury_before_milli"], 500);
    assert_eq!(
        tax_account(&january_snapshot)["treasury_after_milli"],
        1_500
    );
    assert!(
        tax_account(&january_snapshot)["unresolved"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        fs::read(&january_path).unwrap(),
        continuous.save().unwrap().as_bytes()
    );
    assert_eq!(continuous.world().taxation().unwrap().settlement_count, 2);
}

#[test]
fn known_absent_controller_uses_unoccupied_tax_branch() {
    let content = content();
    let temp = tempfile::tempdir().unwrap();
    let pending_path = temp.path().join("pending.json");
    let absent_path = temp.path().join("absent.json");
    let december_path = temp.path().join("absent-december.json");

    run_cli(
        &content,
        &[
            OsStr::new("--trigger"),
            OsStr::new("live_occupation.tax"),
            OsStr::new("--save"),
            pending_path.as_os_str(),
        ],
    );
    let absent_view = run_cli(
        &content,
        &[
            OsStr::new("--load"),
            pending_path.as_os_str(),
            OsStr::new("--choose"),
            OsStr::new("0:1"),
            OsStr::new("--save"),
            absent_path.as_os_str(),
        ],
    );
    assert!(has_log(&absent_view, "controller-marked-known-absent"));
    let absent_snapshot = snapshot(&absent_path);
    assert!(absent_snapshot["province_controllers"]["2"].is_null());
    assert_eq!(absent_snapshot["entities"][2]["owner"], 0);
    assert!(
        absent_snapshot
            .get("province_previous_controllers")
            .is_none()
    );

    let mut direct = EventDemo::from_directory(&content, Input::Native).unwrap();
    direct
        .restore(&fs::read_to_string(&pending_path).unwrap())
        .unwrap();
    direct
        .choose(&EventChoice {
            pending: augustus_core::PendingEventId(0),
            option: "1".into(),
        })
        .unwrap();
    assert_eq!(
        direct
            .world()
            .province_controller_state(EntityId(2))
            .unwrap(),
        Some(None)
    );
    let unoccupied = direct.world().preview_monthly_tax().unwrap().accounts[&EntityId(0)].clone();
    assert_eq!(unoccupied.known_subtotal_milli, 1_000);
    assert_eq!(unoccupied.income_milli, Some(1_000));
    assert_eq!(unoccupied.credited_milli, Some(1_000));
    assert!(unoccupied.unresolved.is_empty());
    assert_eq!(
        fs::read(&absent_path).unwrap(),
        direct.save().unwrap().as_bytes()
    );

    let december_view = run_cli(
        &content,
        &[
            OsStr::new("--load"),
            absent_path.as_os_str(),
            OsStr::new("--days"),
            OsStr::new("1"),
            OsStr::new("--save"),
            december_path.as_os_str(),
        ],
    );
    direct.advance(1).unwrap();
    assert_eq!(
        december_view["country_resources"]["0"]["treasury_milli"],
        1_000
    );
    assert_eq!(december_view["country_resources"]["1"]["treasury_milli"], 0);
    assert_eq!(
        fs::read(&december_path).unwrap(),
        direct.save().unwrap().as_bytes()
    );
}
