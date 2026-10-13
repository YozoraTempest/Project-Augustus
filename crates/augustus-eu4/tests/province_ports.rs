// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{EntityKind, Schema, World};
use augustus_eu4::{ScriptCompiler, ScriptLibrary, TextDocument, TextEncoding, VanillaWorld};
use std::path::Path;

fn document(text: &str) -> TextDocument {
    TextDocument::from_bytes(
        "self-authored-port-test.txt",
        text.as_bytes().to_vec(),
        TextEncoding::Utf8,
    )
}

#[test]
fn province_port_trigger_checks_explicit_map_topology() {
    let schema = Schema::new(Vec::<String>::new()).unwrap();
    let library = ScriptLibrary::new();
    let script = ScriptCompiler::new(&schema, &library, false)
        .compile(
            &document(
                "if = { limit = { has_port = yes } log = coastal }\n\
                 if = { limit = { has_port = no } log = inland }",
            ),
            EntityKind::Province,
        )
        .unwrap();

    let mut world = World::new(schema);
    let coast = world.add_entity("coast", EntityKind::Province);
    let inland = world.add_entity("inland", EntityKind::Province);
    world.set_province_has_port(coast, true).unwrap();
    world.set_province_has_port(inland, false).unwrap();
    world.execute(coast, &script.plan).unwrap();
    world.execute(inland, &script.plan).unwrap();

    assert_eq!(
        world
            .events()
            .iter()
            .map(|event| event.2.as_str())
            .collect::<Vec<_>>(),
        ["coastal", "inland"]
    );
    assert!(
        ScriptCompiler::new(world.schema(), &library, false)
            .compile(&document("has_port = yes"), EntityKind::Country)
            .is_err()
    );
}

#[test]
fn vanilla_adapter_derives_and_retains_port_presence_from_the_province_bitmap() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world");
    let loaded = VanillaWorld::load(root, "1444.11.11").unwrap();

    assert_eq!(loaded.initial.province(1).unwrap().has_port, Some(false));
    assert_eq!(loaded.initial.province(2).unwrap().has_port, Some(true));
    assert_eq!(loaded.initial.province(3).unwrap().has_port, Some(false));
    assert_eq!(loaded.initial.province(4).unwrap().has_port, Some(false));
}
