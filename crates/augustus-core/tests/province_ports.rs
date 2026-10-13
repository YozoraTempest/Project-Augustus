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
