// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    Action, Administration, AdministrativeCategory, AreaStatus, Condition, CoreType, EntityId,
    EntityKind, EntityRef, ProvinceAdministration, Schema, ScopeContext, ScopeSelector, Target,
    World,
};

fn world() -> World {
    let mut world = World::new(Schema::new(["counter"]).unwrap());
    let a = world.add_entity("AAA", EntityKind::Country);
    let b = world.add_entity("BBB", EntityKind::Country);
    let p = world.add_entity("1", EntityKind::Province);
    world.set_owner(p, a).unwrap();
    world.set_administration(Administration {
        areas: ["home".into()].into(),
        country_areas: [(a, [("home".into(), AreaStatus::State)].into())].into(),
        provinces: [(p, ProvinceAdministration {
            area: Some("home".into()), cores_known: true,
            cores: [(a, CoreType::Full), (b, CoreType::Territorial)].into(),
            raw_autonomy_micros: Some(100_000), ..Default::default()
        })].into(),
        rules: serde_json::from_str(r#"{"state_full_minimum_micros":0,"state_territorial_minimum_micros":500000,"state_noncore_minimum_micros":750000}"#).unwrap(),
        ..Default::default()
    }).unwrap();
    world
}

fn named(kind: EntityKind, name: &str) -> EntityRef {
    EntityRef::Selected(ScopeSelector::Named {
        kind,
        name: name.into(),
    })
}

fn core(country: &str, value: Option<CoreType>) -> Action {
    Action::SetProvinceCore {
        province: named(EntityKind::Province, "1"),
        country: named(EntityKind::Country, country),
        core: value,
    }
}

#[test]
fn core_changes_update_administration_without_changing_owner_or_raw_autonomy() {
    let mut world = world();
    let p = EntityId(2);
    for (core_type, category, effective) in [
        (
            Some(CoreType::Territorial),
            AdministrativeCategory::StateTerritorialCore,
            500_000,
        ),
        (
            Some(CoreType::Full),
            AdministrativeCategory::StateFullCore,
            100_000,
        ),
        (None, AdministrativeCategory::StateNonCore, 750_000),
    ] {
        world.set_province_core(p, EntityId(0), core_type).unwrap();
        let evaluation = world.administration().evaluate(&world, p).unwrap();
        assert_eq!(evaluation.category(), Some(category));
        assert_eq!(evaluation.effective_autonomy_micros, Some(effective));
        assert_eq!(evaluation.raw_autonomy_micros, Some(100_000));
        assert_eq!(world.entity(p).unwrap().owner, Some(EntityId(0)));
        assert_eq!(
            world.province_core(p, EntityId(1)).unwrap(),
            Some(CoreType::Territorial)
        );
    }
    let save = world.save("cores").unwrap();
    world.set_province_core(p, EntityId(0), None).unwrap();
    assert_eq!(world.save("cores").unwrap(), save);
}

#[test]
fn failed_plan_keeps_prior_writes_stops_saving_and_leaves_clones_independent() {
    let mut world = world();
    let original = world.clone();
    let mut independent = original.clone();
    independent
        .execute(EntityId(0), &[core("AAA", None)])
        .unwrap();
    assert_ne!(independent, original);
    assert_eq!(
        original.province_core(EntityId(2), EntityId(0)).unwrap(),
        Some(CoreType::Full)
    );
    let plan = [
        core("AAA", None),
        Action::SetOwner {
            target: Target::Current,
            owner: named(EntityKind::Country, "BBB"),
        },
        Action::SetProvinceCore {
            province: named(EntityKind::Country, "AAA"),
            country: named(EntityKind::Country, "BBB"),
            core: Some(CoreType::Full),
        },
    ];
    assert!(world.execute(EntityId(2), &plan).is_err());
    assert_eq!(world.province_core(EntityId(2), EntityId(0)).unwrap(), None);
    assert!(world.owned_provinces(EntityId(0)).unwrap().is_empty());
    assert_eq!(world.owned_provinces(EntityId(1)).unwrap(), [EntityId(2)]);
    assert!(world.save("failed").is_err());
    assert!(
        world
            .set_province_core(EntityId(2), EntityId(0), Some(CoreType::Full))
            .is_err()
    );
    assert_eq!(
        original.owned_provinces(EntityId(0)).unwrap(),
        [EntityId(2)]
    );
}

#[test]
fn queries_distinguish_membership_type_unknown_history_and_invalid_endpoint() {
    let mut world = world();
    let any = Condition::ProvinceCore {
        province: named(EntityKind::Province, "1"),
        country: named(EntityKind::Country, "BBB"),
        core: None,
    };
    let territorial = Condition::ProvinceCore {
        province: named(EntityKind::Province, "1"),
        country: named(EntityKind::Country, "BBB"),
        core: Some(CoreType::Territorial),
    };
    assert!(any.evaluate(&world, EntityId(0)).unwrap());
    assert!(territorial.evaluate(&world, EntityId(0)).unwrap());
    world
        .set_province_core(EntityId(2), EntityId(1), Some(CoreType::Unspecified))
        .unwrap();
    assert!(any.evaluate(&world, EntityId(0)).unwrap());
    assert!(territorial.evaluate(&world, EntityId(0)).is_err());
    assert!(world.province_core(EntityId(0), EntityId(1)).is_err());
    let before = world.save("unknown").unwrap();
    assert!(
        world
            .set_province_core(EntityId(2), EntityId(2), None)
            .is_err()
    );
    assert_eq!(world.save("unknown").unwrap(), before);
    let mut administration = world.administration().clone();
    let state = administration.provinces.get_mut(&EntityId(2)).unwrap();
    state.cores.clear();
    state.cores_known = false;
    world.set_administration(administration).unwrap();
    assert!(any.evaluate(&world, EntityId(0)).is_err());
    assert!(
        world
            .set_province_core(EntityId(2), EntityId(1), Some(CoreType::Full))
            .is_err()
    );
}

#[test]
fn delayed_effects_resolve_live_global_references_and_preserve_root_after_restore() {
    let mut world = world();
    world.bind_global_scope("holder", EntityId(1)).unwrap();
    let country = EntityRef::Selected(ScopeSelector::Global("holder".into()));
    let root = EntityRef::Selected(ScopeSelector::Root);
    world
        .execute_in_context(
            ScopeContext {
                current: EntityId(0),
                root: EntityId(2),
            },
            &[Action::Schedule {
                delay: 2,
                plan: vec![Action::SetProvinceCore {
                    province: root,
                    country,
                    core: None,
                }],
            }],
        )
        .unwrap();
    let mut restored =
        World::restore(&world.save("delayed").unwrap(), "delayed", world.schema()).unwrap();
    world.bind_global_scope("holder", EntityId(0)).unwrap();
    restored.bind_global_scope("holder", EntityId(0)).unwrap();
    world.advance_to(2).unwrap();
    restored.advance_to(2).unwrap();
    assert_eq!(world, restored);
    assert_eq!(world.province_core(EntityId(2), EntityId(0)).unwrap(), None);
    assert_eq!(
        world.province_core(EntityId(2), EntityId(1)).unwrap(),
        Some(CoreType::Territorial)
    );
}
