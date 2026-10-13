// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{EntityId, EventChoice, PendingEventId};
use augustus_demo::{EventDemo, Input};
use std::path::Path;

#[test]
fn both_inputs_share_symmetric_truces_cleanup_and_pending_choices_across_restore() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/truces");
    let mut native = EventDemo::from_directory(&path, Input::Native).unwrap();
    let mut eu4 = EventDemo::from_directory(&path, Input::Eu4).unwrap();
    for stage in 0..9 {
        for demo in [&mut native, &mut eu4] {
            match stage {
                1 => {
                    demo.trigger("tr.begin", 0).unwrap();
                }
                2 | 4 | 6 | 8 => {
                    demo.choose(&EventChoice {
                        pending: PendingEventId((stage / 2 - 1) as u64),
                        option: "0".into(),
                    })
                    .unwrap();
                }
                3 => demo.advance(365).unwrap(),
                5 => demo.advance(31).unwrap(),
                7 => demo.advance(1).unwrap(),
                _ => (),
            }
            let expected = (1..5).contains(&stage);
            assert_eq!(
                demo.world().truce_with(EntityId(0), EntityId(1)).unwrap(),
                expected,
                "stage {stage}"
            );
            assert_eq!(
                demo.world().truce_with(EntityId(1), EntityId(0)).unwrap(),
                expected
            );
            assert_eq!(
                demo.world().truce_with(EntityId(0), EntityId(0)).unwrap(),
                expected
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
    assert_eq!(
        native
            .world()
            .events()
            .iter()
            .map(|e| (e.0, e.2.as_str()))
            .collect::<Vec<_>>(),
        [
            (0, "symmetric"),
            (365, "anniversary"),
            (396, "before_cleanup"),
            (397, "expired"),
            (397, "finished")
        ]
    );
}
