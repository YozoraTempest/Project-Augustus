// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{EntityId, EventChoice, PendingEventId};
use augustus_demo::{EventDemo, Input};

#[test]
fn both_inputs_share_event_state_plans_and_save_identity() {
    let mut native = EventDemo::embedded(Input::Native).unwrap();
    let mut compiled = EventDemo::embedded(Input::Eu4).unwrap();
    assert_eq!(native.world(), compiled.world());
    assert_eq!(native.save().unwrap(), compiled.save().unwrap());
    for demo in [&mut native, &mut compiled] {
        let pending = demo.trigger("augustus_council.1", 0).unwrap();
        assert_eq!(demo.world().events().len(), 1);
        demo.advance(5).unwrap();
        demo.choose(&EventChoice {
            pending,
            option: "0".into(),
        })
        .unwrap();
    }
    assert_eq!(native.save().unwrap(), compiled.save().unwrap());
    let field = native.world().schema().field("treasury").unwrap();
    assert_eq!(native.world().value(EntityId(0), field).unwrap(), 7500);
    let phase = native.world().schema().field("variable/phase").unwrap();
    assert_eq!(native.world().value(EntityId(1), phase).unwrap(), 3000);
    assert_eq!(
        native
            .world()
            .events()
            .iter()
            .map(|event| (event.0, event.2.as_str()))
            .collect::<Vec<_>>(),
        [
            (0, "council_ready"),
            (5, "harbor_funded"),
            (5, "council_closed")
        ]
    );
}

#[test]
fn save_between_dispatch_and_choice_can_resume_through_other_input() {
    let mut continuous = EventDemo::embedded(Input::Eu4).unwrap();
    let id = continuous.trigger("augustus_council.1", 0).unwrap();
    continuous.advance(2).unwrap();
    let save = continuous.save().unwrap();
    let mut resumed = EventDemo::embedded(Input::Native).unwrap();
    resumed.restore(&save).unwrap();
    for demo in [&mut continuous, &mut resumed] {
        demo.advance(3).unwrap();
        demo.choose(&EventChoice {
            pending: id,
            option: "1".into(),
        })
        .unwrap();
        demo.advance(4).unwrap();
    }
    assert_eq!(continuous.save().unwrap(), resumed.save().unwrap());
    assert_eq!(resumed.world().pending_events().len(), 0);
    assert_eq!(
        resumed.trigger("augustus_council.1", 0).unwrap(),
        PendingEventId(1)
    );
}

#[test]
fn directory_scene_matches_embedded_scene_and_rejects_changed_content() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/events");
    for input in [Input::Eu4, Input::Native] {
        let directory = EventDemo::from_directory(&root, input).unwrap();
        let embedded = EventDemo::embedded(input).unwrap();
        assert_eq!(directory.save().unwrap(), embedded.save().unwrap());
    }
    let mut demo = EventDemo::embedded(Input::Native).unwrap();
    let mut json: serde_json::Value = serde_json::from_str(&demo.save().unwrap()).unwrap();
    json["content"] = serde_json::json!("different rules");
    assert!(demo.restore(&json.to_string()).is_err());
}
