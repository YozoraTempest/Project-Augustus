// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::*;
use augustus_eu4::{ScriptCompiler, ScriptLibrary, TextDocument, TextEncoding};

fn document(text: &str) -> TextDocument {
    TextDocument::from_bytes("treasury.txt", text.as_bytes().to_vec(), TextEncoding::Utf8)
}
fn slot() -> NumericSlot {
    NumericSlot::Resource(CountryResource::Treasury)
}

#[test]
fn host_binding_selects_one_store_and_existing_field_plans_remain_identical() {
    let schema = Schema::new(["treasury"]).unwrap();
    let library = ScriptLibrary::new();
    let legacy = ScriptCompiler::new(&schema, &library, true)
        .compile(&document("add_treasury=1"), EntityKind::Country)
        .unwrap();
    assert_eq!(
        legacy.plan,
        vec![Action::Add {
            target: Target::Current,
            field: FieldId(0),
            value: 1000
        }]
    );
    let condition = ScriptCompiler::new(&schema, &library, true)
        .compile(
            &document("if={limit={treasury=1} log=field}"),
            EntityKind::Country,
        )
        .unwrap();
    assert_eq!(
        condition.plan,
        vec![Action::If {
            condition: Condition::All(vec![Condition::AtLeast {
                target: Target::Current,
                field: FieldId(0),
                value: 1000
            }]),
            then: vec![Action::Emit("field".into())],
            otherwise: vec![]
        }]
    );
    let mut world = World::new(schema.clone());
    let aaa = world.add_entity("AAA", EntityKind::Country);
    world.set(aaa, FieldId(0), 9000).unwrap();
    world
        .set_country_resources(
            [(
                aaa,
                CountryResources {
                    treasury_milli: Some(1000),
                    manpower_milli: Some(5000),
                    mercantilism_milli: None,
                },
            )]
            .into(),
        )
        .unwrap();
    world.execute(aaa, &legacy.plan).unwrap();
    assert_eq!(world.value(aaa, FieldId(0)).unwrap(), 10000);
    assert_eq!(
        world
            .resource_value(aaa, CountryResource::Treasury)
            .unwrap(),
        1000
    );
    for optimise in [false, true] {
        let bound = ScriptCompiler::new(&schema, &library, optimise)
            .with_treasury_slot(Some(slot()))
            .compile(
                &document("add_treasury=1 if={limit={treasury=2} log=resource}"),
                EntityKind::Country,
            )
            .unwrap();
        let mut candidate = world.clone();
        candidate.execute(aaa, &bound.plan).unwrap();
        assert_eq!(
            candidate
                .resource_value(aaa, CountryResource::Treasury)
                .unwrap(),
            2000
        );
        assert_eq!(candidate.value(aaa, FieldId(0)).unwrap(), 10000);
        assert_eq!(
            candidate.country_resources()[&aaa].manpower_milli,
            Some(5000)
        );
        assert_eq!(candidate.events()[0].2, "resource");
    }
}

#[test]
fn scripted_treasury_adjustments_clamp_to_signed_native_bounds() {
    let schema = Schema::new(["unused"]).unwrap();
    let library = ScriptLibrary::new();
    let add = ScriptCompiler::new(&schema, &library, true)
        .with_treasury_slot(Some(slot()))
        .compile(&document("add_treasury=1"), EntityKind::Country)
        .unwrap();
    let subtract = ScriptCompiler::new(&schema, &library, true)
        .with_treasury_slot(Some(slot()))
        .compile(&document("add_treasury=-2000000"), EntityKind::Country)
        .unwrap();
    let mut world = World::new(schema);
    let country = world.add_entity("AAA", EntityKind::Country);
    world
        .set_country_resources(
            [(
                country,
                CountryResources {
                    treasury_milli: Some(999_999_500),
                    ..Default::default()
                },
            )]
            .into(),
        )
        .unwrap();

    world.execute(country, &add.plan).unwrap();
    assert_eq!(
        world
            .resource_value(country, CountryResource::Treasury)
            .unwrap(),
        1_000_000_000
    );
    world.execute(country, &subtract.plan).unwrap();
    assert_eq!(
        world
            .resource_value(country, CountryResource::Treasury)
            .unwrap(),
        -1_000_000_000
    );
}

