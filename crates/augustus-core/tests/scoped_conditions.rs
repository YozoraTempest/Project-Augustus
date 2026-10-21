// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    Administration, Condition, EntityId, EntityKind, FieldId, GeographicLevel,
    ProvinceAdministration, Schema, ScopeSelector, Target, World,
};
use std::collections::{BTreeMap, BTreeSet};

fn scoped(target: Target, kind: EntityKind, condition: Condition) -> Condition {
    Condition::With {
        target,
        kind,
        condition: Box::new(condition),
    }
}

fn member(level: GeographicLevel, name: &str) -> Condition {
    Condition::GeographicMember {
        target: Target::Current,
        level,
        name: name.into(),
    }
}

fn world() -> World {
    let mut world = World::new(Schema::new(["value"]).unwrap());
    let country = world.add_entity("AAA", EntityKind::Country);
    world.add_entity("BBB", EntityKind::Country);
    let home = world.add_entity("1", EntityKind::Province);
    let other = world.add_entity("2", EntityKind::Province);
    world.add_entity("Sea", EntityKind::Province);
    world.set_owner(home, country).unwrap();
    world.set_owner(other, country).unwrap();
    world.set_capital(country, home).unwrap();
    world
        .set_administration(Administration {
            areas: BTreeSet::from(["home".into(), "other".into()]),
            regions: BTreeMap::from([(
                "union".into(),
                BTreeSet::from(["home".into(), "other".into()]),
            )]),
            superregions: BTreeMap::from([("outer".into(), BTreeSet::from(["union".into()]))]),
            continents: BTreeMap::from([
                ("land".into(), BTreeSet::from([home, other])),
                ("empty".into(), BTreeSet::new()),
            ]),
            provinces: BTreeMap::from([
                (
                    home,
                    ProvinceAdministration {
                        area: Some("home".into()),
                        ..Default::default()
                    },
                ),
                (
                    other,
                    ProvinceAdministration {
                        area: Some("other".into()),
                        ..Default::default()
                    },
                ),
            ]),
            ..Default::default()
        })
        .unwrap();
    world
}

#[test]
fn nested_conditions_read_live_relationships_and_geography_without_writes() {
    let mut world = world();
    let country = EntityId(0);
    let capital = scoped(
        Target::Capital,
        EntityKind::Province,
        Condition::All(vec![
            member(GeographicLevel::Area, "home"),
            member(GeographicLevel::Region, "union"),
            member(GeographicLevel::Superregion, "outer"),
            member(GeographicLevel::Continent, "land"),
            scoped(
                Target::Owner,
                EntityKind::Country,
                Condition::EntityNamed {
                    target: Target::Current,
                    name: "AAA".into(),
                },
            ),
        ]),
    );
    let before = world.save("content").unwrap();
    assert!(capital.evaluate(&world, country).unwrap());
    assert!(
        !member(GeographicLevel::Continent, "empty")
            .evaluate(&world, EntityId(2))
            .unwrap()
    );
    assert_eq!(world.save("content").unwrap(), before);
    world.set_capital(country, EntityId(3)).unwrap();
    assert!(!capital.evaluate(&world, country).unwrap());
    let changed = scoped(
        Target::Capital,
        EntityKind::Province,
        member(GeographicLevel::Area, "other"),
    );
    assert!(changed.evaluate(&world, country).unwrap());
    let saved = world.save("content").unwrap();
    let restored = World::restore(&saved, "content", world.schema()).unwrap();
    assert!(changed.evaluate(&restored, country).unwrap());
    let owner = scoped(
        Target::Owner,
        EntityKind::Country,
        Condition::EntityNamed {
            target: Target::Current,
            name: "BBB".into(),
        },
    );
    assert!(!owner.evaluate(&world, EntityId(2)).unwrap());
    world.set_owner(EntityId(2), EntityId(1)).unwrap();
    assert!(owner.evaluate(&world, EntityId(2)).unwrap());
}

#[test]
fn province_terrain_conditions_compare_known_values_and_preserve_unknowns() {
    let mut world = world();
    let steppe = EntityId(2);
    let grasslands = EntityId(3);
    let unknown = EntityId(4);
    world
        .set_province_terrain(steppe, Some("steppe".into()))
        .unwrap();
    world
        .set_province_terrain(grasslands, Some("grasslands".into()))
        .unwrap();

    let condition = scoped(
        Target::Current,
        EntityKind::Province,
        Condition::ProvinceTerrain {
            target: Target::Current,
            terrain: "steppe".into(),
        },
    );
    assert!(condition.evaluate(&world, steppe).unwrap());
    assert!(!condition.evaluate(&world, grasslands).unwrap());
    assert!(condition.evaluate(&world, unknown).is_err());
    assert!(condition.evaluate(&world, EntityId(0)).is_err());

    let saved = world.save("terrain-content").unwrap();
    let restored = World::restore(&saved, "terrain-content", world.schema()).unwrap();
    assert!(condition.evaluate(&restored, steppe).unwrap());
    assert!(!condition.evaluate(&restored, grasslands).unwrap());
}

