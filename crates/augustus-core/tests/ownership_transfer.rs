// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    CountryState, EntityId, EntityKind, InitialWorld, ProvinceClass, ProvinceState, Schema, Target,
    World,
};

fn world_fixture() -> World {
    let mut world = World::new(Schema::new([] as [&str; 0]).unwrap());
    let aaa = world.add_entity("AAA", EntityKind::Country);
    let _bbb = world.add_entity("BBB", EntityKind::Country);
    let owned = world.add_entity("1", EntityKind::Province);
    let unknown = world.add_entity("2", EntityKind::Province);
    world.set_owner(owned, aaa).unwrap();
    assert!(world.known_absent_targets().get(&unknown).is_none());
    world
}

#[test]
fn clearing_owner_records_known_absence_and_reassignment_restores_membership() {
    let mut world = world_fixture();
    let aaa = EntityId(0);
    let owned = EntityId(2);
    let unknown = EntityId(3);

    world.clear_owner(unknown).unwrap();
    assert!(world.known_absent_targets()[&unknown].contains(&Target::Owner));

    world.clear_owner(owned).unwrap();
    assert!(world.owned_provinces(aaa).unwrap().is_empty());
    assert!(world.known_absent_targets()[&owned].contains(&Target::Owner));
    assert!(!world.has_owned_province(aaa).unwrap());

    assert!(world.owned_provinces(aaa).unwrap().is_empty());

    world.set_owner(owned, aaa).unwrap();
    assert_eq!(world.owned_provinces(aaa).unwrap(), &[owned]);
    assert!(!world.known_absent_targets().contains_key(&owned));
}

#[test]
fn initial_world_owner_projection_updates_atomically() {
    let mut initial = InitialWorld::new(
        vec![CountryState {
            id: "AAA".into(),
            name: "Alpha".into(),
            color: [1, 2, 3],
            declared_capital: None,
            primary_culture: None,
            religion: None,
            government: None,
            technology_group: None,
            unit_type: None,
        }],
        vec![ProvinceState {
            id: 1,
            name: "One".into(),
            color: [4, 5, 6],
            class: ProvinceClass::Land,
            has_port: None,
            owner: Some("AAA".into()),
            controller: None,
            cores: Vec::new(),
            culture: None,
            religion: None,
            trade_good: None,
            tax_milli: None,
            production_milli: None,
            manpower_milli: None,
            is_city: None,
            in_empire: None,
        }],
    )
    .unwrap();
    initial.set_province_owner(1, None).unwrap();
    assert_eq!(initial.province(1).unwrap().owner, None);
    assert!(
        initial.world().known_absent_targets()[&initial.province_entity(1).unwrap()]
            .contains(&Target::Owner)
    );
    initial.set_province_owner(1, Some("AAA")).unwrap();
    assert_eq!(initial.province(1).unwrap().owner.as_deref(), Some("AAA"));

    let before_invalid = initial.world().save("ownership-transfer").unwrap();
    assert!(initial.set_province_owner(1, Some("ZZZ")).is_err());
    assert_eq!(initial.province(1).unwrap().owner.as_deref(), Some("AAA"));
    assert_eq!(
        initial.world().save("ownership-transfer").unwrap(),
        before_invalid
    );
}

#[test]
fn clearing_a_current_capital_requires_a_separate_capital_move() {
    let mut world = world_fixture();
    let country = EntityId(0);
    let capital = EntityId(2);
    world.set_capital(country, capital).unwrap();
    assert!(world.clear_owner(capital).is_err());
    assert_eq!(world.entity(capital).unwrap().owner, Some(country));
}
