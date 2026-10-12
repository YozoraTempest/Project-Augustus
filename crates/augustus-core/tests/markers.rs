// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    Action, Condition, EntityId, EntityKind, GlobalMarkers, Markers, Schema, Target, World,
};

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

#[test]
fn global_names_are_shared_across_scopes_and_separate_from_entity_markers() {
    let mut world = world();
    assert!(world.has_global_marker("ready").is_err());
    world
        .set_global_markers(GlobalMarkers {
            known: true,
            ..Default::default()
        })
        .unwrap();
    assert!(!world.has_global_marker("ready").unwrap());
    world
        .execute(
            EntityId(0),
            &[
                Action::SetGlobalMarker {
                    name: "ready".into(),
                    present: true,
                },
                Action::With {
                    target: Target::Capital,
                    plan: vec![Action::If {
                        condition: Condition::HasGlobalMarker {
                            name: "ready".into(),
                        },
                        then: vec![Action::Emit("province".into())],
                        otherwise: vec![],
                    }],
                },
                Action::SetMarker {
                    target: Target::Current,
                    name: "ready".into(),
                    present: false,
                },
            ],
        )
        .unwrap();
    assert_eq!(world.events()[0], (0, EntityId(2), "province".into()));
    assert!(
        Condition::HasGlobalMarker {
            name: "ready".into()
        }
        .evaluate(&world, EntityId(1))
        .unwrap()
    );
    assert!(!world.has_marker(EntityId(0), "ready").unwrap());
    world
        .execute(
            EntityId(2),
            &[Action::SetGlobalMarker {
                name: "ready".into(),
                present: false,
            }],
        )
        .unwrap();
    assert!(!world.has_global_marker("ready").unwrap());
    assert!(world.global_markers().values.is_empty());
}

#[test]
fn global_delayed_queries_and_partial_knowledge_survive_restore() {
    let mut world = world();
    world.set_global_marker("cleared", false).unwrap();
    world
        .schedule(
            EntityId(1),
            1,
            vec![Action::If {
                condition: Condition::HasGlobalMarker {
                    name: "ready".into(),
                },
                then: vec![Action::Emit("ready".into())],
                otherwise: vec![],
            }],
        )
        .unwrap();
    world.set_global_marker("ready", true).unwrap();
    let saved = world.save("global-markers").unwrap();
    let mut restored = World::restore(&saved, "global-markers", world.schema()).unwrap();
    for state in [&mut world, &mut restored] {
        assert!(!state.has_global_marker("cleared").unwrap());
        assert!(state.has_global_marker("other").is_err());
        state.advance_to(1).unwrap();
        assert_eq!(state.events()[0].2, "ready");
        state.set_global_marker("ready", false).unwrap();
        assert!(!state.has_global_marker("ready").unwrap());
    }
    assert_eq!(
        world.save("global-markers").unwrap(),
        restored.save("global-markers").unwrap()
    );
}

#[test]
fn global_validation_is_atomic_and_legacy_saves_remain_unknown() {
    let mut world = world();
    let before = world.save("global-markers").unwrap();
    assert!(
        !serde_json::from_str::<serde_json::Value>(&before)
            .unwrap()
            .as_object()
            .unwrap()
            .contains_key("global_markers")
    );
    for operation in [
        vec![Action::SetGlobalMarker {
            name: "".into(),
            present: true,
        }],
        vec![Action::If {
            condition: Condition::Always(false),
            then: vec![Action::If {
                condition: Condition::HasGlobalMarker { name: "".into() },
                then: vec![],
                otherwise: vec![],
            }],
            otherwise: vec![],
        }],
    ] {
        assert!(world.schedule(EntityId(0), 1, operation).is_err());
        assert_eq!(world.save("global-markers").unwrap(), before);
    }
    assert!(
        world
            .set_global_markers(GlobalMarkers {
                known: true,
                values: [("".into(), true)].into()
            })
            .is_err()
    );
    assert_eq!(world.save("global-markers").unwrap(), before);
    let restored = World::restore(&before, "global-markers", world.schema()).unwrap();
    assert!(restored.has_global_marker("missing").is_err());
    for value in [
        serde_json::json!({"known":true,"values":{"":true}}),
        serde_json::json!({"known":true,"values":{},"extra":true}),
    ] {
        let mut corrupt: serde_json::Value = serde_json::from_str(&before).unwrap();
        corrupt["global_markers"] = value;
        assert!(World::restore(&corrupt.to_string(), "global-markers", world.schema()).is_err());
    }
}
