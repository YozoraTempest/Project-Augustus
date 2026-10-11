// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{EntityKind, EventCatalog, EventChoice, Schema, World};
use augustus_eu4::{ScriptCompiler, ScriptLibrary, TextDocument, TextEncoding};

fn document(text: &str) -> TextDocument {
    TextDocument::from_bytes(
        "self-authored-hidden-effects.txt",
        text.as_bytes().to_vec(),
        TextEncoding::Utf8,
    )
}

fn world(schema: &Schema) -> World {
    let mut world = World::new(schema.clone());
    let country = world.add_entity("AAA", EntityKind::Country);
    let capital = world.add_entity("Port", EntityKind::Province);
    world.set_owner(capital, country).unwrap();
    world.set_capital(country, capital).unwrap();
    world
}

#[test]
fn containers_flatten_in_order_with_live_state_and_restored_scope() {
    let schema = Schema::new(["variable/phase"]).unwrap();
    let mut library = ScriptLibrary::new();
    library
        .add_effects(&document(
            "step={hidden_effect={change_variable={which=phase value=1}}}",
        ))
        .unwrap();
    let nested = document(
        r#"
      log=begin
      hidden_effect={
        set_variable={which=phase value=1}
        hidden_effect={step=yes}
        capital_scope={hidden_effect={set_variable={which=phase value=7}
          owner={hidden_effect={if={limit={check_variable={which=phase value=2}} log=owner}}}}}
        if={limit={check_variable={which=phase value=2}} log=country}
      }
      hidden_effect={}
      log=end
    "#,
    );
    let flat = document(
        r#"
      log=begin
      set_variable={which=phase value=1}
      change_variable={which=phase value=1}
      capital_scope={set_variable={which=phase value=7}
        owner={if={limit={check_variable={which=phase value=2}} log=owner}}}
      if={limit={check_variable={which=phase value=2}} log=country}
      log=end
    "#,
    );
    for optimise in [false, true] {
        let compiled = ScriptCompiler::new(&schema, &library, optimise)
            .compile(&nested, EntityKind::Country)
            .unwrap();
        let expected = ScriptCompiler::new(&schema, &library, optimise)
            .compile(&flat, EntityKind::Country)
            .unwrap();
        assert_eq!(compiled.plan, expected.plan);
        assert_eq!(compiled.folded_branches, expected.folded_branches);
        let mut actual = world(&schema);
        let mut reference = actual.clone();
        actual
            .execute(augustus_core::EntityId(0), &compiled.plan)
            .unwrap();
        reference
            .execute(augustus_core::EntityId(0), &expected.plan)
            .unwrap();
        assert_eq!(actual, reference);
        assert_eq!(
            actual
                .events()
                .iter()
                .map(|e| e.2.as_str())
                .collect::<Vec<_>>(),
            ["begin", "owner", "country", "end"]
        );
        assert!(compiled.sources.len() > expected.sources.len());
    }
}

#[test]
fn hidden_event_phases_and_delayed_calls_share_existing_state_and_save_format() {
    let schema = Schema::new(["variable/phase"]).unwrap();
    let library = ScriptLibrary::new();
    let text = document(
        r#"
      country_event={id=group.first is_triggered_only=yes
        immediate={hidden_effect={set_variable={which=phase value=3} log=immediate}}
        option={name=accept hidden_effect={
          if={limit={check_variable={which=phase value=3}} log=option}
          set_variable={which=phase value=4}
          country_event={id=group.next days=1}
        }}
        after={hidden_effect={if={limit={check_variable={which=phase value=4}} log=after}}}
      }
      country_event={id=group.next is_triggered_only=yes
        trigger={check_variable={which=phase value=4}}
        immediate={hidden_effect={log=delayed}} option={name=accept}
      }
    "#,
    );
    let mut states = Vec::new();
    for optimise in [false, true] {
        let compiled = ScriptCompiler::new(&schema, &library, optimise)
            .compile_events(&text)
            .unwrap();
        let mut actual = world(&schema);
        actual
            .set_event_catalog(
                EventCatalog::new(compiled.definitions)
                    .unwrap()
                    .with_conditions(compiled.conditions)
                    .unwrap(),
            )
            .unwrap();
        let id = actual
            .trigger_event("group.first", augustus_core::EntityId(0))
            .unwrap();
        actual
            .choose_event(&EventChoice {
                pending: id,
                option: "0".into(),
            })
            .unwrap();
        let mut resumed = World::restore_with_events(
            &actual.save("groups:v1").unwrap(),
            "groups:v1",
            &schema,
            Vec::new(),
            actual.event_catalog().clone(),
        )
        .unwrap();
        actual.advance_to(1).unwrap();
        resumed.advance_to(1).unwrap();
        assert_eq!(actual, resumed);
        assert_eq!(
            actual
                .events()
                .iter()
                .map(|e| (e.0, e.2.as_str()))
                .collect::<Vec<_>>(),
            [
                (0, "immediate"),
                (0, "option"),
                (0, "after"),
                (1, "delayed")
            ]
        );
        states.push(actual.save("groups:v1").unwrap());
    }
    assert_eq!(states[0], states[1]);
}

#[test]
fn nested_execution_errors_preserve_the_same_prefix_and_stop_the_world() {
    let schema = Schema::new(["variable/phase"]).unwrap();
    let library = ScriptLibrary::new();
    let nested = document(
        "hidden_effect={log=before capital_scope={hidden_effect={log=capital}} log=unreachable}",
    );
    let flat = document("log=before capital_scope={log=capital} log=unreachable");
    let mut states = Vec::new();
    for text in [&nested, &flat] {
        let compiled = ScriptCompiler::new(&schema, &library, true)
            .compile(text, EntityKind::Country)
            .unwrap();
        let mut actual = World::new(schema.clone());
        let country = actual.add_entity("AAA", EntityKind::Country);
        assert!(actual.execute(country, &compiled.plan).is_err());
        assert_eq!(
            actual
                .events()
                .iter()
                .map(|e| e.2.as_str())
                .collect::<Vec<_>>(),
            ["before"]
        );
        assert!(actual.save("failed:v1").is_err());
        assert!(actual.advance_to(1).is_err());
        states.push(actual);
    }
    assert_eq!(states[0], states[1]);
}

#[test]
fn hidden_containers_validate_all_children_and_keep_inner_error_locations() {
    let schema = Schema::new(["treasury"]).unwrap();
    let mut library = ScriptLibrary::new();
    library
        .add_effects(&document("loop={hidden_effect={loop=yes}}"))
        .unwrap();
    for text in [
        "hidden_effect=yes",
        "hidden_effect={hidden_effect={unsupported=yes}}",
        "if={limit={always=no} hidden_effect={unsupported=yes}}",
        "hidden_effect={owner={add_treasury=1}}",
        "hidden_effect={if={limit={always=yes} log=inside}} else={log=outside}",
        "hidden_effect={loop=yes}",
    ] {
        let error = ScriptCompiler::new(&schema, &library, true)
            .compile(&document(text), EntityKind::Country)
            .unwrap_err();
        assert!(error.location().is_some(), "{text}");
        assert_eq!(
            error.path().to_string_lossy(),
            "self-authored-hidden-effects.txt"
        );
    }
    let error = ScriptCompiler::new(&schema, &library, true)
        .compile(
            &document("hidden_effect={\n hidden_effect={\n  unsupported=yes\n }\n}"),
            EntityKind::Country,
        )
        .unwrap_err();
    assert_eq!(error.location().unwrap().line, 3);
    assert_eq!(error.location().unwrap().column, 3);
}
