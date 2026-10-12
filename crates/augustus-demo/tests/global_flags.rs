// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{EntityId, EventChoice, PendingEventId};
use augustus_demo::{EventDemo, Input};
use std::path::Path;

#[test]
fn country_and_province_events_share_globals_across_input_restore() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/global-flags");
    let mut compatible = EventDemo::from_directory(&root, Input::Eu4).unwrap();
    let mut native = EventDemo::from_directory(&root, Input::Native).unwrap();
    assert_eq!(compatible.save().unwrap(), native.save().unwrap());
    for demo in [&mut compatible, &mut native] {
        demo.trigger("global.start", 0).unwrap();
        assert!(demo.world().has_global_marker("ready").unwrap());
        assert!(!demo.world().has_marker(EntityId(0), "ready").unwrap());
        demo.choose(&EventChoice {
            pending: PendingEventId(0),
            option: "0".into(),
        })
        .unwrap();
    }
    compatible.restore(&native.save().unwrap()).unwrap();
    native.restore(&compatible.save().unwrap()).unwrap();
    for demo in [&mut compatible, &mut native] {
        demo.advance(1).unwrap();
        assert!(!demo.world().has_global_marker("ready").unwrap());
        assert_eq!(demo.view().unwrap().pending_events[0].current, EntityId(2));
        demo.choose(&EventChoice {
            pending: PendingEventId(1),
            option: "0".into(),
        })
        .unwrap();
        assert!(demo.view().unwrap().global_markers.values["finished"]);
        let rows: Vec<_> = demo
            .world()
            .events()
            .iter()
            .map(|row| (row.1, row.2.as_str()))
            .collect();
        assert_eq!(
            rows,
            [
                (EntityId(0), "before"),
                (EntityId(0), "owner"),
                (EntityId(0), "after"),
                (EntityId(2), "delayed"),
                (EntityId(2), "province_done")
            ]
        );
    }
    assert_eq!(compatible.save().unwrap(), native.save().unwrap());
    assert_eq!(
        serde_json::to_string(&compatible.view().unwrap()).unwrap(),
        serde_json::to_string(&native.view().unwrap()).unwrap()
    );
}
