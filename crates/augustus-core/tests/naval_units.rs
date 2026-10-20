// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    Action, EntityKind, EntityRef, FleetId, NavalUnitId, Schema, ScopeContext, ScopeSelector, World,
};

fn fixture() -> (World, augustus_core::EntityId, augustus_core::EntityId) {
    let mut world = World::new(Schema::new(std::iter::empty::<String>()).unwrap());
    let country = world.add_entity("SWE", EntityKind::Country);
    let port = world.add_entity("1", EntityKind::Province);
    world.set_province_has_port(port, true).unwrap();
    (world, country, port)
}

#[test]
fn repeated_script_actions_create_individual_ships_and_round_trip() {
    let (mut world, country, port) = fixture();
    let action = [Action::CreateNavalUnit {
        owner: EntityRef::Selected(ScopeSelector::Root),
        unit_type: "light_ship".into(),
    }];
    let context = ScopeContext {
        current: port,
        root: country,
    };
    for _ in 0..6 {
        world.execute_in_context(context, &action).unwrap();
    }

    assert_eq!(
        world
            .naval_units()
            .iter()
            .map(|unit| (unit.id, unit.owner, unit.location, unit.unit_type.as_str()))
            .collect::<Vec<_>>(),
        (0..6)
            .map(|id| (NavalUnitId(id), country, port, "light_ship"))
            .collect::<Vec<_>>()
    );
    assert_eq!(world.fleets().len(), 6);
    assert!(world.fleets().iter().all(|fleet| fleet.ships.len() == 1));

    let saved = world.save("naval-units:1").unwrap();
    let restored = World::restore(&saved, "naval-units:1", world.schema()).unwrap();
    assert_eq!(restored, world);
}

#[test]
fn fleet_keeps_its_ship_group_across_save_restore() {
    let (mut world, country, port) = fixture();
    let ship_types = vec!["galley".into(), "carrack".into()];
    let id = world
        .create_fleet(country, port, Some("Test Fleet".into()), &ship_types)
        .unwrap();

    assert_eq!(id, FleetId(0));
    assert_eq!(world.naval_units().len(), 2);
    assert_eq!(world.fleets().len(), 1);
    assert_eq!(world.fleets()[0].name.as_deref(), Some("Test Fleet"));
    assert_eq!(
        world.fleets()[0].ships,
        vec![NavalUnitId(0), NavalUnitId(1)]
    );
    assert_eq!(
        world.fleets()[0]
            .ships
            .iter()
            .map(|id| world.naval_units()[id.0 as usize].unit_type.as_str())
            .collect::<Vec<_>>(),
        ["galley", "carrack"]
    );

    let saved = world.save("fleet:1").unwrap();
    let restored = World::restore(&saved, "fleet:1", world.schema()).unwrap();
    assert_eq!(restored, world);
}

#[test]
fn ship_creation_requires_a_known_port_province() {
    let (mut world, country, _) = fixture();
    let inland = world.add_entity("2", EntityKind::Province);
    world.set_province_has_port(inland, false).unwrap();
    assert!(
        world
            .create_naval_unit(country, inland, "light_ship")
            .is_err()
    );
}

#[test]
fn empty_navies_keep_the_previous_snapshot_shape_and_invalid_references_are_rejected() {
    let (world, _, _) = fixture();
    let saved = world.save("naval-units:1").unwrap();
    let mut legacy: serde_json::Value = serde_json::from_str(&saved).unwrap();
    legacy.as_object_mut().unwrap().remove("naval_units");
    let restored = World::restore(&legacy.to_string(), "naval-units:1", world.schema()).unwrap();
    assert_eq!(restored, world);

    let (mut world, country, port) = fixture();
    world
        .create_naval_unit(country, port, "light_ship")
        .unwrap();
    assert_eq!(world.fleets().len(), 1);
    let saved = world.save("naval-units:1").unwrap();
    let mut legacy: serde_json::Value = serde_json::from_str(&saved).unwrap();
    legacy.as_object_mut().unwrap().remove("fleets");
    let restored = World::restore(&legacy.to_string(), "naval-units:1", world.schema()).unwrap();
    assert_eq!(restored.naval_units(), world.naval_units());
    assert_eq!(restored.fleets().len(), 1);
    assert_eq!(restored.fleets()[0].ships, vec![NavalUnitId(0)]);

    let saved = world.save("naval-units:1").unwrap();
    let mut corrupt: serde_json::Value = serde_json::from_str(&saved).unwrap();
    corrupt["naval_units"]["units"][0]["owner"] = serde_json::json!(u64::MAX);
    assert!(World::restore(&corrupt.to_string(), "naval-units:1", world.schema()).is_err());
}
