// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    Action, Condition, EntityId, EntityKind, EventCatalog, EventChoice, EventDefinition,
    EventOption, FieldId, PendingEventId, Schema, Target, World,
};

fn set(target: Target, value: i64) -> Action {
    Action::Set {
        target,
        field: FieldId(0),
        value,
    }
}

fn definition() -> EventDefinition {
    EventDefinition {
        id: "council".into(),
        scope: EntityKind::Country,
        immediate: vec![set(Target::Current, 1), Action::Emit("immediate".into())],
        options: vec![EventOption {
            id: "accept".into(),
            effects: vec![
                Action::If {
                    condition: Condition::AtLeast {
                        target: Target::Current,
                        field: FieldId(0),
                        value: 10,
                    },
                    then: vec![set(Target::Current, 20)],
                    otherwise: vec![set(Target::Current, -1)],
                },
                Action::Emit("option".into()),
            ],
        }],
        after: vec![
            Action::With {
                target: Target::Capital,
                plan: vec![Action::Accumulate {
                    target: Target::Current,
                    field: FieldId(0),
                    source: FieldId(0),
                }],
            },
            Action::Emit("after".into()),
        ],
    }
}

fn scenario(event: EventDefinition) -> World {
    let mut world = World::new(Schema::new(["value"]).unwrap());
    let country = world.add_entity("Union", EntityKind::Country);
    let capital = world.add_entity("Harbor", EntityKind::Province);
    world.set_owner(capital, country).unwrap();
    world.set_capital(country, capital).unwrap();
    world.set(capital, FieldId(0), 3).unwrap();
    world
        .set_event_catalog(EventCatalog::new([event]).unwrap())
        .unwrap();
    world
}

fn choose(id: PendingEventId) -> EventChoice {
    EventChoice {
        pending: id,
        option: "accept".into(),
    }
}

#[test]
fn choice_observes_live_state_and_relationships_of_captured_entity() {
    let mut world = scenario(definition());
    let country = EntityId(0);
    let pending = world.trigger_event("council", country).unwrap();
    assert_eq!(world.value(country, FieldId(0)).unwrap(), 1);
    assert_eq!(world.events().len(), 1);
    assert_eq!(world.pending_events()[0].current, country);
    // Changes while a choice waits must not restore an old world or old capital.
    world.set(country, FieldId(0), 10).unwrap();
    let new_capital = world.add_entity("New capital", EntityKind::Province);
    world.set_owner(new_capital, country).unwrap();
    world.set_capital(country, new_capital).unwrap();
    world.set(new_capital, FieldId(0), 4).unwrap();
    world.choose_event(&choose(pending)).unwrap();
    assert_eq!(world.value(country, FieldId(0)).unwrap(), 20);
    assert_eq!(world.value(EntityId(1), FieldId(0)).unwrap(), 3);
    assert_eq!(world.value(new_capital, FieldId(0)).unwrap(), 8);
    assert_eq!(
        world
            .events()
            .iter()
            .map(|event| event.2.as_str())
            .collect::<Vec<_>>(),
        ["immediate", "option", "after"]
    );
    assert!(world.pending_events().is_empty());
}

#[test]
fn waiting_and_completed_choices_resume_without_repeating_effects_or_ids() {
    let mut continuous = scenario(definition());
    let first = continuous.trigger_event("council", EntityId(0)).unwrap();
    let second = continuous.trigger_event("council", EntityId(0)).unwrap();
    continuous.advance_to(9).unwrap();
    let save = continuous.save("test:v1").unwrap();
    let mut resumed = World::restore_with_events(
        &save,
        "test:v1",
        continuous.schema(),
        Vec::new(),
        continuous.event_catalog().clone(),
    )
    .unwrap();
    assert_eq!(continuous, resumed);
    for world in [&mut continuous, &mut resumed] {
        world.choose_event(&choose(second)).unwrap();
        assert_eq!(world.pending_events()[0].id, first);
        world.choose_event(&choose(first)).unwrap();
        world.advance_to(12).unwrap();
    }
    assert_eq!(
        continuous.save("test:v1").unwrap(),
        resumed.save("test:v1").unwrap()
    );
    let save = resumed.save("test:v1").unwrap();
    let mut completed = World::restore_with_events(
        &save,
        "test:v1",
        resumed.schema(),
        Vec::new(),
        resumed.event_catalog().clone(),
    )
    .unwrap();
    assert_eq!(
        completed.trigger_event("council", EntityId(0)).unwrap(),
        PendingEventId(2)
    );
    assert!(completed.choose_event(&choose(second)).is_err());
}

#[test]
fn invalid_and_repeated_commands_do_not_write_or_stop_the_world() {
    let mut world = scenario(definition());
    let id = world.trigger_event("council", EntityId(0)).unwrap();
    let before = world.clone();
    assert!(
        world
            .choose_event(&EventChoice {
                pending: id,
                option: "absent".into()
            })
            .is_err()
    );
    assert!(world.choose_event(&choose(PendingEventId(99))).is_err());
    assert!(world.trigger_event("absent", EntityId(0)).is_err());
    assert!(world.trigger_event("council", EntityId(1)).is_err());
    assert_eq!(world, before);
    world.choose_event(&choose(id)).unwrap();
    let before = world.clone();
    assert!(world.choose_event(&choose(id)).is_err());
    assert_eq!(world, before);
    world.advance_to(1).unwrap();
}

