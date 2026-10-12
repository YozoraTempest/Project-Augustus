// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{EntityId, EventChoice, PendingEventId};
use augustus_demo::{EventDemo, Input};
use std::path::Path;

#[test]
fn owned_effects_share_nested_metadata_randomness_and_cross_input_restore() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/owned-iteration");
    let mut compatible = EventDemo::from_directory(&root, Input::Eu4).unwrap();
    let mut native = EventDemo::from_directory(&root, Input::Native).unwrap();
    assert_eq!(compatible.save().unwrap(), native.save().unwrap());
    for demo in [&mut compatible, &mut native] {
        demo.trigger("traversal.start", 0).unwrap();
        let view = demo.view().unwrap();
        let rows: Vec<_> = view
            .log
            .iter()
            .map(|row| (row.1.0, row.2.as_str()))
            .collect();
        assert_eq!(
            rows,
            [
                (3, "first_only"),
                (3, "member"),
                (4, "member"),
                (5, "member"),
                (5, "selected"),
                (4, "inner"),
                (5, "inner"),
                (3, "outer"),
                (0, "returned"),
            ]
        );
        assert_eq!(view.pending_events[0].description_keys["0"], ["visit_hint"]);
        assert_eq!(
            view.pending_events[0].modifier_duration_labels["0"],
            ["brief_duration"]
        );
        demo.choose(&EventChoice {
            pending: PendingEventId(0),
            option: "0".into(),
        })
        .unwrap();
        assert!(demo.world().has_modifier(EntityId(0), "brief").unwrap());
    }
    assert_eq!(compatible.save().unwrap(), native.save().unwrap());
    compatible.restore(&native.save().unwrap()).unwrap();
    native.restore(&compatible.save().unwrap()).unwrap();
    for demo in [&mut compatible, &mut native] {
        demo.advance(1).unwrap();
        let view = demo.view().unwrap();
        let rows: Vec<_> = view
            .log
            .iter()
            .filter(|row| row.2 == "later")
            .map(|row| row.1.0)
            .collect();
        assert_eq!(rows, [3, 4, 5]);
        let chosen: Vec<_> = view.log.iter().filter(|row| row.2 == "picked").collect();
        assert_eq!(chosen.len(), 1);
        assert!([EntityId(4), EntityId(5)].contains(&chosen[0].1));
        demo.choose(&EventChoice {
            pending: PendingEventId(1),
            option: "0".into(),
        })
        .unwrap();
        demo.advance(1).unwrap();
        assert!(!demo.world().has_modifier(EntityId(0), "brief").unwrap());
    }
    assert_eq!(compatible.save().unwrap(), native.save().unwrap());
    assert_eq!(
        serde_json::to_string(&compatible.view().unwrap()).unwrap(),
        serde_json::to_string(&native.view().unwrap()).unwrap()
    );
}
