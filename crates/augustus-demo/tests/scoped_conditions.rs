// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{EntityId, EventChoice, PendingEventId};
use augustus_demo::{EventDemo, Input};

fn scene(input: Input) -> EventDemo {
    EventDemo::from_directory(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/scoped-conditions"),
        input,
    )
    .unwrap()
}

fn choose(demo: &mut EventDemo, id: u64) {
    demo.choose(&EventChoice {
        pending: PendingEventId(id),
        option: "0".into(),
    })
    .unwrap();
}

#[test]
fn scoped_guards_and_rebound_targets_survive_cross_input_restore() {
    let mut compatibility = scene(Input::Eu4);
    let mut native = scene(Input::Native);
    assert_eq!(compatibility.save().unwrap(), native.save().unwrap());
    compatibility.trigger("scopes.start", 0).unwrap();
    assert_eq!(
        compatibility.view().unwrap().pending_events[0].options,
        ["0"]
    );
    native.restore(&compatibility.save().unwrap()).unwrap();
    for demo in [&mut compatibility, &mut native] {
        choose(demo, 0);
        demo.trigger("scopes.rebind", 1).unwrap();
        assert_eq!(demo.world().global_scopes()["place"], EntityId(3));
    }
    compatibility.restore(&native.save().unwrap()).unwrap();
    for demo in [&mut compatibility, &mut native] {
        demo.advance(1).unwrap();
        assert_eq!(demo.view().unwrap().pending_events[1].options, ["0"]);
        choose(demo, 1);
        choose(demo, 2);
        demo.trigger("scopes.sea", 4).unwrap();
        assert_eq!(demo.view().unwrap().pending_events[0].options, ["0"]);
        choose(demo, 3);
        assert!(demo.world().pending_events().is_empty());
        assert_eq!(
            demo.view()
                .unwrap()
                .log
                .iter()
                .map(|(day, id, text)| (*day, id.0, text.as_str()))
                .collect::<Vec<_>>(),
            [
                (0, 0, "start"),
                (0, 0, "accepted"),
                (1, 0, "changed"),
                (1, 0, "done"),
                (1, 4, "unowned"),
                (1, 4, "sea")
            ]
        );
    }
    assert_eq!(compatibility.save().unwrap(), native.save().unwrap());
    assert_eq!(
        serde_json::to_string(&compatibility.view().unwrap()).unwrap(),
        serde_json::to_string(&native.view().unwrap()).unwrap()
    );
}
