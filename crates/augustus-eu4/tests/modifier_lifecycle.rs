// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    Action, EntityId, EntityKind, EventCatalog, EventChoice, ModifierRenewal, Modifiers,
    PendingEventId, Schema, Target, World,
};
use augustus_eu4::{ScriptCompiler, ScriptLibrary, TextDocument, TextEncoding};

fn document(text: &str) -> TextDocument {
    TextDocument::from_bytes(
        "self-authored-modifiers.txt",
        text.as_bytes().to_vec(),
        TextEncoding::Utf8,
    )
}

fn definitions() -> Modifiers {
    serde_json::from_str(r#"{"definitions":{"boost":{"values_micros":{"global_tax_modifier":125000},"flags":{}},"guard":{"values_micros":{},"flags":{}}},"active":{}}"#).unwrap()
}

#[test]
fn country_modifiers_translate_renewal_scope_and_due_predicates_to_shared_state() {
    let schema = Schema::new(["unused"]).unwrap();
    let library = ScriptLibrary::new();
    let mut results = Vec::new();
    for optimise in [false, true] {
        let compiler = || {
            ScriptCompiler::new(&schema, &library, optimise)
                .with_modifier_ids(definitions().definitions.keys().cloned().collect())
        };
        let plan = compiler()
            .compile(
                &document(
                    r#"
          if={limit={NOT={has_country_modifier=boost}} log=fresh}
          add_country_modifier={name=boost duration=4}
          add_country_modifier={name=boost duration=2 hidden=yes}
          add_country_modifier={name=guard duration=-1}
          capital_scope={owner={if={limit={has_country_modifier=boost} log=owner}}}
          country_event={id=sample.control}
          country_event={id=sample.expired days=4}
        "#,
                ),
                EntityKind::Country,
            )
            .unwrap()
            .plan;
        let compiled=compiler().compile_events(&document(r#"
          country_event={id=sample.control is_triggered_only=yes option={name=accept add_country_modifier={name=boost duration=-1}} after={if={limit={has_country_modifier=boost} log=after}}}
          country_event={id=sample.expired is_triggered_only=yes trigger={NOT={has_country_modifier=boost}} immediate={log=expired} option={name=accept remove_country_modifier=guard}}
        "#)).unwrap();
        let catalog = EventCatalog::new(compiled.definitions)
            .unwrap()
            .with_conditions(compiled.conditions)
            .unwrap();
        let mut world = World::new(schema.clone());
        world.add_entity("AAA", EntityKind::Country);
        world.add_entity("BBB", EntityKind::Country);
        world.add_entity("Port", EntityKind::Province);
        world.set_owner(EntityId(2), EntityId(0)).unwrap();
        world.set_capital(EntityId(0), EntityId(2)).unwrap();
        world.set_modifiers(definitions()).unwrap();
        world.set_event_catalog(catalog.clone()).unwrap();
        world.execute(EntityId(0), &plan).unwrap();
        world
            .choose_event(&EventChoice {
                pending: PendingEventId(0),
                option: "0".into(),
            })
            .unwrap();
        world.advance_to(2).unwrap();
        assert!(world.has_modifier(EntityId(0), "boost").unwrap());
        assert!(!world.has_modifier(EntityId(1), "boost").unwrap());
        assert!(!world.has_modifier(EntityId(2), "boost").unwrap());
        assert_eq!(
            world.modifiers().declared_totals(EntityId(0)).unwrap()["global_tax_modifier"],
            125000
        );
        let saved = world.save("modifiers").unwrap();
        let mut resumed =
            World::restore_with_events(&saved, "modifiers", &schema, vec![], catalog).unwrap();
        for state in [&mut world, &mut resumed] {
            state.advance_to(4).unwrap();
            state
                .choose_event(&EventChoice {
                    pending: PendingEventId(1),
                    option: "0".into(),
                })
                .unwrap();
            assert!(!state.has_modifier(EntityId(0), "boost").unwrap());
            assert!(!state.has_modifier(EntityId(0), "guard").unwrap());
            assert_eq!(
                state
                    .events()
                    .iter()
                    .map(|e| (e.0, e.2.as_str()))
                    .collect::<Vec<_>>(),
                [(0, "fresh"), (0, "owner"), (0, "after"), (4, "expired")]
            );
        }
        assert_eq!(world, resumed);
        results.push(world);
    }
    assert_eq!(results[0], results[1]);
}

#[test]
fn zero_and_omitted_duration_normalize_to_one_day_and_unknown_literal_query_is_false() {
    let schema = Schema::new(["unused"]).unwrap();
    let library = ScriptLibrary::new();
    for input in [
        "add_country_modifier={name=boost}",
        "add_country_modifier={name=boost duration=0}",
    ] {
        let compiled = ScriptCompiler::new(&schema, &library, true)
            .with_modifier_ids(definitions().definitions.keys().cloned().collect())
            .compile(&document(input), EntityKind::Country)
            .unwrap();
        assert_eq!(
            compiled.plan,
            [Action::AttachModifier {
                target: Target::Current,
                name: "boost".into(),
                duration: Some(1),
                hidden: None,
                renewal: ModifierRenewal::KeepLatestDated,
                duration_label: None,
            }]
        );
        let mut world = World::new(schema.clone());
        world.add_entity("AAA", EntityKind::Country);
        world.set_modifiers(definitions()).unwrap();
        world.execute(EntityId(0), &compiled.plan).unwrap();
        assert!(world.has_modifier(EntityId(0), "boost").unwrap());
        world.advance_to(1).unwrap();
        assert!(!world.has_modifier(EntityId(0), "boost").unwrap());
    }
    let plan = ScriptCompiler::new(&schema, &library, true)
        .compile(
            &document("if={limit={has_country_modifier=missing} log=wrong} else={log=missing}"),
            EntityKind::Country,
        )
        .unwrap()
        .plan;
    assert_eq!(plan, [Action::Emit("missing".into())]);
}

#[test]
fn malformed_and_unverified_modifier_forms_keep_source_locations() {
    let schema = Schema::new(["unused"]).unwrap();
    let library = ScriptLibrary::new();
    for input in [
        "add_country_modifier={name=missing duration=1}",
        "add_country_modifier={name=boost duration=-2}",
        "add_country_modifier={name=boost duration=1.5}",
        "add_country_modifier={name=boost duration=variable}",
        "add_country_modifier={name=boost name=guard}",
        "add_country_modifier={name=boost hidden=yes hidden=no}",
        "add_country_modifier={name=boost message=unverified}",
        "add_country_modifier={name=boost desc={key=duration}}",
        "add_country_modifier={name=boost desc=\"\"}",
        "add_country_modifier={name=boost desc=$duration$}",
        "add_country_modifier={name=boost desc=duration@ROOT}",
        "add_country_modifier={name=boost desc=\"[Root.GetName]\"}",
        "add_country_modifier={name=boost desc=one desc=two}",
        "add_country_modifier={duration=1}",
        "remove_country_modifier=missing",
        "capital_scope={add_country_modifier={name=boost duration=1}}",
        "capital_scope={if={limit={has_country_modifier=boost} log=wrong}}",
        "if={limit={always=no} remove_country_modifier=missing}",
        "if={limit={has_country_modifier=boost@ROOT} log=wrong}",
    ] {
        let error = ScriptCompiler::new(&schema, &library, true)
            .with_modifier_ids(definitions().definitions.keys().cloned().collect())
            .compile(&document(input), EntityKind::Country)
            .unwrap_err();
        assert!(error.location().is_some(), "{input}");
        assert_eq!(
            error.path().to_string_lossy(),
            "self-authored-modifiers.txt"
        );
    }
}

#[test]
fn duration_description_is_retained_in_effect_content_and_not_in_attached_state() {
    let schema = Schema::new(["unused"]).unwrap();
    let library = ScriptLibrary::new();
    let text = document(
        "add_country_modifier={name=boost duration=2 desc=council_duration} country_event={id=described.1}",
    );
    for optimise in [false, true] {
        let compiler = || {
            ScriptCompiler::new(&schema, &library, optimise)
                .with_modifier_ids(definitions().definitions.keys().cloned().collect())
        };
        let plan = compiler().compile(&text, EntityKind::Country).unwrap().plan;
        assert!(
            matches!(&plan[0],Action::AttachModifier{duration_label:Some(label),..} if label=="council_duration")
        );
        let events=compiler().compile_events(&document("country_event={id=described.1 is_triggered_only=yes option={name=accept add_country_modifier={name=guard duration=-1 desc=charter_duration}}}")).unwrap();
        let catalog = EventCatalog::new(events.definitions).unwrap();
        let mut world = World::new(schema.clone());
        world.add_entity("AAA", EntityKind::Country);
        world.set_modifiers(definitions()).unwrap();
        world.set_event_catalog(catalog.clone()).unwrap();
        world.execute(EntityId(0), &plan).unwrap();
        let mut restored = World::restore_with_events(
            &world.save("descriptions").unwrap(),
            "descriptions",
            &schema,
            vec![],
            catalog,
        )
        .unwrap();
        for state in [&mut world, &mut restored] {
            state
                .choose_event(&EventChoice {
                    pending: PendingEventId(0),
                    option: "0".into(),
                })
                .unwrap();
            assert_eq!(
                state.modifiers().active[&EntityId(0)]["boost"].expires_on,
                Some(2)
            );
            assert_eq!(
                state.modifiers().active[&EntityId(0)]["guard"].expires_on,
                None
            );
            state.advance_to(2).unwrap();
            assert!(!state.has_modifier(EntityId(0), "boost").unwrap());
            assert!(state.has_modifier(EntityId(0), "guard").unwrap());
        }
        assert_eq!(world, restored);
    }
}

#[test]
fn province_modifier_queries_and_removal_keep_entity_scope_across_restore() {
    let schema = Schema::new(["unused"]).unwrap();
    let library = ScriptLibrary::new();
    let mut results = Vec::new();
    for optimise in [false, true] {
        let compiler = || {
            ScriptCompiler::new(&schema, &library, optimise)
                .with_modifier_ids(definitions().definitions.keys().cloned().collect())
        };
        let plan = compiler()
            .compile(
                &document(
                    r#"
            if={limit={has_province_modifier=boost} log=attached}
            remove_province_modifier=boost
            if={limit={NOT={has_province_modifier=boost}} log=removed}
            if={limit={has_province_modifier=guard} log=retained}
            owner={if={limit={has_country_modifier=boost} log=owner}}
        "#,
                ),
                EntityKind::Province,
            )
            .unwrap()
            .plan;
        let cleanup = compiler().compile(&document(
            "remove_province_modifier=guard if={limit={NOT={has_province_modifier=guard}} log=cleared}"
        ), EntityKind::Province).unwrap().plan;
        let mut world = World::new(schema.clone());
        world.add_entity("AAA", EntityKind::Country);
        world.add_entity("Port", EntityKind::Province);
        world.add_entity("Inland", EntityKind::Province);
        world.set_owner(EntityId(1), EntityId(0)).unwrap();
        world.set_modifiers(definitions()).unwrap();
        for (entity, name) in [
            (EntityId(0), "boost"),
            (EntityId(1), "boost"),
            (EntityId(1), "guard"),
        ] {
            world
                .attach_modifier(entity, name, None, None, ModifierRenewal::Replace)
                .unwrap();
        }
        world.execute(EntityId(1), &plan).unwrap();
        assert!(world.has_modifier(EntityId(0), "boost").unwrap());
        assert!(!world.has_modifier(EntityId(1), "boost").unwrap());
        assert!(!world.has_modifier(EntityId(2), "boost").unwrap());
        assert!(world.has_modifier(EntityId(1), "guard").unwrap());
        let mut resumed = World::restore(
            &world.save("province-modifiers").unwrap(),
            "province-modifiers",
            &schema,
        )
        .unwrap();
        for state in [&mut world, &mut resumed] {
            state.execute(EntityId(1), &cleanup).unwrap();
            assert!(state.has_modifier(EntityId(0), "boost").unwrap());
            assert!(!state.has_modifier(EntityId(1), "guard").unwrap());
            assert_eq!(
                state
                    .events()
                    .iter()
                    .map(|e| e.2.as_str())
                    .collect::<Vec<_>>(),
                ["attached", "removed", "retained", "owner", "cleared"]
            );
        }
        assert_eq!(world, resumed);
        results.push(world);
    }
    assert_eq!(results[0], results[1]);
}

#[test]
fn province_modifier_scope_and_names_are_checked_before_folding() {
    let schema = Schema::new(["unused"]).unwrap();
    let library = ScriptLibrary::new();
    for (scope, text) in [
        (EntityKind::Country, "remove_province_modifier=boost"),
        (
            EntityKind::Country,
            "if={limit={has_province_modifier=boost} log=wrong}",
        ),
        (EntityKind::Province, "add_country_modifier={name=boost}"),
        (EntityKind::Province, "remove_country_modifier=boost"),
        (
            EntityKind::Province,
            "if={limit={has_country_modifier=boost} log=wrong}",
        ),
        (EntityKind::Province, "remove_province_modifier=missing"),
        (EntityKind::Province, "remove_province_modifier=$name$"),
        (
            EntityKind::Province,
            "if={limit={has_province_modifier=\"\"} log=wrong}",
        ),
        (
            EntityKind::Province,
            "if={limit={has_province_modifier=boost@ROOT} log=wrong}",
        ),
        (
            EntityKind::Province,
            "if={limit={has_province_modifier=$name$} log=wrong}",
        ),
        (
            EntityKind::Country,
            "if={limit={always=no} remove_province_modifier=boost}",
        ),
        (
            EntityKind::Province,
            "if={limit={always=no} remove_province_modifier=missing}",
        ),
        (
            EntityKind::Province,
            "add_province_modifier={name=boost duration=2}",
        ),
    ] {
        for optimise in [false, true] {
            let error = ScriptCompiler::new(&schema, &library, optimise)
                .with_modifier_ids(definitions().definitions.keys().cloned().collect())
                .compile(&document(text), scope)
                .unwrap_err();
            assert!(error.location().is_some(), "{text}");
            assert_eq!(
                error.path().to_string_lossy(),
                "self-authored-modifiers.txt"
            );
        }
    }
    for optimise in [false, true] {
        let plan = ScriptCompiler::new(&schema, &library, optimise)
            .with_modifier_ids(definitions().definitions.keys().cloned().collect())
            .compile(
                &document("if={limit={has_province_modifier=missing} log=wrong} else={log=absent}"),
                EntityKind::Province,
            )
            .unwrap()
            .plan;
        let mut world = World::new(schema.clone());
        world.add_entity("Port", EntityKind::Province);
        world.set_modifiers(definitions()).unwrap();
        world.execute(EntityId(0), &plan).unwrap();
        assert_eq!(
            world
                .events()
                .iter()
                .map(|e| e.2.as_str())
                .collect::<Vec<_>>(),
            ["absent"]
        );
    }
}

#[test]
fn unknown_province_attachments_fail_closed_without_treating_unknown_as_absent() {
    let schema = Schema::new(["unused"]).unwrap();
    let library = ScriptLibrary::new();
    let mut modifiers = definitions();
    modifiers.unknown_entities.insert(EntityId(1));
    let compiler = || {
        ScriptCompiler::new(&schema, &library, true)
            .with_modifier_ids(modifiers.definitions.keys().cloned().collect())
    };
    let mut world = World::new(schema.clone());
    world.add_entity("AAA", EntityKind::Country);
    world.add_entity("Port", EntityKind::Province);
    world.set_owner(EntityId(1), EntityId(0)).unwrap();
    world.set_modifiers(modifiers.clone()).unwrap();
    for text in [
        "owner={add_country_modifier={name=guard}} if={limit={NOT={has_province_modifier=boost}} log=wrong}",
        "owner={add_country_modifier={name=guard}} remove_province_modifier=boost",
    ] {
        let plan = compiler()
            .compile(&document(text), EntityKind::Province)
            .unwrap()
            .plan;
        let mut failed = world.clone();
        assert!(failed.execute(EntityId(1), &plan).is_err());
        assert!(failed.events().is_empty());
        assert!(failed.save("unknown-province").is_err());
        assert!(failed.advance_to(1).is_err());
        assert!(!world.has_modifier(EntityId(0), "guard").unwrap());
    }
}
