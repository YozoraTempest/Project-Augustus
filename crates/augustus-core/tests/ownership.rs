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
