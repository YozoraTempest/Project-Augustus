// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    Action, Condition, EntityId, EntityKind, Schema, ScopeSelector, Target, World,
};

fn world() -> World {
    let mut world = World::new(Schema::new(["value"]).unwrap());
    for name in ["AAA", "BBB", "CCC"] {
        world.add_entity(name, EntityKind::Country);
    }
    for name in ["Home", "Other", "Sea"] {
        world.add_entity(name, EntityKind::Province);
    }
    world.set_owner(EntityId(3), EntityId(0)).unwrap();
    world.set_owner(EntityId(4), EntityId(1)).unwrap();
    world
}

#[test]
fn ownership_queries_distinguish_known_presence_absence_and_incomplete_relationships() {
    let mut world = world();
    assert!(world.has_owned_province(EntityId(0)).unwrap());
    assert!(world.has_owned_province(EntityId(2)).is_err());
    let condition = Condition::Not(Box::new(Condition::HasOwnedProvince {
        target: Target::Current,
    }));
    assert!(condition.evaluate(&world, EntityId(2)).is_err());
    for _ in 0..2 {
        world
            .mark_target_absent(EntityId(5), Target::Owner)
            .unwrap();
    }
    assert!(!world.has_owned_province(EntityId(2)).unwrap());
    assert!(condition.evaluate(&world, EntityId(2)).unwrap());
    let before = world.save("ownership").unwrap();
    for _ in 0..20 {
        assert!(world.has_owned_province(EntityId(0)).unwrap());
    }
    assert_eq!(world.save("ownership").unwrap(), before);
    world.set_owner(EntityId(3), EntityId(1)).unwrap();
    assert!(!world.has_owned_province(EntityId(0)).unwrap());
    world.set_owner(EntityId(5), EntityId(2)).unwrap();
    assert!(world.has_owned_province(EntityId(2)).unwrap());
    for _ in 0..2 {
        world.set_owner(EntityId(5), EntityId(2)).unwrap();
    }
    let newcomer = world.add_entity("New", EntityKind::Province);
    assert!(world.has_owned_province(EntityId(0)).is_err());
    world.set_owner(newcomer, EntityId(0)).unwrap();
    assert!(world.has_owned_province(EntityId(0)).unwrap());
    assert!(world.has_owned_province(EntityId(3)).is_err());
    assert!(world.has_owned_province(EntityId(99)).is_err());
    assert!(world.set_owner(EntityId(0), EntityId(1)).is_err());
}

#[test]
fn saved_conditions_use_rebuilt_ownership_after_transfer_and_resume_without_cached_flags() {
    let mut continuous = world();
    continuous
        .mark_target_absent(EntityId(5), Target::Owner)
        .unwrap();
    continuous
        .execute(
            EntityId(0),
            &[Action::Schedule {
                delay: 2,
                plan: vec![Action::If {
                    condition: Condition::HasOwnedProvince {
                        target: Target::Current,
                    },
                    then: vec![Action::Emit("present".into())],
                    otherwise: vec![Action::Emit("absent".into())],
                }],
            }],
        )
        .unwrap();
    continuous.set_owner(EntityId(3), EntityId(1)).unwrap();
    let saved = continuous.save("ownership").unwrap();
    assert!(!saved.contains("OwnershipIndex"));
    let mut resumed = World::restore(&saved, "ownership", continuous.schema()).unwrap();
    assert_eq!(continuous, resumed);
    for world in [&mut continuous, &mut resumed] {
        assert!(!world.has_owned_province(EntityId(0)).unwrap());
        world.advance_to(2).unwrap();
        assert_eq!(world.events().last().unwrap().2, "absent");
        world.set_owner(EntityId(3), EntityId(0)).unwrap();
        assert!(world.has_owned_province(EntityId(0)).unwrap());
    }
    assert_eq!(continuous, resumed);
    assert_eq!(
        World::restore(
            &continuous.save("ownership").unwrap(),
            "ownership",
            continuous.schema()
        )
        .unwrap(),
        continuous
    );
}

#[test]
fn typed_names_select_registered_entities_and_reject_missing_or_ambiguous_names() {
    let mut world = world();
    let selector = ScopeSelector::Named {
        kind: EntityKind::Country,
        name: "AAA".into(),
    };
    assert_eq!(world.resolve_scope(&selector).unwrap(), Some(EntityId(0)));
    world.add_entity("AAA", EntityKind::Province);
    assert_eq!(world.resolve_scope(&selector).unwrap(), Some(EntityId(0)));
    for name in ["Missing", ""] {
        assert!(
            world
                .resolve_scope(&ScopeSelector::Named {
                    kind: EntityKind::Country,
                    name: name.into()
                })
                .is_err()
        );
    }
    world.add_entity("AAA", EntityKind::Country);
    assert!(world.resolve_scope(&selector).is_err());
}

