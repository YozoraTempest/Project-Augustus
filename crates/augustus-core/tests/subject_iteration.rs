// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    Action, Condition, DiplomaticKind, DiplomaticRelation, EntityId, EntityKind, GlobalMarkers,
    Markers, Plan, Politics, Schema, ScopeSelector, Subjects, Target, World,
};

fn relation(first: usize, second: usize, kind: &str) -> DiplomaticRelation {
    DiplomaticRelation {
        kind: DiplomaticKind::Subject,
        first: EntityId(first),
        second: EntityId(second),
        subject_type: Some(kind.into()),
    }
}

fn fixture() -> World {
    let mut world = World::new(Schema::new(["unused"]).unwrap());
    for name in ["AAA", "BBB", "CCC", "DDD", "EEE"] {
        world.add_entity(name, EntityKind::Country);
    }
    world.add_entity("Port", EntityKind::Province);
    world
        .set_politics(Politics {
            relations: vec![
                relation(0, 2, "vassal"),
                relation(0, 1, "tributary"),
                relation(1, 3, "union"),
            ],
            ..Default::default()
        })
        .unwrap();
    world
        .set_subjects(Subjects {
            types: ["vassal", "tributary", "union"].map(String::from).into(),
            known: true,
            counts: [(EntityId(0), 9)].into(),
        })
        .unwrap();
    world
        .set_subject_country_order(vec![
            EntityId(0),
            EntityId(2),
            EntityId(1),
            EntityId(3),
            EntityId(4),
        ])
        .unwrap();
    world
        .set_markers(Markers {
            entities: (0..5)
                .map(|id| (EntityId(id), Default::default()))
                .collect(),
            ..Default::default()
        })
        .unwrap();
    world
        .set_global_markers(GlobalMarkers {
            known: true,
            ..Default::default()
        })
        .unwrap();
    world
}

fn every(condition: Condition, plan: Plan) -> Action {
    Action::ForEachSubjectCountry {
        country: Target::Current,
        condition,
        plan,
    }
}

fn typed(kind: &str) -> Condition {
    Condition::SubjectType {
        target: Target::Current,
        subject_type: kind.into(),
    }
}
fn ids(world: &World) -> Vec<EntityId> {
    world.events().iter().map(|row| row.1).collect()
}

#[test]
fn direct_relation_order_and_types_do_not_use_the_observed_count() {
    let mut world = fixture();
    world
        .execute(
            EntityId(0),
            &[every(
                Condition::Always(true),
                vec![Action::Emit("all".into())],
            )],
        )
        .unwrap();
    assert_eq!(ids(&world), [EntityId(2), EntityId(1)]);
    assert_eq!(world.subject_count(EntityId(0)).unwrap(), 9);
    world
        .execute(
            EntityId(0),
            &[every(
                typed("tributary"),
                vec![Action::Emit("tributary".into())],
            )],
        )
        .unwrap();
    assert_eq!(ids(&world), [EntityId(2), EntityId(1), EntityId(1)]);
    world
        .execute(
            EntityId(4),
            &[every(
                Condition::Always(true),
                vec![Action::Emit("wrong".into())],
            )],
        )
        .unwrap();
    assert_eq!(world.events().len(), 3);
}

#[test]
fn nested_frames_keep_the_callers_root_and_return_to_the_outer_member() {
    let mut world = fixture();
    let root = Condition::InScope {
        selector: ScopeSelector::Root,
        kind: EntityKind::Country,
        condition: Box::new(Condition::EntityNamed {
            target: Target::Current,
            name: "AAA".into(),
        }),
    };
    world
        .execute(
            EntityId(0),
            &[every(
                root.clone(),
                vec![
                    Action::InScope {
                        selector: ScopeSelector::Root,
                        kind: EntityKind::Country,
                        plan: vec![every(root, vec![Action::Emit("inner".into())])],
                    },
                    Action::Emit("outer".into()),
                ],
            )],
        )
        .unwrap();
    assert_eq!(
        ids(&world),
        [
            EntityId(2),
            EntityId(1),
            EntityId(2),
            EntityId(2),
            EntityId(1),
            EntityId(1)
        ]
    );
}

#[test]
fn later_members_read_the_state_written_by_an_earlier_body() {
    let mut world = fixture();
    world.set_global_marker("open", true).unwrap();
    world
        .execute(
            EntityId(0),
            &[every(
                Condition::HasGlobalMarker {
                    name: "open".into(),
                },
                vec![
                    Action::SetGlobalMarker {
                        name: "open".into(),
                        present: false,
                    },
                    Action::Emit("first".into()),
                ],
            )],
        )
        .unwrap();
    assert_eq!(ids(&world), [EntityId(2)]);
    world.set_marker(EntityId(2), "ready", true).unwrap();
    world
        .execute(
            EntityId(0),
            &[every(
                Condition::HasMarker {
                    target: Target::Current,
                    name: "ready".into(),
                },
                vec![
                    Action::InScope {
                        selector: ScopeSelector::Named {
                            kind: EntityKind::Country,
                            name: "BBB".into(),
                        },
                        kind: EntityKind::Country,
                        plan: vec![Action::SetMarker {
                            target: Target::Current,
                            name: "ready".into(),
                            present: true,
                        }],
                    },
                    Action::Emit("opened".into()),
                ],
            )],
        )
        .unwrap();
    assert_eq!(ids(&world), [EntityId(2), EntityId(2), EntityId(1)]);
}