#[test]
fn missing_bindings_and_unsupported_operands_report_source_without_guessing_balances() {
    let schema = Schema::new(["unused"]).unwrap();
    let library = ScriptLibrary::new();
    let unbound = ScriptCompiler::new(&schema, &library, true)
        .compile(&document("add_treasury=1"), EntityKind::Country)
        .unwrap_err();
    assert!(
        unbound
            .to_string()
            .contains("unregistered state field treasury")
    );
    for text in [
        "add_treasury={value=1}",
        "add_treasury=ROOT",
        "capital_scope={add_treasury=1}",
        "capital_scope={if={limit={treasury=1} log=bad}}",
    ] {
        let error = ScriptCompiler::new(&schema, &library, true)
            .with_treasury_slot(Some(slot()))
            .compile(&document(text), EntityKind::Country)
            .unwrap_err();
        assert!(error.to_string().contains("treasury.txt:1:"));
    }
    let error = ScriptCompiler::new(&schema, &library, true)
        .with_treasury_slot(Some(NumericSlot::Attribute {
            name: "income".into(),
            divisor: 1000,
        }))
        .compile(&document("add_treasury=1"), EntityKind::Country)
        .unwrap_err();
    assert!(error.to_string().contains("treasury binding is read only"));
    let mut world = World::new(schema.clone());
    let aaa = world.add_entity("AAA", EntityKind::Country);
    let plan = ScriptCompiler::new(&schema, &library, true)
        .with_treasury_slot(Some(slot()))
        .compile(&document("add_treasury=1"), EntityKind::Country)
        .unwrap();
    world.execute(aaa, &plan.plan).unwrap();
    assert!(world.country_resources().is_empty());
    assert!(world.numeric_value(aaa, &slot()).is_err());
    assert_eq!(world.unbased_numeric_delta(aaa, &slot()), Some(1_000));
    assert!(world.save("treasury:1").is_ok());
}

#[test]
fn unknown_initial_cash_keeps_the_delta_from_a_supported_startup_effect() {
    use augustus_eu4::VanillaWorld;
    use std::{fs, path::Path};
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world");
    let root = tempfile::tempdir().unwrap();
    for entry in walkdir::WalkDir::new(&source) {
        let entry = entry.unwrap();
        let target = root
            .path()
            .join(entry.path().strip_prefix(&source).unwrap());
        if entry.file_type().is_dir() {
            fs::create_dir_all(target).unwrap();
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
    fs::create_dir_all(root.path().join("common/on_actions")).unwrap();
    let path = root.path().join("common/on_actions/control.txt");
    fs::write(&path, "on_startup={if={limit={tag=AAA} add_treasury=1}}").unwrap();
    let loaded = VanillaWorld::load(root.path(), "1444.11.11").unwrap();
    assert!(loaded.report.estates.startup_unresolved.is_empty());
    let aaa = loaded.initial.country_entity("AAA").unwrap();
    assert!(loaded.initial.world().numeric_value(aaa, &slot()).is_err());
    assert_eq!(
        loaded.initial.world().unbased_numeric_delta(aaa, &slot()),
        Some(1_000)
    );
    let saved = loaded.save().unwrap();
    fs::write(path, "on_startup={if={limit={tag=AAA} add_treasury=2}}").unwrap();
    let mut changed = VanillaWorld::load(root.path(), "1444.11.11").unwrap();
    assert!(changed.restore(&saved).is_err());
}

#[test]
fn resource_literals_and_country_references_preserve_fractional_and_negative_balances() {
    let schema = Schema::new(["unused"]).unwrap();
    let library = ScriptLibrary::new();
    let mut world = World::new(schema.clone());
    let aaa = world.add_entity("AAA", EntityKind::Country);
    let bbb = world.add_entity("BBB", EntityKind::Country);
    world
        .set_country_resources(
            [
                (
                    aaa,
                    CountryResources {
                        treasury_milli: Some(999),
                        ..Default::default()
                    },
                ),
                (
                    bbb,
                    CountryResources {
                        treasury_milli: Some(500),
                        ..Default::default()
                    },
                ),
            ]
            .into(),
        )
        .unwrap();
    let compiled=ScriptCompiler::new(&schema,&library,true).with_treasury_slot(Some(slot())).compile(&document("if={limit={treasury=BBB} log=greater} BBB={if={limit={NOT={treasury=ROOT}} log=smaller}} add_treasury=0.001 add_treasury=0.0005 add_treasury=0.0005 if={limit={treasury=1 NOT={treasury=1.001}} log=precise} add_treasury=-1.5 if={limit={NOT={treasury=0} treasury=-0.5} log=negative}"),EntityKind::Country).unwrap();
    world.execute(aaa, &compiled.plan).unwrap();
    assert_eq!(
        world
            .resource_value(aaa, CountryResource::Treasury)
            .unwrap(),
        -500
    );
    assert_eq!(
        world
            .events()
            .iter()
            .map(|event| event.2.as_str())
            .collect::<Vec<_>>(),
        ["greater", "smaller", "precise", "negative"]
    );
}
