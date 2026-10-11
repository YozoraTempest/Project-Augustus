// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{Calendar, EntityKind, EventCatalog, Schema, World, WorldDate};
use augustus_eu4::{ScriptCompiler, ScriptLibrary, TextDocument, TextEncoding};

fn document(text: &str) -> TextDocument {
    TextDocument::from_bytes(
        "self-authored-predicates.txt",
        text.as_bytes().to_vec(),
        TextEncoding::Utf8,
    )
}
fn calendar() -> Calendar {
    Calendar {
        start: WorldDate {
            year: 1444,
            month: 12,
            day: 31,
        },
        month_lengths: [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31],
    }
}

#[test]
fn content_policy_entity_labels_and_years_compile_to_shared_live_predicates() {
    let schema = Schema::new(["unused"]).unwrap();
    let library = ScriptLibrary::new();
    let text = document(
        r#"
      country_event={ id=sample.before is_triggered_only=yes
        trigger={ has_dlc="Domination" tag=AAA not={is_year=1445} }
        immediate={log=before} option={name=accept}
      }
      country_event={ id=sample.after is_triggered_only=yes
        trigger={ has_dlc="Emperor" TAG=AAA is_year=1445 }
        immediate={log=after} option={name=accept}
      }
      country_event={id=sample.control is_triggered_only=yes
        immediate={country_event={id=sample.before} country_event={id=sample.after}
          country_event={id=sample.before days=1} country_event={id=sample.after days=1}}
        option={name=accept}
      }
    "#,
    );
    let mut saved = Vec::new();
    for optimise in [false, true] {
        let compiled = ScriptCompiler::new(&schema, &library, optimise)
            .compile_events(&text)
            .unwrap();
        let mut world = World::new(schema.clone());
        let country = world.add_entity("AAA", EntityKind::Country);
        let other = world.add_entity("BBB", EntityKind::Country);
        world.set_calendar(calendar()).unwrap();
        world
            .set_event_catalog(
                EventCatalog::new(compiled.definitions)
                    .unwrap()
                    .with_conditions(compiled.conditions)
                    .unwrap(),
            )
            .unwrap();
        world.trigger_event("sample.control", other).unwrap();
        world.trigger_event("sample.control", country).unwrap();
        let mut resumed = World::restore_with_events(
            &world.save("predicates:v1").unwrap(),
            "predicates:v1",
            &schema,
            Vec::new(),
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
                .map(|e| (e.0, e.1, e.2.as_str()))
                .collect::<Vec<_>>(),
            [(0, country, "before"), (1, country, "after")]
        );
        saved.push(world.save("predicates:v1").unwrap());
    }
    assert_eq!(saved[0], saved[1]);
}

#[test]
fn boolean_groups_and_aliases_preserve_none_of_semantics_and_order() {
    let schema = Schema::new(["unused"]).unwrap();
    let library = ScriptLibrary::new();
    let text = document(
        r#"
      if={limit={and={always=yes not={always=no}}} log=lower_and}
      if={limit={OR={always=no tag=AAA}} log=upper_or}
      if={limit={or={always=no TAG=AAA}} log=lower_or}
      if={limit={NOT={always=yes always=no}} log=incorrect} else={log=multi_not_false}
      if={limit={not={always=no always=no}} log=multi_not_true}
      if={limit={AND={always=yes OR={always=no always=yes}}} log=constant_true}
    "#,
    );
    let mut saved = Vec::new();
    for optimise in [false, true] {
        let compiled = ScriptCompiler::new(&schema, &library, optimise)
            .compile(&text, EntityKind::Country)
            .unwrap();
        assert_eq!(compiled.folded_branches, if optimise { 4 } else { 0 });
        let mut world = World::new(schema.clone());
        let country = world.add_entity("AAA", EntityKind::Country);
        world.execute(country, &compiled.plan).unwrap();
        assert_eq!(
            world
                .events()
                .iter()
                .map(|e| e.2.as_str())
                .collect::<Vec<_>>(),
            [
                "lower_and",
                "upper_or",
                "lower_or",
                "multi_not_false",
                "multi_not_true",
                "constant_true"
            ]
        );
        saved.push(world.save("logic:v1").unwrap());
    }
    assert_eq!(saved[0], saved[1]);
}

#[test]
fn unknown_policy_names_and_unsupported_dynamic_identifiers_report_source_positions() {
    let schema = Schema::new(["unused"]).unwrap();
    let library = ScriptLibrary::new();
    for trigger in [
        "has_dlc=\"Unknown expansion\"",
        "has_dlc=\"domination\"",
        "has_dlc={name=Domination}",
        "tag=ROOT",
        "tag=FROM",
        "tag=PREV",
        "tag=\"\"",
        "tag=AA",
        "tag=aaa",
        "is_year=0",
        "is_year=-1",
        "is_year=1445.5",
        "is_year=32768",
        "OR={always=yes unsupported=yes}",
        "NOT={}",
    ] {
        let text = format!("\nif={{limit={{{trigger}}} log=selected}}");
        let error = ScriptCompiler::new(&schema, &library, true)
            .compile(&document(&text), EntityKind::Country)
            .unwrap_err();
        assert!(error.location().is_some(), "{trigger}");
        assert_eq!(
            error.path().to_string_lossy(),
            "self-authored-predicates.txt"
        );
    }
    assert!(
        ScriptCompiler::new(&schema, &library, true)
            .compile(
                &document("if={limit={tag=AAA} log=selected}"),
                EntityKind::Province
            )
            .is_err()
    );
}

#[test]
fn legacy_content_names_use_the_same_all_dlc_policy_as_expansions() {
    let schema = Schema::new(["unused"]).unwrap();
    let library = ScriptLibrary::new();
    let mut world = World::new(schema.clone());
    let country = world.add_entity("AAA", EntityKind::Country);
    for name in [
        "American Dream",
        "Purple Phoenix",
        "Star and Crescent",
        "Women in History",
        "Dharma - Hindu Unit Pack",
        "Dharma - Indian Sultanate Pack",
    ] {
        let text =
            format!("if={{limit={{has_dlc=\"{name}\"}} log=selected}} else={{log=incorrect}}");
        let compiled = ScriptCompiler::new(&schema, &library, true)
            .compile(&document(&text), EntityKind::Country)
            .unwrap();
        assert_eq!(compiled.folded_branches, 1);
        world.execute(country, &compiled.plan).unwrap();
    }
    assert_eq!(world.events().len(), 6);
    assert!(world.events().iter().all(|e| e.2 == "selected"));
}
