// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{EntityId, EventChoice, PendingEventId};
use augustus_demo::{EventDemo, Input};
use std::path::Path;

#[test]
fn province_modifier_queries_and_removal_share_cross_input_saves() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/province-modifiers");
    let mut compatible = EventDemo::from_directory(&root, Input::Eu4).unwrap();
    let mut native = EventDemo::from_directory(&root, Input::Native).unwrap();
    assert_eq!(compatible.save().unwrap(), native.save().unwrap());
    for demo in [&mut compatible, &mut native] {
        demo.trigger("province.start", 0).unwrap();
        assert!(demo.world().has_modifier(EntityId(2), "boost").unwrap());
        assert!(!demo.world().has_modifier(EntityId(0), "boost").unwrap());
        assert_eq!(
            demo.world()
                .modifiers()
                .declared_totals(EntityId(2))
                .unwrap()["local_tax_modifier"],
            125000
        );
        demo.choose(&EventChoice {
            pending: PendingEventId(0),
            option: "0".into(),
        })
        .unwrap();
        assert!(!demo.world().has_modifier(EntityId(2), "boost").unwrap());
        assert!(demo.world().has_modifier(EntityId(2), "guard").unwrap());
        assert!(
            demo.world()
                .modifiers()
                .declared_totals(EntityId(2))
                .unwrap()
                .is_empty()
        );
    }
    assert_eq!(compatible.save().unwrap(), native.save().unwrap());
    compatible.restore(&native.save().unwrap()).unwrap();
    native.restore(&compatible.save().unwrap()).unwrap();
    for demo in [&mut compatible, &mut native] {
        demo.advance(1).unwrap();
        assert!(demo.world().has_modifier(EntityId(2), "guard").unwrap());
        demo.advance(1).unwrap();
        assert!(demo.world().has_modifier(EntityId(2), "guard").unwrap());
        assert!(
            demo.world()
                .modifiers()
                .declared_totals(EntityId(2))
                .unwrap()
                .is_empty()
        );
        demo.choose(&EventChoice {
            pending: PendingEventId(1),
            option: "0".into(),
        })
        .unwrap();
        assert!(demo.world().modifiers().active.is_empty());
        assert_eq!(
            demo.view()
                .unwrap()
                .log
                .iter()
                .map(|row| row.2.as_str())
                .collect::<Vec<_>>(),
            [
                "attached",
                "separate",
                "removed",
                "retained",
                "after",
                "later",
                "country_retained",
                "cleared"
            ]
        );
    }
    assert_eq!(compatible.save().unwrap(), native.save().unwrap());
    assert_eq!(
        serde_json::to_string(&compatible.view().unwrap()).unwrap(),
        serde_json::to_string(&native.view().unwrap()).unwrap()
    );
}
