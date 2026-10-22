// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{EntityId, EntityKind, ProvinceConnectivity, Schema, Target, World};

fn fixture() -> (
    World,
    EntityId,
    EntityId,
    EntityId,
    EntityId,
    EntityId,
    EntityId,
) {
    let mut world = World::new(Schema::new(["base"]).unwrap());
    let first_owner = world.add_entity("AAA", EntityKind::Country);
    let other_owner = world.add_entity("BBB", EntityKind::Country);
    let first = world.add_entity("1", EntityKind::Province);
    let second = world.add_entity("2", EntityKind::Province);
    let middle = world.add_entity("3", EntityKind::Province);
    let last = world.add_entity("4", EntityKind::Province);
    world.set_owner(first, first_owner).unwrap();
    world.set_owner(second, first_owner).unwrap();
    world.set_owner(middle, other_owner).unwrap();
    world.set_owner(last, first_owner).unwrap();
    world
        .set_province_connectivity(ProvinceConnectivity::new(
            [(first, second), (second, middle), (middle, last)],
            [],
        ))
        .unwrap();
    (world, first_owner, other_owner, first, second, middle, last)
}

#[test]
fn owner_area_uses_live_ownership_and_symmetric_connectivity() {
    let (mut world, first_owner, other_owner, first, second, middle, last) = fixture();
    assert_eq!(
        world.province_owner_area(first).unwrap(),
        Some(vec![first, second])
    );

    world.set_owner(middle, first_owner).unwrap();
    assert_eq!(
        world.province_owner_area(first).unwrap(),
        Some(vec![first, second, middle, last])
    );

    world.set_owner(middle, other_owner).unwrap();
    assert_eq!(
        world.province_owner_area(first).unwrap(),
        Some(vec![first, second])
    );
}

#[test]
fn unknown_ownership_and_map_edges_remain_unknown_until_resolved() {
    let mut world = World::new(Schema::new(["base"]).unwrap());
    let owner = world.add_entity("AAA", EntityKind::Country);
    let province = world.add_entity("1", EntityKind::Province);
    let neighbor = world.add_entity("2", EntityKind::Province);
    world.set_owner(province, owner).unwrap();
    world
        .set_province_connectivity(ProvinceConnectivity::new([(province, neighbor)], []))
        .unwrap();
    assert_eq!(world.province_owner_area(province).unwrap(), None);

    world.mark_target_absent(neighbor, Target::Owner).unwrap();
    assert_eq!(
        world.province_owner_area(province).unwrap(),
        Some(vec![province])
    );
    world.set_owner(neighbor, owner).unwrap();
    assert_eq!(
        world.province_owner_area(province).unwrap(),
        Some(vec![province, neighbor])
    );

    world
        .set_province_connectivity(ProvinceConnectivity::new([], [province]))
        .unwrap();
    assert_eq!(world.province_owner_area(province).unwrap(), None);
}

#[test]
fn connectivity_survives_save_restore_and_older_saves_stay_unknown() {
    let (world, _, _, first, second, _, _) = fixture();
    let saved = world.save("owner-area-connectivity:1").unwrap();
    let restored = World::restore(&saved, "owner-area-connectivity:1", world.schema()).unwrap();
    assert_eq!(restored, world);
    assert_eq!(
        restored.province_owner_area(first).unwrap(),
        Some(vec![first, second])
    );

    let mut older = World::new(Schema::new(["base"]).unwrap());
    let owner = older.add_entity("AAA", EntityKind::Country);
    let province = older.add_entity("1", EntityKind::Province);
    older.set_owner(province, owner).unwrap();
    let saved = older.save("owner-area-connectivity:1").unwrap();
    let restored = World::restore(&saved, "owner-area-connectivity:1", older.schema()).unwrap();
    assert_eq!(restored.province_owner_area(province).unwrap(), None);
}

#[test]
fn connectivity_rejects_non_province_endpoints() {
    let mut world = World::new(Schema::new(["base"]).unwrap());
    let country = world.add_entity("AAA", EntityKind::Country);
    let province = world.add_entity("1", EntityKind::Province);
    assert!(
        world
            .set_province_connectivity(ProvinceConnectivity::new([(country, province)], []))
            .is_err()
    );
}