#[test]
fn known_absence_is_false_unknown_is_an_error_and_binding_clears_absence() {
    let mut world = world();
    let owner = scoped(Target::Owner, EntityKind::Country, Condition::Always(true));
    let negated = Condition::Not(Box::new(owner.clone()));
    assert!(owner.evaluate(&world, EntityId(4)).is_err());
    assert!(negated.evaluate(&world, EntityId(4)).is_err());
    world
        .mark_target_absent(EntityId(4), Target::Owner)
        .unwrap();
    assert!(!owner.evaluate(&world, EntityId(4)).unwrap());
    assert!(negated.evaluate(&world, EntityId(4)).unwrap());
    assert!(
        !scoped(Target::Owner, EntityKind::Country, Condition::All(vec![]))
            .evaluate(&world, EntityId(4))
            .unwrap()
    );
    let saved = world.save("content").unwrap();
    let restored = World::restore(&saved, "content", world.schema()).unwrap();
    assert!(!owner.evaluate(&restored, EntityId(4)).unwrap());
    assert!(
        world
            .mark_target_absent(EntityId(2), Target::Owner)
            .is_err()
    );
    assert!(
        world
            .mark_target_absent(EntityId(0), Target::Owner)
            .is_err()
    );
    assert!(
        world
            .mark_target_absent(EntityId(4), Target::Current)
            .is_err()
    );
    world
        .mark_target_absent(EntityId(1), Target::Capital)
        .unwrap();
    assert!(
        !scoped(
            Target::Capital,
            EntityKind::Province,
            Condition::Always(true)
        )
        .evaluate(&world, EntityId(1))
        .unwrap()
    );
    world.set_owner(EntityId(4), EntityId(1)).unwrap();
    world.set_capital(EntityId(1), EntityId(4)).unwrap();
    assert!(world.known_absent_targets().is_empty());
    assert!(owner.evaluate(&world, EntityId(4)).unwrap());
    assert!(
        !world
            .save("content")
            .unwrap()
            .contains("known_absent_targets")
    );
}

#[test]
fn named_condition_scopes_follow_binding_changes_and_enforce_types() {
    let mut world = world();
    let condition = Condition::InScope {
        selector: ScopeSelector::Global("place".into()),
        kind: EntityKind::Province,
        condition: Box::new(member(GeographicLevel::Area, "home")),
    };
    assert!(!condition.evaluate(&world, EntityId(0)).unwrap());
    world.bind_global_scope("place", EntityId(2)).unwrap();
    assert!(condition.evaluate(&world, EntityId(0)).unwrap());
    world.bind_global_scope("place", EntityId(3)).unwrap();
    assert!(!condition.evaluate(&world, EntityId(0)).unwrap());
    world.bind_global_scope("place", EntityId(0)).unwrap();
    assert!(condition.evaluate(&world, EntityId(0)).is_err());
    world.clear_global_scope("place").unwrap();
    assert!(!condition.evaluate(&world, EntityId(0)).unwrap());
}

#[test]
fn unknown_geography_and_corrupt_absence_or_continents_are_rejected() {
    let world = world();
    assert!(
        member(GeographicLevel::Region, "absent")
            .evaluate(&world, EntityId(2))
            .is_err()
    );
    assert!(
        Condition::Not(Box::new(member(GeographicLevel::Area, "home")))
            .evaluate(&world, EntityId(4))
            .is_err()
    );
    assert!(
        member(GeographicLevel::Continent, "land")
            .evaluate(&world, EntityId(0))
            .is_err()
    );
    let saved: serde_json::Value = serde_json::from_str(&world.save("content").unwrap()).unwrap();
    for absence in [
        serde_json::json!({"0":["Owner"]}),
        serde_json::json!({"2":["Owner"]}),
        serde_json::json!({"4":[]}),
        serde_json::json!({"90":["Owner"]}),
    ] {
        let mut invalid = saved.clone();
        invalid["known_absent_targets"] = absence;
        assert!(World::restore(&invalid.to_string(), "content", world.schema()).is_err());
    }
    for continents in [
        serde_json::json!({"one":[2],"two":[2]}),
        serde_json::json!({"one":[0]}),
        serde_json::json!({"one":[90]}),
    ] {
        let mut invalid = saved.clone();
        invalid["administration"]["continents"] = continents;
        assert!(World::restore(&invalid.to_string(), "content", world.schema()).is_err());
    }
    let condition = scoped(
        Target::Capital,
        EntityKind::Province,
        Condition::AtLeast {
            target: Target::Current,
            field: FieldId(99),
            value: 1,
        },
    );
    let mut invalid_plan = world.clone();
    assert!(
        invalid_plan
            .execute(
                EntityId(0),
                &[augustus_core::Action::If {
                    condition,
                    then: vec![],
                    otherwise: vec![]
                }]
            )
            .is_err()
    );
}