#[test]
fn filtered_ownership_queries_use_live_members_and_distinguish_missing_relationships() {
    let mut world = world();
    let condition = Condition::EntityNamed {
        target: Target::Current,
        name: "Home".into(),
    };
    let query = Condition::AnyOwnedProvince {
        target: Target::Current,
        condition: Box::new(condition.clone()),
    };
    assert!(query.evaluate(&world, EntityId(0)).unwrap());
    assert!(query.evaluate(&world, EntityId(1)).is_err());
    assert!(
        Condition::Not(Box::new(query.clone()))
            .evaluate(&world, EntityId(1))
            .is_err()
    );
    world
        .mark_target_absent(EntityId(5), Target::Owner)
        .unwrap();
    assert!(!query.evaluate(&world, EntityId(1)).unwrap());
    assert!(!query.evaluate(&world, EntityId(2)).unwrap());
    let before = world.save("owned-filter").unwrap();
    for _ in 0..20 {
        assert!(query.evaluate(&world, EntityId(0)).unwrap());
    }
    assert_eq!(world.save("owned-filter").unwrap(), before);
    world.set_owner(EntityId(3), EntityId(1)).unwrap();
    assert!(!query.evaluate(&world, EntityId(0)).unwrap());
    assert!(query.evaluate(&world, EntityId(1)).unwrap());
    world.set_owner(EntityId(3), EntityId(1)).unwrap();
    assert!(query.evaluate(&world, EntityId(1)).unwrap());
    assert!(world.any_owned_province(EntityId(3), &condition).is_err());
    assert!(world.any_owned_province(EntityId(99), &condition).is_err());
}

#[test]
fn delayed_owned_queries_rebuild_members_after_transfer_and_restore() {
    let mut continuous = world();
    continuous
        .mark_target_absent(EntityId(5), Target::Owner)
        .unwrap();
    let query = Condition::AnyOwnedProvince {
        target: Target::Current,
        condition: Box::new(Condition::AtLeast {
            target: Target::Current,
            field: augustus_core::FieldId(0),
            value: 10,
        }),
    };
    continuous
        .set(EntityId(3), augustus_core::FieldId(0), 10)
        .unwrap();
    continuous
        .schedule(
            EntityId(0),
            2,
            vec![Action::If {
                condition: query.clone(),
                then: vec![Action::Emit("present".into())],
                otherwise: vec![Action::Emit("absent".into())],
            }],
        )
        .unwrap();
    continuous.set_owner(EntityId(3), EntityId(1)).unwrap();
    let mut resumed = World::restore(
        &continuous.save("owned-delay").unwrap(),
        "owned-delay",
        continuous.schema(),
    )
    .unwrap();
    for state in [&mut continuous, &mut resumed] {
        assert!(!query.evaluate(state, EntityId(0)).unwrap());
        assert!(query.evaluate(state, EntityId(1)).unwrap());
        state.advance_to(2).unwrap();
        assert_eq!(state.events().last().unwrap().2, "absent");
        state.set_owner(EntityId(3), EntityId(0)).unwrap();
        assert!(query.evaluate(state, EntityId(0)).unwrap());
    }
    assert_eq!(continuous, resumed);
}

#[test]
fn owned_query_dependencies_are_validated_inside_rules_and_saved_tasks() {
    let mut world = world();
    world
        .mark_target_absent(EntityId(5), Target::Owner)
        .unwrap();
    let query = Condition::AnyOwnedProvince {
        target: Target::Current,
        condition: Box::new(Condition::HasModifier {
            target: Target::Current,
            name: "missing".into(),
        }),
    };
    let plan = vec![Action::If {
        condition: query,
        then: vec![],
        otherwise: vec![],
    }];
    let mut ruled = World::with_rules(world.schema().clone(), vec![plan.clone()]);
    assert!(
        ruled
            .set_modifiers(augustus_core::Modifiers::default())
            .is_err()
    );
    assert!(world.schedule(EntityId(0), 1, plan).is_err());
    assert_eq!(world.pending_tasks(), 0);
    assert!(world.save("owned-dependencies").is_ok());
}