#[test]
fn delayed_frames_select_current_relations_and_restore_without_a_second_member_list() {
    let mut world = fixture();
    world
        .schedule(
            EntityId(0),
            2,
            vec![every(
                Condition::Always(true),
                vec![Action::Emit("later".into())],
            )],
        )
        .unwrap();
    world
        .set_politics(Politics {
            relations: vec![relation(0, 1, "tributary"), relation(1, 3, "union")],
            ..Default::default()
        })
        .unwrap();
    let saved = world.save("subject-later").unwrap();
    let mut resumed = World::restore(&saved, "subject-later", world.schema()).unwrap();
    for state in [&mut world, &mut resumed] {
        state.advance_to(2).unwrap();
    }
    assert_eq!(world, resumed);
    assert_eq!(ids(&world), [EntityId(1)]);
    assert_eq!(
        world.save("subject-later").unwrap(),
        resumed.save("subject-later").unwrap()
    );
}

#[test]
fn scheduled_empty_bodies_still_validate_nested_dependencies() {
    let mut world = fixture();
    let saved = world.save("subject-invalid").unwrap();
    for action in [
        every(typed("missing"), vec![]),
        every(
            Condition::Always(false),
            vec![Action::ForEachSubjectCountry {
                country: Target::Current,
                condition: typed("missing"),
                plan: vec![],
            }],
        ),
    ] {
        assert!(world.schedule(EntityId(0), 2, vec![action]).is_err());
        assert_eq!(world.save("subject-invalid").unwrap(), saved);
    }
}

#[test]
fn unknown_relations_and_non_country_targets_do_not_silently_select_nothing() {
    for entity in [EntityId(5), EntityId(99)] {
        let mut world = fixture();
        assert!(
            world
                .execute(entity, &[every(Condition::Always(false), vec![])])
                .is_err()
        );
        assert!(world.events().is_empty());
    }
    let mut world = fixture();
    let mut subjects = world.subjects().clone();
    subjects.known = false;
    world.set_subjects(subjects).unwrap();
    assert!(
        world
            .execute(EntityId(0), &[every(Condition::Always(false), vec![])])
            .is_err()
    );
    assert!(world.events().is_empty());
}

#[test]
fn large_and_nested_selections_share_instruction_and_depth_limits() {
    let mut world = fixture();
    let mut politics = world.politics().clone();
    for index in 0..100_000 {
        let member = world.add_entity(format!("member-{index}"), EntityKind::Country);
        politics.relations.push(relation(0, member.0, "vassal"));
    }
    world.set_politics(politics).unwrap();
    assert!(
        world
            .execute(EntityId(0), &[every(Condition::Always(false), vec![])])
            .unwrap_err()
            .to_string()
            .contains("instruction limit")
    );
    let mut world = fixture();
    let mut nested = vec![Action::Emit("wrong".into())];
    for _ in 0..100 {
        nested = vec![Action::InScope {
            selector: ScopeSelector::Root,
            kind: EntityKind::Country,
            plan: vec![every(Condition::Always(true), nested)],
        }];
    }
    assert!(world.execute(EntityId(0), &nested).is_err());
    assert!(world.events().is_empty());
}

#[test]
fn priority_round_trips_and_rejects_missing_duplicate_or_non_country_entries() {
    let world = fixture();
    let saved = world.save("subject-order").unwrap();
    let value: serde_json::Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(
        value["subject_country_order"],
        serde_json::json!([0, 2, 1, 3, 4])
    );
    let restored = World::restore(&saved, "subject-order", world.schema()).unwrap();
    assert_eq!(world, restored);
    for order in [
        vec![EntityId(0)],
        vec![EntityId(0); 5],
        vec![
            EntityId(0),
            EntityId(1),
            EntityId(2),
            EntityId(3),
            EntityId(5),
        ],
        vec![
            EntityId(0),
            EntityId(1),
            EntityId(2),
            EntityId(3),
            EntityId(99),
        ],
    ] {
        let mut changed = world.clone();
        assert!(changed.set_subject_country_order(order.clone()).is_err());
        assert_eq!(changed.save("subject-order").unwrap(), saved);
        let mut invalid = value.clone();
        invalid["subject_country_order"] = serde_json::to_value(order).unwrap();
        assert!(World::restore(&invalid.to_string(), "subject-order", world.schema()).is_err());
    }
    for member in [5, 99] {
        let mut invalid = value.clone();
        invalid["politics"]["relations"][0]["second"] = serde_json::json!(member);
        assert!(World::restore(&invalid.to_string(), "subject-order", world.schema()).is_err());
    }
    let mut legacy = value;
    legacy
        .as_object_mut()
        .unwrap()
        .remove("subject_country_order");
    let restored = World::restore(&legacy.to_string(), "subject-order", world.schema()).unwrap();
    assert_eq!(
        restored.subject_countries(EntityId(0)).unwrap(),
        [EntityId(1), EntityId(2)]
    );
    assert!(
        !restored
            .save("subject-order")
            .unwrap()
            .contains("subject_country_order")
    );
}

#[test]
fn priority_applies_to_replaced_relations_and_new_countries_append_once() {
    let mut world = fixture();
    let member = world.add_entity("FFF", EntityKind::Country);
    let mut politics = world.politics().clone();
    politics.relations.push(relation(0, member.0, "union"));
    world.set_politics(politics).unwrap();
    assert_eq!(
        world.subject_countries(EntityId(0)).unwrap(),
        [EntityId(2), EntityId(1), member]
    );
    let saved = world.save("subject-added").unwrap();
    assert_eq!(
        World::restore(&saved, "subject-added", world.schema()).unwrap(),
        world
    );
}
