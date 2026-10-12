// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    Action, Condition, CountryController, EntityId, EntityKind, EventCatalog, EventChoice,
    EventDefinition, EventOption, FieldId, PendingEventId, Schema, Target, World,
};
use std::collections::BTreeMap;

fn catalog() -> EventCatalog {
    EventCatalog::new([EventDefinition {
        id: "test".into(),
        scope: EntityKind::Country,
        immediate: vec![Action::Set {
            target: Target::Current,
            field: FieldId(0),
            value: 0,
        }],
        options: ["0", "1", "2"]
            .into_iter()
            .map(|id| EventOption {
                id: id.into(),
                effects: vec![Action::Emit(id.into())],
            })
            .collect(),
        after: vec![Action::Emit("after".into())],
    }])
    .unwrap()
    .with_option_conditions(BTreeMap::from([(
        "test".into(),
        BTreeMap::from([
            (
                "0".into(),
                Condition::AtLeast {
                    target: Target::Current,
                    field: FieldId(0),
                    value: 1,
                },
            ),
            (
                "1".into(),
                Condition::CountryControlled {
                    target: Target::Current,
                    controller: CountryController::Human,
                },
            ),
        ]),
    )]))
    .unwrap()
}

fn world() -> World {
    let mut world = World::new(Schema::new(["gate"]).unwrap());
    let country = world.add_entity("AAA", EntityKind::Country);
    world
        .set_country_controller(country, CountryController::Human)
        .unwrap();
    world.set(country, FieldId(0), 1).unwrap();
    world.set_event_catalog(catalog()).unwrap();
    world
}

#[test]
fn availability_reads_post_immediate_live_state_and_survives_restore() {
    let mut world = world();
    let pending = world.trigger_event("test", EntityId(0)).unwrap();
    let ids = |world: &World| {
        world
            .available_event_options(pending)
            .unwrap()
            .iter()
            .map(|option| option.id.clone())
            .collect::<Vec<_>>()
    };
    let saved = world.save("content").unwrap();
    assert_eq!(ids(&world), ["1", "2"]);
    assert_eq!(world.save("content").unwrap(), saved);
    let mut restored =
        World::restore_with_events(&saved, "content", world.schema(), vec![], catalog()).unwrap();
    assert_eq!(ids(&restored), ["1", "2"]);
    restored.set(EntityId(0), FieldId(0), 1).unwrap();
    restored
        .set_country_controller(EntityId(0), CountryController::Ai)
        .unwrap();
    assert_eq!(ids(&restored), ["0", "2"]);
    assert_eq!(restored.pending_events(), world.pending_events());
    let later = restored.save("content").unwrap();
    let replay =
        World::restore_with_events(&later, "content", world.schema(), vec![], catalog()).unwrap();
    assert_eq!(ids(&replay), ["0", "2"]);
    assert_eq!(replay.save("content").unwrap(), later);
}

#[test]
fn accepted_command_executes_stable_option_without_availability_recheck() {
    let mut world = world();
    let pending = world.trigger_event("test", EntityId(0)).unwrap();
    assert!(
        !world
            .available_event_options(pending)
            .unwrap()
            .iter()
            .any(|option| option.id == "0")
    );
    world
        .choose_event(&EventChoice {
            pending,
            option: "0".into(),
        })
        .unwrap();
    assert_eq!(
        world
            .events()
            .iter()
            .map(|entry| entry.2.as_str())
            .collect::<Vec<_>>(),
        ["0", "after"]
    );
    assert!(world.pending_events().is_empty());
    assert!(world.available_event_options(pending).is_err());
}

#[test]
fn unknown_control_and_invalid_saved_controllers_remain_errors() {
    let mut world = World::new(Schema::new(["gate"]).unwrap());
    let country = world.add_entity("AAA", EntityKind::Country);
    let province = world.add_entity("1", EntityKind::Province);
    assert_eq!(world.country_controller(country).unwrap(), None);
    assert!(
        world
            .set_country_controller(province, CountryController::Human)
            .is_err()
    );
    world.set_event_catalog(catalog()).unwrap();
    let pending = world.trigger_event("test", country).unwrap();
    let before = world.save("content").unwrap();
    assert!(
        world
            .available_event_options(pending)
            .unwrap_err()
            .to_string()
            .contains("unknown country controller")
    );
    let condition = Condition::Not(Box::new(Condition::CountryControlled {
        target: Target::Current,
        controller: CountryController::Ai,
    }));
    assert!(condition.evaluate(&world, country).is_err());
    assert_eq!(world.save("content").unwrap(), before);
    let mut invalid: serde_json::Value = serde_json::from_str(&before).unwrap();
    invalid["country_controllers"] = serde_json::json!({"1":"human"});
    assert!(
        World::restore_with_events(
            &invalid.to_string(),
            "content",
            world.schema(),
            vec![],
            catalog()
        )
        .is_err()
    );
    assert!(world.available_event_options(PendingEventId(99)).is_err());
}

#[test]
fn option_condition_references_are_validated_even_when_unavailable() {
    let unknown_event = BTreeMap::from([("absent".into(), BTreeMap::new())]);
    assert!(catalog().with_option_conditions(unknown_event).is_err());
    let unknown_option = BTreeMap::from([(
        "test".into(),
        BTreeMap::from([("absent".into(), Condition::Always(false))]),
    )]);
    assert!(catalog().with_option_conditions(unknown_option).is_err());
    let invalid_field = BTreeMap::from([(
        "test".into(),
        BTreeMap::from([(
            "0".into(),
            Condition::All(vec![
                Condition::Always(false),
                Condition::AtLeast {
                    target: Target::Current,
                    field: FieldId(8),
                    value: 0,
                },
            ]),
        )]),
    )]);
    assert!(
        world()
            .set_event_catalog(catalog().with_option_conditions(invalid_field).unwrap())
            .is_err()
    );
}

#[test]
fn no_eligible_options_keeps_pending_record_and_does_not_invent_a_choice() {
    let mut world = world();
    world
        .set_event_catalog(
            catalog()
                .with_option_conditions(BTreeMap::from([(
                    "test".into(),
                    ["0", "1", "2"]
                        .into_iter()
                        .map(|id| (id.into(), Condition::Always(false)))
                        .collect(),
                )]))
                .unwrap(),
        )
        .unwrap();
    let pending = world.trigger_event("test", EntityId(0)).unwrap();
    let before = world.save("content").unwrap();
    assert!(world.available_event_options(pending).unwrap().is_empty());
    assert_eq!(world.pending_events().len(), 1);
    assert_eq!(world.save("content").unwrap(), before);
}
