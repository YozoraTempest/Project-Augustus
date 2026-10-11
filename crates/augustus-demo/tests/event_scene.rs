// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{EntityId, EventChoice, PendingEventId};
use augustus_demo::{EventDemo, Input};

#[test]
fn candidate_batches_and_delayed_conditions_match_across_inputs_and_saved_tasks() {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/event-candidates");
    let mut compiled = EventDemo::from_directory(&root, Input::Eu4).unwrap();
    let mut native = EventDemo::from_directory(&root, Input::Native).unwrap();
    assert_eq!(compiled.save().unwrap(), native.save().unwrap());
    let candidates = vec![
        "candidate.1".into(),
        "candidate.2".into(),
        "candidate.false".into(),
    ];
    for demo in [&mut compiled, &mut native] {
        demo.candidates(&candidates, 0).unwrap();
    }
    assert_eq!(compiled.world().pending_events().len(), 1);
    assert_eq!(compiled.world().pending_events()[0].event, "candidate.1");
    assert_eq!(compiled.save().unwrap(), native.save().unwrap());
    compiled.trigger("delay.control", 0).unwrap();
    assert_eq!(compiled.world().pending_tasks(), 2);
    let saved = compiled.save().unwrap();
    native.restore(&saved).unwrap();
    for demo in [&mut compiled, &mut native] {
        demo.advance(2).unwrap();
    }
    assert_eq!(compiled.save().unwrap(), native.save().unwrap());
    assert_eq!(
        compiled
            .world()
            .events()
            .iter()
            .map(|entry| (entry.0, entry.2.as_str()))
            .collect::<Vec<_>>(),
        [
            (0, "candidate_one"),
            (0, "delay_start"),
            (0, "direct_true"),
            (0, "delay_end"),
            (2, "was_false_now_true")
        ]
    );
    assert_eq!(compiled.world().pending_tasks(), 0);
    assert!(
        !compiled
            .world()
            .pending_events()
            .iter()
            .any(|pending| pending.event == "delay.true" || pending.event == "candidate.false")
    );
    compiled.candidates(&["candidate.2".into()], 0).unwrap();
    assert_eq!(
        compiled.world().pending_events().last().unwrap().event,
        "candidate.2"
    );
    native.trigger("candidate.false", 0).unwrap();
    assert_eq!(native.world().events().last().unwrap().2, "unreachable");
    let mut changed: serde_json::Value = serde_json::from_str(&saved).unwrap();
    let content = changed["content"]
        .as_str()
        .unwrap()
        .replace("imperial_emperor", "different_office");
    changed["content"] = serde_json::json!(content);
    assert!(native.restore(&changed.to_string()).is_err());
}

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

#[test]
fn event_chain_uses_one_core_for_scopes_delays_and_cross_input_restore() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/event-chain");
    let mut continuous = EventDemo::from_directory(&root, Input::Eu4).unwrap();
    let mut native = EventDemo::from_directory(&root, Input::Native).unwrap();
    assert_eq!(continuous.save().unwrap(), native.save().unwrap());
    let parent = continuous.trigger("augustus_council.2", 0).unwrap();
    continuous
        .choose(&EventChoice {
            pending: parent,
            option: "0".into(),
        })
        .unwrap();
    assert_eq!(continuous.view().unwrap().pending_tasks, 1);
    assert_eq!(
        continuous.world().pending_events()[0].event,
        "augustus_council.4"
    );
    continuous.advance(1).unwrap();
    native.restore(&continuous.save().unwrap()).unwrap();
    for demo in [&mut continuous, &mut native] {
        demo.advance(1).unwrap();
        assert_eq!(demo.world().pending_events().len(), 2);
        assert_eq!(demo.world().pending_tasks(), 0);
        demo.choose(&EventChoice {
            pending: PendingEventId(1),
            option: "0".into(),
        })
        .unwrap();
        demo.choose(&EventChoice {
            pending: PendingEventId(2),
            option: "0".into(),
        })
        .unwrap();
    }
    assert_eq!(continuous.save().unwrap(), native.save().unwrap());
    assert_eq!(native.view().unwrap().entities[0].values["treasury"], 9250);
    assert_eq!(
        native.view().unwrap().entities[1].values["variable/phase"],
        4000
    );
    assert_eq!(
        native
            .world()
            .events()
            .iter()
            .map(|entry| (entry.0, entry.2.as_str()))
            .collect::<Vec<_>>(),
        [
            (0, "council_ready"),
            (0, "work_started"),
            (0, "harbor_ready"),
            (0, "work_planned"),
            (0, "council_closed"),
            (2, "shipment_arrived"),
            (2, "harbor_confirmed"),
            (2, "harbor_closed"),
            (2, "shipment_accepted"),
            (2, "shipment_closed")
        ]
    );
}

#[test]
fn named_targets_rebind_across_events_and_cross_input_saved_tasks() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/named-scopes");
    let mut continuous = EventDemo::from_directory(&root, Input::Eu4).unwrap();
    let mut native = EventDemo::from_directory(&root, Input::Native).unwrap();
    assert_eq!(continuous.save().unwrap(), native.save().unwrap());
    continuous.trigger("augustus_reference.1", 0).unwrap();
    continuous.trigger("augustus_reference.2", 1).unwrap();
    native.restore(&continuous.save().unwrap()).unwrap();
    for demo in [&mut continuous, &mut native] {
        demo.advance(1).unwrap();
        for id in 0..3 {
            demo.choose(&EventChoice {
                pending: PendingEventId(id),
                option: "0".into(),
            })
            .unwrap();
        }
        assert_eq!(
            demo.world().global_scopes()["port"],
            augustus_core::EntityId(2)
        );
        assert!(!demo.world().global_scopes().contains_key("chair"));
        let view = demo.view().unwrap();
        assert_eq!(view.entities[0].values["treasury"], 10500);
        assert_eq!(view.entities[1].values["treasury"], 6000);
        assert_eq!(
            view.log
                .iter()
                .map(|e| (e.0, e.1.0, e.2.as_str()))
                .collect::<Vec<_>>(),
            [(1, 1, "current_chair"), (1, 0, "harbor_owner")]
        );
    }
    assert_eq!(continuous.save().unwrap(), native.save().unwrap());
}
