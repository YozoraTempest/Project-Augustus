// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    Action, Completions, Condition, EntityId, EntityKind, EventCatalog, EventDefinition,
    EventOption, Schema, Target, World,
};

fn world() -> World {
    let mut world = World::new(Schema::new(["unused"]).unwrap());
    let first = world.add_entity("First", EntityKind::Country);
    let second = world.add_entity("Second", EntityKind::Country);
    let province = world.add_entity("Port", EntityKind::Province);
    world.set_owner(province, first).unwrap();
    world
        .set_completions(Completions {
            identifiers: ["first".into(), "later".into()].into(),
            entities: [(first, Default::default()), (second, Default::default())].into(),
        })
        .unwrap();
    world
}

#[test]
fn completion_records_are_per_entity_idempotent_and_follow_live_targets() {
    let mut world = world();
    let first = EntityId(0);
    let second = EntityId(1);
    let province = EntityId(2);
    let owner = Condition::CompletionRecorded {
        target: Target::Owner,
        id: "first".into(),
    };
    assert!(!owner.evaluate(&world, province).unwrap());
    assert!(world.record_completion(first, "first").unwrap());
    let once = world.clone();
    assert!(!world.record_completion(first, "first").unwrap());
    assert_eq!(world, once);
    assert!(owner.evaluate(&world, province).unwrap());
    assert!(!world.completion_recorded(second, "first").unwrap());
    world.set_owner(province, second).unwrap();
    assert!(!owner.evaluate(&world, province).unwrap());
    assert!(world.record_completion(second, "first").unwrap());
    assert!(owner.evaluate(&world, province).unwrap());
    assert!(world.events().is_empty());
}

#[test]
fn unknown_state_and_unregistered_identifiers_do_not_become_false_or_create_records() {
    let mut world = world();
    let before = world.clone();
    for (entity, id) in [
        (EntityId(2), "first"),
        (EntityId(0), "unknown"),
        (EntityId(0), ""),
        (EntityId(99), "first"),
    ] {
        assert!(world.completion_recorded(entity, id).is_err());
        assert!(world.record_completion(entity, id).is_err());
        assert_eq!(world, before);
    }
    for condition in [
        Condition::CompletionRecorded {
            target: Target::Current,
            id: "unknown".into(),
        },
        Condition::CompletionRecorded {
            target: Target::Current,
            id: "".into(),
        },
    ] {
        assert!(
            world
                .schedule(
                    EntityId(0),
                    1,
                    vec![Action::If {
                        condition: Condition::Any(vec![Condition::Always(true), condition]),
                        then: vec![],
                        otherwise: vec![]
                    }]
                )
                .is_err()
        );
        assert_eq!(world, before);
    }
}

#[test]
fn delayed_conditions_read_updated_records_after_restore_without_replaying_writes() {
    let mut world = world();
    world
        .schedule(
            EntityId(0),
            1,
            vec![Action::If {
                condition: Condition::CompletionRecorded {
                    target: Target::Current,
                    id: "later".into(),
                },
                then: vec![Action::Emit("done".into())],
                otherwise: vec![Action::Emit("not_done".into())],
            }],
        )
        .unwrap();
    world
        .execute(
            EntityId(0),
            &[Action::RecordCompletion { id: "later".into() }],
        )
        .unwrap();
    let saved = world.save("records:v1").unwrap();
    let mut resumed = World::restore(&saved, "records:v1", world.schema()).unwrap();
    world.advance_to(1).unwrap();
    resumed.advance_to(1).unwrap();
    assert_eq!(world, resumed);
    assert_eq!(world.events(), &[(1, EntityId(0), "done".into())]);
    let mut corrupt: serde_json::Value = serde_json::from_str(&saved).unwrap();
    corrupt["completions"]["entities"]["0"] = serde_json::json!(["unknown"]);
    assert!(World::restore(&corrupt.to_string(), "records:v1", world.schema()).is_err());
    let mut corrupt: serde_json::Value = serde_json::from_str(&saved).unwrap();
    corrupt["completions"]["entities"]["99"] = serde_json::json!([]);
    assert!(World::restore(&corrupt.to_string(), "records:v1", world.schema()).is_err());
    let mut corrupt: serde_json::Value = serde_json::from_str(&saved).unwrap();
    corrupt["tasks"][0]["plan"][0]["If"]["condition"]["CompletionRecorded"]["id"] =
        serde_json::json!("unknown");
    assert!(World::restore(&corrupt.to_string(), "records:v1", world.schema()).is_err());
}

#[test]
fn completion_content_replacement_validates_existing_events_and_tasks_atomically() {
    let mut world = world();
    world
        .set_event_catalog(
            EventCatalog::new([EventDefinition {
                id: "test".into(),
                scope: EntityKind::Country,
                immediate: vec![Action::RecordCompletion { id: "first".into() }],
                options: vec![EventOption {
                    id: "accept".into(),
                    effects: vec![],
                }],
                after: vec![],
            }])
            .unwrap(),
        )
        .unwrap();
    let before = world.clone();
    assert!(world.set_completions(Completions::default()).is_err());
    assert_eq!(world, before);
    let mut invalid = world.completions().clone();
    invalid.identifiers.insert(String::new());
    assert!(world.set_completions(invalid).is_err());
    assert_eq!(world, before);
    world.set_event_catalog(EventCatalog::default()).unwrap();
    world
        .schedule(
            EntityId(0),
            1,
            vec![Action::RecordCompletion { id: "first".into() }],
        )
        .unwrap();
    let before = world.clone();
    assert!(world.set_completions(Completions::default()).is_err());
    assert_eq!(world, before);
}
