// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{EntityId, EntityKind, Markers, Schema, World};
use augustus_eu4::{ScriptCompiler, ScriptLibrary, TextDocument, TextEncoding, VanillaWorld};
use std::{fs, path::Path};

fn document(text: &str) -> TextDocument {
    TextDocument::from_bytes(
        "self-authored-flags.txt",
        text.as_bytes().to_vec(),
        TextEncoding::Utf8,
    )
}

#[test]
fn flags_translate_to_live_scoped_state_in_both_compilation_modes() {
    let schema = Schema::new(["unused"]).unwrap();
    let library = ScriptLibrary::new();
    let mut results = Vec::new();
    for optimise in [false, true] {
        let plan = ScriptCompiler::new(&schema, &library, optimise)
            .compile(
                &document(
                    r#"
            if={limit={NOT={has_country_flag=ready}} log=absent}
            clr_country_flag=missing
            set_country_flag=ready
            set_country_flag=ready
            capital_scope={owner={if={limit={has_country_flag=ready} log=owner}}}
            clr_country_flag=ready
            if={limit={has_country_flag=ready} log=stale} else={log=cleared}
        "#,
                ),
                EntityKind::Country,
            )
            .unwrap()
            .plan;
        let mut world = World::new(schema.clone());
        world.add_entity("AAA", EntityKind::Country);
        world.add_entity("BBB", EntityKind::Country);
        world.add_entity("Port", EntityKind::Province);
        world.set_owner(EntityId(2), EntityId(0)).unwrap();
        world.set_capital(EntityId(0), EntityId(2)).unwrap();
        world
            .set_markers(Markers {
                entities: [
                    (EntityId(0), Default::default()),
                    (EntityId(1), Default::default()),
                ]
                .into(),
                ..Default::default()
            })
            .unwrap();
        world.execute(EntityId(0), &plan).unwrap();
        assert!(world.markers().entities[&EntityId(0)].is_empty());
        assert!(!world.has_marker(EntityId(1), "ready").unwrap());
        assert!(world.has_marker(EntityId(2), "ready").is_err());
        assert_eq!(
            world
                .events()
                .iter()
                .map(|e| e.2.as_str())
                .collect::<Vec<_>>(),
            ["absent", "owner", "cleared"]
        );
        results.push(world.save("flags").unwrap());
    }
    assert_eq!(results[0], results[1]);
}

#[test]
fn unsupported_flag_names_scopes_and_forms_report_the_source_even_in_dead_branches() {
    let schema = Schema::new(["unused"]).unwrap();
    let library = ScriptLibrary::new();
    for text in [
        "set_country_flag = {}",
        "clr_country_flag = \"\"",
        "set_country_flag = ready@ROOT",
        "set_country_flag = $name$",
        "capital_scope = { set_country_flag = ready }",
        "if={limit={always=no} set_country_flag = ready@ROOT}",
        "if={limit={has_country_flag={flag=ready}} log=invalid}",
    ] {
        let error = ScriptCompiler::new(&schema, &library, true)
            .compile(&document(text), EntityKind::Country)
            .unwrap_err();
        assert!(error.location().is_some(), "{error}");
        assert!(error.to_string().contains("self-authored-flags.txt"));
    }
}

