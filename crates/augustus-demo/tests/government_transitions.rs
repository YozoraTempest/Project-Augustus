// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::path::Path;

use augustus_core::{EntityId, EventChoice, PendingEventId};
use augustus_demo::{EventDemo, Input};

#[test]
fn transitions_and_callbacks_match_across_inputs_pending_choices_and_restore() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/government-transitions");
    let mut compatible = EventDemo::from_directory(&root, Input::Eu4).unwrap();
    let mut native = EventDemo::from_directory(&root, Input::Native).unwrap();
    assert_eq!(compatible.save().unwrap(), native.save().unwrap());
    for demo in [&mut compatible, &mut native] {
        demo.trigger("transition.start", 0).unwrap();
        assert_eq!(
            demo.world().government_kind(EntityId(0)).unwrap(),
            "republic"
        );
        assert_eq!(
            demo.world()
                .government_metric_stored_value(EntityId(0), "legitimacy")
                .unwrap(),
            100_000
        );
    }
    assert_eq!(compatible.save().unwrap(), native.save().unwrap());
    compatible.restore(&native.save().unwrap()).unwrap();
    native.restore(&compatible.save().unwrap()).unwrap();
    for demo in [&mut compatible, &mut native] {
        demo.choose(&EventChoice {
            pending: PendingEventId(0),
            option: "0".into(),
        })
        .unwrap();
        assert_eq!(
            demo.world()
                .government_metric_value(EntityId(0), "legitimacy")
                .unwrap(),
            50_000
        );
    }
    assert_eq!(compatible.save().unwrap(), native.save().unwrap());
    compatible.restore(&native.save().unwrap()).unwrap();
    native.restore(&compatible.save().unwrap()).unwrap();
    for demo in [&mut compatible, &mut native] {
        demo.advance(1).unwrap();
        demo.choose(&EventChoice {
            pending: PendingEventId(1),
            option: "0".into(),
        })
        .unwrap();
        assert_eq!(demo.world().government_kind(EntityId(0)).unwrap(), "tribal");
        for name in ["republican_tradition", "devotion"] {
            assert_eq!(
                demo.world()
                    .government_metric_stored_value(EntityId(0), name)
                    .unwrap(),
                50_000
            );
            assert!(
                !demo
                    .world()
                    .government_metric_active(EntityId(0), name)
                    .unwrap()
            );
        }
        assert!(
            !demo
                .world()
                .government_reform_selected(EntityId(0), "charter")
                .unwrap()
        );
        assert_eq!(
            demo.world()
                .events()
                .iter()
                .map(|(_, _, text)| text.as_str())
                .collect::<Vec<_>>(),
            [
                "ordinary",
                "ordinary",
                "ordinary",
                "after_first",
                "ordinary",
                "native",
                "native",
                "ordinary",
                "finished"
            ]
        );
    }
    assert_eq!(compatible.save().unwrap(), native.save().unwrap());
    assert_eq!(
        serde_json::to_string(&compatible.view().unwrap()).unwrap(),
        serde_json::to_string(&native.view().unwrap()).unwrap()
    );
}
