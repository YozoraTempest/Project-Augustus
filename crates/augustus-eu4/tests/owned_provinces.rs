// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{EntityId, EntityKind, ModifierRenewal, Modifiers, Schema, Target, World};
use augustus_eu4::{ScriptCompiler, ScriptLibrary, TextDocument, TextEncoding};

fn document(text: &str) -> TextDocument {
    TextDocument::from_bytes(
        "self-authored-owned-provinces.txt",
        text.as_bytes().to_vec(),
        TextEncoding::Utf8,
    )
}

fn modifiers() -> Modifiers {
    serde_json::from_str(r#"{"definitions":{"local":{"values_micros":{},"flags":{}}},"active":{}}"#)
        .unwrap()
}

fn world(schema: Schema) -> World {
    let mut world = World::new(schema);
    world.add_entity("AAA", EntityKind::Country);
    world.add_entity("BBB", EntityKind::Country);
    for name in ["3", "1", "2", "Sea"] {
        world.add_entity(name, EntityKind::Province);
    }
    world.set_owner(EntityId(2), EntityId(0)).unwrap();
    world.set_owner(EntityId(3), EntityId(1)).unwrap();
    world.set_owner(EntityId(4), EntityId(0)).unwrap();
    world
        .mark_target_absent(EntityId(5), Target::Owner)
        .unwrap();
    world.set_capital(EntityId(0), EntityId(2)).unwrap();
    world.set_modifiers(modifiers()).unwrap();
    world
}

#[test]
fn owned_conditions_follow_transfer_removal_and_nested_owner_scope() {
    let schema = Schema::new(["unused"]).unwrap();
    let library = ScriptLibrary::new();
    let mut results = Vec::new();
    for optimise in [false, true] {
        let compiler = || {
            ScriptCompiler::new(&schema, &library, optimise)
                .with_modifier_ids(modifiers().definitions.keys().cloned().collect())
        };
        let plan = compiler()
            .compile(
                &document(
                    r#"
            if={limit={any_owned_province={province_id=3 has_province_modifier=local}} log=present}
            if={limit={NOT={any_owned_province={province_id=1}}} log=foreign}
            capital_scope={owner={if={limit={any_owned_province={province_id=3}} log=nested}}}
            3={remove_province_modifier=local}
            if={limit={NOT={any_owned_province={has_province_modifier=local}}} log=removed}
            2={set_owner=BBB owner={if={limit={any_owned_province={province_id=2}} log=transferred}}}
        "#,
                ),
                EntityKind::Country,
            )
            .unwrap()
            .plan;
        let query = compiler()
            .compile(
                &document(
                    "if={limit={any_owned_province={province_id=2}} log=here} else={log=away}",
                ),
                EntityKind::Country,
            )
            .unwrap()
            .plan;
        let mut state = world(schema.clone());
        state
            .attach_modifier(EntityId(2), "local", None, None, ModifierRenewal::Replace)
            .unwrap();
        state.execute(EntityId(0), &plan).unwrap();
        let mut restored = World::restore(
            &state.save("owned-compat").unwrap(),
            "owned-compat",
            &schema,
        )
        .unwrap();
        for value in [&mut state, &mut restored] {
            value.execute(EntityId(0), &query).unwrap();
            value.execute(EntityId(1), &query).unwrap();
            assert_eq!(
                value
                    .events()
                    .iter()
                    .map(|row| row.2.as_str())
                    .collect::<Vec<_>>(),
                [
                    "present",
                    "foreign",
                    "nested",
                    "removed",
                    "transferred",
                    "away",
                    "here"
                ]
            );
        }
        assert_eq!(state, restored);
        results.push(state);
    }
    assert_eq!(results[0], results[1]);
}

#[test]
fn set_owner_requires_a_province_scope() {
    let schema = Schema::new([] as [&str; 0]).unwrap();
    let library = ScriptLibrary::new();
    let compiler = |scope| {
        ScriptCompiler::new(&schema, &library, true).compile(&document("set_owner=BBB"), scope)
    };

    assert!(compiler(EntityKind::Province).is_ok());
    assert!(
        compiler(EntityKind::Country)
            .unwrap_err()
            .to_string()
            .contains("set_owner requires province scope")
    );
}

#[test]
fn owned_query_scope_and_body_are_checked_even_when_unreachable() {
    let schema = Schema::new(["unused"]).unwrap();
    let library = ScriptLibrary::new();
    for (scope, text) in [
        (
            EntityKind::Province,
            "if={limit={any_owned_province={always=yes}} log=wrong}",
        ),
        (EntityKind::Country, "if={limit={development=30} log=wrong}"),
        (
            EntityKind::Country,
            "if={limit={any_owned_province=yes} log=wrong}",
        ),
        (
            EntityKind::Country,
            "if={limit={any_owned_province={has_country_modifier=local}} log=wrong}",
        ),
        (
            EntityKind::Country,
            "if={limit={always=no any_owned_province={unknown_predicate=yes}} log=wrong}",
        ),
        (
            EntityKind::Country,
            "if={limit={any_owned_province={limit={always=yes}}} log=wrong}",
        ),
    ] {
        for optimise in [false, true] {
            let error = ScriptCompiler::new(&schema, &library, optimise)
                .with_modifier_ids(modifiers().definitions.keys().cloned().collect())
                .compile(&document(text), scope)
                .unwrap_err();
            assert!(error.location().is_some(), "{text}");
            assert_eq!(
                error.path().to_string_lossy(),
                "self-authored-owned-provinces.txt"
            );
        }
    }
}

#[test]
fn negated_owned_query_keeps_unknown_membership_as_an_error() {
    let schema = Schema::new(["unused"]).unwrap();
    let library = ScriptLibrary::new();
    let plan = ScriptCompiler::new(&schema, &library, true)
        .compile(
            &document("if={limit={NOT={any_owned_province={province_id=1}}} log=wrong}"),
            EntityKind::Country,
        )
        .unwrap()
        .plan;
    let mut state = world(schema);
    state.add_entity("Unknown", EntityKind::Province);
    assert!(state.execute(EntityId(0), &plan).is_err());
    assert!(state.events().is_empty());
    assert!(state.save("unknown-owned").is_err());
}
