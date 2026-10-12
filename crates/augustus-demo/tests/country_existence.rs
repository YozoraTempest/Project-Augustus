// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{EventChoice, PendingEventId};
use augustus_demo::{EventDemo, Input};

fn scene(input: Input) -> EventDemo {
    EventDemo::from_directory(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/country-existence"),
        input,
    )
    .unwrap()
}
fn choose(demo: &mut EventDemo, pending: u64, option: &str) {
    demo.choose(&EventChoice {
        pending: PendingEventId(pending),
        option: option.into(),
    })
    .unwrap();
}

#[test]
fn both_formats_share_conditions_and_read_rebound_targets_after_saved_delays() {
    let mut native = scene(Input::Native);
    let mut compatibility = scene(Input::Eu4);
    assert_eq!(native.save().unwrap(), compatibility.save().unwrap());
    for demo in [&mut native, &mut compatibility] {
        demo.trigger("presence.start", 0).unwrap();
        assert_eq!(demo.view().unwrap().pending_events[0].options, ["0"]);
        choose(demo, 0, "0");
        demo.trigger("presence.bind", 1).unwrap();
        choose(demo, 1, "0");
    }
    assert_eq!(native.save().unwrap(), compatibility.save().unwrap());
    native.restore(&compatibility.save().unwrap()).unwrap();
    compatibility.restore(&native.save().unwrap()).unwrap();
    for demo in [&mut native, &mut compatibility] {
        demo.advance(1).unwrap();
        choose(demo, 2, "0");
        demo.trigger("presence.province", 2).unwrap();
        choose(demo, 3, "0");
        assert_eq!(
            demo.view()
                .unwrap()
                .log
                .iter()
                .map(|(day, _, text)| (*day, text.as_str()))
                .collect::<Vec<_>>(),
            [
                (0, "start"),
                (0, "active"),
                (0, "rebound"),
                (1, "later"),
                (1, "done"),
                (1, "province")
            ]
        );
    }
    assert_eq!(native.save().unwrap(), compatibility.save().unwrap());
    assert_eq!(
        serde_json::to_string(&native.view().unwrap()).unwrap(),
        serde_json::to_string(&compatibility.view().unwrap()).unwrap()
    );
}

#[test]
fn inactive_country_is_registered_and_only_its_inactive_option_is_displayed() {
    for input in [Input::Native, Input::Eu4] {
        let mut demo = scene(input);
        demo.trigger("presence.start", 1).unwrap();
        assert_eq!(demo.view().unwrap().pending_events[0].options, ["1"]);
        choose(&mut demo, 0, "1");
        assert_eq!(demo.view().unwrap().log.last().unwrap().2, "inactive");
        let saved = demo.save().unwrap();
        let mut restored = scene(input);
        restored.restore(&saved).unwrap();
        assert_eq!(saved, restored.save().unwrap());
    }
}
