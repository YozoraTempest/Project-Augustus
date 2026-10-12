// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    Action, Condition, EntityId, EntityKind, EntityRef, Plan, Schema, ScopeSelector, Target, World,
};

fn fixture() -> World {
    let mut world = World::seeded(Schema::new(["unused"]).unwrap(), vec![], [9; 32]);
    for name in ["AAA", "BBB", "Empty"] {
        world.add_entity(name, EntityKind::Country);
    }
    for name in ["72", "73", "128", "Sea"] {
        world.add_entity(name, EntityKind::Province);
    }
    for province in [3, 4, 5] {
        world.set_owner(EntityId(province), EntityId(0)).unwrap();
    }
    world
        .mark_target_absent(EntityId(6), Target::Owner)
        .unwrap();
    world
}

fn named(name: &str) -> Condition {
    Condition::EntityNamed {
        target: Target::Current,
        name: name.into(),
    }
}

fn transfer(name: &str, country: &str) -> Action {
    Action::InScope {
        selector: ScopeSelector::Named {
            kind: EntityKind::Province,
            name: name.into(),
        },
        kind: EntityKind::Province,
        plan: vec![set_owner(country)],
    }
}

fn set_owner(country: &str) -> Action {
    Action::SetOwner {
        target: Target::Current,
        owner: EntityRef::Selected(ScopeSelector::Named {
            kind: EntityKind::Country,
            name: country.into(),
        }),
    }
}

fn every(condition: Condition, plan: Plan) -> Action {
    Action::ForEachOwnedProvince {
        country: Target::Current,
        condition,
        plan,
    }
}

fn random(condition: Condition, plan: Plan) -> Action {
    Action::RandomOwnedProvince {
        country: Target::Current,
        condition,
        plan,
    }
}

fn first_transfers() -> Action {
    Action::If {
        condition: named("72"),
        then: vec![transfer("73", "BBB"), transfer("128", "BBB")],
        otherwise: vec![],
    }
}

fn member_ids(world: &World) -> Vec<EntityId> {
    world.events().iter().map(|row| row.1).collect()
}

#[test]
fn acquisition_order_survives_transfer_reacquisition_and_restore() {
    let mut world = fixture();
    assert_eq!(
        world.owned_provinces(EntityId(0)).unwrap(),
        [EntityId(3), EntityId(4), EntityId(5)]
    );
    assert!(
        !world
            .save("owned-order")
            .unwrap()
            .contains("owned_province_order")
    );
    for province in [3, 4] {
        world.set_owner(EntityId(province), EntityId(1)).unwrap();
    }
    for province in [4, 3] {
        world.set_owner(EntityId(province), EntityId(0)).unwrap();
    }
    world.set_owner(EntityId(4), EntityId(0)).unwrap();
    assert_eq!(
        world.owned_provinces(EntityId(0)).unwrap(),
        [EntityId(5), EntityId(4), EntityId(3)]
    );
    assert!(world.owned_provinces(EntityId(1)).unwrap().is_empty());
    let saved = world.save("owned-order").unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&saved).unwrap()["owned_province_order"]["0"],
        serde_json::json!([5, 4, 3])
    );
    let mut restored = World::restore(&saved, "owned-order", world.schema()).unwrap();
    assert_eq!(world, restored);
    for state in [&mut world, &mut restored] {
        state.set_owner(EntityId(5), EntityId(1)).unwrap();
        assert_eq!(
            state.owned_provinces(EntityId(0)).unwrap(),
            [EntityId(4), EntityId(3)]
        );
        assert_eq!(state.owned_provinces(EntityId(1)).unwrap(), [EntityId(5)]);
    }
    assert_eq!(world, restored);
}

#[test]
fn restored_order_rejects_missing_duplicate_foreign_and_invalid_members() {
    let world = fixture();
    let value: serde_json::Value =
        serde_json::from_str(&world.save("owned-invalid").unwrap()).unwrap();
    for order in [
        serde_json::json!({"0":[3,3,5]}),
        serde_json::json!({"0":[3,4]}),
        serde_json::json!({"0":[3,4,6]}),
        serde_json::json!({"0":[3,4,99]}),
        serde_json::json!({"0":[]}),
        serde_json::json!({"1":[]}),
        serde_json::json!({"3":[3,4,5]}),
        serde_json::json!({"99":[3,4,5]}),
    ] {
        let mut invalid = value.clone();
        invalid["owned_province_order"] = order;
        assert!(World::restore(&invalid.to_string(), "owned-invalid", world.schema()).is_err());
    }
    let mut valid = value;
    valid["owned_province_order"] = serde_json::json!({"0":[5,3,4]});
    assert_eq!(
        World::restore(&valid.to_string(), "owned-invalid", world.schema())
            .unwrap()
            .owned_provinces(EntityId(0))
            .unwrap(),
        [EntityId(5), EntityId(3), EntityId(4)]
    );
}

