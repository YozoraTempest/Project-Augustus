// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::path::Path;

use augustus_core::{EventChoice, PendingEventId};
use augustus_demo::{EventDemo, Input};

#[test]
fn membership_queries_match_across_inputs_and_delayed_restore_for_both_countries() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/imperial-membership");
    for entity in [0, 1] {
        let mut compatible = EventDemo::from_directory(&root, Input::Eu4).unwrap();
        let mut native = EventDemo::from_directory(&root, Input::Native).unwrap();
        assert_eq!(compatible.save().unwrap(), native.save().unwrap());
        for demo in [&mut compatible, &mut native] {
            demo.trigger("membership.start", entity).unwrap();
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
            if entity == 0 {
                demo.choose(&EventChoice {
                    pending: PendingEventId(1),
                    option: "0".into(),
                })
                .unwrap();
            }
            let log = demo
                .world()
                .events()
                .iter()
                .map(|row| row.2.as_str())
                .collect::<Vec<_>>();
            assert_eq!(
                log,
                if entity == 0 {
                    vec![
                        "country_member",
                        "province_member",
                        "later_member",
                        "accepted",
                    ]
                } else {
                    vec!["country_outside", "province_outside"]
                }
            );
        }
        assert_eq!(compatible.save().unwrap(), native.save().unwrap());
        assert_eq!(
            serde_json::to_string(&compatible.view().unwrap()).unwrap(),
            serde_json::to_string(&native.view().unwrap()).unwrap()
        );
    }
}
