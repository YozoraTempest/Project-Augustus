// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    Action, Condition, DiplomaticKind, DiplomaticRelation, EntityId, EntityKind, Politics, Schema,
    Subjects, Target, World,
};

fn count_at_least(value: i64) -> Condition {
    Condition::NumericCompare {
        target: Target::Current,
        slot: augustus_core::NumericSlot::SubjectCount,
        comparison: augustus_core::NumericComparison::AtLeast,
        value,
    }
}

#[test]
fn explicit_counts_can_differ_from_membership_and_round_trip_without_refresh() {
    let mut world = fixture();
    let mut subjects = world.subjects().clone();
    subjects.counts = [(EntityId(0), 3), (EntityId(4), 0)].into();
    world.set_subjects(subjects).unwrap();
    assert_eq!(world.subject_countries(EntityId(0)).unwrap().len(), 2);
    assert_eq!(world.subject_count(EntityId(0)).unwrap(), 3);
    assert!(count_at_least(3).evaluate(&world, EntityId(0)).unwrap());
    assert!(!count_at_least(4).evaluate(&world, EntityId(0)).unwrap());
    assert!(count_at_least(0).evaluate(&world, EntityId(4)).unwrap());
    assert!(!count_at_least(1).evaluate(&world, EntityId(4)).unwrap());
    assert!(count_at_least(0).evaluate(&world, EntityId(1)).is_err());
    assert!(world.subject_count(EntityId(5)).is_err());
    world.advance_to(40).unwrap();
    let saved = world.save("explicit-count").unwrap();
    let restored = World::restore(&saved, "explicit-count", world.schema()).unwrap();
    assert_eq!(world, restored);
    assert_eq!(restored.subject_count(EntityId(0)).unwrap(), 3);
    let mut legacy: serde_json::Value = serde_json::from_str(&saved).unwrap();
    legacy["subjects"].as_object_mut().unwrap().remove("counts");
    let legacy = World::restore(&legacy.to_string(), "explicit-count", world.schema()).unwrap();
    assert!(legacy.subject_count(EntityId(0)).is_err());
    assert!(
        !legacy
            .save("explicit-count")
            .unwrap()
            .contains("\"counts\"")
    );
}

#[test]
fn relationship_replacement_invalidates_only_affected_counts_and_failure_is_atomic() {
    let mut world = fixture();
    let mut subjects = world.subjects().clone();
    subjects.counts = [(EntityId(0), 3), (EntityId(1), 1), (EntityId(4), 0)].into();
    world.set_subjects(subjects).unwrap();
    let saved = world.save("counts").unwrap();
    let mut invalid = world.subjects().clone();
    invalid.counts.insert(EntityId(5), 0);
    assert!(world.set_subjects(invalid).is_err());
    assert_eq!(world.save("counts").unwrap(), saved);
    let mut invalid_save: serde_json::Value = serde_json::from_str(&saved).unwrap();
    invalid_save["subjects"]["counts"]["99"] = serde_json::json!(0);
    assert!(World::restore(&invalid_save.to_string(), "counts", world.schema()).is_err());
    let mut politics = world.politics().clone();
    politics.offices.insert("office".into(), EntityId(4));
    world.set_politics(politics).unwrap();
    assert_eq!(world.subject_count(EntityId(0)).unwrap(), 3);
    let mut politics = world.politics().clone();
    politics.relations.push(relation(4, 1, "vassal"));
    let saved = world.save("counts").unwrap();
    assert!(world.set_politics(politics).is_err());
    assert_eq!(world.save("counts").unwrap(), saved);
    let mut politics = world.politics().clone();
    politics.relations.retain(|r| r.second != EntityId(2));
    world.set_politics(politics).unwrap();
    assert!(world.subject_count(EntityId(0)).is_err());
    assert_eq!(world.subject_count(EntityId(1)).unwrap(), 1);
    assert_eq!(world.subject_count(EntityId(4)).unwrap(), 0);
}

