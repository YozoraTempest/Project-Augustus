// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{EventChoice, PendingEventId};
use augustus_demo::{EventDemo, Input};
use std::{
    fs,
    path::{Path, PathBuf},
};

fn path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/event-decisions")
}
fn scene(input: Input) -> EventDemo {
    EventDemo::from_directory(&path(), input).unwrap()
}
fn equal(left: &EventDemo, right: &EventDemo) {
    assert_eq!(left.save().unwrap(), right.save().unwrap());
    assert_eq!(
        serde_json::to_string(&left.view().unwrap()).unwrap(),
        serde_json::to_string(&right.view().unwrap()).unwrap()
    );
}

#[test]
fn both_inputs_restore_selected_commands_before_execution_and_keep_later_draws_identical() {
    let mut native = scene(Input::Native);
    let mut eu4 = scene(Input::Eu4);
    equal(&native, &eu4);
    for demo in [&mut native, &mut eu4] {
        demo.trigger("decisions.start", 1).unwrap();
        let view = demo.view().unwrap();
        assert_eq!(view.queued_event_choices.len(), 2);
        assert_eq!(view.queued_event_choices[0].option, "0");
        assert_eq!(view.pending_events[0].options, ["1"]);
    }
    equal(&native, &eu4);
    let saved = native.save().unwrap();
    eu4.restore(&saved).unwrap();
    native.restore(&eu4.save().unwrap()).unwrap();
    for demo in [&mut native, &mut eu4] {
        demo.advance(0).unwrap();
        assert_eq!(
            demo.view()
                .unwrap()
                .log
                .iter()
                .map(|row| row.2.as_str())
                .collect::<Vec<_>>(),
            [
                "queued",
                "gate_clear",
                "captured_0",
                "child_after",
                "parent"
            ]
        );
        assert!(demo.view().unwrap().pending_events.is_empty());
        demo.advance(2).unwrap();
        assert!(demo.view().unwrap().pending_events.is_empty());
        assert!(demo.view().unwrap().queued_event_choices.is_empty());
    }
    equal(&native, &eu4);
}

#[test]
fn human_and_ai_empty_option_events_follow_different_explicit_paths() {
    for input in [Input::Native, Input::Eu4] {
        let mut demo = scene(input);
        demo.trigger("decisions.zero", 0).unwrap();
        demo.trigger("decisions.zero", 1).unwrap();
        demo.advance(0).unwrap();
        let view = demo.view().unwrap();
        assert_eq!(view.pending_events.len(), 1);
        assert!(view.pending_events[0].options.is_empty());
        assert_eq!(view.log[0].2, "zero_0");
        demo.choose(&EventChoice {
            pending: PendingEventId(0),
            option: "1".into(),
        })
        .unwrap();
        assert_eq!(demo.view().unwrap().log[1].2, "zero_1");
    }
}

#[test]
fn weights_and_selection_configuration_are_part_of_effective_content() {
    let original = scene(Input::Native).save().unwrap();
    for name in ["option_weights.json", "scene.json"] {
        let temp = tempfile::tempdir().unwrap();
        for file in [
            "scene.json",
            "events.json",
            "option_conditions.json",
            "option_weights.json",
        ] {
            fs::copy(path().join(file), temp.path().join(file)).unwrap();
        }
        let target = temp.path().join(name);
        let mut data: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&target).unwrap()).unwrap();
        if name == "scene.json" {
            data["event_decisions"]["empty"] = "keep_pending".into();
        } else {
            data["decisions.random"]["1"]["factor"] = 8000.into();
        }
        fs::write(target, serde_json::to_vec(&data).unwrap()).unwrap();
        let mut changed = EventDemo::from_directory(temp.path(), Input::Native).unwrap();
        assert!(changed.restore(&original).is_err());
    }
}

#[test]
fn native_weights_use_fractional_factors_without_legacy_numeric_translation() {
    let temp = tempfile::tempdir().unwrap();
    for file in [
        "scene.json",
        "events.json",
        "option_conditions.json",
        "option_weights.json",
    ] {
        fs::copy(path().join(file), temp.path().join(file)).unwrap();
    }
    let target = temp.path().join("option_weights.json");
    let mut data: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&target).unwrap()).unwrap();
    data["decisions.random"]["0"]["factor"] = 500.into();
    data["decisions.random"]["0"]["modifiers"] = serde_json::json!([
        {"factor":1500,"condition":{"Always":true}}
    ]);
    fs::write(target, serde_json::to_vec(&data).unwrap()).unwrap();
    let mut demo = EventDemo::from_directory(temp.path(), Input::Native).unwrap();
    demo.trigger("decisions.random", 1).unwrap();
    assert_eq!(
        demo.world()
            .event_option_weights(&demo.world().pending_events()[0])
            .unwrap(),
        [("0".into(), 750), ("1".into(), 9000)]
    );
    demo.advance(0).unwrap();
    assert!(demo.view().unwrap().pending_events.is_empty());
    assert_eq!(demo.view().unwrap().log.last().unwrap().2, "random_after");
}
