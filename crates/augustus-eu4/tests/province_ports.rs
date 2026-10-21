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
fn province_terrain_trigger_reads_the_core_value_and_rejects_other_scopes() {
    let schema = Schema::new(Vec::<String>::new()).unwrap();
    let library = ScriptLibrary::new();
    let script = ScriptCompiler::new(&schema, &library, false)
        .compile(
            &document("if = { limit = { has_terrain = steppe } log = steppe }"),
            EntityKind::Province,
        )
        .unwrap();

    let mut world = World::new(schema);
    let steppe = world.add_entity("steppe", EntityKind::Province);
    let hills = world.add_entity("hills", EntityKind::Province);
    let unknown = world.add_entity("unknown", EntityKind::Province);
    world
        .set_province_terrain(steppe, Some("steppe".into()))
        .unwrap();
    world
        .set_province_terrain(hills, Some("hills".into()))
        .unwrap();
    world.execute(steppe, &script.plan).unwrap();
    world.execute(hills, &script.plan).unwrap();
    assert!(world.execute(unknown, &script.plan).is_err());
    assert_eq!(world.events().len(), 1);

    assert!(
        ScriptCompiler::new(world.schema(), &library, false)
            .compile(&document("has_terrain = steppe"), EntityKind::Country)
            .is_err()
    );
}

#[test]
fn center_of_trade_trigger_uses_a_minimum_level_and_requires_province_scope() {
    let schema = Schema::new(Vec::<String>::new()).unwrap();
    let library = ScriptLibrary::new();
    let script = ScriptCompiler::new(&schema, &library, false)
        .compile(
            &document(
                "if = { limit = { province_has_center_of_trade_of_level = 1 } log = any }\n\
                 if = { limit = { province_has_center_of_trade_of_level = 2 } log = two }\n\
                 if = { limit = { province_has_center_of_trade_of_level = 3 } log = three }",
            ),
            EntityKind::Province,
        )
        .unwrap();

    let mut world = World::new(schema);
    let none = world.add_entity("none", EntityKind::Province);
    let level_one = world.add_entity("level-one", EntityKind::Province);
    let level_two = world.add_entity("level-two", EntityKind::Province);
    let level_three = world.add_entity("level-three", EntityKind::Province);
    let unknown = world.add_entity("unknown", EntityKind::Province);
    for (province, level) in [
        (none, Some(0)),
        (level_one, Some(1)),
        (level_two, Some(2)),
        (level_three, Some(3)),
        (unknown, None),
    ] {
        world
            .set_province_center_of_trade_level(province, level)
            .unwrap();
    }
    world.execute(none, &script.plan).unwrap();
    world.execute(level_one, &script.plan).unwrap();
    world.execute(level_two, &script.plan).unwrap();
    world.execute(level_three, &script.plan).unwrap();
    assert!(world.execute(unknown, &script.plan).is_err());
    assert_eq!(
        world
            .events()
            .iter()
            .map(|event| event.2.as_str())
            .collect::<Vec<_>>(),
        ["any", "any", "two", "any", "two", "three"]
    );
    assert!(
        ScriptCompiler::new(world.schema(), &library, false)
            .compile(
                &document("province_has_center_of_trade_of_level = 1"),
                EntityKind::Country,
            )
            .is_err()
    );
    assert!(
        ScriptCompiler::new(world.schema(), &library, false)
            .compile(
                &document("province_has_center_of_trade_of_level = 0"),
                EntityKind::Province,
            )
            .is_err()
    );
}

#[test]
fn center_of_trade_effects_set_and_adjust_the_live_level() {
    let schema = Schema::new(Vec::<String>::new()).unwrap();
    let library = ScriptLibrary::new();
    let script = ScriptCompiler::new(&schema, &library, false)
        .compile(
            &document(
                "center_of_trade = 3\n\
                 add_center_of_trade_level = -1\n\
                 if = { limit = { province_has_center_of_trade_of_level = 2 } log = level_two }",
            ),
            EntityKind::Province,
        )
        .unwrap();
    let capped = ScriptCompiler::new(&schema, &library, false)
        .compile(
            &document("add_center_of_trade_level = 3"),
            EntityKind::Province,
        )
        .unwrap();

    let mut world = World::new(schema.clone());
    let province = world.add_entity("province", EntityKind::Province);
    world
        .set_province_center_of_trade_level(province, Some(0))
        .unwrap();
    world.execute(province, &script.plan).unwrap();
    assert_eq!(
        world.province_center_of_trade_level(province).unwrap(),
        Some(2)
    );
    assert_eq!(world.events()[0].2, "level_two");

    world
        .set_province_center_of_trade_level(province, Some(1))
        .unwrap();
    world.execute(province, &capped.plan).unwrap();
    assert_eq!(
        world.province_center_of_trade_level(province).unwrap(),
        Some(3)
    );

    assert!(
        ScriptCompiler::new(&schema, &library, false)
            .compile(&document("center_of_trade = 2"), EntityKind::Country)
            .is_err()
    );
    for invalid in ["0", "4"] {
        assert!(
            ScriptCompiler::new(&schema, &library, false)
                .compile(
                    &document(&format!("center_of_trade = {invalid}")),
                    EntityKind::Province,
                )
                .is_err()
        );
    }

    let unknown_script = ScriptCompiler::new(&schema, &library, false)
        .compile(
            &document("add_center_of_trade_level = 1"),
            EntityKind::Province,
        )
        .unwrap();
    let mut unknown_world = World::new(schema);
    let unknown = unknown_world.add_entity("unknown", EntityKind::Province);
    assert!(
        unknown_world
            .execute(unknown, &unknown_script.plan)
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
    assert_eq!(
        loaded.initial.province_center_of_trade_level(1).unwrap(),
        Some(0)
    );
    assert_eq!(
        loaded.initial.province_center_of_trade_level(2).unwrap(),
        Some(2)
    );
    assert_eq!(
        loaded.initial.province_center_of_trade_level(3).unwrap(),
        None
    );

    let later = VanillaWorld::load(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world"),
        "1445.1.1",
    )
    .unwrap();
    assert_eq!(
        later.initial.province_center_of_trade_level(2).unwrap(),
        Some(0)
    );
}
