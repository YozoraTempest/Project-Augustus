// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    CountryPolitics, DiplomaticKind, DiplomaticRelation, EntityId, EntityKind, PersonState,
    Politics, Schema, World,
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
