// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{Action, Condition, EntityId, EntityKind, Schema, Target, World};

fn world() -> World {
    let mut world = World::new(Schema::new(["value"]).unwrap());
    world.add_entity("Union", EntityKind::Country);
    world.add_entity("League", EntityKind::Country);
    world.add_entity("Port", EntityKind::Province);
    world.set_owner(EntityId(2), EntityId(0)).unwrap();
    world.set_capital(EntityId(0), EntityId(2)).unwrap();
    world
        .set_affiliations(
            serde_json::from_str(
                r#"{
        "faith": {"groups":{"first":"near","second":"near","third":"far","unknown":null},
        "members":{"0":"first","1":"second","2":"third"},
        "acceptance":{"0":{"members":["third"],"groups":["far"]}}}
    }"#,
            )
            .unwrap(),
        )
        .unwrap();
    world
}

fn member(target: Target, name: &str) -> Condition {
    Condition::AffiliationMember {
        target,
        dimension: "faith".into(),
        member: name.into(),
    }
}

fn group(target: Target, name: &str) -> Condition {
    Condition::AffiliationGroup {
        target,
        dimension: "faith".into(),
        group: name.into(),
    }
}

#[test]
fn conditions_distinguish_members_from_groups_and_acceptance_and_read_live_targets() {
    let mut world = world();
    assert!(
        member(Target::Current, "first")
            .evaluate(&world, EntityId(0))
            .unwrap()
    );
    assert!(
        !member(Target::Current, "second")
            .evaluate(&world, EntityId(0))
            .unwrap()
    );
    assert!(
        group(Target::Current, "near")
            .evaluate(&world, EntityId(0))
            .unwrap()
    );
    assert!(
        group(Target::Current, "near")
            .evaluate(&world, EntityId(1))
            .unwrap()
    );
    assert!(
        !group(Target::Current, "far")
            .evaluate(&world, EntityId(0))
            .unwrap()
    );
    assert!(
        member(Target::Capital, "third")
            .evaluate(&world, EntityId(0))
            .unwrap()
    );
    assert!(
        group(Target::Capital, "far")
            .evaluate(&world, EntityId(0))
            .unwrap()
    );
    assert!(
        member(Target::Owner, "first")
            .evaluate(&world, EntityId(2))
            .unwrap()
    );
    world.set_owner(EntityId(2), EntityId(1)).unwrap();
    assert!(
        member(Target::Owner, "second")
            .evaluate(&world, EntityId(2))
            .unwrap()
    );
    assert!(
        group(Target::Owner, "near")
            .evaluate(&world, EntityId(2))
            .unwrap()
    );
    assert!(
        group(Target::Owner, "near")
            .evaluate(&world, EntityId(0))
            .is_err()
    );
}

#[test]
fn delayed_plans_query_changed_memberships_after_cross_save_restore() {
    let mut world = world();
    let plan = vec![Action::If {
        condition: Condition::All(vec![
            member(Target::Current, "third"),
            group(Target::Current, "far"),
        ]),
        then: vec![Action::Emit("changed".into())],
        otherwise: vec![Action::Emit("stale".into())],
    }];
    world.schedule(EntityId(0), 1, plan).unwrap();
    let mut affiliations = world.affiliations().clone();
    affiliations
        .0
        .get_mut("faith")
        .unwrap()
        .members
        .insert(EntityId(0), Some("third".into()));
    world.set_affiliations(affiliations).unwrap();
    let saved = world.save("live-affiliations").unwrap();
    let mut resumed = World::restore(&saved, "live-affiliations", world.schema()).unwrap();
    for state in [&mut world, &mut resumed] {
        state.advance_to(1).unwrap();
        assert_eq!(state.events(), &[(1, EntityId(0), "changed".into())]);
    }
    assert_eq!(world, resumed);
    assert_eq!(
        world.save("live-affiliations").unwrap(),
        resumed.save("live-affiliations").unwrap()
    );
}

#[test]
fn unknown_members_and_groups_are_errors_even_under_negation() {
    let mut world = world();
    let mut affiliations = world.affiliations().clone();
    let data = affiliations.0.get_mut("faith").unwrap();
    data.members.insert(EntityId(0), Some("unknown".into()));
    data.members.insert(EntityId(1), None);
    data.members.remove(&EntityId(2));
    world.set_affiliations(affiliations).unwrap();
    assert!(
        member(Target::Current, "unknown")
            .evaluate(&world, EntityId(0))
            .unwrap()
    );
    assert!(
        group(Target::Current, "near")
            .evaluate(&world, EntityId(0))
            .is_err()
    );
    for entity in [EntityId(1), EntityId(2), EntityId(99)] {
        assert!(
            member(Target::Current, "first")
                .evaluate(&world, entity)
                .is_err()
        );
        assert!(
            Condition::Not(Box::new(group(Target::Current, "near")))
                .evaluate(&world, entity)
                .is_err()
        );
    }
    let missing = Condition::AffiliationMember {
        target: Target::Current,
        dimension: "missing".into(),
        member: "first".into(),
    };
    assert!(missing.evaluate(&world, EntityId(0)).is_err());
    let saved = world.save("read-only").unwrap();
    for _ in 0..3 {
        assert!(missing.evaluate(&world, EntityId(0)).is_err());
    }
    assert_eq!(world.save("read-only").unwrap(), saved);
}

#[test]
fn invalid_identifiers_are_rejected_in_unselected_saved_plans() {
    let mut world = world();
    for invalid in [
        member(Target::Current, ""),
        group(Target::Current, ""),
        Condition::AffiliationMember {
            target: Target::Current,
            dimension: "".into(),
            member: "first".into(),
        },
        Condition::AffiliationGroup {
            target: Target::Current,
            dimension: "".into(),
            group: "near".into(),
        },
    ] {
        assert!(invalid.evaluate(&world, EntityId(0)).is_err());
        let before = world.clone();
        assert!(
            world
                .schedule(
                    EntityId(0),
                    1,
                    vec![Action::If {
                        condition: Condition::Any(vec![Condition::Always(true), invalid]),
                        then: Vec::new(),
                        otherwise: Vec::new(),
                    }]
                )
                .is_err()
        );
        assert_eq!(world, before);
    }
    world
        .schedule(
            EntityId(0),
            1,
            vec![Action::If {
                condition: group(Target::Current, "near"),
                then: Vec::new(),
                otherwise: Vec::new(),
            }],
        )
        .unwrap();
    let mut saved: serde_json::Value =
        serde_json::from_str(&world.save("invalid-plan").unwrap()).unwrap();
    saved["tasks"][0]["plan"][0]["If"]["condition"]["AffiliationGroup"]["dimension"] =
        serde_json::json!("");
    assert!(World::restore(&saved.to_string(), "invalid-plan", world.schema()).is_err());
}
