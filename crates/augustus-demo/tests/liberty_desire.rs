// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{EntityId, EventChoice, PendingEventId};
use augustus_demo::{EventDemo, Input};
use std::path::Path;

#[test]
fn both_inputs_share_unclipped_stock_months_and_pending_choices_across_restore() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/liberty-desire");
    let mut native = EventDemo::from_directory(&path, Input::Native).unwrap();
    let mut eu4 = EventDemo::from_directory(&path, Input::Eu4).unwrap();
    for (stage, expected) in [0, 150000, 100000, 0, 500, 400, -600, -500, -500]
        .into_iter()
        .enumerate()
    {
        for demo in [&mut native, &mut eu4] {
            match stage {
                1 => {
                    demo.trigger("ld.begin", 0).unwrap();
                }
                2 | 4 | 6 | 8 => {
                    demo.choose(&EventChoice {
                        pending: PendingEventId((stage / 2 - 1) as u64),
                        option: "0".into(),
                    })
                    .unwrap();
                }
                3 => demo.advance(1).unwrap(),
                5 => demo.advance(20).unwrap(),
                7 => demo.advance(31).unwrap(),
                _ => (),
            }
            assert_eq!(
                demo.world()
                    .metrics()
                    .value(demo.world(), EntityId(1), "temporary_liberty_desire")
                    .unwrap(),
                expected
            );
            let effective = demo
                .world()
                .attributes()
                .evaluate(demo.world(), EntityId(1), "liberty_desire")
                .unwrap()
                .value_micros;
            assert_eq!(
                effective,
                Some((15_000_000 + expected * 1000).clamp(0, 100_000_000))
            );
            if stage >= 5 {
                assert_eq!(
                    demo.world()
                        .metrics()
                        .value(demo.world(), EntityId(2), "temporary_liberty_desire")
                        .unwrap(),
                    0
                );
            }
        }
        assert_eq!(native.save().unwrap(), eu4.save().unwrap(), "stage {stage}");
        assert_eq!(
            serde_json::to_string(&native.view().unwrap()).unwrap(),
            serde_json::to_string(&eu4.view().unwrap()).unwrap()
        );
        let saved = native.save().unwrap();
        native.restore(&eu4.save().unwrap()).unwrap();
        eu4.restore(&saved).unwrap();
    }
    assert_eq!(
        native
            .world()
            .events()
            .iter()
            .map(|e| e.2.as_str())
            .collect::<Vec<_>>(),
        [
            "clipped",
            "retained",
            "recovered",
            "independent",
            "positive_month",
            "negative_month",
            "finished"
        ]
    );
}
