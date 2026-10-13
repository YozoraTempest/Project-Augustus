// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{EntityId, EntityKind, EventCatalog, Schema, World};
use augustus_eu4::{ScriptCompiler, ScriptLibrary, TextDocument, TextEncoding};
fn document(text: &str) -> TextDocument {
    TextDocument::from_bytes(
        "self-authored-locations.txt",
        text.as_bytes().to_vec(),
        TextEncoding::Utf8,
    )
}

#[test]
fn literal_locations_translate_as_metadata_for_country_and_province_events() {
    let schema = Schema::new(["unused"]).unwrap();
    let library = ScriptLibrary::new();
    for optimise in [false, true] {
        let compiled=ScriptCompiler::new(&schema,&library,optimise).compile_events(&document(
            "country_event={id=a is_triggered_only=yes goto=008 option={name=visit goto=7} option={name=stay}}\nprovince_event={id=b is_triggered_only=yes option={name=visit goto=8}}"
        )).unwrap();
        assert_eq!(compiled.locations["a"].event.as_deref(), Some("8"));
        assert_eq!(compiled.locations["a"].options["0"], "7");
        assert_eq!(compiled.locations["b"].event, None);
        assert!(
            compiled
                .definitions
                .iter()
                .all(|event| event.immediate.is_empty()
                    && event.after.is_empty()
                    && event.options.iter().all(|option| option.effects.is_empty()))
        );
        let mut world = World::new(schema.clone());
        let country = world.add_entity("AAA", EntityKind::Country);
        let province = world.add_entity("7", EntityKind::Province);
        world.add_entity("8", EntityKind::Province);
        world
            .set_event_catalog(
                EventCatalog::new(compiled.definitions)
                    .unwrap()
                    .with_locations(compiled.locations)
                    .unwrap(),
            )
            .unwrap();
        let a = world.trigger_event("a", country).unwrap();
        let b = world.trigger_event("b", province).unwrap();
        assert_eq!(world.pending_event_location(a).unwrap(), Some(EntityId(2)));
        assert_eq!(
            world.pending_event_option_location(a, "0").unwrap(),
            Some(province)
        );
        assert_eq!(world.pending_event_location(b).unwrap(), None);
        assert_eq!(
            world.pending_event_option_location(b, "0").unwrap(),
            Some(EntityId(2))
        );
    }
}

#[test]
fn invalid_or_contextual_goto_is_reported_at_the_source_and_never_ignored() {
    let schema = Schema::new(["unused"]).unwrap();
    let library = ScriptLibrary::new();
    for placement in ["event", "option"] {
        for value in [
            "0",
            "-1",
            "4294967296",
            "ROOT",
            "FROM",
            "some_target",
            "event_target:place",
            "estate_nobles",
            "{ province=7 }",
        ] {
            let fields = if placement == "event" {
                format!("goto={value} option={{name=ok}}")
            } else {
                format!("option={{name=ok goto={value}}}")
            };
            let error = ScriptCompiler::new(&schema, &library, true)
                .compile_events(&document(&format!(
                    "country_event={{id=a is_triggered_only=yes {fields}}}"
                )))
                .unwrap_err();
            assert!(
                error.to_string().contains("self-authored-locations.txt"),
                "{error}"
            );
        }
    }
    for body in [
        "goto=7 goto=8 option={name=ok}",
        "option={name=ok goto=7 goto=8}",
    ] {
        let error = ScriptCompiler::new(&schema, &library, true)
            .compile_events(&document(&format!(
                "country_event={{id=a is_triggered_only=yes {body}}}"
            )))
            .unwrap_err();
        assert!(error.to_string().contains("duplicate"), "{error}");
    }
}
