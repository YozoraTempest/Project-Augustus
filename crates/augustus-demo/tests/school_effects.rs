// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{EventChoice, PendingEventId};
use augustus_demo::{EventDemo, Input};

fn scene(input: Input) -> EventDemo {
    EventDemo::from_directory(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/school-effects"),
        input,
    )
    .unwrap()
}
fn value(demo: &EventDemo, entity: usize) -> Option<i64> {
    demo.view().unwrap().entities[entity].attributes["yield"].value_micros
}
fn choose(demo: &mut EventDemo, id: u64) {
    demo.choose(&EventChoice {
        pending: PendingEventId(id),
        option: "0".into(),
    })
    .unwrap();
}

#[test]
fn native_and_compatibility_events_share_live_school_effects_across_saved_delays() {
    let mut native = scene(Input::Native);
    let mut compatibility = scene(Input::Eu4);
    assert_eq!(native.save().unwrap(), compatibility.save().unwrap());
    assert_eq!(value(&native, 0), Some(1_000_000));
    compatibility.trigger("choice.start", 0).unwrap();
    assert_eq!(value(&compatibility, 0), Some(1_250_000));
    native.restore(&compatibility.save().unwrap()).unwrap();
    for demo in [&mut native, &mut compatibility] {
        choose(demo, 0);
        assert_eq!(value(demo, 0), Some(500_000));
    }
    compatibility.restore(&native.save().unwrap()).unwrap();
    for demo in [&mut native, &mut compatibility] {
        demo.advance(1).unwrap();
        assert_eq!(value(demo, 0), Some(1_250_000));
        choose(demo, 1);
        assert_eq!(
            demo.view()
                .unwrap()
                .log
                .iter()
                .map(|(day, _, text)| (*day, text.as_str()))
                .collect::<Vec<_>>(),
            [(0, "first"), (0, "second"), (1, "later"), (1, "done")]
        );
    }
    assert_eq!(native.save().unwrap(), compatibility.save().unwrap());
    assert_eq!(
        serde_json::to_string(&native.view().unwrap()).unwrap(),
        serde_json::to_string(&compatibility.view().unwrap()).unwrap()
    );
}

#[test]
fn incompatible_current_religion_group_keeps_school_absent_in_both_formats() {
    for input in [Input::Native, Input::Eu4] {
        let mut demo = scene(input);
        demo.trigger("choice.foreign", 1).unwrap();
        assert_eq!(value(&demo, 1), Some(1_000_000));
        assert_eq!(
            demo.world()
                .affiliations()
                .present("religious_school", augustus_core::EntityId(1)),
            Some(false)
        );
        choose(&mut demo, 0);
        let saved = demo.save().unwrap();
        let mut resumed = scene(input);
        resumed.restore(&saved).unwrap();
        assert_eq!(saved, resumed.save().unwrap());
    }
}
