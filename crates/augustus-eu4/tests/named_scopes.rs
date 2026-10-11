// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{EntityId, EntityKind, FieldId, Politics, Schema, ScopeSelector, World};
use augustus_eu4::{ScriptCompiler, ScriptLibrary, TextDocument, TextEncoding};

fn document(text: &str) -> TextDocument {
    TextDocument::from_bytes(
        "self-authored-scopes.txt",
        text.as_bytes().to_vec(),
        TextEncoding::Utf8,
    )
}

#[test]
fn compiler_binds_office_and_typed_global_scopes_to_shared_core_actions() {
    let schema = Schema::new(["variable/seen"]).unwrap();
    let library = ScriptLibrary::new();
    let types = [
        ("holder".into(), EntityKind::Country),
        ("port".into(), EntityKind::Province),
    ]
    .into();
    let compiled = ScriptCompiler::new(&schema, &library, true).with_global_scope_types(types)
        .compile(&document("emperor = { save_global_event_target_as = holder } event_target:holder = { set_variable = { which = seen value = 1 } capital_scope = { save_global_event_target_as = port } } event_target:port = { change_variable = { which = seen value = 2 } } clear_global_event_target = holder event_target:holder = { log = incorrect }"), EntityKind::Country).unwrap();
    let mut world = World::new(schema);
    let country = world.add_entity("Union", EntityKind::Country);
    let port = world.add_entity("Port", EntityKind::Province);
    world.set_owner(port, country).unwrap();
    world.set_capital(country, port).unwrap();
    world
        .set_politics(Politics {
            offices: [("imperial_emperor".into(), country)].into(),
            ..Default::default()
        })
        .unwrap();
    world.execute(country, &compiled.plan).unwrap();
    assert_eq!(world.value(country, FieldId(0)).unwrap(), 1000);
    assert_eq!(world.value(port, FieldId(0)).unwrap(), 2000);
    assert_eq!(
        world
            .resolve_scope(&ScopeSelector::Global("port".into()))
            .unwrap(),
        Some(port)
    );
    assert_eq!(
        world
            .resolve_scope(&ScopeSelector::Global("holder".into()))
            .unwrap(),
        None
    );
    assert!(world.events().is_empty());
    assert!(!compiled.sources.is_empty());
}

#[test]
fn unresolved_types_and_invalid_bindings_report_locations_without_guessing() {
    let schema = Schema::new(["treasury"]).unwrap();
    let library = ScriptLibrary::new();
    for text in [
        "event_target:undeclared = { log = invalid }",
        "event_target: = { log = invalid }",
        "save_global_event_target_as = \"\"",
        "clear_global_event_target = {}",
        "capital_scope = { save_global_event_target_as = holder }",
        "event_target:port = { country_event = { id = example.1 } }",
    ] {
        let error = ScriptCompiler::new(&schema, &library, true)
            .with_global_scope_types(
                [
                    ("holder".into(), EntityKind::Country),
                    ("port".into(), EntityKind::Province),
                ]
                .into(),
            )
            .compile(&document(text), EntityKind::Country)
            .unwrap_err();
        assert!(error.location().is_some(), "{text}");
    }
    let mut world = World::new(schema);
    world.add_entity("Union", EntityKind::Country);
    // Native interfaces likewise reject an invalid entity before inserting a binding.
    assert!(world.bind_global_scope("holder", EntityId(10)).is_err());
}
