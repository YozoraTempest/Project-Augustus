// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    Action, Condition, CountryPolitics, DiplomaticKind, DiplomaticRelation, EntityId, EntityKind,
    PersonState, Politics, Schema, Target, World,
};

fn world() -> World {
    let mut world = World::new(Schema::new([] as [&str; 0]).unwrap());
    for tag in ["AAA", "BBB", "CCC"] {
        world.add_entity(tag, EntityKind::Country);
    }
    world.add_entity("1", EntityKind::Province);
    world
}

fn relation(kind: DiplomaticKind, first: usize, second: usize) -> DiplomaticRelation {
    DiplomaticRelation {
        kind,
        first: EntityId(first),
        second: EntityId(second),
        subject_type: (kind == DiplomaticKind::Subject).then(|| "test_subject".into()),
    }
}

#[test]
fn office_holder_action_replaces_the_holder_and_survives_snapshot_resume() {
    let mut world = world();
    world
        .set_politics(Politics {
            offices: [("imperial_emperor".into(), EntityId(0))].into(),
            ..Default::default()
        })
        .unwrap();
    world
        .schedule(
            EntityId(1),
            1,
            vec![Action::SetOfficeHolder {
                office: "imperial_emperor".into(),
            }],
        )
        .unwrap();
    world.advance_to(1).unwrap();

    assert_eq!(
        world.politics().offices.get("imperial_emperor"),
        Some(&EntityId(1))
    );
    let save = world.save("office-holder").unwrap();
    assert_eq!(
        World::restore(&save, "office-holder", world.schema()).unwrap(),
        world
    );
}

#[test]
fn office_holder_action_rejects_province_scope() {
    let mut world = world();
    world
        .schedule(
            EntityId(3),
            1,
            vec![Action::SetOfficeHolder {
                office: "imperial_emperor".into(),
            }],
        )
        .unwrap();

    assert!(world.advance_to(1).is_err());
}

#[test]
fn political_state_survives_save_resume_and_empty_legacy_snapshots() {
    let mut world = world();
    let legacy = world.save("test").unwrap();
    assert!(!legacy.contains("politics"));
    assert_eq!(
        World::restore(&legacy, "test", world.schema()).unwrap(),
        world
    );
    let mut politics = Politics::default();
    politics.countries.insert(
        EntityId(0),
        CountryPolitics {
            ruler: Some(PersonState {
                name: Some("Iris".into()),
                country_of_origin: Some(EntityId(1)),
                administration: Some(2),
                ..Default::default()
            }),
            ..Default::default()
        },
    );
    politics
        .relations
        .push(relation(DiplomaticKind::Subject, 0, 1));
    politics.offices.insert("test_office".into(), EntityId(0));
    world.set_politics(politics).unwrap();
    world.schedule(EntityId(0), 7, vec![]).unwrap();
    let save = world.save("test").unwrap();
    let mut resumed = World::restore(&save, "test", world.schema()).unwrap();
    world.advance_to(10).unwrap();
    resumed.advance_to(10).unwrap();
    assert_eq!(resumed, world);
    let mut corrupt: serde_json::Value = serde_json::from_str(&save).unwrap();
    corrupt["politics"]["relations"][0]["second"] = 3.into();
    assert!(World::restore(&corrupt.to_string(), "test", world.schema()).is_err());
    corrupt = serde_json::from_str(&save).unwrap();
    corrupt["politics"]["countries"]["0"]["ruler"]["country_of_origin"] = 999.into();
    assert!(World::restore(&corrupt.to_string(), "test", world.schema()).is_err());
}

#[test]
fn office_preferences_survive_snapshots_and_older_snapshots_without_them_load() {
    let mut world = world();
    let mut politics = Politics::default();
    politics.countries.insert(
        EntityId(0),
        CountryPolitics {
            office_preferences: [("imperial_emperor".into(), EntityId(1))].into(),
            ..Default::default()
        },
    );
    world.set_politics(politics).unwrap();

    let save = world.save("office-preferences").unwrap();
    let restored = World::restore(&save, "office-preferences", world.schema()).unwrap();
    assert_eq!(restored, world);
    assert_eq!(
        restored.politics().countries[&EntityId(0)]
            .office_preferences
            .get("imperial_emperor"),
        Some(&EntityId(1))
    );

    let mut older: serde_json::Value = serde_json::from_str(&save).unwrap();
    older["politics"]["countries"]["0"]
        .as_object_mut()
        .unwrap()
        .remove("office_preferences");
    let restored_older =
        World::restore(&older.to_string(), "office-preferences", world.schema()).unwrap();
    assert!(
        restored_older.politics().countries[&EntityId(0)]
            .office_preferences
            .is_empty()
    );
}

#[test]
fn office_preferences_require_country_targets_and_nonempty_offices() {
    let mut world = world();
    for (office, holder) in [("imperial_emperor", EntityId(3)), ("", EntityId(1))] {
        let mut politics = Politics::default();
        politics.countries.insert(
            EntityId(0),
            CountryPolitics {
                office_preferences: [(office.into(), holder)].into(),
                ..Default::default()
            },
        );

        assert!(world.set_politics(politics).is_err());
        assert!(world.politics().is_empty());
    }
}

