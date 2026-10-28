// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::NumericSlot;
use augustus_eu4::VanillaWorld;
use std::{collections::BTreeMap, path::Path};

const ADM: &str = "native_adm_development";
const DIP: &str = "native_dip_development";
const MIL: &str = "native_mil_development";

fn fixture() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world")
}

fn values(
    entries: impl IntoIterator<Item = (String, Option<i64>)>,
) -> BTreeMap<String, BTreeMap<String, Option<i64>>> {
    BTreeMap::from([("AAA".into(), entries.into_iter().collect())])
}

#[test]
fn native_development_registers_complete_metric_set_and_preserves_unknowns() {
    let input = BTreeMap::from([
        (
            "AAA".into(),
            BTreeMap::from([
                (ADM.into(), Some(i64::from(i32::MIN))),
                (DIP.into(), Some(-1_234)),
                (MIL.into(), None),
            ]),
        ),
        (
            "BBB".into(),
            BTreeMap::from([(ADM.into(), Some(i64::from(i32::MAX)))]),
        ),
    ]);
    let loaded =
        VanillaWorld::load_with_country_metric_values(fixture(), "1444.11.11", &input).unwrap();
    let world = loaded.initial.world();
    let metrics = world.metrics();

    for name in [ADM, DIP, MIL] {
        let definition = &metrics.definitions[name];
        assert_eq!(definition.minimum, i64::from(i32::MIN));
        assert_eq!(definition.maximum, i64::from(i32::MAX));
        assert_eq!(definition.baseline, 0);
        assert_eq!(definition.divisor, 1_000);
        assert!(definition.modifier.is_none());
        assert!(!definition.growth_ceiling);
    }

    let aaa = loaded.initial.country_entity("AAA").unwrap();
    let bbb = loaded.initial.country_entity("BBB").unwrap();
    assert_eq!(metrics.entities[&aaa][ADM], Some(i64::from(i32::MIN)));
    assert_eq!(metrics.entities[&aaa][DIP], Some(-1_234));
    assert_eq!(metrics.entities[&aaa][MIL], None);
    assert_eq!(metrics.entities[&bbb][ADM], Some(i64::from(i32::MAX)));
    assert!(!metrics.entities[&bbb].contains_key(DIP));
    assert!(metrics.value(world, aaa, MIL).is_err());
    assert_eq!(
        loaded.country("AAA").unwrap().metrics.unwrap()[DIP],
        Some(-1_234)
    );
}

#[test]
fn native_development_values_update_atomically_and_restore() {
    let initial = values([(ADM.into(), Some(12_345))]);
    let mut loaded =
        VanillaWorld::load_with_country_metric_values(fixture(), "1444.11.11", &initial).unwrap();
    let aaa = loaded.initial.country_entity("AAA").unwrap();
    let slot = NumericSlot::Metric(ADM.into());
    for name in [ADM, DIP, MIL] {
        assert!(
            loaded
                .initial
                .world()
                .metrics()
                .definitions
                .contains_key(name)
        );
    }
    assert!(!loaded.initial.world().metrics().entities[&aaa].contains_key(DIP));
    assert!(!loaded.initial.world().metrics().entities[&aaa].contains_key(MIL));
    assert_eq!(
        loaded.initial.world().numeric_value(aaa, &slot).unwrap(),
        12_345
    );

    loaded
        .set_country_metric_values(values([(ADM.into(), Some(13_579))]))
        .unwrap();
    assert_eq!(
        loaded.initial.world().numeric_value(aaa, &slot).unwrap(),
        13_579
    );

    let before_failed_update = loaded.initial.world().clone();
    let invalid = BTreeMap::from([
        ("AAA".into(), BTreeMap::from([(ADM.into(), Some(20_000))])),
        (
            "BBB".into(),
            BTreeMap::from([(DIP.into(), Some(i64::from(i32::MAX) + 1))]),
        ),
    ]);
    assert!(loaded.set_country_metric_values(invalid).is_err());
    assert_eq!(loaded.initial.world(), &before_failed_update);

    let snapshot = loaded.save().unwrap();
    let mut restored =
        VanillaWorld::load_with_country_metric_values(fixture(), "1444.11.11", &initial).unwrap();
    restored.restore(&snapshot).unwrap();
    assert_eq!(restored.initial.world(), loaded.initial.world());
    assert_eq!(
        restored.initial.world().numeric_value(aaa, &slot).unwrap(),
        13_579
    );
}

#[test]
fn explicit_unknown_native_development_registers_definitions_without_a_value() {
    let input = values([(DIP.into(), None)]);
    let loaded =
        VanillaWorld::load_with_country_metric_values(fixture(), "1444.11.11", &input).unwrap();
    let world = loaded.initial.world();
    let country = loaded.initial.country_entity("AAA").unwrap();

    for name in [ADM, DIP, MIL] {
        assert!(world.metrics().definitions.contains_key(name));
    }
    assert_eq!(world.metrics().entities[&country][DIP], None);
    assert!(world.metrics().value(world, country, DIP).is_err());
    assert!(!world.metrics().entities[&country].contains_key(ADM));
    assert!(!world.metrics().entities[&country].contains_key(MIL));
}

#[test]
fn ordinary_content_does_not_register_native_development_metrics() {
    let loaded = VanillaWorld::load(fixture(), "1444.11.11").unwrap();
    let definitions = &loaded.initial.world().metrics().definitions;
    assert!(!definitions.contains_key(ADM));
    assert!(!definitions.contains_key(DIP));
    assert!(!definitions.contains_key(MIL));
    let snapshot = loaded.save().unwrap();
    assert!(!snapshot.contains(ADM));
    assert!(!snapshot.contains(DIP));
    assert!(!snapshot.contains(MIL));
}
