// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{EventChoice, PendingEventId};
use augustus_demo::{EventDemo, Input};
use std::path::Path;

fn scene(input: Input) -> EventDemo {
    EventDemo::from_directory(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/government-stored-values"),
        input,
    )
    .unwrap()
}

#[test]
fn both_inputs_preserve_inactive_values_and_cross_restore_delayed_queries() {
    let mut compatible = scene(Input::Eu4);
    let mut native = scene(Input::Native);
    assert_eq!(compatible.save().unwrap(), native.save().unwrap());
    for demo in [&mut compatible, &mut native] {
        demo.trigger("stored.start", 1).unwrap();
        assert_eq!(
            demo.view()
                .unwrap()
                .log
                .iter()
                .map(|row| row.2.as_str())
                .collect::<Vec<_>>(),
            ["retained", "owner"]
        );
        demo.choose(&EventChoice {
            pending: PendingEventId(0),
            option: "0".into(),
        })
        .unwrap();
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
            ["retained", "owner", "after", "later"]
        );
    }
    assert_eq!(compatible.save().unwrap(), native.save().unwrap());
    assert_eq!(
        serde_json::to_string(&compatible.view().unwrap()).unwrap(),
        serde_json::to_string(&native.view().unwrap()).unwrap()
    );
}
