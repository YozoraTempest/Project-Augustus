// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{EntityId, EventChoice, FieldId, PendingEventId};
use augustus_demo::{EventDemo, Input};
use std::path::Path;

#[test]
fn explicit_events_with_inactive_timing_share_plans_choices_and_restore() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/event-timing");
    let mut native = EventDemo::from_directory(&root, Input::Native).unwrap();
    let mut eu4 = EventDemo::from_directory(&root, Input::Eu4).unwrap();
    for stage in 0..10 {
        for demo in [&mut native, &mut eu4] {
            if stage > 0 && stage < 9 {
                if stage % 2 == 1 {
                    demo.trigger(&format!("timing.{}", stage / 2 + 1), 0)
                        .unwrap();
                } else {
                    demo.choose(&EventChoice {
                        pending: PendingEventId((stage / 2 - 1) as u64),
                        option: "0".into(),
                    })
                    .unwrap();
                }
            } else if stage == 9 {
                demo.advance(60).unwrap();
            }
            assert_eq!(
                demo.world().value(EntityId(0), FieldId(0)).unwrap(),
                ((stage + 1) / 2).min(4) as i64 * 1000
            );
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
    assert_eq!(native.world().events().len(), 4);
}
