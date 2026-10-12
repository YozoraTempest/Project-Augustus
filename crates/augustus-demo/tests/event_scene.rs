// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

#[test]
fn country_flags_share_live_choices_delays_and_cross_input_restore() {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/flag-lifecycle");
    let mut continuous = EventDemo::from_directory(&root, Input::Eu4).unwrap();
    let mut native = EventDemo::from_directory(&root, Input::Native).unwrap();
    assert_eq!(continuous.save().unwrap(), native.save().unwrap());
    let pending = continuous.trigger("flags.control", 0).unwrap();
    continuous
        .choose(&augustus_core::EventChoice {
            pending,
            option: "0".into(),
        })
        .unwrap();
    native.restore(&continuous.save().unwrap()).unwrap();
    for demo in [&mut continuous, &mut native] {
        assert!(
            !demo
                .world()
                .has_marker(augustus_core::EntityId(1), "chosen")
                .unwrap()
        );
        demo.advance(1).unwrap();
        demo.choose(&augustus_core::EventChoice {
            pending: augustus_core::PendingEventId(1),
            option: "0".into(),
        })
        .unwrap();
        assert!(demo.world().markers().entities[&augustus_core::EntityId(0)].is_empty());
        assert_eq!(
            demo.world()
                .events()
                .iter()
                .map(|e| (e.0, e.2.as_str()))
                .collect::<Vec<_>>(),
            [(0, "before"), (0, "owner"), (0, "after"), (1, "delayed")]
        );
        assert!(demo.world().pending_events().is_empty());
        assert_eq!(demo.world().pending_tasks(), 0);
    }
    assert_eq!(continuous.save().unwrap(), native.save().unwrap());
}

use augustus_core::{EntityId, EventChoice, PendingEventId};
use augustus_demo::{EventDemo, Input};

#[test]
fn modifier_renewal_expiry_and_waiting_choices_share_cross_input_saves() {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/modifier-lifecycle");
    let mut compatible = EventDemo::from_directory(&root, Input::Eu4).unwrap();
    let mut native = EventDemo::from_directory(&root, Input::Native).unwrap();
    assert_eq!(compatible.save().unwrap(), native.save().unwrap());
    compatible.trigger("modifier.control", 0).unwrap();
    native.restore(&compatible.save().unwrap()).unwrap();
    for demo in [&mut compatible, &mut native] {
        demo.choose(&EventChoice {
            pending: PendingEventId(0),
            option: "0".into(),
        })
        .unwrap();
        demo.advance(2).unwrap();
        assert!(demo.world().has_modifier(EntityId(0), "boost").unwrap());
        assert!(!demo.world().has_modifier(EntityId(1), "boost").unwrap());
        demo.advance(2).unwrap();
        demo.choose(&EventChoice {
            pending: PendingEventId(1),
            option: "0".into(),
        })
        .unwrap();
        assert!(demo.view().unwrap().modifiers.active.is_empty());
        assert_eq!(
            demo.world()
                .events()
                .iter()
                .map(|e| (e.0, e.2.as_str()))
                .collect::<Vec<_>>(),
            [(0, "before"), (0, "owner"), (0, "after"), (4, "expired")]
        );
    }
    assert_eq!(compatible.save().unwrap(), native.save().unwrap());
}

#[test]
fn named_variables_share_cross_input_saves_scopes_choices_and_live_delays() {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/named-variables");
    let mut compatible = EventDemo::from_directory(&root, Input::Eu4).unwrap();
    let mut native = EventDemo::from_directory(&root, Input::Native).unwrap();
    assert_eq!(compatible.save().unwrap(), native.save().unwrap());
    compatible.trigger("numeric.control", 0).unwrap();
    native.restore(&compatible.save().unwrap()).unwrap();
    for demo in [&mut compatible, &mut native] {
        for id in 0..2 {
            demo.choose(&EventChoice {
                pending: PendingEventId(id),
                option: "0".into(),
            })
            .unwrap();
        }
        demo.advance(1).unwrap();
        demo.choose(&EventChoice {
            pending: PendingEventId(2),
            option: "0".into(),
        })
        .unwrap();
        assert_eq!(
            demo.world()
                .events()
                .iter()
                .map(|e| (e.0, e.2.as_str()))
                .collect::<Vec<_>>(),
            [(0, "before"), (0, "owner"), (0, "after"), (1, "delayed")]
        );
        assert_eq!(
            demo.world()
                .numeric_value(
                    EntityId(0),
                    &augustus_core::NumericSlot::Named("count".into())
                )
                .unwrap(),
            1000
        );
        assert_eq!(
            demo.world()
                .numeric_value(
                    EntityId(2),
                    &augustus_core::NumericSlot::Named("count".into())
                )
                .unwrap(),
            4500
        );
        let saved = demo.save().unwrap();
        demo.candidates(&["numeric.gated".into()], 1).unwrap();
        assert_eq!(demo.save().unwrap(), saved);
    }
    assert_eq!(compatible.save().unwrap(), native.save().unwrap());
}