#[test]
fn queued_count_reads_use_explicit_state_and_numeric_writes_are_rejected() {
    let mut world = fixture();
    let mut subjects = world.subjects().clone();
    subjects.known = false;
    subjects.counts.insert(EntityId(0), 1);
    world.set_subjects(subjects).unwrap();
    assert!(world.subject_countries(EntityId(0)).is_err());
    assert_eq!(world.subject_count(EntityId(0)).unwrap(), 1);
    world
        .schedule(
            EntityId(0),
            2,
            vec![Action::If {
                condition: count_at_least(2),
                then: vec![Action::Emit("two".into())],
                otherwise: vec![Action::Emit("one".into())],
            }],
        )
        .unwrap();
    let saved = world.save("queued-count").unwrap();
    let action = Action::UpdateNumeric {
        target: Target::Current,
        slot: augustus_core::NumericSlot::SubjectCount,
        operation: augustus_core::NumericOperation::Set,
        value: 2,
    };
    assert!(world.schedule(EntityId(0), 1, vec![action]).is_err());
    assert!(
        world
            .update_numeric(
                EntityId(0),
                &augustus_core::NumericSlot::SubjectCount,
                augustus_core::NumericOperation::Set,
                2
            )
            .is_err()
    );
    assert_eq!(world.save("queued-count").unwrap(), saved);
    let mut subjects = world.subjects().clone();
    subjects.counts.insert(EntityId(0), 2);
    world.set_subjects(subjects).unwrap();
    let saved = world.save("queued-count").unwrap();
    let mut resumed = World::restore(&saved, "queued-count", world.schema()).unwrap();
    world.advance_to(2).unwrap();
    resumed.advance_to(2).unwrap();
    assert_eq!(world, resumed);
    assert_eq!(world.events()[0].2, "two");
}

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
    world.set_owner(EntityId(5), EntityId(1)).unwrap();
    world.set_capital(EntityId(1), EntityId(5)).unwrap();
    world
        .set_politics(Politics {
            relations: vec![
                relation(0, 1, "vassal"),
                relation(0, 2, "tributary"),
                relation(1, 3, "union"),
            ],
            ..Default::default()
        })
        .unwrap();
    world
        .set_subjects(Subjects {
            types: ["vassal", "tributary", "union"].map(String::from).into(),
            known: true,
            ..Default::default()
        })
        .unwrap();
    world
}

fn typed(kind: &str) -> Condition {
    Condition::SubjectType {
        target: Target::Current,
        subject_type: kind.into(),
    }
}

fn any(condition: Condition) -> Condition {
    Condition::AnySubjectCountry {
        target: Target::Current,
        condition: Box::new(condition),
    }
}

#[test]
fn direct_members_include_each_type_and_scoped_queries_read_the_same_graph() {
    let world = fixture();
    assert_eq!(
        world.subject_countries(EntityId(0)).unwrap(),
        [EntityId(1), EntityId(2)]
    );
    assert_eq!(world.subject_countries(EntityId(1)).unwrap(), [EntityId(3)]);
    assert!(world.subject_countries(EntityId(4)).unwrap().is_empty());
    assert!(any(typed("vassal")).evaluate(&world, EntityId(0)).unwrap());
    assert!(
        any(typed("tributary"))
            .evaluate(&world, EntityId(0))
            .unwrap()
    );
    assert!(!any(typed("union")).evaluate(&world, EntityId(0)).unwrap());
    assert!(any(typed("union")).evaluate(&world, EntityId(1)).unwrap());
    assert!(!typed("vassal").evaluate(&world, EntityId(4)).unwrap());
    assert!(
        !any(Condition::Always(false))
            .evaluate(&world, EntityId(0))
            .unwrap()
    );
    let capital_owner = any(Condition::With {
        target: Target::Capital,
        kind: EntityKind::Province,
        condition: Box::new(Condition::With {
            target: Target::Owner,
            kind: EntityKind::Country,
            condition: Box::new(typed("vassal")),
        }),
    });
    assert!(capital_owner.evaluate(&world, EntityId(0)).unwrap());
    assert!(typed("vassal").evaluate(&world, EntityId(5)).is_err());
}

#[test]
fn membership_replacement_rebuilds_lookup_atomically_and_restore_uses_current_records() {
    let mut world = fixture();
    world
        .set_politics(Politics {
            relations: vec![relation(4, 1, "union"), relation(0, 2, "tributary")],
            ..Default::default()
        })
        .unwrap();
    assert_eq!(world.subject_countries(EntityId(0)).unwrap(), [EntityId(2)]);
    assert_eq!(world.subject_countries(EntityId(4)).unwrap(), [EntityId(1)]);
    assert!(!world.is_subject_of_type(EntityId(1), "vassal").unwrap());
    assert!(world.is_subject_of_type(EntityId(1), "union").unwrap());
    assert!(!world.is_subject_of_type(EntityId(3), "union").unwrap());
    let before = world.save("relations").unwrap();
    for relations in [
        vec![relation(4, 1, "missing")],
        vec![relation(4, 1, "union"), relation(1, 4, "union")],
    ] {
        assert!(
            world
                .set_politics(Politics {
                    relations,
                    ..Default::default()
                })
                .is_err()
        );
        assert_eq!(world.save("relations").unwrap(), before);
    }
    assert!(!before.contains("subject_index"));
    let restored = World::restore(&before, "relations", world.schema()).unwrap();
    assert_eq!(world, restored);
    assert_eq!(
        restored.subject_countries(EntityId(4)).unwrap(),
        [EntityId(1)]
    );
    let mut corrupt: serde_json::Value = serde_json::from_str(&before).unwrap();
    corrupt["subjects"]["types"] = serde_json::json!(["vassal"]);
    assert!(World::restore(&corrupt.to_string(), "relations", world.schema()).is_err());
}