#[test]
fn invalid_relations_fail_atomically_including_cycles_and_multiple_overlords() {
    let mut world = world();
    for relations in [
        vec![relation(DiplomaticKind::Alliance, 0, 0)],
        vec![relation(DiplomaticKind::Alliance, 0, 3)],
        vec![
            relation(DiplomaticKind::Alliance, 0, 1),
            relation(DiplomaticKind::Alliance, 1, 0),
        ],
        vec![
            relation(DiplomaticKind::Subject, 0, 1),
            relation(DiplomaticKind::Subject, 2, 1),
        ],
        vec![
            relation(DiplomaticKind::Subject, 0, 1),
            relation(DiplomaticKind::Subject, 1, 0),
        ],
        vec![
            relation(DiplomaticKind::TradeLeague, 0, 1),
            relation(DiplomaticKind::TradeLeague, 2, 1),
        ],
    ] {
        assert!(
            world
                .set_politics(Politics {
                    relations,
                    ..Default::default()
                })
                .is_err()
        );
        assert!(world.politics().is_empty());
    }
}

#[test]
fn symmetric_pairs_have_stable_identity_and_directed_pairs_keep_direction() {
    let mut first = world();
    let mut second = world();
    let relations = vec![
        relation(DiplomaticKind::Guarantee, 1, 0),
        relation(DiplomaticKind::Alliance, 1, 0),
    ];
    first
        .set_politics(Politics {
            relations,
            ..Default::default()
        })
        .unwrap();
    second
        .set_politics(Politics {
            relations: vec![
                relation(DiplomaticKind::Alliance, 0, 1),
                relation(DiplomaticKind::Guarantee, 1, 0),
            ],
            ..Default::default()
        })
        .unwrap();
    assert_eq!(first, second);
    let guarantee = first
        .politics()
        .relations
        .iter()
        .find(|r| r.kind == DiplomaticKind::Guarantee)
        .unwrap();
    assert_eq!(
        (guarantee.first, guarantee.second),
        (EntityId(1), EntityId(0))
    );
}

#[test]
fn explicit_statuses_preserve_false_unknown_and_updates_through_snapshots() {
    let mut world = world();
    let mut politics = Politics::default();
    politics.countries.insert(
        EntityId(0),
        CountryPolitics {
            statuses: [
                ("created".into(), Some(true)),
                ("union".into(), Some(false)),
                ("origin".into(), None),
            ]
            .into_iter()
            .collect(),
            ..Default::default()
        },
    );
    world.set_politics(politics.clone()).unwrap();
    assert_eq!(world.politics().status(EntityId(0), "created"), Some(true));
    assert_eq!(world.politics().status(EntityId(0), "union"), Some(false));
    assert_eq!(world.politics().status(EntityId(0), "origin"), None);
    assert_eq!(world.politics().status(EntityId(0), "absent"), None);
    assert_eq!(world.politics().status(EntityId(1), "created"), None);
    let save = world.save("status").unwrap();
    assert_eq!(
        world,
        World::restore(&save, "status", world.schema()).unwrap()
    );
    politics
        .countries
        .get_mut(&EntityId(0))
        .unwrap()
        .statuses
        .insert("created".into(), Some(false));
    world.set_politics(politics.clone()).unwrap();
    assert_eq!(world.politics().status(EntityId(0), "created"), Some(false));
    let before = world.clone();
    politics
        .countries
        .get_mut(&EntityId(0))
        .unwrap()
        .statuses
        .insert(String::new(), Some(true));
    assert!(world.set_politics(politics).is_err());
    assert_eq!(world, before);
}

#[test]
fn political_status_conditions_distinguish_false_from_unknown_and_round_trip() {
    let mut world = world();
    let mut politics = Politics::default();
    politics.countries.insert(
        EntityId(0),
        CountryPolitics {
            statuses: [
                ("at_war".into(), Some(true)),
                ("in_union".into(), Some(false)),
                ("origin".into(), None),
            ]
            .into_iter()
            .collect(),
            ..Default::default()
        },
    );
    world.set_politics(politics).unwrap();

    let at_war = Condition::PoliticalStatus {
        target: Target::Current,
        name: "at_war".into(),
    };
    let in_union = Condition::PoliticalStatus {
        target: Target::Current,
        name: "in_union".into(),
    };
    let unknown = Condition::PoliticalStatus {
        target: Target::Current,
        name: "origin".into(),
    };
    assert!(world.validate_condition(&at_war).is_ok());
    assert!(at_war.evaluate(&world, EntityId(0)).unwrap());
    assert!(!in_union.evaluate(&world, EntityId(0)).unwrap());
    assert!(unknown.evaluate(&world, EntityId(0)).is_err());
    assert!(at_war.evaluate(&world, EntityId(3)).is_err());
    assert!(
        world
            .validate_condition(&Condition::PoliticalStatus {
                target: Target::Current,
                name: String::new(),
            })
            .is_err()
    );

    let save = world.save("political-status-condition").unwrap();
    let restored = World::restore(&save, "political-status-condition", world.schema()).unwrap();
    assert!(at_war.evaluate(&restored, EntityId(0)).unwrap());
}
