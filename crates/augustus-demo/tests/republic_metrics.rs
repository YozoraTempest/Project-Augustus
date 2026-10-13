// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{EntityId, EventChoice, PendingEventId};
use augustus_demo::{EventDemo, Input};
use std::path::Path;

fn equal(left: &EventDemo, right: &EventDemo) {
    assert_eq!(left.save().unwrap(), right.save().unwrap());
    assert_eq!(
        serde_json::to_string(&left.view().unwrap()).unwrap(),
        serde_json::to_string(&right.view().unwrap()).unwrap()
    );
}

#[test]
fn institution_changes_share_manual_ai_and_restored_event_execution() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/republic-metrics");
    for (entity, manual) in [(0, true), (2, false)] {
        let mut native = EventDemo::from_directory(&root, Input::Native).unwrap();
        let mut eu4 = EventDemo::from_directory(&root, Input::Eu4).unwrap();
        equal(&native, &eu4);
        for demo in [&mut native, &mut eu4] {
            demo.trigger("institutions.start", entity).unwrap();
            assert_eq!(
                demo.world()
                    .government_metric_value(EntityId(entity), "republican_tradition")
                    .unwrap(),
                10_125
            );
        }
        equal(&native, &eu4);
        let saved = native.save().unwrap();
        eu4.restore(&saved).unwrap();
        native.restore(&eu4.save().unwrap()).unwrap();
        for demo in [&mut native, &mut eu4] {
            if manual {
                demo.choose(&EventChoice {
                    pending: PendingEventId(0),
                    option: "0".into(),
                })
                .unwrap();
            }
            demo.advance(0).unwrap();
            assert_eq!(
                demo.world()
                    .government_metric_value(EntityId(entity), "republican_tradition")
                    .unwrap(),
                13_000
            );
        }
        equal(&native, &eu4);
        let saved = eu4.save().unwrap();
        native.restore(&saved).unwrap();
        eu4.restore(&native.save().unwrap()).unwrap();
        for demo in [&mut native, &mut eu4] {
            demo.advance(2).unwrap();
            if manual {
                demo.choose(&EventChoice {
                    pending: PendingEventId(1),
                    option: "0".into(),
                })
                .unwrap();
            }
            assert_eq!(
                demo.world()
                    .government_metric_value(EntityId(entity), "devotion")
                    .unwrap(),
                10_000
            );
            assert_eq!(
                demo.world()
                    .government_metric_stored_value(EntityId(entity), "republican_tradition")
                    .unwrap(),
                13_000
            );
            demo.trigger("institutions.return", entity).unwrap();
            if manual {
                demo.choose(&EventChoice {
                    pending: PendingEventId(2),
                    option: "0".into(),
                })
                .unwrap();
            }
            demo.advance(2).unwrap();
            assert_eq!(
                demo.world()
                    .government_metric_value(EntityId(entity), "republican_tradition")
                    .unwrap(),
                50_000
            );
            assert_eq!(
                demo.world()
                    .government_metric_stored_value(EntityId(entity), "devotion")
                    .unwrap(),
                10_000
            );
            assert!(demo.view().unwrap().pending_events.is_empty());
            assert_eq!(
                demo.world()
                    .events()
                    .iter()
                    .map(|row| row.2.as_str())
                    .collect::<Vec<_>>(),
                [
                    "tradition_immediate",
                    "tradition_after",
                    "tradition_retained",
                    "devotion_immediate",
                    "devotion_after",
                    "devotion_retained",
                    "tradition_reset",
                    "finished"
                ]
            );
        }
        equal(&native, &eu4);
    }
}
