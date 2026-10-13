// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{EntityId, EventChoice, PendingEventId};
use augustus_demo::{EventDemo, Input};
use std::path::Path;

#[test]
fn both_inputs_share_stocks_monthly_flows_and_pending_choices_across_restore() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/manpower");
    let mut native = EventDemo::from_directory(&path, Input::Native).unwrap();
    let mut eu4 = EventDemo::from_directory(&path, Input::Eu4).unwrap();
    for (stage, expected) in [
        1501000, 101501000, 101501000, 101501000, 501000, 1141000, 0, 0, 0,
    ]
    .into_iter()
    .enumerate()
    {
        for demo in [&mut native, &mut eu4] {
            match stage {
                1 => {
                    demo.trigger("mp.begin", 0).unwrap();
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
                demo.world().country_resources()[&EntityId(0)].manpower_milli,
                Some(expected)
            );
            assert_eq!(
                demo.world().country_resources()[&EntityId(1)].manpower_milli,
                Some(if stage == 0 { 0 } else { 500000 })
            );
            assert_eq!(
                demo.world().country_resources()[&EntityId(0)].treasury_milli,
                Some(5000)
            );
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
            .map(|event| event.2.as_str())
            .collect::<Vec<_>>(),
        [
            "per_literal",
            "excess",
            "excess_preserved",
            "before_growth",
            "zero",
            "finished"
        ]
    );
}
