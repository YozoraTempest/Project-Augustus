// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{EntityId, EntityKind, Governments, Schema, World};

fn world() -> World {
    let mut world = World::new(Schema::new(["base"]).unwrap());
    world.add_entity("A", EntityKind::Country);
    world.add_entity("B", EntityKind::Country);
    world.add_entity("P", EntityKind::Province);
    world.set_owner(EntityId(2), EntityId(0)).unwrap();
    world.set_governments(serde_json::from_str(r#"{
        "reforms": {
            "first": {"modifier":{"values_micros":{"yield":125000},"flags":{}},"unresolved_properties":[]},
            "second": {"modifier":{"values_micros":{"yield":-250000},"flags":{}},"unresolved_properties":[]}
        },
        "metrics": {"authority":{"minimum_milli":0,"maximum_milli":100000,"baseline_milli":25000,"divisor_milli":50000,
            "modifier":{"values_micros":{"yield":400000,"other":-750000},"flags":{}},"unresolved_properties":[]}},
        "countries": {
            "0":{"kind":"first_kind","reforms":["first"],"metrics":{"authority":75125}},
            "1":{"kind":"second_kind","reforms":["second"],"metrics":{}}
        }
    }"#).unwrap()).unwrap();
    world.set_attributes(serde_json::from_str(r#"{
        "static_modifiers":{},"building_modifiers":{},"trade_good_modifiers":{},
        "formulas":{"2":{"yield":{"base":{"kind":"constant","micros":1000000},
            "channels":[{"flat":[],"rates":[{"kind":"government","target":"Owner","property":"yield"}]}],
            "minimum_micros":null,"maximum_micros":null,"unresolved":[]}}}
    }"#).unwrap()).unwrap();
    world
}

fn yield_value(world: &World) -> augustus_core::AttributeEvaluation {
    world
        .attributes()
        .evaluate(world, EntityId(2), "yield")
        .unwrap()
}

#[test]
fn reform_metric_and_owner_changes_recalculate_with_signed_fixed_point_values() {
    let mut world = world();
    assert_eq!(yield_value(&world).value_micros, Some(1_526_000));
    let effects = world.governments().declared_effects(EntityId(0)).unwrap();
    assert_eq!(effects.known_totals_micros["other"], -751_875);
    assert_eq!(effects.contributions.len(), 3);
    for (metric, expected) in [
        (0, 925_000),
        (25_000, 1_125_000),
        (50_000, 1_325_000),
        (100_000, 1_725_000),
    ] {
        let mut governments = world.governments().clone();
        *governments
            .countries
            .get_mut(&EntityId(0))
            .unwrap()
            .metrics
            .as_mut()
            .unwrap()
            .get_mut("authority")
            .unwrap() = Some(metric);
        world.set_governments(governments).unwrap();
        assert_eq!(yield_value(&world).value_micros, Some(expected));
    }
    let mut governments = world.governments().clone();
    governments
        .countries
        .get_mut(&EntityId(0))
        .unwrap()
        .reforms
        .as_mut()
        .unwrap()
        .clear();
    world.set_governments(governments).unwrap();
    assert_eq!(yield_value(&world).value_micros, Some(1_600_000));
    world.set_owner(EntityId(2), EntityId(1)).unwrap();
    assert_eq!(yield_value(&world).value_micros, Some(750_000));
}

#[test]
fn unknown_inputs_preserve_known_contributions_and_property_scope() {
    let mut world = world();
    let mut governments = world.governments().clone();
    governments
        .metrics
        .get_mut("authority")
        .unwrap()
        .unresolved_properties
        .insert("other".into());
    world.set_governments(governments.clone()).unwrap();
    assert_eq!(yield_value(&world).value_micros, Some(1_526_000));
    governments
        .countries
        .get_mut(&EntityId(0))
        .unwrap()
        .metrics
        .as_mut()
        .unwrap()
        .insert("authority".into(), None);
    world.set_governments(governments.clone()).unwrap();
    let value = yield_value(&world);
    assert_eq!(value.known_subtotal_micros, Some(1_125_000));
    assert_eq!(value.value_micros, None);
    assert!(
        value
            .unresolved
            .contains("government:metric:authority:unknown_value:yield")
    );
    governments.countries.get_mut(&EntityId(0)).unwrap().metrics = Some(Default::default());
    governments
        .countries
        .get_mut(&EntityId(0))
        .unwrap()
        .reforms
        .as_mut()
        .unwrap()
        .insert("missing".into());
    world.set_governments(governments.clone()).unwrap();
    assert!(
        yield_value(&world)
            .unresolved
            .contains("government:reform:missing:missing_definition")
    );
    governments.countries.get_mut(&EntityId(0)).unwrap().reforms = None;
    world.set_governments(governments).unwrap();
    assert!(
        yield_value(&world)
            .unresolved
            .contains("government:unknown_reforms")
    );
    assert_eq!(yield_value(&world).value_micros, None);
}

#[test]
fn invalid_replacements_overflow_and_corrupt_saves_fail_before_replacing_state() {
    let mut world = world();
    let before = world.clone();
    let mut bad = world.governments().clone();
    bad.countries
        .insert(EntityId(2), bad.countries[&EntityId(0)].clone());
    assert!(world.set_governments(bad).is_err());
    let mut bad = world.governments().clone();
    bad.metrics.get_mut("authority").unwrap().divisor_milli = 0;
    assert!(world.set_governments(bad).is_err());
    let mut bad = world.governments().clone();
    bad.countries
        .get_mut(&EntityId(0))
        .unwrap()
        .metrics
        .as_mut()
        .unwrap()
        .insert("authority".into(), Some(100_001));
    assert!(world.set_governments(bad).is_err());
    let mut bad = world.governments().clone();
    let metric = bad.metrics.get_mut("authority").unwrap();
    metric.divisor_milli = 1;
    metric
        .modifier
        .as_mut()
        .unwrap()
        .values_micros
        .insert("yield".into(), i64::MAX);
    assert!(world.set_governments(bad).is_err());
    assert_eq!(world, before);
    let mut bad = world.governments().clone();
    let reform = bad.reforms.get_mut("first").unwrap();
    reform.modifier.values_micros.remove("yield");
    reform.modifier.flags.insert("yield".into(), true);
    assert!(world.set_governments(bad).is_err());
    assert_eq!(world, before);
    let mut saved: serde_json::Value =
        serde_json::from_str(&world.save("government").unwrap()).unwrap();
    saved["governments"]["countries"]["0"]["metrics"]["authority"] = serde_json::json!(-1);
    assert!(World::restore(&saved.to_string(), "government", world.schema()).is_err());
}

#[test]
fn government_state_roundtrips_and_existing_empty_snapshots_remain_readable() {
    let world = world();
    let saved = world.save("government").unwrap();
    let restored = World::restore(&saved, "government", world.schema()).unwrap();
    assert_eq!(restored, world);
    assert_eq!(yield_value(&restored), yield_value(&world));
    let empty = World::new(Schema::new(["base"]).unwrap());
    let saved = empty.save("old").unwrap();
    assert!(!saved.contains("governments"));
    assert_eq!(
        World::restore(&saved, "old", empty.schema()).unwrap(),
        empty
    );
    let mut replacement = world.governments().clone();
    replacement.metrics.get_mut("authority").unwrap().modifier = None;
    let mut changed = world;
    changed.set_governments(replacement).unwrap();
    assert!(
        yield_value(&changed)
            .unresolved
            .contains("government:metric:authority:missing_definition")
    );
    let _: Governments =
        serde_json::from_str(&serde_json::to_string(changed.governments()).unwrap()).unwrap();
}
