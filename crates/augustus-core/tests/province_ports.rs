// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{Condition, EntityId, EntityKind, Schema, World};

#[test]
fn port_presence_is_explicit_queryable_and_saved_with_the_province() {
    let schema = Schema::new(Vec::<String>::new()).unwrap();
    let mut world = World::new(schema.clone());
    let coast = world.add_entity("coast", EntityKind::Province);
    let inland = world.add_entity("inland", EntityKind::Province);
    let country = world.add_entity("country", EntityKind::Country);
    world.set_province_has_port(coast, true).unwrap();
    world.set_province_has_port(inland, false).unwrap();

    assert!(Condition::ProvinceHasPort.evaluate(&world, coast).unwrap());
    assert!(!Condition::ProvinceHasPort.evaluate(&world, inland).unwrap());
    assert!(
        Condition::ProvinceHasPort
            .evaluate(&world, country)
            .is_err()
    );
    let unknown = world.add_entity("unknown", EntityKind::Province);
    assert!(
        Condition::ProvinceHasPort
            .evaluate(&world, unknown)
            .is_err()
    );

    let saved = world.save("port-test-content").unwrap();
    let restored = World::restore(&saved, "port-test-content", &schema).unwrap();
    assert!(
        Condition::ProvinceHasPort
            .evaluate(&restored, EntityId(coast.0))
            .unwrap()
    );
    assert!(
        !Condition::ProvinceHasPort
            .evaluate(&restored, EntityId(inland.0))
            .unwrap()
    );
}

#[test]
fn center_of_trade_thresholds_compare_known_levels_and_preserve_unknowns() {
    let schema = Schema::new(Vec::<String>::new()).unwrap();
    let mut world = World::new(schema.clone());
    let none = world.add_entity("none", EntityKind::Province);
    let level_one = world.add_entity("level-one", EntityKind::Province);
    let level_two = world.add_entity("level-two", EntityKind::Province);
    let level_three = world.add_entity("level-three", EntityKind::Province);
    let unknown = world.add_entity("unknown", EntityKind::Province);
    let country = world.add_entity("country", EntityKind::Country);
    world
        .set_province_center_of_trade_level(none, Some(0))
        .unwrap();
    world
        .set_province_center_of_trade_level(level_one, Some(1))
        .unwrap();
    world
        .set_province_center_of_trade_level(level_two, Some(2))
        .unwrap();
    world
        .set_province_center_of_trade_level(level_three, Some(3))
        .unwrap();

    let at_least = |level| Condition::ProvinceCenterOfTradeAtLeast {
        target: augustus_core::Target::Current,
        level,
    };
    assert!(!at_least(1).evaluate(&world, none).unwrap());
    assert!(at_least(1).evaluate(&world, level_one).unwrap());
    assert!(at_least(1).evaluate(&world, level_two).unwrap());
    assert!(!at_least(2).evaluate(&world, level_one).unwrap());
    assert!(at_least(2).evaluate(&world, level_two).unwrap());
    assert!(!at_least(3).evaluate(&world, level_two).unwrap());
    assert!(at_least(3).evaluate(&world, level_three).unwrap());
    assert!(at_least(1).evaluate(&world, unknown).is_err());
    assert!(at_least(1).evaluate(&world, country).is_err());
    assert!(at_least(0).evaluate(&world, level_one).is_err());

    let saved = world.save("center-of-trade-content").unwrap();
    let restored = World::restore(&saved, "center-of-trade-content", &schema).unwrap();
    assert_eq!(
        restored.province_center_of_trade_level(level_two).unwrap(),
        Some(2)
    );
    assert_eq!(
        restored.province_center_of_trade_level(unknown).unwrap(),
        None
    );
}