#[test]
fn after_reads_option_writes_and_uses_the_same_instruction_budget() {
    let mut event = definition();
    event.options[0].effects = vec![set(Target::Current, 8)];
    event.after = vec![Action::If {
        condition: Condition::AtLeast {
            target: Target::Current,
            field: FieldId(0),
            value: 8,
        },
        then: vec![set(Target::Current, 9)],
        otherwise: vec![set(Target::Current, -1)],
    }];
    let mut world = scenario(event);
    let pending = world.trigger_event("council", EntityId(0)).unwrap();
    world.choose_event(&choose(pending)).unwrap();
    assert_eq!(world.value(EntityId(0), FieldId(0)).unwrap(), 9);

    let mut event = definition();
    event.options[0].effects = vec![set(Target::Current, 8); 60_000];
    event.after = vec![set(Target::Current, 9); 60_000];
    let mut world = scenario(event);
    let pending = world.trigger_event("council", EntityId(0)).unwrap();
    let error = world.choose_event(&choose(pending)).unwrap_err();
    assert!(error.to_string().contains("instruction limit"));
    assert_eq!(world.value(EntityId(0), FieldId(0)).unwrap(), 9);
    assert_eq!(world.pending_events()[0].id, pending);
    assert!(world.save("test:v1").is_err());
    assert!(world.advance_to(1).is_err());
}

#[test]
fn effect_failure_preserves_prefix_and_stops_without_claiming_completion() {
    let mut event = definition();
    event.options[0].effects = vec![set(Target::Current, i64::MAX)];
    event.after = vec![
        Action::Add {
            target: Target::Current,
            field: FieldId(0),
            value: 1,
        },
        Action::Emit("incorrect".into()),
    ];
    let mut world = scenario(event);
    let id = world.trigger_event("council", EntityId(0)).unwrap();
    assert!(world.choose_event(&choose(id)).is_err());
    assert_eq!(world.value(EntityId(0), FieldId(0)).unwrap(), i64::MAX);
    assert_eq!(world.pending_events()[0].id, id);
    assert_eq!(world.events().len(), 1);
    assert!(world.choose_event(&choose(id)).is_err());
    assert!(world.save("test:v1").is_err());

    let mut event = definition();
    event.immediate = vec![
        set(Target::Current, i64::MAX),
        Action::Add {
            target: Target::Current,
            field: FieldId(0),
            value: 1,
        },
    ];
    let mut world = scenario(event);
    assert!(world.trigger_event("council", EntityId(0)).is_err());
    assert!(world.pending_events().is_empty());
    assert!(world.save("test:v1").is_err());
}

#[test]
fn corrupt_pending_records_and_missing_content_are_rejected_on_restore() {
    let mut world = scenario(definition());
    world.trigger_event("council", EntityId(0)).unwrap();
    let save = world.save("test:v1").unwrap();
    assert!(World::restore(&save, "test:v1", world.schema()).is_err());
    for (field, value) in [
        ("id", serde_json::json!(1)),
        ("event", serde_json::json!("missing")),
        ("current", serde_json::json!(1)),
        ("current", serde_json::json!(500)),
        ("day", serde_json::json!(1)),
    ] {
        let mut json: serde_json::Value = serde_json::from_str(&save).unwrap();
        json["event_queue"]["pending"][0][field] = value;
        assert!(
            World::restore_with_events(
                &json.to_string(),
                "test:v1",
                world.schema(),
                Vec::new(),
                world.event_catalog().clone()
            )
            .is_err(),
            "{field}"
        );
    }
    let mut json: serde_json::Value = serde_json::from_str(&save).unwrap();
    let record = json["event_queue"]["pending"][0].clone();
    json["event_queue"]["pending"]
        .as_array_mut()
        .unwrap()
        .push(record);
    assert!(
        World::restore_with_events(
            &json.to_string(),
            "test:v1",
            world.schema(),
            Vec::new(),
            world.event_catalog().clone()
        )
        .is_err()
    );
}

#[test]
fn catalog_validation_and_replacement_preserve_session_content() {
    assert!(EventCatalog::new([definition(), definition()]).is_err());
    let mut event = definition();
    event.options.push(event.options[0].clone());
    assert!(EventCatalog::new([event]).is_err());
    let mut event = definition();
    event.after.push(Action::Set {
        target: Target::Current,
        field: FieldId(99),
        value: 0,
    });
    let mut world = scenario(definition());
    assert!(
        world
            .set_event_catalog(EventCatalog::new([event]).unwrap())
            .is_err()
    );
    let pending = world.trigger_event("council", EntityId(0)).unwrap();
    assert!(world.set_event_catalog(EventCatalog::default()).is_err());
    world.choose_event(&choose(pending)).unwrap();
    assert!(world.set_event_catalog(EventCatalog::default()).is_err());
}

#[test]
fn worlds_without_event_choices_keep_the_existing_save_shape() {
    let world = World::new(Schema::new(["value"]).unwrap());
    let save = world.save("old:v1").unwrap();
    assert!(!save.contains("event_queue"));
    assert_eq!(
        World::restore(&save, "old:v1", world.schema()).unwrap(),
        world
    );
}
