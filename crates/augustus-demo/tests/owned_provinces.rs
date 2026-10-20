// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{EntityId, EventChoice, PendingEventId};
use augustus_demo::{EventDemo, Input};
use std::path::Path;

#[test]
fn owner_effect_updates_live_queries_and_cross_input_saves() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/owned-provinces");
    let mut eu4 = EventDemo::from_directory(&root, Input::Eu4).unwrap();
    let mut native = EventDemo::from_directory(&root, Input::Native).unwrap();
    assert_eq!(eu4.save().unwrap(), native.save().unwrap());

    for demo in [&mut eu4, &mut native] {
        demo.trigger("owned.start", 0).unwrap();
        demo.choose(&EventChoice {
            pending: PendingEventId(0),
            option: "0".into(),
        })
        .unwrap();

        assert_eq!(
            demo.world().entity(EntityId(4)).unwrap().owner,
            Some(EntityId(1))
        );
        assert_eq!(
            demo.world().owned_provinces(EntityId(0)).unwrap(),
            [EntityId(2)]
        );
        assert_eq!(
            demo.world().owned_provinces(EntityId(1)).unwrap(),
            [EntityId(3), EntityId(4)]
        );
        assert_eq!(
            demo.view()
                .unwrap()
                .log
                .iter()
                .map(|row| row.2.as_str())
                .collect::<Vec<_>>(),
            [
                "found",
                "foreign",
                "owner",
                "removed",
                "transferred",
                "after"
            ]
        );
    }

    native.restore(&eu4.save().unwrap()).unwrap();
    for demo in [&mut eu4, &mut native] {
        demo.advance(1).unwrap();
        demo.choose(&EventChoice {
            pending: PendingEventId(1),
            option: "0".into(),
        })
        .unwrap();
    }
    assert_eq!(eu4.save().unwrap(), native.save().unwrap());
    assert_eq!(eu4.view().unwrap().log, native.view().unwrap().log);
}
