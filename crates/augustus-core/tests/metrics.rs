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
    world
        .update_numeric(EntityId(1), &slot(), NumericOperation::Add, 1000)
        .unwrap();
    world
        .update_numeric(EntityId(1), &slot(), NumericOperation::Subtract, 250)
        .unwrap();
    assert!(world.numeric_value(EntityId(1), &slot()).is_err());
    assert_eq!(world.unbased_numeric_delta(EntityId(1), &slot()), Some(750));
    let saved = world.save("metrics:unknown-delta").unwrap();
    let mut resumed = World::restore(&saved, "metrics:unknown-delta", world.schema()).unwrap();
    assert_eq!(resumed, world);
    resumed
        .set_metric_value(EntityId(1), "wear", Some(40_000))
        .unwrap();
    assert_eq!(resumed.unbased_numeric_delta(EntityId(1), &slot()), None);

    world
        .update_numeric(EntityId(1), &slot(), NumericOperation::Set, 0)
        .unwrap();
    assert_eq!(world.unbased_numeric_delta(EntityId(1), &slot()), None);
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

fn segmented_world() -> World {
    let mut world = world();
    world.set_owner(EntityId(1), EntityId(0)).unwrap();
    let metrics: Metrics = serde_json::from_str(r#"{
        "definitions":{"confidence":{"kind":"Country","minimum":-3,"maximum":3,
            "baseline":0,"divisor":1,
            "modifier":{"values_micros":{"yield":125000},"flags":{}},"unresolved_properties":[],
            "below_baseline":{"modifier":{"values_micros":{"yield":75000,"lower_only":21000},"flags":{}},"unresolved_properties":[]}}},
        "entities":{"0":{"confidence":0}}
    }"#).unwrap();
    // Remove the previous attribute dependency before replacing its metric catalog.
    world.set_attributes(Attributes::default()).unwrap();
    world.set_metrics(metrics).unwrap();
    world.set_attributes(serde_json::from_str(r#"{
        "static_modifiers":{},"building_modifiers":{},"trade_good_modifiers":{},
        "formulas":{"1":{"yield":{"base":{"kind":"constant","micros":12000000},
            "channels":[{"flat":[],"rates":[{"kind":"metrics","target":"Owner","property":"yield"}]}],
            "minimum_micros":0,"maximum_micros":null,"unresolved":[]}}}
    }"#).unwrap()).unwrap();
    world
}

#[test]
fn signed_profiles_feed_owned_attributes_and_monthly_settlement_across_restore() {
    let mut world = segmented_world();
    for (value, expected) in [
        (-3, 9_300_000),
        (-1, 11_100_000),
        (0, 12_000_000),
        (1, 13_500_000),
        (3, 16_500_000),
    ] {
        world
            .set_metric_value(EntityId(0), "confidence", Some(value))
            .unwrap();
        assert_eq!(
            world
                .attributes()
                .evaluate(&world, EntityId(1), "yield")
                .unwrap()
                .value_micros,
            Some(expected)
        );
    }
    world
        .set_calendar(Calendar {
            start: WorldDate {
                year: 1444,
                month: 11,
                day: 11,
            },
            month_lengths: [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31],
        })
        .unwrap();
    world
        .set_country_resources(
            [(
                EntityId(0),
                CountryResources {
                    treasury_milli: Some(0),
                    ..Default::default()
                },
            )]
            .into(),
        )
        .unwrap();
    world
        .set_taxation(Taxation::new("yield", [EntityId(0)].into()))
        .unwrap();
    world.advance_to(20).unwrap();
    assert_eq!(
        world
            .taxation()
            .unwrap()
            .last_settlement
            .as_ref()
            .unwrap()
            .accounts[&EntityId(0)]
            .income_milli,
        Some(1375)
    );
    world
        .set_metric_value(EntityId(0), "confidence", Some(-3))
        .unwrap();
    let saved = world.save("segmented").unwrap();
    let mut resumed = World::restore(&saved, "segmented", world.schema()).unwrap();
    world.advance_to(51).unwrap();
    resumed.advance_to(51).unwrap();
    assert_eq!(world, resumed);
    assert_eq!(
        world
            .taxation()
            .unwrap()
            .last_settlement
            .as_ref()
            .unwrap()
            .accounts[&EntityId(0)]
            .income_milli,
        Some(775)
    );
}

#[test]
fn unknown_values_include_both_reachable_profiles_without_treating_them_as_zero() {
    let mut world = segmented_world();
    world
        .set_metric_value(EntityId(0), "confidence", None)
        .unwrap();
    assert!(
        world
            .metrics()
            .property_effects(&world, EntityId(0), "lower_only")
            .unwrap()
            .unresolved
            .contains("metric:confidence:unknown_value")
    );
    assert_eq!(
        world
            .attributes()
            .evaluate(&world, EntityId(1), "yield")
            .unwrap()
            .value_micros,
        None
    );
    world
        .set_metric_value(EntityId(0), "confidence", Some(1))
        .unwrap();
    assert!(
        world
            .metrics()
            .property_effects(&world, EntityId(0), "lower_only")
            .unwrap()
            .contributions
            .is_empty()
    );
    let mut metrics = world.metrics().clone();
    metrics
        .definitions
        .get_mut("confidence")
        .unwrap()
        .below_baseline
        .as_mut()
        .unwrap()
        .unresolved_properties
        .insert("yield".into());
    world.set_metrics(metrics).unwrap();
    assert_eq!(
        world
            .attributes()
            .evaluate(&world, EntityId(1), "yield")
            .unwrap()
            .value_micros,
        Some(13_500_000)
    );
    world
        .set_metric_value(EntityId(0), "confidence", Some(-1))
        .unwrap();
    assert_eq!(
        world
            .attributes()
            .evaluate(&world, EntityId(1), "yield")
            .unwrap()
            .value_micros,
        None
    );
}

#[test]
fn invalid_lower_profiles_and_overflowing_updates_leave_state_unchanged() {
    let mut world = segmented_world();
    let before = world.clone();
    for variant in 0..3 {
        let mut bad = world.metrics().clone();
        let effect = bad
            .definitions
            .get_mut("confidence")
            .unwrap()
            .below_baseline
            .as_mut()
            .unwrap();
        match variant {
            0 => {
                effect.unresolved_properties.insert(String::new());
            }
            1 => {
                effect
                    .modifier
                    .as_mut()
                    .unwrap()
                    .flags
                    .insert("yield".into(), true);
            }
            _ => {
                effect
                    .modifier
                    .as_mut()
                    .unwrap()
                    .values_micros
                    .insert(String::new(), 1);
            }
        }
        assert!(world.set_metrics(bad).is_err());
    }
    assert_eq!(world, before);
    let mut metrics = world.metrics().clone();
    metrics
        .definitions
        .get_mut("confidence")
        .unwrap()
        .below_baseline
        .as_mut()
        .unwrap()
        .modifier
        .as_mut()
        .unwrap()
        .values_micros
        .insert("yield".into(), i64::MAX);
    world.set_metrics(metrics).unwrap();
    let before = world.clone();
    assert!(
        world
            .update_numeric(
                EntityId(0),
                &NumericSlot::Metric("confidence".into()),
                NumericOperation::Add,
                -3
            )
            .is_err()
    );
    assert_eq!(world, before);
}

#[test]
fn corrupt_unbased_metric_deltas_cannot_be_restored() {
    let world = world();
    let mut saved: serde_json::Value =
        serde_json::from_str(&world.save("metrics:unbased-delta").unwrap()).unwrap();
    saved["unbased_metric_deltas"]["1"]["wear"] = serde_json::json!(1);
    assert!(World::restore(&saved.to_string(), "metrics:unbased-delta", world.schema()).is_err());
}
