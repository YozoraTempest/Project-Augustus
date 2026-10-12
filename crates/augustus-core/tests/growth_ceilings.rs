// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::*;

fn world() -> World {
    let mut world = World::new(Schema::new(["unused"]).unwrap());
    world.add_entity("AAA", EntityKind::Country);
    world.add_entity("BBB", EntityKind::Country);
    world.add_entity("Port", EntityKind::Province);
    world.set_metrics(serde_json::from_str(r#"{
        "definitions":{"stock":{"kind":"Country","minimum":-100,"maximum":9223372036854775807,
            "baseline":0,"divisor":1,"modifier":null,"unresolved_properties":[],"growth_ceiling":true}},
        "entities":{"0":{"stock":999},"1":{"stock":40}},
        "ceilings":{"0":{"stock":999},"1":{"stock":50}}
    }"#).unwrap()).unwrap();
    world
}

fn slot() -> NumericSlot {
    NumericSlot::Metric("stock".into())
}

#[test]
fn reduced_ceiling_preserves_excess_across_save_and_scheduled_spending() {
    let mut world = world();
    world
        .set_metric_ceiling(EntityId(0), "stock", Some(500))
        .unwrap();
    assert_eq!(world.numeric_value(EntityId(0), &slot()).unwrap(), 999);
    world
        .schedule(
            EntityId(0),
            1,
            vec![Action::UpdateNumeric {
                target: Target::Current,
                slot: slot(),
                operation: NumericOperation::Add,
                value: -1,
            }],
        )
        .unwrap();
    let saved = world.save("fixture").unwrap();
    let mut restored = World::restore(&saved, "fixture", world.schema()).unwrap();
    restored.advance_to(1).unwrap();
    for (amount, expected) in [
        (1, 998),
        (100000, 998),
        (-600, 398),
        (200, 500),
        (-100000, -100),
        (100000, 500),
    ] {
        restored
            .update_numeric(EntityId(0), &slot(), NumericOperation::Add, amount)
            .unwrap();
        assert_eq!(
            restored.numeric_value(EntityId(0), &slot()).unwrap(),
            expected
        );
    }
    assert_eq!(restored.numeric_value(EntityId(1), &slot()).unwrap(), 40);
    restored
        .update_numeric(EntityId(1), &slot(), NumericOperation::Add, 1000)
        .unwrap();
    assert_eq!(restored.numeric_value(EntityId(1), &slot()).unwrap(), 50);
}

#[test]
fn unknown_ceiling_allows_reduction_but_rejects_growth_without_writing() {
    let mut world = world();
    world
        .set_metric_ceiling(EntityId(0), "stock", None)
        .unwrap();
    world
        .update_numeric(EntityId(0), &slot(), NumericOperation::Add, -10000)
        .unwrap();
    world
        .update_numeric(EntityId(0), &slot(), NumericOperation::Add, 0)
        .unwrap();
    let before = world.save("fixture").unwrap();
    assert!(
        world
            .update_numeric(EntityId(0), &slot(), NumericOperation::Add, 1)
            .is_err()
    );
    assert_eq!(before, world.save("fixture").unwrap());
    world.set_metric_value(EntityId(0), "stock", None).unwrap();
    assert!(
        world
            .update_numeric(EntityId(0), &slot(), NumericOperation::Set, 0)
            .is_err()
    );
    assert!(world.numeric_value(EntityId(0), &slot()).is_err());
}

#[test]
fn invalid_ceiling_scope_and_saved_bounds_are_rejected_atomically() {
    let mut world = world();
    let before = world.save("fixture").unwrap();
    for (entity, value) in [(EntityId(0), Some(-101)), (EntityId(2), Some(500))] {
        assert!(world.set_metric_ceiling(entity, "stock", value).is_err());
        assert_eq!(before, world.save("fixture").unwrap());
    }
    let mut bad: serde_json::Value = serde_json::from_str(&before).unwrap();
    bad["metrics"]["ceilings"]["0"]["stock"] = (-101).into();
    assert!(World::restore(&bad.to_string(), "fixture", world.schema()).is_err());
    let mut replacement = world.metrics().clone();
    replacement
        .definitions
        .get_mut("stock")
        .unwrap()
        .growth_ceiling = false;
    assert!(world.set_metrics(replacement).is_err());
    assert_eq!(before, world.save("fixture").unwrap());
}
