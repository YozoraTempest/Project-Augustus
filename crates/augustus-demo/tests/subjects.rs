// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{EntityId, EventChoice, PendingEventId};
use augustus_demo::{EventDemo, Input};
use std::path::Path;

#[test]
fn direct_subject_queries_and_delayed_owner_conditions_match_across_inputs_and_restore() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/subject-queries");
    let mut compatible = EventDemo::from_directory(&root, Input::Eu4).unwrap();
    let mut native = EventDemo::from_directory(&root, Input::Native).unwrap();
    assert_eq!(compatible.save().unwrap(), native.save().unwrap());
    for demo in [&mut compatible, &mut native] {
        demo.trigger("subject.start", 0).unwrap();
        assert_eq!(
            demo.world().subject_countries(EntityId(0)).unwrap(),
            [EntityId(1), EntityId(2)]
        );
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
        demo.choose(&EventChoice {
            pending: PendingEventId(1),
            option: "0".into(),
        })
        .unwrap();
        assert_eq!(
            demo.world()
                .events()
                .iter()
                .map(|row| row.2.as_str())
                .collect::<Vec<_>>(),
            [
                "direct_vassal",
                "direct_tributary",
                "indirect_excluded",
                "first_choice",
                "independent_owner",
                "later_choice"
            ]
        );
    }
    assert_eq!(compatible.save().unwrap(), native.save().unwrap());
    assert_eq!(
        serde_json::to_string(&compatible.view().unwrap()).unwrap(),
        serde_json::to_string(&native.view().unwrap()).unwrap()
    );
}
