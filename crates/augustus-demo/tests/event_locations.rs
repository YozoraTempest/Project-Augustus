// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{EntityId, EventChoice, PendingEventId};
use augustus_demo::{EventDemo, Input};
use std::{
    fs,
    path::{Path, PathBuf},
};
fn path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/event-locations")
}
fn view(demo: &EventDemo) -> String {
    serde_json::to_string(&demo.view().unwrap()).unwrap()
}

#[test]
fn both_inputs_expose_only_available_locations_without_consuming_choices_and_restore_each_other() {
    let mut native = EventDemo::from_directory(&path(), Input::Native).unwrap();
    let mut eu4 = EventDemo::from_directory(&path(), Input::Eu4).unwrap();
    for demo in [&mut native, &mut eu4] {
        demo.trigger("locations.visit", 0).unwrap();
        demo.trigger("locations.province", 1).unwrap();
        demo.trigger("locations.plain", 0).unwrap();
        demo.advance(3).unwrap();
        let save = demo.save().unwrap();
        let state = demo.view().unwrap();
        assert_eq!(state.pending_events[0].location, Some(EntityId(2)));
        assert_eq!(
            state.pending_events[0].option_locations,
            std::collections::BTreeMap::from([("0".into(), EntityId(1))])
        );
        assert_eq!(state.pending_events[1].location, None);
        assert_eq!(state.pending_events[1].option_locations["0"], EntityId(2));
        assert_eq!(state.pending_events[2].location, None);
        assert!(state.pending_events[2].option_locations.is_empty());
        assert_eq!(demo.save().unwrap(), save);
    }
    assert_eq!(view(&native), view(&eu4));
    assert_eq!(native.save().unwrap(), eu4.save().unwrap());
    let native_save = native.save().unwrap();
    let eu4_save = eu4.save().unwrap();
    native.restore(&eu4_save).unwrap();
    eu4.restore(&native_save).unwrap();
    for demo in [&mut native, &mut eu4] {
        for id in [0, 1, 2] {
            demo.choose(&EventChoice {
                pending: PendingEventId(id),
                option: "0".into(),
            })
            .unwrap();
        }
        assert!(demo.view().unwrap().pending_events.is_empty());
    }
    assert_eq!(view(&native), view(&eu4));
    assert_eq!(native.save().unwrap(), eu4.save().unwrap());
}

#[test]
fn changing_location_content_rejects_a_save_instead_of_retargeting_a_pending_event() {
    let temp = tempfile::tempdir().unwrap();
    for name in [
        "scene.json",
        "events.json",
        "locations.json",
        "option_conditions.json",
    ] {
        fs::copy(path().join(name), temp.path().join(name)).unwrap();
    }
    let mut original = EventDemo::from_directory(temp.path(), Input::Native).unwrap();
    original.trigger("locations.visit", 0).unwrap();
    let save = original.save().unwrap();
    let mut locations: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(temp.path().join("locations.json")).unwrap())
            .unwrap();
    locations["locations.visit"]["event"] = "7".into();
    fs::write(temp.path().join("locations.json"), locations.to_string()).unwrap();
    let mut changed = EventDemo::from_directory(temp.path(), Input::Native).unwrap();
    let before = changed.save().unwrap();
    assert!(changed.restore(&save).is_err());
    assert_eq!(changed.save().unwrap(), before);
}
