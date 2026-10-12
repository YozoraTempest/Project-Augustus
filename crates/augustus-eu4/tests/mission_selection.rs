// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::*;
use augustus_eu4::{ScriptCompiler, ScriptLibrary, TextDocument, TextEncoding};

fn document(name: &str, text: &str) -> TextDocument {
    TextDocument::from_bytes(name, text.as_bytes().to_vec(), TextEncoding::Utf8)
}

#[test]
fn ordered_files_translate_range_shadowing_and_live_refresh_without_rewards() {
    let schema = Schema::new(["unused"]).unwrap();
    let library = ScriptLibrary::new();
    let ids: std::collections::BTreeSet<String> = ["low", "wide_low", "wide_high", "special"]
        .map(String::from)
        .into();
    let files = [
        document(
            "first.txt",
            "low_series={slot=1 generic=yes potential={tag=AAA} low={position=1}}",
        ),
        document(
            "second.txt",
            "wide_series={slot=1 generic=yes potential={tag=AAA} wide_low={position=1} wide_high={position=3}} special_series={slot=1 potential={tag=AAA NOT={has_country_flag=hide has_country_flag=other}} special={position=3 trigger={always=no} effect={log=reward}}}",
        ),
    ];
    let mut results = Vec::new();
    for optimise in [false, true] {
        let objectives = ScriptCompiler::new(&schema, &library, optimise)
            .with_mission_ids(ids.clone())
            .compile_missions(&files)
            .unwrap();
        assert!(
            objectives
                .groups
                .iter()
                .all(|g| g.priority == 0 && g.claims.is_empty())
        );
        let mut world = World::new(schema.clone());
        let entity = world.add_entity("AAA", EntityKind::Country);
        world
            .set_completions(Completions {
                identifiers: ids.clone(),
                entities: [(entity, Default::default())].into(),
            })
            .unwrap();
        world
            .set_markers(Markers {
                entities: [(entity, Default::default())].into(),
                ..Default::default()
            })
            .unwrap();
        world.set_objectives(objectives).unwrap();
        world.refresh_objectives(entity).unwrap();
        // The eligible wide fallback excludes the low fallback even when the
        // wide fallback is itself excluded by the non-generic series.
        for id in ["low", "wide_low", "wide_high"] {
            assert!(!world.objective_available(entity, id).unwrap());
        }
        assert!(world.objective_available(entity, "special").unwrap());
        let plan = ScriptCompiler::new(&schema, &library, optimise).with_mission_ids(ids.clone()).compile(&document("refresh.txt", "complete_mission=special set_country_flag=hide if={limit={has_mission=special} log=cached} swap_non_generic_missions=no if={limit={has_mission=wide_low} log=updated}"), EntityKind::Country).unwrap();
        world.execute(entity, &plan.plan).unwrap();
        assert!(!world.objective_available(entity, "special").unwrap());
        assert!(world.objective_available(entity, "wide_high").unwrap());
        assert!(world.completion_recorded(entity, "special").unwrap());
        assert_eq!(
            world
                .events()
                .iter()
                .map(|e| e.2.as_str())
                .collect::<Vec<_>>(),
            ["cached", "updated"]
        );
        results.push(world.save("selection").unwrap());
    }
    assert_eq!(results[0], results[1]);
}

#[test]
fn untranslated_conditions_remain_unknown_for_relevant_countries() {
    let schema = Schema::new(["unused"]).unwrap();
    let library = ScriptLibrary::new();
    let objectives = ScriptCompiler::new(&schema, &library, true)
        .compile_missions(&[document(
            "partial.txt",
            "series={slot=1 potential={unimplemented=yes tag=AAA} sample={}}",
        )])
        .unwrap();
    let mut world = World::new(schema);
    let first = world.add_entity("AAA", EntityKind::Country);
    let second = world.add_entity("BBB", EntityKind::Country);
    world
        .set_completions(Completions {
            identifiers: ["sample".into()].into(),
            ..Default::default()
        })
        .unwrap();
    world.set_objectives(objectives).unwrap();
    assert!(
        world
            .refresh_objectives(first)
            .unwrap_err()
            .to_string()
            .contains("partial.txt:1")
    );
    assert!(world.objective_available(first, "sample").is_err());
    world.refresh_objectives(second).unwrap();
    assert!(!world.objective_available(second, "sample").unwrap());
    for predicate in ["OR={}", "NOT={}"] {
        let objectives = ScriptCompiler::new(world.schema(), &library, true)
            .compile_missions(&[document(
                "invalid.txt",
                &format!("series={{slot=1 potential={{{predicate}}} sample={{}}}}"),
            )])
            .unwrap();
        world.set_objectives(objectives).unwrap();
        assert!(world.refresh_objectives(first).is_err());
    }
}

#[test]
fn malformed_metadata_refresh_flags_and_wrong_scopes_have_source_errors() {
    let schema = Schema::new(["unused"]).unwrap();
    let library = ScriptLibrary::new();
    for text in [
        "series={sample={}}",
        "series={slot=0 sample={}}",
        "series={slot=1 sample={position=@row}}",
    ] {
        let error = ScriptCompiler::new(&schema, &library, true)
            .compile_missions(&[document("invalid.txt", text)])
            .unwrap_err();
        assert!(error.to_string().contains("invalid.txt:1"));
    }
    for (text, scope) in [
        ("swap_non_generic_missions=maybe", EntityKind::Country),
        ("swap_non_generic_missions=yes", EntityKind::Province),
        (
            "if={limit={has_mission=missing} log=invalid}",
            EntityKind::Country,
        ),
    ] {
        assert!(
            ScriptCompiler::new(&schema, &library, true)
                .compile(&document("invalid.txt", text), scope)
                .unwrap_err()
                .to_string()
                .contains("invalid.txt:1")
        );
    }
}
