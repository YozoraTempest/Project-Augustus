// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{Action, Condition, EntityId, EntityKind, Markers, Schema, Target, World};

fn world() -> World {
    let mut world = World::new(Schema::new(["balance"]).unwrap());
    world.add_entity("AAA", EntityKind::Country);
    world.add_entity("BBB", EntityKind::Country);
    world.add_entity("Port", EntityKind::Province);
    world.set_owner(EntityId(2), EntityId(0)).unwrap();
    world.set_capital(EntityId(0), EntityId(2)).unwrap();
    world
        .set_markers(Markers {
            entities: [
                (EntityId(0), Default::default()),
                (EntityId(1), Default::default()),
            ]
            .into(),
            ..Default::default()
        })
        .unwrap();
    world
}

fn has(target: Target, name: &str) -> Condition {
    Condition::HasMarker {
        target,
        name: name.into(),
    }
}

#[test]
fn known_absence_partial_writes_and_entity_namespaces_are_distinct() {
    let mut world = world();
    assert!(!world.has_marker(EntityId(0), "ready").unwrap());
    assert!(world.has_marker(EntityId(2), "ready").is_err());
    world.set_marker(EntityId(0), "ready", true).unwrap();
    world.set_marker(EntityId(0), "ready", true).unwrap();
    assert_eq!(world.markers().entities[&EntityId(0)].len(), 1);
    assert!(!world.has_marker(EntityId(1), "ready").unwrap());
    world.set_marker(EntityId(2), "ready", false).unwrap();
    assert!(!world.has_marker(EntityId(2), "ready").unwrap());
    assert!(!world.marker_namespace_known(EntityId(2)).unwrap());
    assert!(world.has_marker(EntityId(2), "other").is_err());
    world.set_marker(EntityId(2), "ready", true).unwrap();
    assert!(
        has(Target::Capital, "ready")
            .evaluate(&world, EntityId(0))
            .unwrap()
    );
    assert!(
        has(Target::Owner, "ready")
            .evaluate(&world, EntityId(2))
            .unwrap()
    );
    world.set_marker(EntityId(0), "ready", false).unwrap();
    world.set_marker(EntityId(0), "missing", false).unwrap();
    assert!(world.markers().entities[&EntityId(0)].is_empty());
    let mut markers = world.markers().clone();
    markers.mark_unknown(EntityId(2));
    world.set_markers(markers).unwrap();
    assert!(world.has_marker(EntityId(2), "ready").is_err());
}

#[test]
fn delayed_conditions_read_live_state_and_restore_preserves_known_false() {
    let mut world = world();
    world.set_marker(EntityId(2), "cleared", false).unwrap();
    world
        .schedule(
            EntityId(0),
            2,
            vec![Action::If {
                condition: has(Target::Current, "ready"),
                then: vec![Action::Emit("ready".into())],
                otherwise: vec![Action::Emit("absent".into())],
            }],
        )
        .unwrap();
    world.advance_to(1).unwrap();
    world
        .execute(
            EntityId(0),
            &[Action::SetMarker {
                target: Target::Current,
                name: "ready".into(),
                present: true,
            }],
        )
        .unwrap();
    let saved = world.save("markers").unwrap();
    let mut resumed = World::restore(&saved, "markers", world.schema()).unwrap();
    for state in [&mut world, &mut resumed] {
        state.advance_to(2).unwrap();
        assert_eq!(state.events()[0].2, "ready");
        assert!(!state.has_marker(EntityId(2), "cleared").unwrap());
        assert!(state.has_marker(EntityId(2), "other").is_err());
    }
    assert_eq!(
        world.save("markers").unwrap(),
        resumed.save("markers").unwrap()
    );
}

#[test]
fn invalid_states_and_dead_branch_names_fail_before_execution() {
    let mut world = world();
    let before = world.clone();
    for (entity, name) in [(EntityId(0), ""), (EntityId(99), "ready")] {
        assert!(world.set_marker(entity, name, true).is_err());
        assert!(world.has_marker(entity, name).is_err());
        assert_eq!(world, before);
    }
    assert!(
        world
            .schedule(
                EntityId(0),
                1,
                vec![Action::If {
                    condition: Condition::Always(false),
                    then: vec![Action::SetMarker {
                        target: Target::Current,
                        name: "".into(),
                        present: true
                    }],
                    otherwise: vec![],
                }]
            )
            .is_err()
    );
    assert_eq!(world, before);
    let saved = world.save("markers").unwrap();
    for value in [
        serde_json::json!({"entities":{"99":{}}}),
        serde_json::json!({"entities":{"0":{"":true}}}),
        serde_json::json!({"entities":{},"unknown_entities":[99]}),
        serde_json::json!({"entities":{},"extra":true}),
    ] {
        let mut corrupt: serde_json::Value = serde_json::from_str(&saved).unwrap();
        corrupt["markers"] = value;
        assert!(World::restore(&corrupt.to_string(), "markers", world.schema()).is_err());
    }
    let mut empty = World::new(world.schema().clone());
    empty.add_entity("AAA", EntityKind::Country);
    let saved: serde_json::Value = serde_json::from_str(&empty.save("markers").unwrap()).unwrap();
    assert!(saved.get("markers").is_none());
}