#[test]
fn incomplete_relations_remain_unknown_including_old_positive_records_and_negation() {
    let mut world = fixture();
    let mut declarations = world.subjects().clone();
    declarations.known = false;
    world.set_subjects(declarations).unwrap();
    assert!(typed("vassal").evaluate(&world, EntityId(1)).is_err());
    assert!(
        any(Condition::Always(false))
            .evaluate(&world, EntityId(4))
            .is_err()
    );
    assert!(
        Condition::Not(Box::new(any(Condition::Always(true))))
            .evaluate(&world, EntityId(0))
            .is_err()
    );
    let save = world.save("unknown").unwrap();
    let restored = World::restore(&save, "unknown", world.schema()).unwrap();
    assert_eq!(world, restored);
    assert!(restored.is_subject_of_type(EntityId(1), "vassal").is_err());
    let mut legacy: serde_json::Value = serde_json::from_str(&save).unwrap();
    legacy.as_object_mut().unwrap().remove("subjects");
    let legacy = World::restore(&legacy.to_string(), "unknown", world.schema()).unwrap();
    assert!(legacy.subject_countries(EntityId(0)).is_err());
    assert!(!legacy.save("unknown").unwrap().contains("subjects"));
}

#[test]
fn delayed_queries_read_live_relations_and_declarations_cannot_invalidate_queued_conditions() {
    let mut world = fixture();
    world
        .schedule(
            EntityId(0),
            2,
            vec![Action::If {
                condition: any(typed("vassal")),
                then: vec![Action::Emit("present".into())],
                otherwise: vec![Action::Emit("absent".into())],
            }],
        )
        .unwrap();
    let before = world.save("later").unwrap();
    assert!(
        world
            .set_subjects(Subjects {
                types: ["union", "tributary"].map(String::from).into(),
                known: false,
                ..Default::default()
            })
            .is_err()
    );
    assert_eq!(world.save("later").unwrap(), before);
    let invalid = Action::If {
        condition: Condition::Always(false),
        then: vec![Action::If {
            condition: typed("missing"),
            then: vec![],
            otherwise: vec![],
        }],
        otherwise: vec![],
    };
    assert!(world.schedule(EntityId(0), 1, vec![invalid]).is_err());
    assert_eq!(world.save("later").unwrap(), before);
    let mut politics = world.politics().clone();
    politics
        .relations
        .retain(|relation| relation.second != EntityId(1));
    world.set_politics(politics).unwrap();
    let save = world.save("later").unwrap();
    let mut resumed = World::restore(&save, "later", world.schema()).unwrap();
    world.advance_to(2).unwrap();
    resumed.advance_to(2).unwrap();
    assert_eq!(world, resumed);
    assert_eq!(world.events()[0].2, "absent");
}

#[test]
fn large_direct_graph_and_unrelated_relations_do_not_change_membership() {
    let mut world = World::new(Schema::new(["unused"]).unwrap());
    for index in 0..20_001 {
        world.add_entity(format!("C{index}"), EntityKind::Country);
    }
    let relations = (1..20_001)
        .map(|index| relation(0, index, "vassal"))
        .collect();
    world
        .set_politics(Politics {
            relations,
            ..Default::default()
        })
        .unwrap();
    world
        .set_subjects(Subjects {
            types: ["vassal".into()].into(),
            known: true,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(world.subject_countries(EntityId(0)).unwrap().len(), 20_000);
    assert!(
        world
            .is_subject_of_type(EntityId(20_000), "vassal")
            .unwrap()
    );
    let mut politics = world.politics().clone();
    politics.relations.clear();
    politics.relations.push(DiplomaticRelation {
        kind: DiplomaticKind::Alliance,
        first: EntityId(0),
        second: EntityId(1),
        subject_type: None,
    });
    world.set_politics(politics).unwrap();
    assert!(world.subject_countries(EntityId(0)).unwrap().is_empty());
    assert!(!world.is_subject_of_type(EntityId(1), "vassal").unwrap());
}
