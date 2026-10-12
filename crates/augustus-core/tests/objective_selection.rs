// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::*;

fn world() -> World {
    let mut world = World::new(Schema::new(["unused"]).unwrap());
    for name in ["AAA", "BBB"] {
        world.add_entity(name, EntityKind::Country);
    }
    world
        .set_completions(Completions {
            identifiers: ["base".into(), "first".into(), "second".into()].into(),
            entities: [
                (EntityId(0), Default::default()),
                (EntityId(1), Default::default()),
            ]
            .into(),
        })
        .unwrap();
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

fn group(id: &str, priority: u32, claims: &[&str], condition: Condition) -> ObjectiveGroup {
    ObjectiveGroup {
        id: id.into(),
        condition,
        objectives: [id.into()].into(),
        claims: claims.iter().map(|s| (*s).into()).collect(),
        priority,
    }
}

#[test]
fn explicit_refresh_retains_records_and_delayed_work_survives_restore() {
    let mut world = world();
    let entity = EntityId(0);
    let flag = Condition::HasMarker {
        target: Target::Current,
        name: "second".into(),
    };
    world
        .set_objectives(Objectives {
            groups: vec![
                group("base", 0, &["branch"], Condition::Always(true)),
                group(
                    "first",
                    1,
                    &["branch"],
                    Condition::Not(Box::new(flag.clone())),
                ),
                group("second", 1, &["branch"], flag),
            ],
            ..Default::default()
        })
        .unwrap();
    assert!(world.objective_available(entity, "first").is_err());
    world.refresh_objectives(entity).unwrap();
    world.record_completion(entity, "first").unwrap();
    world.set_marker(entity, "second", true).unwrap();
    assert!(world.objective_available(entity, "first").unwrap());
    assert!(!world.objective_available(entity, "second").unwrap());
    world
        .schedule(
            entity,
            2,
            vec![
                Action::RefreshObjectives,
                Action::If {
                    condition: Condition::ObjectiveAvailable {
                        target: Target::Current,
                        id: "second".into(),
                    },
                    then: vec![Action::Emit("selected".into())],
                    otherwise: vec![],
                },
            ],
        )
        .unwrap();
    let saved = world.save("selection").unwrap();
    let mut restored = World::restore(&saved, "selection", world.schema()).unwrap();
    for state in [&mut world, &mut restored] {
        state.advance_to(2).unwrap();
        assert!(state.objective_available(entity, "second").unwrap());
        assert!(!state.objective_available(entity, "first").unwrap());
        assert!(state.completion_recorded(entity, "first").unwrap());
        assert!(state.objective_available(EntityId(1), "first").is_err());
    }
    assert_eq!(world, restored);
    assert_eq!(world.events().len(), 1);
}

#[test]
fn equal_priorities_coexist_and_rejected_groups_do_not_reserve_modern_claims() {
    let mut world = world();
    world
        .set_objectives(Objectives {
            groups: vec![
                group("base", 0, &["left"], Condition::Always(true)),
                group("first", 1, &["left", "right"], Condition::Always(true)),
                group("second", 2, &["right"], Condition::Always(true)),
            ],
            ..Default::default()
        })
        .unwrap();
    world.refresh_objectives(EntityId(0)).unwrap();
    assert!(world.objective_available(EntityId(0), "base").unwrap());
    assert!(!world.objective_available(EntityId(0), "first").unwrap());
    let mut catalog = world.objectives().clone();
    catalog.groups[1].priority = 2;
    world.set_objectives(catalog).unwrap();
    world.refresh_objectives(EntityId(0)).unwrap();
    assert!(world.objective_available(EntityId(0), "first").unwrap());
    assert!(world.objective_available(EntityId(0), "second").unwrap());
    assert!(!world.objective_available(EntityId(0), "base").unwrap());
}

#[test]
fn unresolved_eligibility_is_atomic_and_conclusive_branches_can_settle_it() {
    let mut world = world();
    world
        .set_objectives(Objectives {
            groups: vec![group("first", 0, &[], Condition::Always(true))],
            ..Default::default()
        })
        .unwrap();
    world.refresh_objectives(EntityId(0)).unwrap();
    let mut catalog = world.objectives().clone();
    catalog.groups[0].condition = Condition::All(vec![
        Condition::Unavailable("unimplemented criterion".into()),
        Condition::EntityNamed {
            target: Target::Current,
            name: "AAA".into(),
        },
    ]);
    world.set_objectives(catalog).unwrap();
    let before = world.clone();
    assert!(world.refresh_objectives(EntityId(0)).is_err());
    assert_eq!(world, before);
    world.refresh_objectives(EntityId(1)).unwrap();
    assert!(!world.objective_available(EntityId(1), "first").unwrap());
    assert!(world.objective_available(EntityId(1), "missing").is_err());
    let mut catalog = world.objectives().clone();
    catalog.groups[0].condition = Condition::Any(vec![
        Condition::Unavailable("unimplemented criterion".into()),
        Condition::Always(true),
    ]);
    world.set_objectives(catalog).unwrap();
    world.refresh_objectives(EntityId(1)).unwrap();
    assert!(world.objective_available(EntityId(1), "first").unwrap());
}

#[test]
fn registry_changes_and_tampered_saves_cannot_invalidate_selection() {
    let mut world = world();
    world
        .set_objectives(Objectives {
            groups: vec![group("first", 0, &[], Condition::Always(true))],
            ..Default::default()
        })
        .unwrap();
    let before = world.clone();
    let mut records = world.completions().clone();
    records.identifiers.remove("first");
    assert!(world.set_completions(records).is_err());
    assert_eq!(world, before);
    let mut data: serde_json::Value =
        serde_json::from_str(&world.save("selection").unwrap()).unwrap();
    data["objectives"]["entities"] = serde_json::json!({"0": ["second"]});
    assert!(World::restore(&data.to_string(), "selection", world.schema()).is_err());
    data["objectives"]["entities"] = serde_json::json!({"99": ["first"]});
    assert!(World::restore(&data.to_string(), "selection", world.schema()).is_err());
}