#[test]
fn captured_members_remain_in_the_frame_after_future_transfers() {
    let mut world = fixture();
    world
        .execute(
            EntityId(0),
            &[every(
                Condition::Always(true),
                vec![first_transfers(), Action::Emit("member".into())],
            )],
        )
        .unwrap();
    assert_eq!(member_ids(&world), [EntityId(3), EntityId(4), EntityId(5)]);
    assert_eq!(world.entity(EntityId(4)).unwrap().owner, Some(EntityId(1)));
    assert_eq!(world.entity(EntityId(5)).unwrap().owner, Some(EntityId(1)));
}

#[test]
fn live_owner_filter_skips_transferred_members_and_accepts_reacquired_members() {
    for reacquire in [false, true] {
        let mut world = fixture();
        let condition = Condition::With {
            target: Target::Owner,
            kind: EntityKind::Country,
            condition: Box::new(named("AAA")),
        };
        let mut changes = vec![transfer("73", "BBB"), transfer("128", "BBB")];
        if reacquire {
            changes.push(transfer("73", "AAA"));
        }
        world
            .execute(
                EntityId(0),
                &[every(
                    condition,
                    vec![
                        Action::If {
                            condition: named("72"),
                            then: changes,
                            otherwise: vec![],
                        },
                        Action::Emit("member".into()),
                    ],
                )],
            )
            .unwrap();
        assert_eq!(
            member_ids(&world),
            if reacquire {
                vec![EntityId(3), EntityId(4)]
            } else {
                vec![EntityId(3)]
            }
        );
    }
}

#[test]
fn additions_enter_the_next_frame_and_current_removals_do_not_skip_members() {
    let mut world = fixture();
    world.set_owner(EntityId(4), EntityId(1)).unwrap();
    world.set_owner(EntityId(5), EntityId(1)).unwrap();
    world
        .execute(
            EntityId(0),
            &[every(
                Condition::Always(true),
                vec![
                    transfer("73", "AAA"),
                    transfer("128", "AAA"),
                    Action::Emit("added".into()),
                ],
            )],
        )
        .unwrap();
    assert_eq!(member_ids(&world), [EntityId(3)]);
    world
        .execute(
            EntityId(0),
            &[every(
                Condition::Always(true),
                vec![set_owner("BBB"), Action::Emit("removed".into())],
            )],
        )
        .unwrap();
    assert_eq!(
        member_ids(&world),
        [EntityId(3), EntityId(3), EntityId(4), EntityId(5)]
    );
    assert!(world.owned_provinces(EntityId(0)).unwrap().is_empty());
}

#[test]
fn nested_and_delayed_frames_keep_order_and_their_own_current_entity() {
    let mut world = fixture();
    let inner = Action::With {
        target: Target::Owner,
        plan: vec![every(
            Condition::Always(true),
            vec![Action::Emit("inner".into())],
        )],
    };
    world
        .schedule(
            EntityId(0),
            2,
            vec![every(
                Condition::Always(true),
                vec![inner, Action::Emit("outer".into())],
            )],
        )
        .unwrap();
    world.set_owner(EntityId(3), EntityId(1)).unwrap();
    world.set_owner(EntityId(3), EntityId(0)).unwrap();
    let mut restored = World::restore(
        &world.save("owned-nested").unwrap(),
        "owned-nested",
        world.schema(),
    )
    .unwrap();
    for state in [&mut world, &mut restored] {
        state.advance_to(2).unwrap();
    }
    assert_eq!(world, restored);
    assert_eq!(
        member_ids(&world),
        [
            EntityId(4),
            EntityId(5),
            EntityId(3),
            EntityId(4),
            EntityId(4),
            EntityId(5),
            EntityId(3),
            EntityId(5),
            EntityId(4),
            EntityId(5),
            EntityId(3),
            EntityId(3)
        ]
    );
}

