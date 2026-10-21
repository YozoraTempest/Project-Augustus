// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{Action, Condition, EntityId, EntityKind, Schema, Target, World};

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

    let mut scheduled = World::new(schema.clone());
    let scheduled_province = scheduled.add_entity("scheduled", EntityKind::Province);
    scheduled
        .set_province_center_of_trade_level(scheduled_province, Some(0))
        .unwrap();
    scheduled
        .schedule(
            scheduled_province,
            1,
            vec![
                Action::SetProvinceCenterOfTradeLevel {
                    target: Target::Current,
                    level: 3,
                },
                Action::AddProvinceCenterOfTradeLevel {
                    target: Target::Current,
                    amount: -1,
                },
            ],
        )
        .unwrap();
    let saved = scheduled.save("scheduled-center-of-trade-content").unwrap();
    let mut resumed = World::restore(&saved, "scheduled-center-of-trade-content", &schema).unwrap();
    resumed.advance_to(1).unwrap();
    assert_eq!(
        resumed
            .province_center_of_trade_level(scheduled_province)
            .unwrap(),
        Some(2)
    );
}

#[test]
fn center_of_trade_adjustments_clamp_to_vanilla_bounds_and_preserve_unknowns() {
    let schema = Schema::new(Vec::<String>::new()).unwrap();
    let mut world = World::new(schema.clone());
    let province = world.add_entity("province", EntityKind::Province);
    let unknown = world.add_entity("unknown", EntityKind::Province);
    let country = world.add_entity("country", EntityKind::Country);
    world
        .set_province_center_of_trade_level(province, Some(0))
        .unwrap();
    assert!(
        world
            .set_province_center_of_trade_level(province, Some(4))
            .is_err()
    );

    world
        .adjust_province_center_of_trade_level(province, 1)
        .unwrap();
    assert_eq!(
        world.province_center_of_trade_level(province).unwrap(),
        Some(1)
    );
    world
        .adjust_province_center_of_trade_level(province, 3)
        .unwrap();
    assert_eq!(
        world.province_center_of_trade_level(province).unwrap(),
        Some(3)
    );
    world
        .adjust_province_center_of_trade_level(province, -1)
        .unwrap();
    assert_eq!(
        world.province_center_of_trade_level(province).unwrap(),
        Some(2)
    );
    world
        .adjust_province_center_of_trade_level(province, -8)
        .unwrap();
    assert_eq!(
        world.province_center_of_trade_level(province).unwrap(),
        Some(0)
    );
    assert!(
        world
            .adjust_province_center_of_trade_level(unknown, 1)
            .is_err()
    );
    assert!(
        world
            .adjust_province_center_of_trade_level(country, 1)
            .is_err()
    );

    let saved = world.save("center-of-trade-adjustment-content").unwrap();
    let restored = World::restore(&saved, "center-of-trade-adjustment-content", &schema).unwrap();
    assert_eq!(
        restored.province_center_of_trade_level(province).unwrap(),
        Some(0)
    );
    assert_eq!(
        restored.province_center_of_trade_level(unknown).unwrap(),
        None
    );
    let mut invalid_snapshot: serde_json::Value = serde_json::from_str(&saved).unwrap();
    invalid_snapshot["entities"][province.0]["province_center_of_trade_level"] = 4.into();
    let invalid_saved = serde_json::to_string(&invalid_snapshot).unwrap();
    assert!(
        World::restore(
            &invalid_saved,
            "center-of-trade-adjustment-content",
            &schema
        )
        .is_err()
    );
}
