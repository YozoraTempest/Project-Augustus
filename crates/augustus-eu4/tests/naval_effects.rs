// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{EntityKind, Schema, ScopeContext, World};
use augustus_eu4::{ScriptCompiler, ScriptLibrary, TextDocument, TextEncoding};

fn document(text: &str) -> TextDocument {
    TextDocument::from_bytes(
        "self-authored.txt",
        text.as_bytes().to_vec(),
        TextEncoding::Utf8,
    )
}

#[test]
fn province_ship_effect_resolves_root_country_and_creates_a_ship() {
    let schema = Schema::new(std::iter::empty::<String>()).unwrap();
    let compiled = ScriptCompiler::new(&schema, &ScriptLibrary::new(), true)
        .compile_events(&document(
            "country_event={id=naval.1 is_triggered_only=yes immediate={1={light_ship=ROOT}} option={name=continue}}",
        ))
        .unwrap();
    let mut world = World::new(schema);
    let country = world.add_entity("SWE", EntityKind::Country);
    let province = world.add_entity("1", EntityKind::Province);
    world.set_province_has_port(province, true).unwrap();
    world
        .execute_in_context(
            ScopeContext {
                current: country,
                root: country,
            },
            &compiled.definitions[0].immediate,
        )
        .unwrap();

    assert_eq!(world.naval_units().len(), 1);
    assert_eq!(world.naval_units()[0].owner, country);
    assert_eq!(world.naval_units()[0].location, province);
    assert_eq!(world.naval_units()[0].unit_type, "light_ship");
}

#[test]
fn naval_creation_is_rejected_outside_province_scope_or_without_country_root() {
    let schema = Schema::new(std::iter::empty::<String>()).unwrap();
    let library = ScriptLibrary::new();
    for scope in [EntityKind::Country, EntityKind::Province] {
        assert!(
            ScriptCompiler::new(&schema, &library, true)
                .compile(&document("light_ship=ROOT"), scope)
                .is_err()
        );
    }
}