#[test]
fn random_selection_filters_once_executes_one_member_and_restores_the_stream() {
    let mut world = fixture();
    world.set_owner(EntityId(3), EntityId(1)).unwrap();
    world.set_owner(EntityId(3), EntityId(0)).unwrap();
    let action = random(
        Condition::Any(vec![named("73"), named("128")]),
        vec![Action::Emit("chosen".into())],
    );
    for _ in 0..40 {
        world
            .execute(EntityId(0), std::slice::from_ref(&action))
            .unwrap();
    }
    let mut restored = World::restore(
        &world.save("owned-random").unwrap(),
        "owned-random",
        world.schema(),
    )
    .unwrap();
    for state in [&mut world, &mut restored] {
        for _ in 0..60 {
            state
                .execute(EntityId(0), std::slice::from_ref(&action))
                .unwrap();
        }
        assert_eq!(state.events().len(), 100);
        assert!(
            member_ids(state)
                .iter()
                .all(|id| [EntityId(4), EntityId(5)].contains(id))
        );
    }
    assert_eq!(world, restored);
    assert_eq!(
        world.save("owned-random").unwrap(),
        restored.save("owned-random").unwrap()
    );
}

#[test]
fn empty_and_false_selections_preserve_randomness_and_do_not_execute_the_body() {
    let mut world = fixture();
    let before = world.save("owned-empty").unwrap();
    for action in [
        every(Condition::Always(false), vec![Action::Emit("wrong".into())]),
        random(Condition::Always(false), vec![Action::Emit("wrong".into())]),
    ] {
        world
            .execute(EntityId(0), std::slice::from_ref(&action))
            .unwrap();
        assert_eq!(world.save("owned-empty").unwrap(), before);
    }
    for action in [
        every(Condition::Always(true), vec![Action::Emit("wrong".into())]),
        random(Condition::Always(true), vec![Action::Emit("wrong".into())]),
    ] {
        world
            .execute(EntityId(2), std::slice::from_ref(&action))
            .unwrap();
        assert_eq!(world.save("owned-empty").unwrap(), before);
    }
}

#[test]
fn member_selection_rejects_unknown_ownership_and_wrong_entity_types() {
    let mut world = fixture();
    assert!(world.owned_provinces(EntityId(3)).is_err());
    assert!(world.owned_provinces(EntityId(99)).is_err());
    world.add_entity("Unknown", EntityKind::Province);
    let before = world.save("owned-unknown").unwrap();
    assert!(world.owned_provinces(EntityId(0)).is_err());
    assert_eq!(world.save("owned-unknown").unwrap(), before);
    for action in [
        every(Condition::Always(true), vec![Action::Emit("wrong".into())]),
        random(Condition::Always(true), vec![Action::Emit("wrong".into())]),
    ] {
        let mut invalid = world.clone();
        assert!(invalid.execute(EntityId(0), &[action]).is_err());
        assert!(invalid.events().is_empty());
        assert!(invalid.save("owned-unknown").is_err());
    }
}

#[test]
fn owned_effect_dependencies_are_checked_inside_rules_and_delayed_tasks() {
    for action in [
        every(
            Condition::HasModifier {
                target: Target::Current,
                name: "missing".into(),
            },
            vec![],
        ),
        random(
            Condition::Always(true),
            vec![Action::RemoveModifier {
                target: Target::Current,
                name: "missing".into(),
            }],
        ),
        set_owner(""),
    ] {
        let mut world = fixture();
        assert!(
            world
                .schedule(EntityId(0), 1, vec![action.clone()])
                .is_err()
        );
        assert_eq!(world.pending_tasks(), 0);
        let mut ruled = World::with_rules(world.schema().clone(), vec![vec![action]]);
        assert!(
            ruled
                .set_modifiers(augustus_core::Modifiers::default())
                .is_err()
        );
    }
}

#[test]
fn owned_effects_share_instruction_and_call_depth_limits() {
    let mut large = fixture();
    for i in 0..100_000 {
        let province = large.add_entity(format!("extra-{i}"), EntityKind::Province);
        large.set_owner(province, EntityId(0)).unwrap();
    }
    for action in [
        every(Condition::Always(false), vec![]),
        random(Condition::Always(false), vec![]),
    ] {
        let mut world = large.clone();
        let error = world.execute(EntityId(0), &[action]).unwrap_err();
        assert!(error.to_string().contains("instruction limit"));
    }
    let mut plan = vec![Action::Emit("too-deep".into())];
    for _ in 0..64 {
        plan = vec![every(
            Condition::Always(true),
            vec![Action::With {
                target: Target::Owner,
                plan,
            }],
        )];
    }
    let mut world = fixture();
    assert!(
        world
            .execute(EntityId(0), &plan)
            .unwrap_err()
            .to_string()
            .contains("call depth")
    );
    assert!(world.events().is_empty());
}
