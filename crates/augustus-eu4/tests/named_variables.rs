// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{EntityKind, EventCatalog, NamedValues, NumericSlot, Schema, World};
use augustus_eu4::{ScriptCompiler, ScriptLibrary, TextDocument, TextEncoding};

fn document(text: &str) -> TextDocument {
    TextDocument::from_bytes(
        "self-authored-variables.txt",
        text.as_bytes().to_vec(),
        TextEncoding::Utf8,
    )
}

#[test]
fn literal_variables_translate_without_schema_fields_and_share_saved_event_state() {
    let schema = Schema::new(["unused"]).unwrap();
    let library = ScriptLibrary::new();
    let effects = document(
        r#"
      if={limit={check_variable={which=count value=0}} log=fresh}
      change_variable={which=count value=2.125}
      subtract_variable={which=count value=3.375}
      multiply_variable={which=count value=-2}
      divide_variable={which=count value=4}
      if={limit={is_variable_equal={which=count value=0.625}} log=arithmetic}
      capital_scope={set_variable={which=count value=4.5} owner={if={limit={is_variable_equal={which=count value=0.625}} log=owner}}}
      country_event={id=numeric.1 days=1}
    "#,
    );
    let event = document(
        "country_event={id=numeric.1 is_triggered_only=yes trigger={check_variable={which=count value=1}} immediate={log=delayed} option={name=accept}} ",
    );
    let mut states = Vec::new();
    for optimise in [false, true] {
        let plan = ScriptCompiler::new(&schema, &library, optimise)
            .compile(&effects, EntityKind::Country)
            .unwrap()
            .plan;
        let events = ScriptCompiler::new(&schema, &library, optimise)
            .compile_events(&event)
            .unwrap();
        let mut world = World::new(schema.clone());
        let country = world.add_entity("AAA", EntityKind::Country);
        let other = world.add_entity("BBB", EntityKind::Country);
        let province = world.add_entity("Port", EntityKind::Province);
        world.set_owner(province, country).unwrap();
        world.set_capital(country, province).unwrap();
        world
            .set_named_values(NamedValues {
                entities: [
                    (country, Default::default()),
                    (other, Default::default()),
                    (province, Default::default()),
                ]
                .into(),
                ..Default::default()
            })
            .unwrap();
        world
            .set_event_catalog(
                EventCatalog::new(events.definitions)
                    .unwrap()
                    .with_conditions(events.conditions)
                    .unwrap(),
            )
            .unwrap();
        world.execute(country, &plan).unwrap();
        assert_eq!(
            world
                .numeric_value(other, &NumericSlot::Named("count".into()))
                .unwrap(),
            0
        );
        assert_eq!(
            world
                .numeric_value(province, &NumericSlot::Named("count".into()))
                .unwrap(),
            4500
        );
        let update = ScriptCompiler::new(&schema, &library, true)
            .compile(
                &document("change_variable={which=count value=0.375}"),
                EntityKind::Country,
            )
            .unwrap();
        world.execute(country, &update.plan).unwrap();
        let mut restored = World::restore_with_events(
            &world.save("numeric:v1").unwrap(),
            "numeric:v1",
            &schema,
            vec![],
            world.event_catalog().clone(),
        )
        .unwrap();
        world.advance_to(1).unwrap();
        restored.advance_to(1).unwrap();
        assert_eq!(world, restored);
        assert_eq!(
            world
                .events()
                .iter()
                .map(|e| (e.0, e.2.as_str()))
                .collect::<Vec<_>>(),
            [
                (0, "fresh"),
                (0, "arithmetic"),
                (0, "owner"),
                (1, "delayed")
            ]
        );
        states.push(world.save("numeric:v1").unwrap());
    }
    assert_eq!(states[0], states[1]);
}

#[test]
fn existing_explicit_bindings_keep_their_plans_and_extended_arithmetic_uses_the_field() {
    let schema = Schema::new(["variable/count"]).unwrap();
    let library = ScriptLibrary::new();
    let compiled=ScriptCompiler::new(&schema,&library,false).compile(&document("set_variable={which=count value=2} change_variable={which=count value=0.5} if={limit={check_variable={which=count value=2.5}} log=bound}"),EntityKind::Country).unwrap();
    assert!(matches!(
        compiled.plan[0],
        augustus_core::Action::Set { .. }
    ));
    assert!(matches!(
        compiled.plan[1],
        augustus_core::Action::Add { .. }
    ));
    let mut world = World::new(schema.clone());
    let country = world.add_entity("AAA", EntityKind::Country);
    world.execute(country, &compiled.plan).unwrap();
    let extended=ScriptCompiler::new(&schema,&library,true).compile(&document("divide_variable={which=count value=4} if={limit={is_variable_equal={which=count value=0.625}} log=extended}"),EntityKind::Country).unwrap();
    world.execute(country, &extended.plan).unwrap();
    assert_eq!(
        world.value(country, augustus_core::FieldId(0)).unwrap(),
        625
    );
    assert!(world.named_values().is_empty());
    assert_eq!(world.events().len(), 2);
}

#[test]
fn unsupported_operands_and_malformed_variables_are_located_errors_even_in_false_branches() {
    let schema = Schema::new(["unused"]).unwrap();
    let library = ScriptLibrary::new();
    for text in [
        "set_variable={which=\"\" value=1}",
        "set_variable={which=count value=source}",
        "set_variable={which=count@ROOT value=1}",
        "divide_variable={which=count value=0.0001}",
        "change_variable={which=count which=other}",
        "if={limit={always=no} multiply_variable={which=\"\" value=2}}",
    ] {
        let error = ScriptCompiler::new(&schema, &library, true)
            .compile(&document(text), EntityKind::Country)
            .unwrap_err();
        assert!(error.location().is_some(), "{text}");
        assert_eq!(
            error.path().to_string_lossy(),
            "self-authored-variables.txt"
        );
    }
}
