// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{EntityId, EventChoice, PendingEventId};
use augustus_demo::{EventDemo, Input};
use std::path::Path;

#[test]
fn native_and_compatibility_inputs_share_prestige_state() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/prestige");
    let mut native = EventDemo::from_directory(&path, Input::Native).unwrap();
    let mut eu4 = EventDemo::from_directory(&path, Input::Eu4).unwrap();
    for (stage, expected) in [
        12001, 50000, 50000, 49875, -50000, -49525, 100000, 100000, 100000,
    ]
    .into_iter()
    .enumerate()
    {
        for demo in [&mut native, &mut eu4] {
            match stage {
                1 => {
                    demo.trigger("prestige.begin", 0).unwrap();
                }
                2 | 4 | 6 | 8 => {
                    demo.choose(&EventChoice {
                        pending: PendingEventId((stage / 2 - 1) as u64),
                        option: "0".into(),
                    })
                    .unwrap();
                }
                3 => demo.advance(20).unwrap(),
                5 => demo.advance(31).unwrap(),
                7 => demo.advance(1).unwrap(),
                _ => (),
            }
            assert_eq!(
                demo.world().metrics().entities[&EntityId(0)]["prestige"],
                Some(expected)
            );
            assert_eq!(
                demo.world().metrics().entities[&EntityId(1)]["prestige"],
                Some(5000)
            );
        }
        assert_eq!(native.save().unwrap(), eu4.save().unwrap(), "stage {stage}");
        let saved = native.save().unwrap();
        native.restore(&eu4.save().unwrap()).unwrap();
        eu4.restore(&saved).unwrap();
    }
    assert_eq!(
        native
            .world()
            .events()
            .iter()
            .map(|event| event.2.as_str())
            .collect::<Vec<_>>(),
        [
            "initial",
            "quantized",
            "upper_bound",
            "country_compare",
            "before_decay",
            "before_recovery",
            "bounded",
            "finished"
        ]
    );
}
