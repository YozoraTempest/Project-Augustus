// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{EntityId, EventChoice, PendingEventId};
use augustus_demo::{EventDemo, Input};
use std::path::Path;

#[test]
fn translated_and_native_events_share_cash_tax_receipts_and_restored_pending_choices() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/treasury");
    let mut native = EventDemo::from_directory(&path, Input::Native).unwrap();
    let mut eu4 = EventDemo::from_directory(&path, Input::Eu4).unwrap();
    for (stage, expected) in [
        999, -500, -500, 500, -500, 500, -9999500, -9999500, -9999500,
    ]
    .into_iter()
    .enumerate()
    {
        for demo in [&mut native, &mut eu4] {
            match stage {
                1 => {
                    demo.trigger("cash.begin", 0).unwrap();
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
                demo.world().country_resources()[&EntityId(0)].treasury_milli,
                Some(expected)
            );
            assert_eq!(
                demo.world().country_resources()[&EntityId(1)].treasury_milli,
                Some(if stage < 6 { 500 } else { 501 })
            );
            assert_eq!(
                demo.world().country_resources()[&EntityId(0)].manpower_milli,
                Some(5000)
            );
            let taxation = demo.world().taxation().unwrap();
            assert_eq!(
                taxation.settlement_count,
                if stage < 3 {
                    0
                } else if stage < 5 {
                    1
                } else {
                    2
                }
            );
            if let Some(receipt) = &taxation.last_settlement {
                assert_eq!(receipt.accounts.len(), 1);
                let account = &receipt.accounts[&EntityId(0)];
                assert_eq!(account.treasury_before_milli, Some(-500));
                assert_eq!(account.credited_milli, Some(1000));
                assert_eq!(account.treasury_after_milli, Some(500));
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
            .map(|event| event.2.as_str())
            .collect::<Vec<_>>(),
        [
            "greater",
            "smaller",
            "precise",
            "negative",
            "before_tax",
            "equal",
            "before_second_tax",
            "changed",
            "large_negative",
            "finished"
        ]
    );
}
