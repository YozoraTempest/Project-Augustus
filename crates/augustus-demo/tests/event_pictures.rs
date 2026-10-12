// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{EventChoice, PendingEventId};
use augustus_demo::{EventDemo, Input};
use std::{
    fs,
    path::{Path, PathBuf},
};

fn path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/event-pictures")
}

fn scene(input: Input) -> EventDemo {
    EventDemo::from_directory(&path(), input).unwrap()
}

fn view(demo: &EventDemo) -> String {
    serde_json::to_string(&demo.view().unwrap()).unwrap()
}

#[test]
fn both_inputs_keep_the_opened_picture_while_new_events_read_changed_state() {
    let mut native = scene(Input::Native);
    let mut compatibility = scene(Input::Eu4);
    assert_eq!(native.save().unwrap(), compatibility.save().unwrap());
    for demo in [&mut native, &mut compatibility] {
        demo.trigger("pictures.start", 0).unwrap();
        assert_eq!(
            demo.view().unwrap().pending_events[0].picture.as_deref(),
            Some("council_ready")
        );
    }
    assert_eq!(native.save().unwrap(), compatibility.save().unwrap());
    native.restore(&compatibility.save().unwrap()).unwrap();
    compatibility.restore(&native.save().unwrap()).unwrap();
    for demo in [&mut native, &mut compatibility] {
        demo.advance(1).unwrap();
        demo.trigger("pictures.later", 0).unwrap();
        demo.trigger("pictures.fallback", 0).unwrap();
        demo.trigger("pictures.mixed", 0).unwrap();
        let saved = demo.save().unwrap();
        assert_eq!(
            demo.view()
                .unwrap()
                .pending_events
                .iter()
                .map(|event| event.picture.as_deref())
                .collect::<Vec<_>>(),
            [
                Some("council_ready"),
                None,
                Some("council_waiting"),
                Some("first_declared"),
                Some("unconditional")
            ]
        );
        assert_eq!(demo.save().unwrap(), saved);
    }
    assert_eq!(view(&native), view(&compatibility));
    assert_eq!(native.save().unwrap(), compatibility.save().unwrap());
    let waiting = native.save().unwrap();
    compatibility.restore(&waiting).unwrap();
    for demo in [&mut native, &mut compatibility] {
        for id in [4, 0, 3, 2, 1] {
            demo.choose(&EventChoice {
                pending: PendingEventId(id),
                option: "0".into(),
            })
            .unwrap();
        }
        demo.advance(2).unwrap();
        assert!(demo.view().unwrap().pending_events.is_empty());
    }
    assert_eq!(view(&native), view(&compatibility));
    assert_eq!(native.save().unwrap(), compatibility.save().unwrap());
}

fn native_copy() -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    for name in ["scene.json", "events.json", "pictures.json"] {
        fs::copy(path().join(name), temp.path().join(name)).unwrap();
    }
    temp
}

#[test]
fn pictures_are_effective_content_and_a_changed_key_rejects_an_old_save() {
    let temp = native_copy();
    let mut before = scene(Input::Native);
    before.trigger("pictures.start", 0).unwrap();
    let mut pictures: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(temp.path().join("pictures.json")).unwrap())
            .unwrap();
    pictures["pictures.start"]["variants"][0]["key"] = "different".into();
    fs::write(temp.path().join("pictures.json"), pictures.to_string()).unwrap();
    let mut changed = EventDemo::from_directory(temp.path(), Input::Native).unwrap();
    assert!(changed.restore(&before.save().unwrap()).is_err());
}

#[test]
fn native_content_can_omit_the_fallback_without_inheriting_eu4_rules() {
    let temp = native_copy();
    let mut pictures: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(temp.path().join("pictures.json")).unwrap())
            .unwrap();
    pictures["pictures.fallback"]
        .as_object_mut()
        .unwrap()
        .remove("fallback");
    fs::write(temp.path().join("pictures.json"), pictures.to_string()).unwrap();
    let mut demo = EventDemo::from_directory(temp.path(), Input::Native).unwrap();
    demo.trigger("pictures.fallback", 0).unwrap();
    assert_eq!(demo.view().unwrap().pending_events[0].picture, None);
    let save = demo.save().unwrap();
    let mut restored = EventDemo::from_directory(temp.path(), Input::Native).unwrap();
    restored.restore(&save).unwrap();
    assert_eq!(restored.save().unwrap(), save);
    assert_eq!(restored.view().unwrap().pending_events[0].picture, None);
}
