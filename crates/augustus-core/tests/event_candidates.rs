// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    Action, Condition, EntityKind, EventCatalog, EventDefinition, EventOption, FieldId, Politics,
    Schema, ScopeSelector, Target, World,
};

fn event(id: &str, immediate: Vec<Action>) -> EventDefinition {
    EventDefinition {
        id: id.into(),
        scope: EntityKind::Country,
        immediate,
        options: vec![EventOption {
            id: "accept".into(),
            effects: Vec::new(),
        }],
        after: Vec::new(),
    }
}

#[test]
fn conditional_calls_check_live_state_and_forced_calls_bypass_conditions() {
    let mut world = World::new(Schema::new(["phase"]).unwrap());
    let country = world.add_entity("Union", EntityKind::Country);
    let definitions = vec![
        event(
            "first",
            vec![Action::Set {
                target: Target::Current,
                field: FieldId(0),
                value: 2,
            }],
        ),
        event("second", vec![Action::Emit("second".into())]),
        event("false", vec![Action::Emit("direct".into())]),
    ];
    world
        .set_event_catalog(
            EventCatalog::new(definitions)
                .unwrap()
                .with_conditions(
                    [
                        (
                            "second".into(),
                            Condition::AtLeast {
                                target: Target::Current,
                                field: FieldId(0),
                                value: 2,
                            },
                        ),
                        ("false".into(), Condition::Always(false)),
                    ]
                    .into(),
                )
                .unwrap(),
        )
        .unwrap();
    world
        .execute(
            country,
            &[
                Action::TryEvent {
                    event: "second".into(),
                },
                Action::TryEvent {
                    event: "first".into(),
                },
                Action::TryEvent {
                    event: "second".into(),
                },
                Action::TryEvent {
                    event: "false".into(),
                },
            ],
        )
        .unwrap();
    assert_eq!(
        world
            .pending_events()
            .iter()
            .map(|p| p.event.as_str())
            .collect::<Vec<_>>(),
        ["first", "second"]
    );
    world.trigger_event("false", country).unwrap();
    assert_eq!(
        world
            .events()
            .iter()
            .map(|e| e.2.as_str())
            .collect::<Vec<_>>(),
        ["second", "direct"]
    );
    let saved = world.save("candidates:v1").unwrap();
    let restored = World::restore_with_events(
        &saved,
        "candidates:v1",
        world.schema(),
        Vec::new(),
        world.event_catalog().clone(),
    )
    .unwrap();
    assert_eq!(world, restored);
}

#[test]
fn candidate_batch_is_frozen_before_immediate_effects_then_rebuilt_on_next_call() {
    let mut world = World::new(Schema::new(["phase"]).unwrap());
    let country = world.add_entity("Union", EntityKind::Country);
    let definitions = vec![
        event(
            "first",
            vec![Action::Set {
                target: Target::Current,
                field: FieldId(0),
                value: 2,
            }],
        ),
        event("second", vec![Action::Emit("second".into())]),
        event("initial", vec![Action::Emit("initial".into())]),
    ];
    world
        .set_event_catalog(
            EventCatalog::new(definitions)
                .unwrap()
                .with_conditions(
                    [
                        (
                            "second".into(),
                            Condition::AtLeast {
                                target: Target::Current,
                                field: FieldId(0),
                                value: 2,
                            },
                        ),
                        (
                            "initial".into(),
                            Condition::Not(Box::new(Condition::AtLeast {
                                target: Target::Current,
                                field: FieldId(0),
                                value: 1,
                            })),
                        ),
                    ]
                    .into(),
                )
                .unwrap(),
        )
        .unwrap();
    world
        .execute(
            country,
            &[
                Action::EventCandidates {
                    events: vec!["first".into(), "second".into(), "initial".into()],
                },
                Action::Emit("startup".into()),
            ],
        )
        .unwrap();
    assert_eq!(
        world
            .pending_events()
            .iter()
            .map(|p| p.event.as_str())
            .collect::<Vec<_>>(),
        ["first", "initial"]
    );
    assert_eq!(
        world
            .events()
            .iter()
            .map(|p| p.2.as_str())
            .collect::<Vec<_>>(),
        ["initial", "startup"]
    );
    world
        .execute(
            country,
            &[Action::EventCandidates {
                events: vec!["second".into(), "initial".into()],
            }],
        )
        .unwrap();
    assert_eq!(world.pending_events().last().unwrap().event, "second");
}

