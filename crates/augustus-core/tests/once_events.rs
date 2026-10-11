// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    Action, Condition, EntityId, EntityKind, EventCatalog, EventChoice, EventDefinition,
    EventOption, FieldId, PendingEventId, Schema, Target, World,
};

fn event(id: &str, scope: EntityKind, immediate: Vec<Action>) -> EventDefinition {
    EventDefinition {
        id: id.into(),
        scope,
        immediate,
        options: vec![EventOption {
            id: "accept".into(),
            effects: Vec::new(),
        }],
        after: Vec::new(),
    }
}

fn call(id: &str) -> Action {
    Action::TriggerEvent { event: id.into() }
}
fn conditional(id: &str) -> Action {
    Action::TryEvent { event: id.into() }
}

fn world(definitions: Vec<EventDefinition>, once: &[&str]) -> World {
    let mut world = World::new(Schema::new(["phase"]).unwrap());
    world.add_entity("Union", EntityKind::Country);
    world.add_entity("League", EntityKind::Country);
    world.add_entity("Port", EntityKind::Province);
    world.add_entity("Market", EntityKind::Province);
    world
        .set_event_catalog(
            EventCatalog::new(definitions)
                .unwrap()
                .with_once_events(once.iter().map(|id| (*id).into()).collect())
                .unwrap(),
        )
        .unwrap();
    world
}

fn resume(world: &World) -> World {
    World::restore_with_events(
        &world.save("once:v1").unwrap(),
        "once:v1",
        world.schema(),
        Vec::new(),
        world.event_catalog().clone(),
    )
    .unwrap()
}

#[test]
fn once_records_are_shared_across_entities_and_survive_waiting_and_completed_choices() {
    let mut country = event(
        "country",
        EntityKind::Country,
        vec![Action::Emit("country".into())],
    );
    country.options[0].effects = vec![call("country"), Action::Emit("option".into())];
    country.after = vec![call("country"), Action::Emit("after".into())];
    let province = event(
        "province",
        EntityKind::Province,
        vec![Action::Emit("province".into())],
    );
    let mut world = world(vec![country, province], &["country", "province"]);
    let id = world.trigger_event("country", EntityId(0)).unwrap();
    world
        .execute(EntityId(1), &[conditional("country"), call("country")])
        .unwrap();
    world.trigger_event("province", EntityId(2)).unwrap();
    world.execute(EntityId(3), &[call("province")]).unwrap();
    assert_eq!(world.pending_events().len(), 2);
    assert_eq!(
        world.fired_events(),
        &["country".into(), "province".into()].into()
    );
    let before = world.clone();
    assert!(world.trigger_event("country", EntityId(1)).is_err());
    assert_eq!(world, before);
    let mut restored = resume(&world);
    for session in [&mut world, &mut restored] {
        session
            .choose_event(&EventChoice {
                pending: id,
                option: "accept".into(),
            })
            .unwrap();
        session
            .choose_event(&EventChoice {
                pending: PendingEventId(1),
                option: "accept".into(),
            })
            .unwrap();
        session.advance_to(10).unwrap();
        session.execute(EntityId(1), &[call("country")]).unwrap();
        assert!(session.pending_events().is_empty());
    }
    assert_eq!(world, restored);
    assert_eq!(world, resume(&world));
    assert_eq!(
        world
            .events()
            .iter()
            .map(|e| e.2.as_str())
            .collect::<Vec<_>>(),
        ["country", "province", "option", "after"]
    );
}

#[test]
fn registration_follows_immediate_execution_and_allows_bounded_reentry() {
    let definition = event(
        "reentry",
        EntityKind::Country,
        vec![
            Action::Add {
                target: Target::Current,
                field: FieldId(0),
                value: 1,
            },
            Action::Emit("enter".into()),
            Action::If {
                condition: Condition::AtLeast {
                    target: Target::Current,
                    field: FieldId(0),
                    value: 2,
                },
                then: vec![Action::Emit("inner".into())],
                otherwise: vec![
                    call("reentry"),
                    call("reentry"),
                    Action::Emit("outer".into()),
                ],
            },
        ],
    );
    let mut world = world(vec![definition], &["reentry"]);
    assert_eq!(
        world.trigger_event("reentry", EntityId(0)).unwrap(),
        PendingEventId(1)
    );
    assert_eq!(world.value(EntityId(0), FieldId(0)).unwrap(), 2);
    assert_eq!(world.pending_events().len(), 2);
    assert_eq!(world.fired_events().len(), 1);
    assert_eq!(
        world
            .events()
            .iter()
            .map(|e| e.2.as_str())
            .collect::<Vec<_>>(),
        ["enter", "enter", "inner", "outer"]
    );
    assert_eq!(world, resume(&world));
}

#[test]
fn duplicate_delayed_dispatches_check_records_at_execution_and_after_restore() {
    let mut world = world(
        vec![event(
            "delayed",
            EntityKind::Country,
            vec![Action::Emit("delayed".into())],
        )],
        &["delayed"],
    );
    world
        .schedule(EntityId(0), 2, vec![conditional("delayed")])
        .unwrap();
    world
        .schedule(EntityId(1), 2, vec![conditional("delayed")])
        .unwrap();
    world
        .schedule(EntityId(1), 3, vec![call("delayed")])
        .unwrap();
    assert!(world.fired_events().is_empty());
    world.advance_to(1).unwrap();
    let mut restored = resume(&world);
    for session in [&mut world, &mut restored] {
        session.advance_to(2).unwrap();
        assert_eq!(session.pending_events().len(), 1);
        assert_eq!(session.pending_events()[0].current, EntityId(0));
        assert_eq!(session.pending_events()[0].day, 2);
    }
    restored = resume(&restored);
    world.advance_to(3).unwrap();
    restored.advance_to(3).unwrap();
    assert_eq!(world, restored);
    assert_eq!(world.pending_tasks(), 0);
    assert_eq!(world.events().len(), 1);
}

