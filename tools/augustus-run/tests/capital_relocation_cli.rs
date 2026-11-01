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

fn entity<'a>(view: &'a Value, name: &str) -> &'a Value {
    view["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entity| entity["name"] == name)
        .unwrap()
}

fn has_log(view: &Value, message: &str) -> bool {
    view["log"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry[2] == message)
}

fn saved_entities(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

#[test]
fn native_capital_relocation_updates_live_target_and_resumes_saved_plan() {
    let content = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/capital-relocation");
    let temp = tempfile::tempdir().unwrap();
    let initial_save = temp.path().join("initial.json");
    let moved_save = temp.path().join("moved.json");
    let returned_save = temp.path().join("returned.json");

    let initial = run_cli(&[
        OsStr::new("--content"),
        content.as_os_str(),
        OsStr::new("--input"),
        OsStr::new("native"),
        OsStr::new("--save"),
        initial_save.as_os_str(),
    ]);
    assert_eq!(entity(&initial, "Old")["values"]["probe"], 11);
    assert_eq!(entity(&initial, "New")["values"]["probe"], 22);
    let initial_snapshot = saved_entities(&initial_save);
    assert_eq!(initial_snapshot["entities"][0]["capital"], 1);
    assert_eq!(initial_snapshot["entities"][1]["owner"], 0);
    assert_eq!(initial_snapshot["entities"][2]["owner"], 0);

    let moved = run_cli(&[
        OsStr::new("--content"),
        content.as_os_str(),
        OsStr::new("--input"),
        OsStr::new("native"),
        OsStr::new("--trigger"),
        OsStr::new("capital.relocate"),
        OsStr::new("--choose"),
        OsStr::new("0:0"),
        OsStr::new("--save"),
        moved_save.as_os_str(),
    ]);
    assert_eq!(entity(&moved, "Old")["values"]["probe"], 11);
    assert_eq!(entity(&moved, "New")["values"]["probe"], 220);
    assert_eq!(moved["pending_tasks"], 1);
    assert!(has_log(&moved, "relocation-offered"));
    assert!(has_log(&moved, "capital-relocated"));
    assert!(has_log(&moved, "relocation-choice-complete"));
    let moved_snapshot = saved_entities(&moved_save);
    assert_eq!(moved_snapshot["entities"][0]["capital"], 2);
    assert_eq!(moved_snapshot["entities"][1]["owner"], 0);
    assert_eq!(moved_snapshot["entities"][2]["owner"], 0);
    assert!(moved_snapshot.get("country_original_capitals").is_none());
    assert!(moved_snapshot.get("country_capital_history").is_none());

    let mut continuous = EventDemo::from_directory(&content, Input::Native).unwrap();
    let pending = continuous.trigger("capital.relocate", 0).unwrap();
    continuous.advance(0).unwrap();
    continuous
        .choose(&EventChoice {
            pending,
            option: "0".into(),
        })
        .unwrap();
    assert_eq!(
        serde_json::to_value(continuous.view().unwrap()).unwrap(),
        moved
    );
    assert_eq!(
        continuous.world().entity(EntityId(0)).unwrap().capital,
        Some(EntityId(2))
    );
    continuous.advance(1).unwrap();

    let resumed = run_cli(&[
        OsStr::new("--content"),
        content.as_os_str(),
        OsStr::new("--input"),
        OsStr::new("native"),
        OsStr::new("--load"),
        moved_save.as_os_str(),
        OsStr::new("--days"),
        OsStr::new("1"),
        OsStr::new("--save"),
        returned_save.as_os_str(),
    ]);
    assert_eq!(
        serde_json::to_value(continuous.view().unwrap()).unwrap(),
        resumed
    );
    assert_eq!(entity(&resumed, "Old")["values"]["probe"], 110);
    assert_eq!(entity(&resumed, "New")["values"]["probe"], 220);
    assert_eq!(resumed["pending_tasks"], 0);
    assert!(has_log(&resumed, "capital-returned"));
    assert_eq!(
        continuous.world().entity(EntityId(0)).unwrap().capital,
        Some(EntityId(1))
    );

    let returned_snapshot = saved_entities(&returned_save);
    assert_eq!(returned_snapshot["entities"][0]["capital"], 1);
    assert_eq!(returned_snapshot["entities"][1]["owner"], 0);
    assert_eq!(returned_snapshot["entities"][2]["owner"], 0);
    assert!(returned_snapshot.get("country_original_capitals").is_none());
    assert!(returned_snapshot.get("country_capital_history").is_none());
}
