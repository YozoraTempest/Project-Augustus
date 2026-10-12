// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{EntityId, EntityKind, Schema, World};
use augustus_eu4::{ScriptCompiler, ScriptLibrary, TextDocument, TextEncoding};

fn document(text: &str) -> TextDocument {
    TextDocument::from_bytes(
        "self-authored-faith.txt",
        text.as_bytes().to_vec(),
        TextEncoding::Utf8,
    )
}

#[test]
fn religion_conditions_read_country_province_and_owner_state_without_folding() {
    let schema = Schema::new(["value"]).unwrap();
    let mut library = ScriptLibrary::new();
    library
        .add_triggers(&document("same_family={religion_group=north}"))
        .unwrap();
    let text = document(
        r#"
        if={limit={religion=alpha same_family=yes NOT={religion=beta}} log=country}
        capital_scope={if={limit={religion=gamma religion_group=south} log=province}
            owner={if={limit={religion=alpha religion_group=north} log=owner}}}
    "#,
    );
    let mut results = Vec::new();
    for optimise in [false, true] {
        let script = ScriptCompiler::new(&schema, &library, optimise)
            .compile(&text, EntityKind::Country)
            .unwrap();
        assert_eq!(script.folded_branches, 0);
        assert!(script.sources.iter().any(|(path, location)| {
            path.ends_with("self-authored-faith.txt") && location.line == 2
        }));
        let mut world = World::new(schema.clone());
        world.add_entity("AAA", EntityKind::Country);
        world.add_entity("Port", EntityKind::Province);
        world.set_owner(EntityId(1), EntityId(0)).unwrap();
        world.set_capital(EntityId(0), EntityId(1)).unwrap();
        world
            .set_affiliations(
                serde_json::from_str(
                    r#"{"religion":{
            "groups":{"alpha":"north","beta":"north","gamma":"south"},
            "members":{"0":"alpha","1":"gamma"},"acceptance":{}}}"#,
                )
                .unwrap(),
            )
            .unwrap();
        world.execute(EntityId(0), &script.plan).unwrap();
        assert_eq!(
            world
                .events()
                .iter()
                .map(|event| event.2.as_str())
                .collect::<Vec<_>>(),
            ["country", "province", "owner"]
        );
        let mut affiliations = world.affiliations().clone();
        affiliations
            .0
            .get_mut("religion")
            .unwrap()
            .members
            .insert(EntityId(0), Some("beta".into()));
        world.set_affiliations(affiliations).unwrap();
        world.execute(EntityId(0), &script.plan).unwrap();
        assert_eq!(
            world
                .events()
                .iter()
                .map(|event| event.2.as_str())
                .collect::<Vec<_>>(),
            ["country", "province", "owner", "province"]
        );
        results.push(world);
    }
    assert_eq!(results[0], results[1]);
}

#[test]
fn dynamic_special_and_block_operands_report_their_source_even_in_unselected_branches() {
    let schema = Schema::new(["value"]).unwrap();
    let library = ScriptLibrary::new();
    for trigger in ["religion", "religion_group"] {
        for value in [
            "\"\"",
            "\" \"",
            "ROOT",
            "FROM",
            "PREV",
            "THIS",
            "HAB",
            "134",
            "secondary",
            "dominant",
            "heretic",
            "heathen",
            "event_target:faith",
            "@faith",
            "$FAITH$",
            "\"[Root.Religion]\"",
            "{}",
        ] {
            let text = document(&format!(
                "if={{limit={{always=no {trigger}={value}}} log=unused}}"
            ));
            let error = ScriptCompiler::new(&schema, &library, true)
                .compile(&text, EntityKind::Province)
                .unwrap_err()
                .to_string();
            assert!(
                error.contains("self-authored-faith.txt:1:"),
                "{value}: {error}"
            );
        }
    }
}

#[test]
fn compiled_unknown_state_does_not_turn_into_a_negative_match() {
    let schema = Schema::new(["value"]).unwrap();
    let library = ScriptLibrary::new();
    let script = ScriptCompiler::new(&schema, &library, true)
        .compile(
            &document("if={limit={NOT={religion_group=north}} log=must_not_run}"),
            EntityKind::Country,
        )
        .unwrap();
    let mut world = World::new(schema);
    world.add_entity("AAA", EntityKind::Country);
    let error = world
        .execute(EntityId(0), &script.plan)
        .unwrap_err()
        .to_string();
    assert!(error.contains("unknown affiliation group religion"));
    assert!(world.events().is_empty());
}
