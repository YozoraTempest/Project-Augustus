// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::*;

fn world() -> World {
    let mut world = World::new(Schema::new(["value"]).unwrap());
    world.add_entity("AAA", EntityKind::Country);
    world
}

fn set(value: i64) -> Action {
    Action::Set {
        target: Target::Current,
        field: FieldId(0),
        value,
    }
}

#[test]
fn preview_does_not_mutate_resolve_scopes_evaluate_conditions_or_draw_randomness() {
    let mut actual = world();
    let mut reference = actual.clone();
    let roll = Action::RandomBranch {
        then: vec![set(1)],
        otherwise: vec![set(2)],
    };
    let preview = Action::Preview {
        plan: vec![
            roll.clone(),
            Action::Schedule {
                delay: 1,
                plan: vec![set(99)],
            },
            Action::With {
                target: Target::Owner,
                plan: vec![set(99)],
            },
            Action::If {
                condition: Condition::YearAtLeast(1444),
                then: vec![Action::Emit("unexpected".into())],
                otherwise: vec![set(99)],
            },
            Action::UpdateNumeric {
                target: Target::Current,
                slot: NumericSlot::Field(FieldId(0)),
                operation: NumericOperation::Divide { scale: 1000 },
                value: 0,
            },
        ],
    };
    for _ in 0..64 {
        actual
            .execute(EntityId(0), &[preview.clone(), roll.clone()])
            .unwrap();
        reference
            .execute(EntityId(0), std::slice::from_ref(&roll))
            .unwrap();
        assert_eq!(actual, reference);
    }
    assert_eq!(actual.pending_tasks(), 0);
    assert!(actual.events().is_empty());
}

#[test]
fn delayed_previews_survive_restore_and_their_bodies_are_validated() {
    let mut actual = world();
    actual
        .schedule(
            EntityId(0),
            1,
            vec![
                Action::Preview {
                    plan: vec![set(99)],
                },
                Action::Emit("actual".into()),
            ],
        )
        .unwrap();
    let saved = actual.save("preview:v1").unwrap();
    let mut resumed = World::restore(&saved, "preview:v1", actual.schema()).unwrap();
    let mut corrupt: serde_json::Value = serde_json::from_str(&saved).unwrap();
    corrupt["tasks"][0]["plan"][0]["Preview"]["plan"][0]["Set"]["field"] = 99.into();
    assert!(World::restore(&corrupt.to_string(), "preview:v1", actual.schema()).is_err());
    let before = resumed.clone();
    assert!(
        resumed
            .schedule(
                EntityId(0),
                1,
                vec![Action::Preview {
                    plan: vec![Action::TriggerEvent {
                        event: "missing".into()
                    }],
                }]
            )
            .is_err()
    );
    assert_eq!(before, resumed);
    for state in [&mut actual, &mut resumed] {
        state.advance_to(1).unwrap();
        assert_eq!(state.value(EntityId(0), FieldId(0)).unwrap(), 0);
        assert_eq!(state.events(), &[(1, EntityId(0), "actual".into())]);
    }
    assert_eq!(
        actual.save("preview:v1").unwrap(),
        resumed.save("preview:v1").unwrap()
    );
}

#[test]
fn event_preview_calls_do_not_create_choices_or_consume_once_events() {
    let mut state = world();
    let child = Action::TriggerEvent {
        event: "child".into(),
    };
    let catalog = EventCatalog::new([
        EventDefinition {
            id: "parent".into(),
            scope: EntityKind::Country,
            immediate: vec![Action::Preview {
                plan: vec![child.clone()],
            }],
            options: vec![EventOption {
                id: "0".into(),
                effects: vec![Action::Preview { plan: vec![child] }, set(7)],
            }],
            after: vec![Action::Emit("after".into())],
        },
        EventDefinition {
            id: "child".into(),
            scope: EntityKind::Country,
            immediate: vec![Action::Emit("child".into())],
            options: vec![EventOption {
                id: "0".into(),
                effects: vec![],
            }],
            after: vec![],
        },
    ])
    .unwrap()
    .with_once_events(["child".into()].into())
    .unwrap();
    state.set_event_catalog(catalog.clone()).unwrap();
    let pending = state.trigger_event("parent", EntityId(0)).unwrap();
    assert_eq!(state.pending_events().len(), 1);
    let saved = state.save("events-preview").unwrap();
    let mut resumed =
        World::restore_with_events(&saved, "events-preview", state.schema(), vec![], catalog)
            .unwrap();
    for state in [&mut state, &mut resumed] {
        state
            .choose_event(&EventChoice {
                pending,
                option: "0".into(),
            })
            .unwrap();
        assert!(state.pending_events().is_empty());
        assert_eq!(state.value(EntityId(0), FieldId(0)).unwrap(), 7);
        assert_eq!(state.events(), &[(0, EntityId(0), "after".into())]);
        state.trigger_event("child", EntityId(0)).unwrap();
        assert_eq!(state.pending_events().len(), 1);
        assert_eq!(state.events().last().unwrap().2, "child");
    }
    assert_eq!(state, resumed);
}
