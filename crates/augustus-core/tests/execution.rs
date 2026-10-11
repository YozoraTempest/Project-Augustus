// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{Action, Condition, EntityKind, FieldId, RuleId, Schema, Target, World};

fn scenario() -> (World, augustus_core::EntityId, augustus_core::EntityId) {
    let mut world = World::new(Schema::new(["treasury", "works"]).unwrap());
    let country = world.add_entity("Demo", EntityKind::Country);
    let province = world.add_entity("Harbor", EntityKind::Province);
    world.set_owner(province, country).unwrap();
    world.set_capital(country, province).unwrap();
    world.set(country, FieldId(0), 150_000).unwrap();
    (world, country, province)
}

#[test]
fn instructions_observe_previous_writes_and_scope_changes() {
    let (mut world, country, province) = scenario();
    world
        .execute(
            country,
            &[
                Action::Add {
                    target: Target::Current,
                    field: FieldId(0),
                    value: -100_000,
                },
                Action::If {
                    condition: Condition::AtLeast {
                        target: Target::Current,
                        field: FieldId(0),
                        value: 100_000,
                    },
                    then: vec![Action::Emit("old value".into())],
                    otherwise: vec![Action::With {
                        target: Target::Capital,
                        plan: vec![Action::Set {
                            target: Target::Current,
                            field: FieldId(1),
                            value: 1,
                        }],
                    }],
                },
            ],
        )
        .unwrap();
    assert_eq!(world.value(country, FieldId(0)).unwrap(), 50_000);
    assert_eq!(world.value(province, FieldId(1)).unwrap(), 1);
    assert!(world.events().is_empty());
}

#[test]
fn resumed_queue_matches_continuous_execution_including_same_day_order() {
    let (mut continuous, country, province) = scenario();
    continuous
        .schedule(
            province,
            30,
            vec![
                Action::Emit("first".into()),
                Action::Schedule {
                    delay: 5,
                    plan: vec![Action::Emit("follow-up".into())],
                },
            ],
        )
        .unwrap();
    continuous
        .schedule(country, 30, vec![Action::Emit("second".into())])
        .unwrap();
    continuous.advance_to(14).unwrap();
    let save = continuous.save("self-authored:v1").unwrap();
    let mut resumed = World::restore(&save, "self-authored:v1", continuous.schema()).unwrap();
    continuous.advance_to(60).unwrap();
    for day in 15..=60 {
        resumed.advance_to(day).unwrap();
    }
    assert_eq!(continuous, resumed);
    assert_eq!(
        continuous
            .events()
            .iter()
            .map(|event| (event.0, event.2.as_str()))
            .collect::<Vec<_>>(),
        [(30, "first"), (30, "second"), (35, "follow-up")]
    );
    assert_eq!(continuous.pending_tasks(), 0);
    assert!(World::restore(&save, "different", continuous.schema()).is_err());
}

#[test]
fn corrupt_saves_are_rejected_before_resuming() {
    let (mut world, country, _) = scenario();
    world
        .schedule(
            country,
            10,
            vec![Action::Set {
                target: Target::Current,
                field: FieldId(0),
                value: 1,
            }],
        )
        .unwrap();
    let original: serde_json::Value = serde_json::from_str(&world.save("v1").unwrap()).unwrap();
    for (pointer, bad) in [
        ("/entities/0/capital", serde_json::json!(100)),
        ("/tasks/0/day", serde_json::json!(0)),
        ("/tasks/0/plan/0/Set/field", serde_json::json!(99)),
        ("/next_sequence", serde_json::json!(0)),
    ] {
        let mut save = original.clone();
        *save.pointer_mut(pointer).unwrap() = bad;
        assert!(
            World::restore(&save.to_string(), "v1", world.schema()).is_err(),
            "{pointer}"
        );
    }
}

#[test]
fn invalid_dates_and_overflow_do_not_wrap() {
    let (mut world, country, _) = scenario();
    world.advance_to(3).unwrap();
    assert!(world.advance_to(2).is_err());
    assert!(world.schedule(country, 0, Vec::new()).is_err());
    assert!(world.schedule(country, u64::MAX, Vec::new()).is_err());
    world.set(country, FieldId(0), i64::MAX).unwrap();
    assert!(
        world
            .execute(
                country,
                &[Action::Add {
                    target: Target::Current,
                    field: FieldId(0),
                    value: 1
                }]
            )
            .is_err()
    );
    assert_eq!(world.value(country, FieldId(0)).unwrap(), i64::MAX);
    assert!(world.save("v1").is_err());
    assert!(world.advance_to(4).is_err());
}

#[test]
fn a_runtime_failure_cannot_be_saved_or_silently_resumed() {
    let (mut world, country, _) = scenario();
    assert!(
        world
            .execute(
                country,
                &[
                    Action::Set {
                        target: Target::Current,
                        field: FieldId(0),
                        value: 42
                    },
                    Action::Set {
                        target: Target::Current,
                        field: FieldId(90),
                        value: 1
                    },
                ]
            )
            .is_err()
    );
    assert_eq!(world.value(country, FieldId(0)).unwrap(), 42);
    assert!(world.execute(country, &[]).is_err());
    assert!(world.save("v1").is_err());
}

#[test]
fn immediate_rule_recursion_reports_an_error_instead_of_overflowing_the_stack() {
    let mut world = World::with_rules(
        Schema::new(["value"]).unwrap(),
        vec![vec![Action::Run(RuleId(0))]],
    );
    let entity = world.add_entity("Demo", EntityKind::Country);
    assert!(world.execute(entity, &[Action::Run(RuleId(0))]).is_err());
    assert!(world.save("v1").is_err());
}

#[test]
fn saved_sequence_identifiers_must_be_unique_across_dates() {
    let (mut world, country, _) = scenario();
    world.schedule(country, 10, Vec::new()).unwrap();
    world.schedule(country, 20, Vec::new()).unwrap();
    let mut save: serde_json::Value = serde_json::from_str(&world.save("v1").unwrap()).unwrap();
    save["tasks"][1]["sequence"] = serde_json::json!(0);
    assert!(World::restore(&save.to_string(), "v1", world.schema()).is_err());
}

#[test]
fn random_draw_order_survives_a_save_in_the_middle_of_the_stream() {
    let (mut world, country, _) = scenario();
    let roll = vec![Action::RandomBranch {
        then: vec![Action::Add {
            target: Target::Current,
            field: FieldId(1),
            value: 1,
        }],
        otherwise: vec![Action::Add {
            target: Target::Current,
            field: FieldId(1),
            value: 3,
        }],
    }];
    for _ in 0..3 {
        world.execute(country, &roll).unwrap();
    }
    world.schedule(country, 10, roll.clone()).unwrap();
    let mut resumed = World::restore(&world.save("v1").unwrap(), "v1", world.schema()).unwrap();
    for _ in 0..50 {
        world.execute(country, &roll).unwrap();
        resumed.execute(country, &roll).unwrap();
        assert_eq!(world, resumed);
    }
    world.advance_to(10).unwrap();
    resumed.advance_to(10).unwrap();
    assert_eq!(world.save("v1").unwrap(), resumed.save("v1").unwrap());
}
