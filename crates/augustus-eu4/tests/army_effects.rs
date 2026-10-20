// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::{collections::BTreeMap, path::Path};

use augustus_core::{
    AffiliationDimension, Affiliations, EntityKind, Schema, ScopeContext, Technology,
    TechnologyLevel, UnitTemplate, UnitTemplateProperty, UnitTemplates, World,
};
use augustus_eu4::{ScriptCompiler, ScriptLibrary, TextDocument, TextEncoding, VanillaWorld};

fn document(text: &str) -> TextDocument {
    TextDocument::from_bytes(
        "self-authored.txt",
        text.as_bytes().to_vec(),
        TextEncoding::Utf8,
    )
}

fn configure_units(world: &mut World, country: augustus_core::EntityId) {
    let mut affiliations = Affiliations::default();
    affiliations.0.insert(
        "unit_type".into(),
        AffiliationDimension {
            members: BTreeMap::from([(country, Some("western".into()))]),
            ..Default::default()
        },
    );
    world.set_affiliations(affiliations).unwrap();

    let mut levels = vec![
        TechnologyLevel {
            year: 1350,
            values_micros: BTreeMap::new(),
            flags: BTreeMap::new(),
            enabled_units: vec![],
            unresolved_properties: Default::default(),
        },
        TechnologyLevel {
            year: 1390,
            values_micros: BTreeMap::new(),
            flags: BTreeMap::new(),
            enabled_units: vec![
                "western_medieval_infantry".into(),
                "western_medieval_knights".into(),
                "eastern_medieval_infantry".into(),
            ],
            unresolved_properties: Default::default(),
        },
    ];
    levels.resize_with(6, || TechnologyLevel {
        year: 1500,
        values_micros: BTreeMap::new(),
        flags: BTreeMap::new(),
        enabled_units: vec![],
        unresolved_properties: Default::default(),
    });
    levels[5].enabled_units.push("western_men_at_arms".into());

    world
        .set_technology(Technology {
            tables: BTreeMap::from([("military".into(), levels)]),
            countries: BTreeMap::from([(country, BTreeMap::from([("military".into(), 3)]))]),
        })
        .unwrap();

    let template = |category: &str, group: &str| UnitTemplate {
        category: category.into(),
        properties: vec![UnitTemplateProperty {
            name: "unit_type".into(),
            value: group.into(),
        }],
    };
    world
        .set_unit_templates(UnitTemplates {
            definitions: BTreeMap::from([
                (
                    "western_medieval_infantry".into(),
                    template("infantry", "western"),
                ),
                (
                    "western_medieval_knights".into(),
                    template("cavalry", "western"),
                ),
                (
                    "eastern_medieval_infantry".into(),
                    template("infantry", "eastern"),
                ),
                (
                    "western_men_at_arms".into(),
                    template("infantry", "western"),
                ),
            ]),
        })
        .unwrap();
    world
        .set_selected_unit(country, "infantry", "western_medieval_infantry")
        .unwrap();
    world
        .set_selected_unit(country, "cavalry", "western_medieval_knights")
        .unwrap();
}

#[test]
fn province_land_unit_effects_create_country_specific_regiments() {
    let schema = Schema::new(std::iter::empty::<String>()).unwrap();
    let compiled = ScriptCompiler::new(&schema, &ScriptLibrary::new(), true)
        .compile_events(&document(
            "country_event={id=army.1 is_triggered_only=yes immediate={1={infantry=ROOT cavalry=ROOT}} option={name=continue}}",
        ))
        .unwrap();
    let mut world = World::new(schema);
    let country = world.add_entity("SWE", EntityKind::Country);
    let province = world.add_entity("1", EntityKind::Province);
    configure_units(&mut world, country);

    world
        .execute_in_context(
            ScopeContext::new(country),
            &compiled.definitions[0].immediate,
        )
        .unwrap();

    assert_eq!(world.armies().len(), 2);
    assert_eq!(world.armies()[0].owner, country);
    assert_eq!(world.armies()[0].location, province);
    assert_eq!(
        world.armies()[0].regiments[0].unit_type,
        "western_medieval_infantry"
    );
    assert_eq!(
        world.armies()[1].regiments[0].unit_type,
        "western_medieval_knights"
    );
}

#[test]
fn land_unit_effect_rejects_non_province_scope() {
    let schema = Schema::new(std::iter::empty::<String>()).unwrap();
    assert!(
        ScriptCompiler::new(&schema, &ScriptLibrary::new(), true)
            .compile(&document("infantry=ROOT"), EntityKind::Country)
            .is_err()
    );
}

