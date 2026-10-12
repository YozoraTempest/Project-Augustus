// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{EntityId, EventChoice, PendingEventId};
use augustus_demo::{EventDemo, Input};
use std::path::Path;

fn scene(input: Input) -> EventDemo {
    EventDemo::from_directory(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/area-effects"),
        input,
    )
    .unwrap()
}

#[test]
fn both_formats_share_ordered_effects_scaled_attributes_and_delayed_save_continuation() {
    let mut compatible = scene(Input::Eu4);
    let mut native = scene(Input::Native);
    assert_eq!(compatible.save().unwrap(), native.save().unwrap());
    for demo in [&mut compatible, &mut native] {
        demo.trigger("area.start", 0).unwrap();
        let view = demo.view().unwrap();
        assert_eq!(
            view.log
                .iter()
                .map(|row| (row.1, row.2.as_str()))
                .collect::<Vec<_>>(),
            [
                (EntityId(3), "first_only"),
                (EntityId(3), "member"),
                (EntityId(1), "member"),
                (EntityId(2), "member"),
                (EntityId(0), "return")
            ]
        );
        assert_eq!(
            view.entities[1].attributes["manpower"].value_micros,
            Some(500_000)
        );
    }
    assert_eq!(compatible.save().unwrap(), native.save().unwrap());
    native.restore(&compatible.save().unwrap()).unwrap();
    for demo in [&mut compatible, &mut native] {
        demo.choose(&EventChoice {
            pending: PendingEventId(0),
            option: "0".into(),
        })
        .unwrap();
        demo.advance(1).unwrap();
    }
    let saved = native.save().unwrap();
    compatible.restore(&saved).unwrap();
    for demo in [&mut compatible, &mut native] {
        demo.advance(1).unwrap();
        let view = demo.view().unwrap();
        assert_eq!(
            view.entities[1].attributes["manpower"].value_micros,
            Some(620_000)
        );
        assert_eq!(
            view.metrics.entities[&EntityId(1)]["devastation"],
            Some(38_000)
        );
        assert_eq!(
            view.log
                .iter()
                .filter(|row| row.2 == "settled")
                .map(|row| row.1)
                .collect::<Vec<_>>(),
            [EntityId(3), EntityId(1), EntityId(2)]
        );
        demo.choose(&EventChoice {
            pending: PendingEventId(1),
            option: "0".into(),
        })
        .unwrap();
    }
    assert_eq!(compatible.save().unwrap(), native.save().unwrap());
    assert_eq!(
        serde_json::to_string(&compatible.view().unwrap()).unwrap(),
        serde_json::to_string(&native.view().unwrap()).unwrap()
    );
}
