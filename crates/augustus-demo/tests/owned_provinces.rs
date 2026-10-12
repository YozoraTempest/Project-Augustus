// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{EntityId, EventChoice, PendingEventId};
use augustus_demo::{EventDemo, Input};
use std::path::Path;

#[test]
fn owned_queries_share_immediate_changes_and_cross_input_delayed_restore() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/owned-provinces");
    let mut compatible = EventDemo::from_directory(&root, Input::Eu4).unwrap();
    let mut native = EventDemo::from_directory(&root, Input::Native).unwrap();
    assert_eq!(compatible.save().unwrap(), native.save().unwrap());
    for demo in [&mut compatible, &mut native] {
        demo.trigger("owned.start", 0).unwrap();
        demo.choose(&EventChoice {
            pending: PendingEventId(0),
            option: "0".into(),
        })
        .unwrap();
        assert!(!demo.world().has_modifier(EntityId(2), "local").unwrap());
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
        assert_eq!(
            demo.view()
                .unwrap()
                .log
                .iter()
                .map(|row| row.2.as_str())
                .collect::<Vec<_>>(),
            ["found", "foreign", "owner", "removed", "after", "later"]
        );
    }
    assert_eq!(compatible.save().unwrap(), native.save().unwrap());
    assert_eq!(
        serde_json::to_string(&compatible.view().unwrap()).unwrap(),
        serde_json::to_string(&native.view().unwrap()).unwrap()
    );
}