#[test]
fn parameterized_unit_spawn_effect_repeats_dynamic_creation_in_previous_province() {
    let schema = Schema::new([] as [&str; 0]).unwrap();
    let mut library = ScriptLibrary::new();
    library
        .add_effects(&document(
            r#"
            create_units_of_type_in_province = {
                [[custom_tooltip]
                    custom_tooltip = $custom_tooltip$
                ]
                $who$ = {
                    for = {
                        amount = $amount$
                        effect = "PREV = { $unit_type$ = $who$ }"
                    }
                }
            }
            "#,
        ))
        .unwrap();
    let event_document = |arguments: &str| {
        let source = r#"
            country_event = {
                id = units.1
                is_triggered_only = yes
                immediate = {
                    112 = {
                        create_units_of_type_in_province = { $UNIT_ARGUMENTS$ }
                    }
                }
                option = { name = continue }
            }
        "#
        .replace("$UNIT_ARGUMENTS$", arguments);
        document(&source)
    };
    let compiled = ScriptCompiler::new(&schema, &library, true)
        .compile_events(&event_document("who=ROOT amount=3 unit_type=galley"))
        .unwrap();
    ScriptCompiler::new(&schema, &library, true)
        .compile_events(&event_document(
            "who=ROOT amount=3 unit_type=galley custom_tooltip=unit_spawn_tooltip",
        ))
        .unwrap();
    let mut world = World::new(schema);
    let country = world.add_entity("VEN", EntityKind::Country);
    let province = world.add_entity("112", EntityKind::Province);
    world.set_province_has_port(province, true).unwrap();

    world
        .execute_in_context(
            ScopeContext::new(country),
            &compiled.definitions[0].immediate,
        )
        .unwrap();

    assert_eq!(world.naval_units().len(), 3);
    assert_eq!(world.fleets().len(), 3);
    assert!(world.naval_units().iter().all(|unit| {
        unit.owner == country && unit.location == province && unit.unit_type == "galley"
    }));
}

#[test]
fn country_unit_spawn_effect_uses_previous_country_for_parameterized_province() {
    let schema = Schema::new([] as [&str; 0]).unwrap();
    let mut library = ScriptLibrary::new();
    library
        .add_effects(&document(
            r#"
            create_units_of_type = {
                [[custom_tooltip]
                    custom_tooltip = $custom_tooltip$
                ]
                hidden_effect = {
                    for = {
                        amount = $amount$
                        effect = "$province$ = { $unit_type$ = PREV }"
                    }
                }
            }
            "#,
        ))
        .unwrap();
    let event_for_unit = |unit_type: &str| {
        let source = r#"
            country_event = {
                id = units.country_spawn
                is_triggered_only = yes
                immediate = {
                    create_units_of_type = {
                        province = 112
                        amount = 2
                        unit_type = $UNIT_TYPE$
                    }
                }
                option = { name = continue }
            }
        "#
        .replace("$UNIT_TYPE$", unit_type);
        ScriptCompiler::new(&schema, &library, true)
            .compile_events(&document(&source))
            .unwrap()
    };
    let naval = event_for_unit("galley");
    let land = event_for_unit("infantry");
    let mut world = World::new(schema);
    let country = world.add_entity("VEN", EntityKind::Country);
    let province = world.add_entity("112", EntityKind::Province);
    configure_units(&mut world, country);
    world.set_province_has_port(province, true).unwrap();

    for compiled in [&naval, &land] {
        world
            .execute_in_context(
                ScopeContext::new(country),
                &compiled.definitions[0].immediate,
            )
            .unwrap();
    }

    assert_eq!(world.naval_units().len(), 2);
    assert!(world.naval_units().iter().all(|unit| {
        unit.owner == country && unit.location == province && unit.unit_type == "galley"
    }));
    assert_eq!(world.armies().len(), 2);
    assert!(world.armies().iter().all(|army| {
        army.owner == country
            && army.location == province
            && army.regiments[0].unit_type == "western_medieval_infantry"
    }));
}

#[test]
fn loader_binds_the_country_unit_group_used_by_unit_selection() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world");
    let loaded = VanillaWorld::load(root, "1444.11.11").unwrap();
    let country = loaded.initial.country_entity("AAA").unwrap();
    assert_eq!(
        loaded
            .initial
            .world()
            .affiliations()
            .member("unit_type", country),
        Some("test_group")
    );
    let available = loaded
        .initial
        .world()
        .available_unit_templates(country)
        .unwrap();
    assert_eq!(available["infantry"], ["test_infantry"]);
    assert_eq!(available["cavalry"], ["test_cavalry"]);
    assert_eq!(available["artillery"], ["test_artillery"]);
}
