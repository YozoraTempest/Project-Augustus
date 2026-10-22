// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::{BTreeMap, BTreeSet};

use augustus_core::{EntityId, EntityKind, MapKnowledge, ProvinceDiscovery, Schema, World};

fn world() -> (World, EntityId, EntityId, EntityId, EntityId) {
    let mut world = World::new(Schema::new([] as [&str; 0]).unwrap());
    let discovered = world.add_entity("AAA", EntityKind::Country);
    let unresolved = world.add_entity("BBB", EntityKind::Country);
    let absent = world.add_entity("CCC", EntityKind::Country);
    let province = world.add_entity("1", EntityKind::Province);
    world
        .set_map_knowledge(MapKnowledge {
            provinces: BTreeMap::from([(
                province,
                ProvinceDiscovery {
                    discovered_by: BTreeSet::from([discovered]),
                    unresolved_countries: BTreeSet::from([unresolved]),
                },
            )]),
        })
        .unwrap();
    (world, discovered, unresolved, absent, province)
}

#[test]
fn discovery_queries_distinguish_known_true_false_and_unknown() {
    let (world, discovered, unresolved, absent, province) = world();
    assert_eq!(
        world.province_discovered_by(province, discovered).unwrap(),
        Some(true)
    );
    assert_eq!(
        world.province_discovered_by(province, unresolved).unwrap(),
        None
    );
    assert_eq!(
        world.province_discovered_by(province, absent).unwrap(),
        Some(false)
    );
    assert_eq!(
        world
            .province_discovered_by(EntityId(99), discovered)
            .unwrap_err()
            .to_string(),
        "invalid simulation state: unknown entity 99"
    );
}

#[test]
fn discovery_state_survives_snapshot_restore() {
    let (world, discovered, unresolved, absent, province) = world();
    let saved = world.save("map-knowledge").unwrap();
    let restored = World::restore(&saved, "map-knowledge", world.schema()).unwrap();
    assert_eq!(world, restored);
    assert_eq!(saved, restored.save("map-knowledge").unwrap());
    assert_eq!(
        restored
            .province_discovered_by(province, discovered)
            .unwrap(),
        Some(true)
    );
    assert_eq!(
        restored
            .province_discovered_by(province, unresolved)
            .unwrap(),
        None
    );
    assert_eq!(
        restored.province_discovered_by(province, absent).unwrap(),
        Some(false)
    );
}

#[test]
fn invalid_or_contradictory_discovery_state_is_rejected_atomically() {
    let (mut world, discovered, unresolved, _, province) = world();
    let before = world.save("map-knowledge").unwrap();
    let conflicting = MapKnowledge {
        provinces: BTreeMap::from([(
            province,
            ProvinceDiscovery {
                discovered_by: BTreeSet::from([discovered]),
                unresolved_countries: BTreeSet::from([discovered]),
            },
        )]),
    };
    assert!(world.set_map_knowledge(conflicting).is_err());
    assert_eq!(world.save("map-knowledge").unwrap(), before);

    let wrong_kind = MapKnowledge {
        provinces: BTreeMap::from([(
            province,
            ProvinceDiscovery {
                discovered_by: BTreeSet::from([province]),
                unresolved_countries: BTreeSet::new(),
            },
        )]),
    };
    assert!(world.set_map_knowledge(wrong_kind).is_err());
    assert_eq!(world.save("map-knowledge").unwrap(), before);

    let mut invalid: serde_json::Value = serde_json::from_str(&before).unwrap();
    invalid["map_knowledge"] = serde_json::json!({
        "provinces": {
            "3": {
                "discovered_by": [unresolved.0],
                "unresolved_countries": [unresolved.0]
            }
        }
    });
    assert!(World::restore(&invalid.to_string(), "map-knowledge", world.schema()).is_err());
}
