// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{Economy, EntityKind, Schema, World};

fn world() -> World {
    let mut world = World::new(Schema::new([] as [&str; 0]).unwrap());
    world.add_entity("AAA", EntityKind::Country);
    world.add_entity("1", EntityKind::Province);
    world.add_entity("2", EntityKind::Province);
    world
}

fn economy() -> Economy {
    serde_json::from_value(data()).unwrap()
}

fn data() -> serde_json::Value {
    serde_json::json!({
        "buildings":{"depot":{"cost_milli":87125,"construction_months":9}},
        "trade_goods":{"grain":{"base_price_milli":2125}},
        "trade_nodes":{
            "river":{"location":1,"members":[1],"outgoing":["harbor"]},
            "harbor":{"location":2,"members":[2],"outgoing":[],"end":true}
        },
        "provinces":{"1":{"buildings":["depot"],"trade_good":"grain"}}
    })
}

#[test]
fn economic_state_roundtrips_and_older_demo_saves_remain_readable() {
    let mut world = world();
    let old = world.save("test").unwrap();
    assert!(!old.contains("economy"));
    assert_eq!(World::restore(&old, "test", world.schema()).unwrap(), world);
    world.set_economy(economy()).unwrap();
    assert_eq!(
        world.economy().province_node(augustus_core::EntityId(1)),
        Some("river")
    );
    world
        .schedule(augustus_core::EntityId(1), 5, vec![])
        .unwrap();
    let mut resumed = World::restore(&world.save("test").unwrap(), "test", world.schema()).unwrap();
    world.advance_to(10).unwrap();
    resumed.advance_to(10).unwrap();
    assert_eq!(world, resumed);
}

#[test]
fn invalid_economic_references_and_graphs_fail_atomically() {
    let mut world = world();
    world.set_economy(economy()).unwrap();
    let original = world.clone();
    let changes: [(&[&str], serde_json::Value); 8] = [
        (&["trade_nodes", "river", "location"], 0.into()),
        (
            &["trade_nodes", "river", "outgoing"],
            serde_json::json!(["missing"]),
        ),
        (
            &["trade_nodes", "river", "outgoing"],
            serde_json::json!(["river"]),
        ),
        (
            &["trade_nodes", "harbor", "members"],
            serde_json::json!([1, 2]),
        ),
        (
            &["provinces", "1", "buildings"],
            serde_json::json!(["missing"]),
        ),
        (&["provinces", "1", "trade_good"], "missing".into()),
        (&["buildings", "depot", "cost_milli"], (-1).into()),
        (&["buildings", "depot", "construction_months"], 0.into()),
    ];
    for (path, replacement) in changes {
        let mut invalid = data();
        let mut slot = &mut invalid;
        for &key in path {
            slot = &mut slot[key];
        }
        *slot = replacement;
        let economy: Economy = serde_json::from_value(invalid).unwrap();
        assert!(world.set_economy(economy).is_err());
        assert_eq!(world, original);
    }
    let mut cyclic = economy();
    let harbor = cyclic.trade_nodes.get_mut("harbor").unwrap();
    harbor.end = None;
    harbor.outgoing.insert("river".into());
    assert!(
        world
            .set_economy(cyclic)
            .unwrap_err()
            .to_string()
            .contains("cyclic")
    );
    let mut save: serde_json::Value = serde_json::from_str(&world.save("test").unwrap()).unwrap();
    save["economy"]["provinces"]["1"]["trade_good"] = "missing".into();
    assert!(World::restore(&save.to_string(), "test", world.schema()).is_err());
}