#[test]
fn false_conditions_do_not_consume_once_records_and_forced_calls_still_obey_them() {
    let mut world = world(
        vec![
            event("gate", EntityKind::Country, Vec::new()),
            event("forced", EntityKind::Country, Vec::new()),
        ],
        &["gate", "forced"],
    );
    let catalog = world
        .event_catalog()
        .clone()
        .with_conditions(
            [
                (
                    "gate".into(),
                    Condition::AtLeast {
                        target: Target::Current,
                        field: FieldId(0),
                        value: 1,
                    },
                ),
                ("forced".into(), Condition::Always(false)),
            ]
            .into(),
        )
        .unwrap();
    world.set_event_catalog(catalog).unwrap();
    world
        .execute(EntityId(0), &[conditional("gate"), conditional("forced")])
        .unwrap();
    assert!(world.fired_events().is_empty());
    world.set(EntityId(0), FieldId(0), 1).unwrap();
    world
        .execute(EntityId(0), &[conditional("gate"), conditional("gate")])
        .unwrap();
    world.trigger_event("forced", EntityId(0)).unwrap();
    assert_eq!(world.pending_events().len(), 2);
    assert_eq!(world.fired_events().len(), 2);
    world.execute(EntityId(0), &[call("forced")]).unwrap();
    assert_eq!(world.pending_events().len(), 2);
}

#[test]
fn frozen_candidates_still_check_once_records_before_dispatch() {
    let mut world = world(
        vec![
            event("parent", EntityKind::Country, vec![call("once")]),
            event(
                "once",
                EntityKind::Country,
                vec![Action::Emit("once".into())],
            ),
            event("repeatable", EntityKind::Country, Vec::new()),
        ],
        &["once"],
    );
    world
        .execute(
            EntityId(0),
            &[Action::EventCandidates {
                events: vec![
                    "parent".into(),
                    "once".into(),
                    "once".into(),
                    "repeatable".into(),
                    "repeatable".into(),
                ],
            }],
        )
        .unwrap();
    assert_eq!(
        world
            .pending_events()
            .iter()
            .map(|p| p.event.as_str())
            .collect::<Vec<_>>(),
        ["once", "parent", "repeatable", "repeatable"]
    );
    assert_eq!(world.events().len(), 1);
}

#[test]
fn failed_dispatch_does_not_register_completion_and_unbounded_reentry_stops() {
    let mut failed = world(
        vec![event(
            "broken",
            EntityKind::Country,
            vec![
                Action::Emit("prefix".into()),
                Action::Add {
                    target: Target::Current,
                    field: FieldId(0),
                    value: i64::MAX,
                },
                Action::Add {
                    target: Target::Current,
                    field: FieldId(0),
                    value: 1,
                },
            ],
        )],
        &["broken"],
    );
    assert!(failed.trigger_event("broken", EntityId(0)).is_err());
    assert!(failed.fired_events().is_empty());
    assert!(failed.pending_events().is_empty());
    assert_eq!(failed.events().len(), 1);
    assert!(failed.save("failed").is_err());
    let mut cycle = world(
        vec![event("cycle", EntityKind::Country, vec![call("cycle")])],
        &["cycle"],
    );
    assert!(
        cycle
            .trigger_event("cycle", EntityId(0))
            .unwrap_err()
            .to_string()
            .contains("call depth")
    );
    assert!(cycle.fired_events().is_empty());
    assert!(cycle.pending_events().is_empty());
}

#[test]
fn saved_registry_and_content_are_validated_without_changing_unused_save_shape() {
    assert!(
        EventCatalog::default()
            .with_once_events(["missing".into()].into())
            .is_err()
    );
    let mut world = world(
        vec![
            event("once", EntityKind::Country, Vec::new()),
            event("repeatable", EntityKind::Country, Vec::new()),
        ],
        &["once"],
    );
    let initial = world.save("once:v1").unwrap();
    assert!(!initial.contains("event_queue"));
    world.trigger_event("repeatable", EntityId(0)).unwrap();
    assert!(!world.save("once:v1").unwrap().contains("\"fired\""));
    world.trigger_event("once", EntityId(0)).unwrap();
    let saved: serde_json::Value = serde_json::from_str(&world.save("once:v1").unwrap()).unwrap();
    for records in [
        serde_json::json!([]),
        serde_json::json!(["missing"]),
        serde_json::json!(["repeatable"]),
    ] {
        let mut corrupt = saved.clone();
        corrupt["event_queue"]["fired"] = records;
        assert!(
            World::restore_with_events(
                &corrupt.to_string(),
                "once:v1",
                world.schema(),
                Vec::new(),
                world.event_catalog().clone()
            )
            .is_err()
        );
    }
    let before = world.clone();
    assert!(
        world
            .set_event_catalog(world.event_catalog().clone())
            .is_err()
    );
    assert_eq!(world, before);
}