#[test]
fn completions_survive_cross_input_choices_and_delayed_checks_without_becoming_global() {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/completion-records");
    let mut compatible = EventDemo::from_directory(&root, Input::Eu4).unwrap();
    let mut native = EventDemo::from_directory(&root, Input::Native).unwrap();
    assert_eq!(compatible.save().unwrap(), native.save().unwrap());
    compatible.trigger("records.control", 0).unwrap();
    let waiting = compatible.save().unwrap();
    native.restore(&waiting).unwrap();
    for demo in [&mut compatible, &mut native] {
        for id in 0..2 {
            demo.choose(&EventChoice {
                pending: PendingEventId(id),
                option: "0".into(),
            })
            .unwrap();
        }
        demo.advance(1).unwrap();
        assert_eq!(
            demo.world()
                .events()
                .iter()
                .map(|e| (e.0, e.2.as_str()))
                .collect::<Vec<_>>(),
            [(0, "before"), (0, "after"), (1, "gated")]
        );
        demo.choose(&EventChoice {
            pending: PendingEventId(2),
            option: "0".into(),
        })
        .unwrap();
        let saved = demo.save().unwrap();
        demo.candidates(&["records.gated".into()], 1).unwrap();
        assert_eq!(demo.save().unwrap(), saved);
    }
    assert_eq!(compatible.save().unwrap(), native.save().unwrap());
    let before = native.save().unwrap();
    let mut corrupt: serde_json::Value = serde_json::from_str(&before).unwrap();
    corrupt["completions"]["identifiers"] = serde_json::json!(["invented", "sample_first"]);
    assert!(native.restore(&corrupt.to_string()).is_err());
    assert_eq!(native.save().unwrap(), before);
}

#[test]
fn runtime_calendar_predicates_share_content_and_cross_input_saved_tasks() {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/runtime-predicates");
    let mut continuous = EventDemo::from_directory(&root, Input::Eu4).unwrap();
    let mut native = EventDemo::from_directory(&root, Input::Native).unwrap();
    assert_eq!(continuous.save().unwrap(), native.save().unwrap());
    continuous.trigger("calendar.control", 0).unwrap();
    let saved = continuous.save().unwrap();
    native.restore(&saved).unwrap();
    for demo in [&mut continuous, &mut native] {
        demo.advance(1).unwrap();
        assert_eq!(demo.world().date().unwrap().unwrap().year, 1445);
        assert_eq!(
            demo.world()
                .events()
                .iter()
                .map(|e| (e.0, e.2.as_str()))
                .collect::<Vec<_>>(),
            [(0, "before"), (1, "after")]
        );
        assert_eq!(demo.world().pending_tasks(), 0);
        demo.candidates(&["calendar.before".into(), "calendar.after".into()], 1)
            .unwrap();
        assert_eq!(demo.world().pending_events().len(), 3);
        for id in 0..3 {
            demo.choose(&EventChoice {
                pending: PendingEventId(id),
                option: "0".into(),
            })
            .unwrap();
        }
    }
    assert_eq!(continuous.save().unwrap(), native.save().unwrap());
    let temp = tempfile::tempdir().unwrap();
    for file in ["scene.json", "events.json", "conditions.json"] {
        std::fs::copy(root.join(file), temp.path().join(file)).unwrap();
    }
    let mut scene: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(temp.path().join("scene.json")).unwrap())
            .unwrap();
    scene["calendar"]["month_lengths"][0] = serde_json::json!(30);
    std::fs::write(temp.path().join("scene.json"), scene.to_string()).unwrap();
    assert!(
        EventDemo::from_directory(temp.path(), Input::Native)
            .unwrap()
            .restore(&saved)
            .is_err()
    );
}

