// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{EntityId, EntityKind, EventCatalog, FieldId, Schema, World};
use augustus_eu4::{ScriptCompiler, ScriptLibrary, TextDocument, TextEncoding};

fn document(text: &str) -> TextDocument {
    TextDocument::from_bytes(
        "self-authored-pictures.txt",
        text.as_bytes().to_vec(),
        TextEncoding::Utf8,
    )
}

fn compile(text: &str, optimise: bool) -> augustus_eu4::CompiledEvents {
    ScriptCompiler::new(
        &Schema::new(["treasury"]).unwrap(),
        &ScriptLibrary::new(),
        optimise,
    )
    .compile_events(&document(text))
    .unwrap()
}

#[test]
fn scalar_and_conditional_pictures_preserve_order_and_first_declaration_fallback() {
    let text = "country_event={id=a is_triggered_only=yes picture={trigger={always=no} picture=first} picture={trigger={always=yes} picture=second} picture=third option={name=ok}}
        country_event={id=b is_triggered_only=yes picture={trigger={always=no} picture=first} picture={trigger={always=no} picture=second} option={name=ok}}
        country_event={id=c is_triggered_only=yes picture=first picture={trigger={always=yes} picture=second} option={name=ok}}";
    for optimise in [false, true] {
        let compiled = compile(text, optimise);
        assert_eq!(
            compiled.pictures["a"]
                .variants
                .iter()
                .map(|v| v.key.as_str())
                .collect::<Vec<_>>(),
            ["first", "second", "third"]
        );
        let mut world = World::new(Schema::new(["treasury"]).unwrap());
        let country = world.add_entity("AAA", EntityKind::Country);
        world
            .set_event_catalog(
                EventCatalog::new(compiled.definitions)
                    .unwrap()
                    .with_pictures(compiled.pictures)
                    .unwrap(),
            )
            .unwrap();
        for (event, expected) in [("a", "second"), ("b", "first"), ("c", "first")] {
            let id = world.trigger_event(event, country).unwrap();
            assert_eq!(world.pending_event_picture(id).unwrap(), Some(expected));
        }
    }
}

#[test]
fn country_and_province_picture_conditions_read_post_immediate_state() {
    let text = "country_event={id=a is_triggered_only=yes picture={trigger={has_country_flag=ready} picture=ready} picture=other immediate={set_country_flag=ready} option={name=ok}}
        province_event={id=b is_triggered_only=yes picture={trigger={owner={has_country_flag=ready}} picture=owner_ready} picture=other option={name=ok}}";
    let mut saves = vec![];
    for optimise in [false, true] {
        let compiled = compile(text, optimise);
        let mut world = World::new(Schema::new(["treasury"]).unwrap());
        let country = world.add_entity("AAA", EntityKind::Country);
        let province = world.add_entity("1", EntityKind::Province);
        world.set_owner(province, country).unwrap();
        world
            .set_event_catalog(
                EventCatalog::new(compiled.definitions)
                    .unwrap()
                    .with_pictures(compiled.pictures)
                    .unwrap(),
            )
            .unwrap();
        let first = world.trigger_event("a", country).unwrap();
        let second = world.trigger_event("b", province).unwrap();
        world.set_marker(country, "ready", false).unwrap();
        assert_eq!(world.pending_event_picture(first).unwrap(), Some("ready"));
        assert_eq!(
            world.pending_event_picture(second).unwrap(),
            Some("owner_ready")
        );
        saves.push(world.save("compiled").unwrap());
    }
    assert_eq!(saves[0], saves[1]);
}

#[test]
fn unsupported_and_malformed_picture_declarations_keep_source_locations() {
    for picture in [
        "picture=\"\"",
        "picture={}",
        "picture={trigger={always=yes}}",
        "picture={picture=key}",
        "picture={trigger={always=yes} trigger={} picture=key}",
        "picture={trigger={} picture=a picture=b}",
        "picture={trigger={} picture=key weight=1}",
        "picture={trigger={unsupported=yes} picture=key}",
        "picture={trigger={} picture={}}",
        "picture={trigger=yes picture=key}",
        "picture > key",
    ] {
        let text = format!(
            "country_event={{\nid=a is_triggered_only=yes\n{picture}\noption={{name=ok}}\n}}"
        );
        let error = ScriptCompiler::new(
            &Schema::new(["treasury"]).unwrap(),
            &ScriptLibrary::new(),
            true,
        )
        .compile_events(&document(&text))
        .unwrap_err();
        assert!(error.location().is_some(), "{picture}: {error}");
        assert!(error.to_string().contains("self-authored-pictures.txt"));
    }
    assert!(ScriptCompiler::new(&Schema::new(["treasury"]).unwrap(), &ScriptLibrary::new(), true).compile_events(&document("country_event={id=a is_triggered_only=yes picture=first picture={trigger={unsupported=yes} picture=later} option={name=ok}}" )).is_err());
}

#[test]
fn repeatable_picture_fields_do_not_allow_other_duplicate_event_metadata() {
    for field in ["title", "desc", "id", "immediate", "trigger"] {
        let pair = if matches!(field, "immediate" | "trigger") {
            format!("{field}={{}} {field}={{}}")
        } else {
            format!("{field}=a {field}=b")
        };
        assert!(
            ScriptCompiler::new(
                &Schema::new(["treasury"]).unwrap(),
                &ScriptLibrary::new(),
                true
            )
            .compile_events(&document(&format!(
                "country_event={{id=x is_triggered_only=yes {pair} option={{name=ok}}}}"
            )))
            .is_err()
        );
    }
}

#[test]
fn compilation_does_not_cache_state_dependent_picture_conditions() {
    let compiled = compile(
        "country_event={id=a is_triggered_only=yes picture={trigger={treasury=1} picture=rich} picture=poor option={name=ok}}",
        true,
    );
    let mut world = World::new(Schema::new(["treasury"]).unwrap());
    world.add_entity("AAA", EntityKind::Country);
    world
        .set_event_catalog(
            EventCatalog::new(compiled.definitions)
                .unwrap()
                .with_pictures(compiled.pictures)
                .unwrap(),
        )
        .unwrap();
    let first = world.trigger_event("a", EntityId(0)).unwrap();
    world.set(EntityId(0), FieldId(0), 1000).unwrap();
    let second = world.trigger_event("a", EntityId(0)).unwrap();
    assert_eq!(world.pending_event_picture(first).unwrap(), Some("poor"));
    assert_eq!(world.pending_event_picture(second).unwrap(), Some("rich"));
}