fn fixture() -> tempfile::TempDir {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world");
    let temp = tempfile::tempdir().unwrap();
    for entry in walkdir::WalkDir::new(&root) {
        let entry = entry.unwrap();
        let target = temp.path().join(entry.path().strip_prefix(&root).unwrap());
        if entry.file_type().is_dir() {
            fs::create_dir_all(target).unwrap();
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
    temp
}

fn write(root: &Path, name: &str, text: &str) {
    let path = root.join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

#[test]
fn history_markers_feed_ideas_estates_startup_events_and_resumable_state() {
    let temp = fixture();
    let history = "history/countries/AAA - Alpha.txt";
    let original = fs::read_to_string(temp.path().join(history)).unwrap();
    write(
        temp.path(),
        history,
        &(original
            + r#"
        set_country_flag=history_ready
        set_country_flag=removed
        1444.11.10={clr_country_flag=removed}
        1445.1.1={clr_country_flag=history_ready}
    "#),
    );
    write(
        temp.path(),
        "common/estates/flags.txt",
        r#"
        estate_test={trigger={has_country_flag=history_ready}}
    "#,
    );
    write(
        temp.path(),
        "common/ideas/flags.txt",
        r#"
        test_ideas={free=yes start={global_tax_modifier=0.1} trigger={tag=AAA has_country_flag=history_ready} bonus={} test={global_tax_modifier=0.1}}
    "#,
    );
    write(
        temp.path(),
        "common/on_actions/flags.txt",
        r#"
        on_startup={
            if={limit={tag=AAA has_country_flag=history_ready}
                clr_country_flag=history_ready
                set_country_flag=startup_ready
                if={limit={has_country_flag=startup_ready} country_event={id=test.flags}}
            }
        }
    "#,
    );
    write(
        temp.path(),
        "events/flags.txt",
        r#"
        country_event={id=test.flags is_triggered_only=yes
            immediate={clr_country_flag=startup_ready set_country_flag=event_ready}
            option={name=accept clr_country_flag=event_ready set_country_flag=choice_ready}
            after={if={limit={has_country_flag=choice_ready} log=after}}
        }
    "#,
    );
    let mut loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(
        loaded.report.estates.pending_startup_effects.is_empty(),
        "{:?}",
        loaded.report.estates.startup_unresolved
    );
    let country = loaded.initial.country_entity("AAA").unwrap();
    assert_eq!(
        loaded.country("AAA").unwrap().estates.unwrap()["estate_test"].active,
        Some(true)
    );
    assert!(
        loaded
            .country("AAA")
            .unwrap()
            .ideas
            .unwrap()
            .groups
            .as_ref()
            .unwrap()
            .contains_key("test_ideas")
    );
    assert!(
        !loaded
            .initial
            .world()
            .has_marker(country, "history_ready")
            .unwrap()
    );
    assert!(
        !loaded
            .initial
            .world()
            .has_marker(country, "removed")
            .unwrap()
    );
    assert!(
        !loaded
            .initial
            .world()
            .has_marker(country, "startup_ready")
            .unwrap()
    );
    assert!(
        loaded
            .initial
            .world()
            .has_marker(country, "event_ready")
            .unwrap()
    );
    assert!(loaded.country("AAA").unwrap().flag_namespace_known);
    let saved = loaded.save().unwrap();
    let mut resumed = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    resumed.restore(&saved).unwrap();
    for state in [&mut loaded, &mut resumed] {
        state
            .initial
            .choose_event(&augustus_core::EventChoice {
                pending: augustus_core::PendingEventId(0),
                option: "0".into(),
            })
            .unwrap();
        state.advance(30).unwrap();
        assert!(
            state
                .initial
                .world()
                .has_marker(country, "choice_ready")
                .unwrap()
        );
        assert_eq!(state.initial.world().events().last().unwrap().2, "after");
    }
    assert_eq!(loaded.save().unwrap(), resumed.save().unwrap());
}

#[test]
fn unknown_history_and_failed_startup_do_not_turn_absent_flags_into_false() {
    let temp = fixture();
    let name = "history/countries/AAA - Alpha.txt";
    let original = fs::read_to_string(temp.path().join(name)).unwrap();
    write(
        temp.path(),
        name,
        &(original.clone()
            + r#"
        set_country_flag=old
        if={limit={always=yes} clr_country_flag=old}
        set_country_flag=resolved
        clr_country_flag=removed
    "#),
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let country = loaded.initial.country_entity("AAA").unwrap();
    assert!(!loaded.country("AAA").unwrap().flag_namespace_known);
    assert!(loaded.initial.world().has_marker(country, "old").is_err());
    assert!(
        loaded
            .initial
            .world()
            .has_marker(country, "resolved")
            .unwrap()
    );
    assert!(
        !loaded
            .initial
            .world()
            .has_marker(country, "removed")
            .unwrap()
    );
    write(
        temp.path(),
        name,
        &(original + "\nset_country_flag=historical"),
    );
    write(
        temp.path(),
        "common/on_actions/flags.txt",
        "on_startup={set_country_flag=prefix unavailable_effect=yes}",
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert_eq!(loaded.report.estates.pending_startup_effects.len(), 1);
    assert!(!loaded.country("AAA").unwrap().flag_namespace_known);
    for name in ["prefix", "historical", "absent"] {
        assert!(loaded.initial.world().has_marker(country, name).is_err());
    }
    assert!(loaded.initial.world().events().is_empty());
}
