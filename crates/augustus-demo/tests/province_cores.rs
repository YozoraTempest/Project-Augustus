// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{CoreType, EntityId, EventChoice, PendingEventId};
use augustus_demo::{EventDemo, Input};
use std::path::Path;

#[test]
fn both_inputs_mutate_the_same_core_state_and_restore_pending_choices_across_formats() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/province-cores");
    let mut native = EventDemo::from_directory(&path, Input::Native).unwrap();
    let mut eu4 = EventDemo::from_directory(&path, Input::Eu4).unwrap();
    for stage in 0..7 {
        for demo in [&mut native, &mut eu4] {
            match stage {
                1 => {
                    demo.trigger("cores.seed", 0).unwrap();
                }
                2 | 4 | 6 => {
                    demo.choose(&EventChoice {
                        pending: PendingEventId((stage / 2 - 1) as u64),
                        option: "0".into(),
                    })
                    .unwrap();
                }
                3 => {
                    demo.trigger("cores.promote", 0).unwrap();
                }
                5 => {
                    demo.trigger("cores.province", 3).unwrap();
                }
                _ => (),
            }
            let view = demo.view().unwrap();
            assert_eq!(
                demo.world().entity(EntityId(4)).unwrap().owner,
                Some(EntityId(1))
            );
            assert_eq!(
                view.administration.provinces[&EntityId(4)]
                    .cores
                    .get(&EntityId(0))
                    .copied(),
                match stage {
                    1 | 3 => Some(CoreType::Full),
                    2 => Some(CoreType::Territorial),
                    _ => None,
                }
            );
            assert_eq!(
                view.administration.provinces[&EntityId(3)]
                    .cores
                    .get(&EntityId(2))
                    .copied(),
                if stage == 5 {
                    Some(CoreType::Territorial)
                } else {
                    None
                }
            );
        }
        assert_eq!(native.save().unwrap(), eu4.save().unwrap(), "stage {stage}");
        assert_eq!(
            serde_json::to_string(&native.view().unwrap()).unwrap(),
            serde_json::to_string(&eu4.view().unwrap()).unwrap()
        );
        let native_save = native.save().unwrap();
        native.restore(&eu4.save().unwrap()).unwrap();
        eu4.restore(&native_save).unwrap();
    }
}
