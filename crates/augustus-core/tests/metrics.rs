// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::*;

fn world() -> World {
    let mut world = World::new(Schema::new(["unused"]).unwrap());
    world.add_entity("AAA", EntityKind::Country);
    world.add_entity("1", EntityKind::Province);
    world
        .set_metrics(
            serde_json::from_str(
                r#"{
        "definitions":{"wear":{"kind":"Province","minimum":0,"maximum":100000,
            "baseline":25000,"divisor":100000,
            "modifier":{"values_micros":{"yield":-2000000},"flags":{}},"unresolved_properties":[]}},
        "entities":{"1":{"wear":50125}}
    }"#,
            )
            .unwrap(),
        )
        .unwrap();
    world.set_attributes(serde_json::from_str(r#"{
        "static_modifiers":{},"building_modifiers":{},"trade_good_modifiers":{},
        "formulas":{"1":{"yield":{"base":{"kind":"constant","micros":4000000},
            "channels":[{"flat":[],"rates":[{"kind":"metrics","target":"Current","property":"yield"}]}],
            "minimum_micros":0,"maximum_micros":null,"unresolved":[]}}}
    }"#).unwrap()).unwrap();
    world
}

fn slot() -> NumericSlot {
    NumericSlot::Metric("wear".into())
}

#[test]
fn bounded_operations_scale_properties_live_and_restore_without_rounding_drift() {
    let mut world = world();
    let value = world
        .attributes()
        .evaluate(&world, EntityId(1), "yield")
        .unwrap();
    assert_eq!(value.value_micros, Some(1_990_000));
    for (op, input, expected) in [
        (NumericOperation::Add, i64::MAX, 100_000),
        (NumericOperation::Subtract, 50_000, 50_000),
        (NumericOperation::Multiply { scale: 1000 }, 1251, 62_550),
        (NumericOperation::Divide { scale: 1000 }, 1251, 50_000),
        (NumericOperation::Subtract, i64::MAX, 0),
        (NumericOperation::Set, 50_125, 50_125),
    ] {
        world
            .update_numeric(EntityId(1), &slot(), op, input)
            .unwrap();
        assert_eq!(world.numeric_value(EntityId(1), &slot()).unwrap(), expected);
    }
    let saved = world.save("metrics").unwrap();
    assert_eq!(
        World::restore(&saved, "metrics", world.schema()).unwrap(),
        world
    );
    let before = world.clone();
    assert!(
        world
            .update_numeric(
                EntityId(1),
                &slot(),
                NumericOperation::Divide { scale: 1 },
                0
            )
            .is_err()
    );
    assert_eq!(world, before);
}

#[test]
fn unknown_is_not_zero_and_only_set_can_recover_authoritative_state() {
    let mut world = world();
    world.set_metric_value(EntityId(1), "wear", None).unwrap();
    assert!(world.numeric_value(EntityId(1), &slot()).is_err());
    let result = world
        .attributes()
        .evaluate(&world, EntityId(1), "yield")
        .unwrap();
    assert_eq!(result.value_micros, None);
    assert!(result.unresolved.contains("metric:wear:unknown_value"));
    let before = world.clone();
    assert!(
        world
            .update_numeric(EntityId(1), &slot(), NumericOperation::Add, 1)
            .is_err()
    );
    assert_eq!(world, before);
    world
        .update_numeric(EntityId(1), &slot(), NumericOperation::Set, 0)
        .unwrap();
    assert_eq!(
        world
            .attributes()
            .evaluate(&world, EntityId(1), "yield")
            .unwrap()
            .value_micros,
        Some(6_000_000)
    );
    let mut missing = world.metrics().clone();
    missing.entities.clear();
    world.set_metrics(missing).unwrap();
    assert!(world.numeric_value(EntityId(1), &slot()).is_err());
}

#[test]
fn invalid_definitions_values_and_scope_fail_without_mutating_world() {
    let mut world = world();
    let before = world.clone();
    for value in [-1, 100_001] {
        assert!(
            world
                .set_metric_value(EntityId(1), "wear", Some(value))
                .is_err()
        );
    }
    assert!(
        world
            .set_metric_value(EntityId(0), "wear", Some(0))
            .is_err()
    );
    assert!(
        world
            .set_metric_value(EntityId(1), "missing", Some(0))
            .is_err()
    );
    for variant in 0..4 {
        let mut bad = world.metrics().clone();
        let definition = bad.definitions.get_mut("wear").unwrap();
        match variant {
            0 => definition.divisor = 0,
            1 => definition.minimum = 200_000,
            2 => definition.kind = EntityKind::Country,
            _ => {
                definition.divisor = 1;
                definition
                    .modifier
                    .as_mut()
                    .unwrap()
                    .values_micros
                    .insert("yield".into(), i64::MAX);
            }
        }
        assert!(world.set_metrics(bad).is_err());
    }
    assert_eq!(world, before);
}

#[test]
fn scheduled_metric_dependencies_prevent_definition_removal_and_invalid_saved_bounds() {
    let mut world = world();
    world
        .schedule(
            EntityId(1),
            2,
            vec![Action::UpdateNumeric {
                target: Target::Current,
                slot: slot(),
                operation: NumericOperation::Add,
                value: 1000,
            }],
        )
        .unwrap();
    let before = world.clone();
    assert!(world.set_metrics(Metrics::default()).is_err());
    assert_eq!(world, before);
    let saved = world.save("metrics").unwrap();
    let mut malformed: serde_json::Value = serde_json::from_str(&saved).unwrap();
    malformed["metrics"]["entities"]["1"]["wear"] = serde_json::json!(100001);
    assert!(World::restore(&malformed.to_string(), "metrics", world.schema()).is_err());
    let mut restored = World::restore(&saved, "metrics", world.schema()).unwrap();
    world.advance_to(2).unwrap();
    restored.advance_to(2).unwrap();
    assert_eq!(world, restored);
}
