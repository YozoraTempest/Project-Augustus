// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_demo::{Demo, Input, System};

fn view(demo: &Demo) -> serde_json::Value {
    serde_json::to_value(demo.view().unwrap()).unwrap()
}

#[test]
fn both_inputs_pay_once_finish_and_resume_identically() {
    for system in [System::Fixed, System::Capacity] {
        let mut legacy = Demo::embedded(Input::Eu4, system).unwrap();
        let mut native = Demo::embedded(Input::Native, system).unwrap();
        for demo in [&mut legacy, &mut native] {
            demo.build(0, 0).unwrap();
            demo.build(1, 1).unwrap();
            assert_eq!(demo.view().unwrap().treasury_milli, 650_000);
            let before = demo.save().unwrap();
            assert!(demo.build(0, 1).is_err());
            assert_eq!(before, demo.save().unwrap());
            demo.advance(14).unwrap();
        }
        let mut resumed = Demo::embedded(Input::Eu4, system).unwrap();
        resumed.restore(&legacy.save().unwrap()).unwrap();
        legacy.advance(86).unwrap();
        native.advance(86).unwrap();
        for _ in 0..86 {
            resumed.advance(1).unwrap();
        }
        assert_eq!(legacy.save().unwrap(), resumed.save().unwrap());
        let mut legacy_view = view(&legacy);
        let mut native_view = view(&native);
        legacy_view.as_object_mut().unwrap().remove("input");
        native_view.as_object_mut().unwrap().remove("input");
        assert_eq!(legacy_view, native_view);
        assert_eq!(legacy.view().unwrap().treasury_milli, 667_500);
        assert_eq!(legacy.view().unwrap().pending_tasks, 0);
        assert!(legacy.build(0, 0).is_err());
    }
}

#[test]
fn selected_provider_replaces_the_complete_settlement() {
    let mut fixed = Demo::embedded(Input::Eu4, System::Fixed).unwrap();
    let mut capacity = Demo::embedded(Input::Eu4, System::Capacity).unwrap();
    fixed.build(1, 0).unwrap();
    capacity.build(1, 0).unwrap();
    fixed.advance(15).unwrap();
    capacity.advance(15).unwrap();
    assert!(fixed.view().unwrap().provinces[1].buildings.is_empty());
    assert_eq!(capacity.view().unwrap().provinces[1].buildings, ["market"]);
    assert_eq!(capacity.view().unwrap().pending_tasks, 0);
    capacity.advance(15).unwrap();
    assert_eq!(
        capacity
            .view()
            .unwrap()
            .events
            .iter()
            .filter(|event| event.2 == "completed:market")
            .count(),
        1
    );
    assert!(fixed.restore(&capacity.save().unwrap()).is_err());
}

#[test]
fn directory_compilation_and_embedded_scene_agree() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/construction");
    let mut embedded = Demo::embedded(Input::Eu4, System::Fixed).unwrap();
    let mut directory = Demo::from_directory(&root, Input::Eu4, System::Fixed, true).unwrap();
    let mut unoptimised = Demo::from_directory(&root, Input::Eu4, System::Fixed, false).unwrap();
    for demo in [&mut embedded, &mut directory, &mut unoptimised] {
        demo.build(0, 0).unwrap();
        demo.advance(30).unwrap();
    }
    assert_eq!(view(&embedded), view(&directory));
    assert_eq!(view(&directory), view(&unoptimised));
}

#[test]
fn capacity_changes_and_random_stream_resume_with_the_unfinished_queue() {
    let mut uninterrupted = Demo::embedded(Input::Eu4, System::Capacity).unwrap();
    uninterrupted.build(1, 1).unwrap();
    uninterrupted.reorganise(1).unwrap();
    uninterrupted.advance(14).unwrap();
    let mut resumed = Demo::embedded(Input::Eu4, System::Capacity).unwrap();
    resumed.restore(&uninterrupted.save().unwrap()).unwrap();
    for day in 0..100 {
        if day % 7 == 0 {
            uninterrupted.reorganise(1).unwrap();
            resumed.reorganise(1).unwrap();
        }
        uninterrupted.advance(1).unwrap();
        resumed.advance(1).unwrap();
        assert_eq!(uninterrupted.save().unwrap(), resumed.save().unwrap());
    }
    assert_eq!(
        uninterrupted
            .view()
            .unwrap()
            .events
            .iter()
            .filter(|event| event.2 == "completed:workshop")
            .count(),
        1
    );
}

#[test]
fn malformed_construction_state_is_rejected_without_replacing_the_session() {
    let mut demo = Demo::embedded(Input::Eu4, System::Capacity).unwrap();
    demo.build(0, 0).unwrap();
    let snapshot = demo.save().unwrap();
    let original: serde_json::Value = serde_json::from_str(&snapshot).unwrap();
    let fields = original["schema"]["fields"].as_array().unwrap();
    for (name, value) in [("queued", 99), ("capacity", 0), ("building/market", 1)] {
        let mut changed = original.clone();
        let index = fields.iter().position(|field| field == name).unwrap();
        changed["entities"][1]["values"][index] = value.into();
        assert!(demo.restore(&changed.to_string()).is_err());
        assert_eq!(demo.save().unwrap(), snapshot);
    }
    let mut missing_task = original;
    missing_task["tasks"] = serde_json::json!([]);
    assert!(demo.restore(&missing_task.to_string()).is_err());
    assert_eq!(demo.save().unwrap(), snapshot);
}

#[test]
fn an_external_provider_replaces_starting_cost_and_future_settlement() {
    use augustus_core::{Action, EntityId, Plan, Target, World};
    use augustus_demo::{Building, ConstructionSystem, Error, Slots};

    struct InstantConstruction;
    impl ConstructionSystem for InstantConstruction {
        fn id(&self) -> &'static str {
            "instant:1"
        }
        fn rules(&self, _: &[Building], _: Slots) -> Vec<Plan> {
            Vec::new()
        }
        fn start(
            &self,
            world: &World,
            province: EntityId,
            _: usize,
            building: &Building,
            _: Slots,
        ) -> Result<Plan, Error> {
            if world.value(province, building.built_field)? != 0 {
                return Err(Error::Invalid("building already exists".into()));
            }
            let mut plan = vec![Action::Set {
                target: Target::Current,
                field: building.built_field,
                value: 1,
            }];
            plan.extend(building.completion.clone());
            Ok(plan)
        }
        fn reorganise(&self, _: Slots) -> Result<Plan, Error> {
            Err(Error::Invalid(
                "instant construction has no work schedule".into(),
            ))
        }
    }

    for input in [Input::Eu4, Input::Native] {
        let mut demo = Demo::embedded_with_system(input, Box::new(InstantConstruction)).unwrap();
        demo.build(0, 0).unwrap();
        assert_eq!(demo.view().unwrap().treasury_milli, 1_005_000);
        assert_eq!(demo.view().unwrap().provinces[0].buildings, ["market"]);
        assert_eq!(demo.view().unwrap().pending_tasks, 0);
        let mut resumed = Demo::embedded_with_system(input, Box::new(InstantConstruction)).unwrap();
        resumed.restore(&demo.save().unwrap()).unwrap();
        demo.advance(100).unwrap();
        resumed.advance(100).unwrap();
        assert_eq!(demo.save().unwrap(), resumed.save().unwrap());
        assert_eq!(demo.view().unwrap().treasury_milli, 1_005_000);
        assert_eq!(demo.view().unwrap().events.len(), 1);
    }
}
