// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{EntityId, EventChoice, PendingEventId};
use augustus_demo::{EventDemo, Input};
use std::path::Path;

#[test]
fn subjects_share_current_filters_event_scopes_and_crossed_restore() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/subject-iteration");
    let mut native = EventDemo::from_directory(&root, Input::Native).unwrap();
    let mut eu4 = EventDemo::from_directory(&root, Input::Eu4).unwrap();
    for stage in 0..7 {
        for demo in [&mut native, &mut eu4] {
            match stage {
                1 => {
                    demo.trigger("subject_demo.1", 0).unwrap();
                }
                2 | 5 => demo
                    .choose(&EventChoice {
                        pending: PendingEventId(if stage == 2 { 0 } else { 1 }),
                        option: "0".into(),
                    })
                    .unwrap(),
                3 | 4 => demo.advance(1).unwrap(),
                6 => demo.advance(6).unwrap(),
                _ => (),
            }
            if stage == 1 {
                assert_eq!(
                    demo.world()
                        .events()
                        .iter()
                        .map(|row| row.1)
                        .collect::<Vec<_>>(),
                    [
                        EntityId(2),
                        EntityId(1),
                        EntityId(2),
                        EntityId(2),
                        EntityId(2),
                        EntityId(2),
                        EntityId(1)
                    ]
                );
                let view = demo.view().unwrap();
                assert_eq!(
                    view.pending_events[0].description_keys["0"],
                    ["subject_group"]
                );
                assert_eq!(
                    view.pending_events[0].modifier_duration_labels["0"],
                    ["subject_duration"]
                );
            }
            assert_eq!(demo.world().subject_count(EntityId(0)).unwrap(), 9);
            assert_eq!(
                demo.world()
                    .has_marker(EntityId(2), "child_started")
                    .unwrap(),
                stage >= 4
            );
            assert!(!demo.world().has_marker(EntityId(3), "visited").unwrap());
        }
        assert_eq!(native.save().unwrap(), eu4.save().unwrap(), "stage {stage}");
        assert_eq!(
            serde_json::to_string(&native.view().unwrap()).unwrap(),
            serde_json::to_string(&eu4.view().unwrap()).unwrap()
        );
        let saved = native.save().unwrap();
        native.restore(&eu4.save().unwrap()).unwrap();
        eu4.restore(&saved).unwrap();
    }
    assert!(native.world().pending_events().is_empty());
    assert_eq!(native.world().events().len(), 9);
}