#[test]
fn candidate_batch_errors_before_dispatch_and_has_a_bounded_scan() {
    let mut world = World::new(Schema::new(["phase"]).unwrap());
    let country = world.add_entity("Union", EntityKind::Country);
    world
        .set_event_catalog(
            EventCatalog::new([event("first", vec![Action::Emit("first".into())])]).unwrap(),
        )
        .unwrap();
    let mut invalid = world.clone();
    assert!(
        invalid
            .execute(
                country,
                &[Action::EventCandidates {
                    events: vec!["first".into(), "missing".into()]
                }]
            )
            .is_err()
    );
    assert!(invalid.pending_events().is_empty());
    assert!(invalid.events().is_empty());
    assert!(invalid.save("invalid").is_err());
    assert!(
        world
            .execute(
                country,
                &[Action::EventCandidates {
                    events: vec!["first".into(); 100_000]
                }]
            )
            .is_err()
    );
    assert!(world.pending_events().is_empty());
}

#[test]
fn eligibility_and_office_predicates_remain_live_across_content_binding() {
    let mut world = World::new(Schema::new(["phase"]).unwrap());
    let first = world.add_entity("Union", EntityKind::Country);
    let second = world.add_entity("League", EntityKind::Country);
    let condition = Condition::ScopeIsCurrent(ScopeSelector::Office("council".into()));
    world
        .set_event_catalog(
            EventCatalog::new([event("meeting", Vec::new())])
                .unwrap()
                .with_conditions([("meeting".into(), condition.clone())].into())
                .unwrap(),
        )
        .unwrap();
    assert!(!condition.evaluate(&world, first).unwrap());
    world
        .set_politics(Politics {
            offices: [("council".into(), second)].into(),
            ..Default::default()
        })
        .unwrap();
    world
        .execute(
            first,
            &[Action::TryEvent {
                event: "meeting".into(),
            }],
        )
        .unwrap();
    world
        .execute(
            second,
            &[Action::TryEvent {
                event: "meeting".into(),
            }],
        )
        .unwrap();
    assert_eq!(world.pending_events()[0].current, second);
    let mut politics = world.politics().clone();
    politics.offices.insert("council".into(), first);
    world.set_politics(politics).unwrap();
    assert!(condition.evaluate(&world, first).unwrap());
    assert!(!condition.evaluate(&world, second).unwrap());
}

#[test]
fn candidate_content_rejects_missing_events_and_invalid_predicate_fields() {
    assert!(
        EventCatalog::default()
            .with_conditions([("absent".into(), Condition::Always(true))].into())
            .is_err()
    );
    let mut world = World::new(Schema::new(["phase"]).unwrap());
    world.add_entity("Union", EntityKind::Country);
    let before = world.clone();
    let invalid = EventCatalog::new([event("meeting", Vec::new())])
        .unwrap()
        .with_conditions(
            [(
                "meeting".into(),
                Condition::AtLeast {
                    target: Target::Current,
                    field: FieldId(9),
                    value: 0,
                },
            )]
            .into(),
        )
        .unwrap();
    assert!(world.set_event_catalog(invalid).is_err());
    assert_eq!(world, before);
    let invalid = EventCatalog::new([event("meeting", Vec::new())])
        .unwrap()
        .with_conditions(
            [(
                "meeting".into(),
                Condition::ScopeIsCurrent(ScopeSelector::Global("".into())),
            )]
            .into(),
        )
        .unwrap();
    assert!(world.set_event_catalog(invalid).is_err());
    assert_eq!(world, before);
}
