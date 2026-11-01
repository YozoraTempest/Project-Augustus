// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::{ffi::OsStr, fs, path::Path, process::Command};

use augustus_core::{EntityId, EventChoice};
use augustus_demo::{EventDemo, Input};
use serde_json::Value;

fn run_cli(arguments: &[&OsStr]) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_augustus-events"))
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

fn has_log(view: &Value, message: &str) -> bool {
    view["log"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry[2] == message)
}

fn snapshot(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

fn assert_owner_and_unknown_previous_controller(saved: &Value) {
    assert_eq!(saved["entities"][2]["owner"], 0);
    assert!(saved.get("province_previous_controllers").is_none());
}

#[test]
fn native_controller_action_saves_resumes_and_matches_continuous_execution() {
    let content =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/province-controller-actions");
    let temp = tempfile::tempdir().unwrap();
    let pending_path = temp.path().join("controller-pending.json");
    let bbb_path = temp.path().join("controller-bbb.json");
    let aaa_path = temp.path().join("controller-aaa.json");

    let pending = run_cli(&[
        OsStr::new("--content"),
        content.as_os_str(),
        OsStr::new("--input"),
        OsStr::new("native"),
        OsStr::new("--trigger"),
        OsStr::new("province_controller_actions.change"),
        OsStr::new("--save"),
        pending_path.as_os_str(),
    ]);
    assert_eq!(
        pending["pending_events"][0]["options"],
        serde_json::json!(["0", "1"])
    );
    assert!(has_log(&pending, "controller-established"));
    let pending_snapshot = snapshot(&pending_path);
    assert_eq!(pending_snapshot["province_controllers"]["2"], 0);
    assert_owner_and_unknown_previous_controller(&pending_snapshot);

    let mut continuous = EventDemo::from_directory(&content, Input::Native).unwrap();
    let pending_id = continuous
        .trigger("province_controller_actions.change", 0)
        .unwrap();
    continuous.advance(0).unwrap();
    assert_eq!(
        continuous
            .world()
            .province_controller_state(EntityId(2))
            .unwrap(),
        Some(Some(EntityId(0)))
    );
    assert_eq!(
        continuous
            .world()
            .province_previous_controller_state(EntityId(2))
            .unwrap(),
        None
    );
    assert_eq!(
        fs::read(&pending_path).unwrap(),
        continuous.save().unwrap().as_bytes()
    );

    let changed = run_cli(&[
        OsStr::new("--content"),
        content.as_os_str(),
        OsStr::new("--input"),
        OsStr::new("native"),
        OsStr::new("--load"),
        pending_path.as_os_str(),
        OsStr::new("--choose"),
        OsStr::new("0:0"),
        OsStr::new("--save"),
        bbb_path.as_os_str(),
    ]);
    assert_eq!(changed["pending_tasks"], 1);
    assert!(has_log(&changed, "controller-changed-to-bbb"));
    assert!(has_log(&changed, "controller-choice-complete"));
    assert_eq!(snapshot(&bbb_path)["province_controllers"]["2"], 1);
    assert_owner_and_unknown_previous_controller(&snapshot(&bbb_path));

    continuous
        .choose(&EventChoice {
            pending: pending_id,
            option: "0".into(),
        })
        .unwrap();
    assert_eq!(
        continuous
            .world()
            .province_controller_state(EntityId(2))
            .unwrap(),
        Some(Some(EntityId(1)))
    );
    assert_eq!(
        serde_json::to_value(continuous.view().unwrap()).unwrap(),
        changed
    );
    assert_eq!(
        fs::read(&bbb_path).unwrap(),
        continuous.save().unwrap().as_bytes()
    );

    let returned = run_cli(&[
        OsStr::new("--content"),
        content.as_os_str(),
        OsStr::new("--input"),
        OsStr::new("native"),
        OsStr::new("--load"),
        bbb_path.as_os_str(),
        OsStr::new("--days"),
        OsStr::new("1"),
        OsStr::new("--save"),
        aaa_path.as_os_str(),
    ]);
    assert_eq!(returned["pending_tasks"], 0);
    assert!(has_log(&returned, "controller-returned-to-aaa"));
    assert_eq!(snapshot(&aaa_path)["province_controllers"]["2"], 0);
    assert_owner_and_unknown_previous_controller(&snapshot(&aaa_path));

    continuous.advance(1).unwrap();
    assert_eq!(
        continuous
            .world()
            .province_controller_state(EntityId(2))
            .unwrap(),
        Some(Some(EntityId(0)))
    );
    assert_eq!(
        serde_json::to_value(continuous.view().unwrap()).unwrap(),
        returned
    );
    assert_eq!(
        fs::read(&aaa_path).unwrap(),
        continuous.save().unwrap().as_bytes()
    );
}

#[test]
fn native_controller_action_records_known_absence_without_touching_owner_or_history() {
    let content =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/province-controller-actions");
    let temp = tempfile::tempdir().unwrap();
    let pending_path = temp.path().join("controller-pending.json");
    let absent_path = temp.path().join("controller-absent.json");

    run_cli(&[
        OsStr::new("--content"),
        content.as_os_str(),
        OsStr::new("--input"),
        OsStr::new("native"),
        OsStr::new("--trigger"),
        OsStr::new("province_controller_actions.change"),
        OsStr::new("--save"),
        pending_path.as_os_str(),
    ]);
    let absent = run_cli(&[
        OsStr::new("--content"),
        content.as_os_str(),
        OsStr::new("--input"),
        OsStr::new("native"),
        OsStr::new("--load"),
        pending_path.as_os_str(),
        OsStr::new("--choose"),
        OsStr::new("0:1"),
        OsStr::new("--save"),
        absent_path.as_os_str(),
    ]);

    let saved = snapshot(&absent_path);
    assert!(
        saved["province_controllers"]
            .get("2")
            .is_some_and(Value::is_null)
    );
    assert_owner_and_unknown_previous_controller(&saved);
    assert!(has_log(&absent, "controller-marked-absent"));

    let mut resumed = EventDemo::from_directory(&content, Input::Native).unwrap();
    resumed
        .restore(&fs::read_to_string(&pending_path).unwrap())
        .unwrap();
    resumed
        .choose(&EventChoice {
            pending: augustus_core::PendingEventId(0),
            option: "1".into(),
        })
        .unwrap();
    assert_eq!(
        resumed
            .world()
            .province_controller_state(EntityId(2))
            .unwrap(),
        Some(None)
    );
    assert_eq!(
        resumed
            .world()
            .province_previous_controller_state(EntityId(2))
            .unwrap(),
        None
    );
    assert_eq!(
        serde_json::to_value(resumed.view().unwrap()).unwrap(),
        absent
    );
    assert_eq!(
        fs::read(&absent_path).unwrap(),
        resumed.save().unwrap().as_bytes()
    );
}
