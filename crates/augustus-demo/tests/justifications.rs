// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{EntityId, EventChoice, PendingEventId};
use augustus_demo::{EventDemo, Input};

fn scene(input: Input) -> EventDemo {
    EventDemo::from_directory(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/justifications"),
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
fn both_inputs_resume_monthly_grants_and_recheck_choices_after_due_plans() {
    let mut native = scene(Input::Native);
    let mut compatibility = scene(Input::Eu4);
    assert_eq!(native.save().unwrap(), compatibility.save().unwrap());
    for demo in [&mut native, &mut compatibility] {
        demo.trigger("grounds.start", 0).unwrap();
        choose(demo, 0, "0");
        demo.advance(31).unwrap();
    }
    assert_eq!(native.save().unwrap(), compatibility.save().unwrap());
    native.restore(&compatibility.save().unwrap()).unwrap();
    compatibility.restore(&native.save().unwrap()).unwrap();
    for demo in [&mut native, &mut compatibility] {
        demo.advance(20).unwrap();
        let view = demo.view().unwrap();
        assert_eq!(view.pending_events[0].options, ["1"]);
        assert_eq!(
            view.log
                .iter()
                .map(|(day, _, text)| (*day, text.as_str()))
                .collect::<Vec<_>>(),
            [
                (0, "granted"),
                (0, "direct"),
                (0, "any"),
                (0, "shortened"),
                (51, "boundary_present"),
                (51, "instant_absent"),
                (51, "council_present"),
            ]
        );
        assert_eq!(
            view.justifications.active[&EntityId(0)][&EntityId(1)]
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["council"]
        );
        assert!(view.justifications.active[&EntityId(1)][&EntityId(0)].contains_key("honour"));
        choose(demo, 1, "1");
        demo.advance(1).unwrap();
        assert_eq!(demo.view().unwrap().log.last().unwrap().2, "later_absent");
        choose(demo, 2, "0");
        demo.advance(30).unwrap();
        assert!(
            !demo
                .view()
                .unwrap()
                .justifications
                .active
                .contains_key(&EntityId(1))
        );
    }
    assert_eq!(native.save().unwrap(), compatibility.save().unwrap());
    assert_eq!(
        serde_json::to_string(&native.view().unwrap()).unwrap(),
        serde_json::to_string(&compatibility.view().unwrap()).unwrap()
    );
}

#[test]
fn removal_and_inactive_country_qualification_are_shared_by_both_formats() {
    for input in [Input::Native, Input::Eu4] {
        let mut demo = scene(input);
        demo.trigger("grounds.start", 0).unwrap();
        choose(&mut demo, 0, "1");
        let view = demo.view().unwrap();
        assert!(
            !view.justifications.active[&EntityId(0)][&EntityId(1)].contains_key("restoration")
        );
        assert!(!view.justifications.active.contains_key(&EntityId(1)));
        demo.trigger("grounds.dormant", 0).unwrap();
        choose(&mut demo, 1, "0");
        assert!(
            demo.view().unwrap().justifications.active[&EntityId(0)].contains_key(&EntityId(2))
        );
        demo.advance(1).unwrap();
        assert_eq!(
            demo.view().unwrap().log.last().unwrap().2,
            "dormant_cleared"
        );
        assert!(
            !demo.view().unwrap().justifications.active[&EntityId(0)].contains_key(&EntityId(2))
        );
        choose(&mut demo, 2, "0");
        let saved = demo.save().unwrap();
        let mut restored = scene(input);
        restored.restore(&saved).unwrap();
        assert_eq!(saved, restored.save().unwrap());
    }
}
