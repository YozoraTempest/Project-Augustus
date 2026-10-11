// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{Completions, EntityKind, EventCatalog, Schema, World};
use augustus_eu4::{ScriptCompiler, ScriptLibrary, TextDocument, TextEncoding};

fn document(text: &str) -> TextDocument {
    TextDocument::from_bytes(
        "self-authored-missions.txt",
        text.as_bytes().to_vec(),
        TextEncoding::Utf8,
    )
}

#[test]
fn forced_completion_translates_to_records_and_live_conditions_without_rewards() {
    let schema = Schema::new(["unused"]).unwrap();
    let library = ScriptLibrary::new();
    let ids: std::collections::BTreeSet<String> = ["first".into(), "blocked".into()].into();
    let text = document(
        r#"
      if={limit={mission_completed=first} log=incorrect} else={log=fresh}
      complete_mission=first complete_mission=first complete_mission=blocked
      if={limit={AND={mission_completed=first mission_completed=blocked}} log=done}
      country_event={id=records.1 days=1}
    "#,
    );
    let events = document(
        "country_event={id=records.1 is_triggered_only=yes trigger={mission_completed=first} immediate={log=delayed} option={name=accept}}",
    );
    let mut states = Vec::new();
    for optimise in [false, true] {
        let compiled = ScriptCompiler::new(&schema, &library, optimise)
            .with_mission_ids(ids.clone())
            .compile(&text, EntityKind::Country)
            .unwrap();
        let events = ScriptCompiler::new(&schema, &library, optimise)
            .with_mission_ids(ids.clone())
            .compile_events(&events)
            .unwrap();
        let mut world = World::new(schema.clone());
        let country = world.add_entity("AAA", EntityKind::Country);
        let other = world.add_entity("BBB", EntityKind::Country);
        world
            .set_completions(Completions {
                identifiers: ids.clone(),
                entities: [(country, Default::default()), (other, Default::default())].into(),
            })
            .unwrap();
        world
            .set_event_catalog(
                EventCatalog::new(events.definitions)
                    .unwrap()
                    .with_conditions(events.conditions)
                    .unwrap(),
            )
            .unwrap();
        world.execute(country, &compiled.plan).unwrap();
        assert!(!world.completion_recorded(other, "first").unwrap());
        let mut resumed = World::restore_with_events(
            &world.save("missions:v1").unwrap(),
            "missions:v1",
            &schema,
            vec![],
            world.event_catalog().clone(),
        )
        .unwrap();
        world.advance_to(1).unwrap();
        resumed.advance_to(1).unwrap();
        assert_eq!(world, resumed);
        assert_eq!(
            world
                .events()
                .iter()
                .map(|e| (e.0, e.2.as_str()))
                .collect::<Vec<_>>(),
            [(0, "fresh"), (0, "done"), (1, "delayed")]
        );
        states.push(world.save("missions:v1").unwrap());
    }
    assert_eq!(states[0], states[1]);
}

#[test]
fn undeclared_dynamic_and_wrong_scope_mission_records_have_source_errors() {
    let schema = Schema::new(["unused"]).unwrap();
    let library = ScriptLibrary::new();
    for text in [
        "complete_mission=unknown",
        "complete_mission=\"\"",
        "complete_mission={id=first}",
        "if={limit={mission_completed=unknown} log=incorrect}",
        "if={limit={always=no} complete_mission=unknown}",
        "complete_mission=FIRST",
    ] {
        let error = ScriptCompiler::new(&schema, &library, true)
            .with_mission_ids(["first".into()].into())
            .compile(&document(text), EntityKind::Country)
            .unwrap_err();
        assert!(error.location().is_some(), "{text}");
        assert_eq!(error.path().to_string_lossy(), "self-authored-missions.txt");
    }
    for text in [
        "complete_mission=first",
        "if={limit={mission_completed=first} log=incorrect}",
    ] {
        assert!(
            ScriptCompiler::new(&schema, &library, true)
                .with_mission_ids(["first".into()].into())
                .compile(&document(text), EntityKind::Province)
                .is_err()
        );
    }
}
