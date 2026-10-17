// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    CountryResources, EntityId, EntityKind, ModifierDefinition, ModifierInstance, Modifiers,
    Schema, Technology, TechnologyLevel, TechnologyProperty, UnitTemplate, UnitTemplateProperty,
    UnitTemplates, World,
};
use std::collections::BTreeMap;

fn world() -> World {
    let mut world = World::new(Schema::new([] as [&str; 0]).unwrap());
    world.add_entity("AAA", EntityKind::Country);
    world.add_entity("1", EntityKind::Province);
    world
}

fn technology() -> Technology {
    Technology {
        tables: [(
            "research".into(),
            vec![TechnologyLevel {
                year: 1400,
                values_micros: BTreeMap::new(),
                flags: BTreeMap::new(),
                enabled_units: Vec::new(),
                unresolved_properties: Default::default(),
            }],
        )]
        .into_iter()
        .collect(),
        countries: [(EntityId(0), [("research".into(), 0)].into_iter().collect())]
            .into_iter()
            .collect(),
    }
}

#[test]
fn technology_property_query_keeps_absent_and_unknown_distinct() {
    let mut technology = technology();
    let level = &mut technology.tables.get_mut("research").unwrap()[0];
    level
        .values_micros
        .insert("production_efficiency".into(), 20_000_000);
    level.flags.insert("enabled".into(), true);
    level.unresolved_properties.insert("unresolved".into());

    assert_eq!(
        technology.country_property(EntityId(0), "research", "production_efficiency"),
        TechnologyProperty::Value(20_000_000)
    );
    assert_eq!(
        technology.country_property(EntityId(0), "research", "enabled"),
        TechnologyProperty::Flag(true)
    );
    assert_eq!(
        technology.country_property(EntityId(0), "research", "missing"),
        TechnologyProperty::Absent
    );
    assert_eq!(
        technology.country_property(EntityId(0), "research", "unresolved"),
        TechnologyProperty::Unknown
    );
    assert_eq!(
        technology.country_property(EntityId(1), "research", "production_efficiency"),
        TechnologyProperty::Unknown
    );
}

fn modifiers() -> Modifiers {
    let definition = |value| ModifierDefinition {
        values_micros: [("test.rate".into(), value)].into_iter().collect(),
        flags: BTreeMap::new(),
    };
    Modifiers {
        definitions: [
            ("first".into(), definition(12500)),
            ("second".into(), definition(-2500)),
        ]
        .into_iter()
        .collect(),
        active: [(
            EntityId(1),
            [
                ("first".into(), ModifierInstance::default()),
                ("second".into(), ModifierInstance::default()),
            ]
            .into_iter()
            .collect(),
        )]
        .into_iter()
        .collect(),
        ..Default::default()
    }
}

fn unit_templates() -> UnitTemplates {
    UnitTemplates {
        definitions: [(
            "barque".into(),
            UnitTemplate {
                category: "light_ship".into(),
                properties: vec![UnitTemplateProperty {
                    name: "hull_size".into(),
                    value: "8".into(),
                }],
            },
        )]
        .into_iter()
        .collect(),
    }
}

#[test]
fn setup_roundtrips_and_preserves_old_demo_saves_and_unknown_stocks() {
    let mut world = world();
    let old = world.save("test").unwrap();
    assert!(
        !old.contains("technology")
            && !old.contains("country_resources")
            && !old.contains("modifiers")
    );
    assert_eq!(World::restore(&old, "test", world.schema()).unwrap(), world);
    world.set_technology(technology()).unwrap();
    world.set_unit_templates(unit_templates()).unwrap();
    world.set_modifiers(modifiers()).unwrap();
    world
        .set_country_resources(
            [(
                EntityId(0),
                CountryResources {
                    mercantilism_milli: Some(18500),
                    ..Default::default()
                },
            )]
            .into_iter()
            .collect(),
        )
        .unwrap();
    assert_eq!(
        world.modifiers().declared_totals(EntityId(1)).unwrap()["test.rate"],
        10000
    );
    assert_eq!(world.country_resources()[&EntityId(0)].treasury_milli, None);
    assert_eq!(
        World::restore(&world.save("test").unwrap(), "test", world.schema()).unwrap(),
        world
    );
    let mut bad: serde_json::Value = serde_json::from_str(&world.save("test").unwrap()).unwrap();
    bad["technology"]["countries"]["0"]["research"] = 1.into();
    assert!(World::restore(&bad.to_string(), "test", world.schema()).is_err());
    bad = serde_json::from_str(&world.save("test").unwrap()).unwrap();
    bad["modifiers"]["active"]["1"]["missing"] = serde_json::json!({});
    assert!(World::restore(&bad.to_string(), "test", world.schema()).is_err());
    bad = serde_json::from_str(&world.save("test").unwrap()).unwrap();
    bad["country_resources"]["0"]["manpower_milli"] = (-1).into();
    assert!(World::restore(&bad.to_string(), "test", world.schema()).is_err());
}

#[test]
fn invalid_setup_replacements_are_atomic_and_native_rules_have_independent_bounds() {
    let mut world = world();
    let original = world.clone();
    let mut invalid = technology();
    invalid
        .countries
        .insert(EntityId(1), [("research".into(), 0)].into_iter().collect());
    assert!(world.set_technology(invalid).is_err());
    let mut invalid = technology();
    invalid
        .countries
        .get_mut(&EntityId(0))
        .unwrap()
        .insert("missing".into(), 0);
    assert!(world.set_technology(invalid).is_err());
    let mut invalid = modifiers();
    invalid
        .active
        .get_mut(&EntityId(1))
        .unwrap()
        .insert("missing".into(), ModifierInstance::default());
    assert!(world.set_modifiers(invalid).is_err());
    let mut invalid = modifiers();
    invalid
        .definitions
        .get_mut("first")
        .unwrap()
        .values_micros
        .insert("test.rate".into(), i64::MAX);
    invalid
        .definitions
        .get_mut("second")
        .unwrap()
        .values_micros
        .insert("test.rate".into(), 1);
    assert!(
        world
            .set_modifiers(invalid)
            .unwrap_err()
            .to_string()
            .contains("overflow")
    );
    assert!(
        world
            .set_country_resources(
                [(EntityId(1), CountryResources::default())]
                    .into_iter()
                    .collect()
            )
            .is_err()
    );
    assert_eq!(world, original);
    // Content adapters impose game-specific limits; the shared core retains only
    // stock/type invariants and can accept a native system's own numeric range.
    world
        .set_country_resources(
            [(
                EntityId(0),
                CountryResources {
                    mercantilism_milli: Some(200_000),
                    ..Default::default()
                },
            )]
            .into_iter()
            .collect(),
        )
        .unwrap();
}