#[test]
fn once_events_share_registry_reentry_and_delayed_restore_across_inputs() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/once-events");
    let mut compiled = EventDemo::from_directory(&root, Input::Eu4).unwrap();
    let mut native = EventDemo::from_directory(&root, Input::Native).unwrap();
    assert_eq!(compiled.save().unwrap(), native.save().unwrap());
    for demo in [&mut compiled, &mut native] {
        let pending = demo.trigger("once.1", 0).unwrap();
        demo.candidates(&["once.1".into()], 1).unwrap();
        let before = demo.save().unwrap();
        assert!(demo.trigger("once.1", 1).is_err());
        assert_eq!(demo.save().unwrap(), before);
        demo.choose(&EventChoice {
            pending,
            option: "0".into(),
        })
        .unwrap();
        assert_eq!(demo.trigger("once.2", 0).unwrap(), PendingEventId(2));
        demo.trigger("once.control", 0).unwrap();
        demo.advance(1).unwrap();
    }
    let saved = compiled.save().unwrap();
    assert_eq!(saved, native.save().unwrap());
    native.restore(&saved).unwrap();
    for demo in [&mut compiled, &mut native] {
        demo.advance(1).unwrap();
        for id in 1..=4 {
            demo.choose(&EventChoice {
                pending: PendingEventId(id),
                option: "0".into(),
            })
            .unwrap();
        }
        demo.trigger("once.control", 1).unwrap();
        demo.advance(2).unwrap();
        assert_eq!(demo.world().pending_events().len(), 1);
        assert_eq!(demo.world().fired_events().len(), 3);
        assert_eq!(
            demo.view().unwrap().entities[0].values["variable/depth"],
            2000
        );
        assert_eq!(
            demo.world()
                .events()
                .iter()
                .map(|e| (e.0, e.2.as_str()))
                .collect::<Vec<_>>(),
            [
                (0, "once_ready"),
                (0, "once_selected"),
                (0, "once_closed"),
                (0, "enter"),
                (0, "enter"),
                (0, "inner"),
                (0, "outer"),
                (2, "delayed")
            ]
        );
    }
    assert_eq!(compiled.save().unwrap(), native.save().unwrap());
}

#[test]
fn once_policy_is_immutable_content_even_before_the_event_is_fired() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/once-events");
    let temp = tempfile::tempdir().unwrap();
    for file in ["scene.json", "events.json", "once_events.json"] {
        std::fs::copy(root.join(file), temp.path().join(file)).unwrap();
    }
    let mut demo = EventDemo::from_directory(temp.path(), Input::Native).unwrap();
    let original = demo.save().unwrap();
    std::fs::write(temp.path().join("once_events.json"), "[]").unwrap();
    let mut changed = EventDemo::from_directory(temp.path(), Input::Native).unwrap();
    assert!(changed.restore(&original).is_err());
    assert!(demo.restore(&changed.save().unwrap()).is_err());
    std::fs::write(temp.path().join("once_events.json"), "[\"missing\"]").unwrap();
    assert!(EventDemo::from_directory(temp.path(), Input::Native).is_err());
}

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

#[test]
fn government_metric_events_preserve_live_queries_and_cross_input_restore() {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/government-metrics");
    let mut continuous = EventDemo::from_directory(&root, Input::Eu4).unwrap();
    let mut native = EventDemo::from_directory(&root, Input::Native).unwrap();
    assert_eq!(continuous.save().unwrap(), native.save().unwrap());
    continuous.trigger("government.control", 0).unwrap();
    assert_eq!(
        continuous
            .world()
            .government_metric_value(augustus_core::EntityId(0), "legitimacy")
            .unwrap(),
        60_000
    );
    native.restore(&continuous.save().unwrap()).unwrap();
    for demo in [&mut continuous, &mut native] {
        demo.choose(&EventChoice {
            pending: PendingEventId(0),
            option: "0".into(),
        })
        .unwrap();
        assert_eq!(
            demo.world()
                .government_metric_value(augustus_core::EntityId(0), "legitimacy")
                .unwrap(),
            65_125
        );
        demo.advance(1).unwrap();
        demo.choose(&EventChoice {
            pending: PendingEventId(1),
            option: "0".into(),
        })
        .unwrap();
        assert_eq!(
            demo.world()
                .government_metric_value(augustus_core::EntityId(0), "legitimacy")
                .unwrap(),
            10_125
        );
        assert_eq!(
            demo.view()
                .unwrap()
                .log
                .iter()
                .map(|e| (e.0, e.2.as_str()))
                .collect::<Vec<_>>(),
            [(0, "fraction"), (0, "owner"), (0, "after"), (1, "delayed")]
        );
    }
    assert_eq!(continuous.save().unwrap(), native.save().unwrap());
    for input in [Input::Eu4, Input::Native] {
        let mut inactive = EventDemo::from_directory(&root, input).unwrap();
        inactive.trigger("government.control", 1).unwrap();
        inactive
            .choose(&EventChoice {
                pending: PendingEventId(0),
                option: "0".into(),
            })
            .unwrap();
        inactive.advance(1).unwrap();
        assert!(inactive.world().pending_events().is_empty());
        assert!(
            !inactive
                .world()
                .government_metric_active(augustus_core::EntityId(1), "legitimacy")
                .unwrap()
        );
        assert!(inactive.view().unwrap().log.is_empty());
    }
}
