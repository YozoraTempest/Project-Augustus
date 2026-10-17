// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{EntityId, EntityKind, GlobalMarkers, Markers, Schema, World};
use augustus_eu4::{
    ScriptCompiler, ScriptLibrary, TextDocument, TextEncoding, VanillaExternalState, VanillaWorld,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

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
fn external_country_markers_resolve_startup_content_conditions() {
    let temp = fixture();
    write(
        temp.path(),
        "common/estates/external-markers.txt",
        "estate_external = { trigger = { has_country_flag = external_ready } }",
    );
    let state = VanillaExternalState {
        country_markers: BTreeMap::from([(
            "AAA".into(),
            Some(BTreeSet::from(["external_ready".into()])),
        )]),
        ..Default::default()
    };

    let loaded = VanillaWorld::load_with_external_state(temp.path(), "1444.11.11", &state).unwrap();
    let country = loaded.initial.country_entity("AAA").unwrap();
    assert_eq!(
        loaded.country("AAA").unwrap().estates.unwrap()["estate_external"].active,
        Some(true)
    );
    assert!(
        loaded
            .initial
            .world()
            .marker_namespace_known(country)
            .unwrap()
    );
    assert!(
        loaded
            .initial
            .world()
            .has_marker(country, "external_ready")
            .unwrap()
    );
}

#[test]
fn external_unknown_country_markers_preserve_unknown_namespace() {
    let temp = fixture();
    write(
        temp.path(),
        "common/estates/external-markers.txt",
        "estate_external = { trigger = { has_country_flag = external_ready } }",
    );
    let state = VanillaExternalState {
        country_markers: BTreeMap::from([("AAA".into(), None)]),
        ..Default::default()
    };

    let loaded = VanillaWorld::load_with_external_state(temp.path(), "1444.11.11", &state).unwrap();
    let country = loaded.initial.country_entity("AAA").unwrap();
    assert!(
        !loaded
            .initial
            .world()
            .marker_namespace_known(country)
            .unwrap()
    );
    assert!(
        loaded
            .initial
            .world()
            .has_marker(country, "external_ready")
            .is_err()
    );
    assert_eq!(
        loaded.country("AAA").unwrap().estates.unwrap()["estate_external"].active,
        None
    );
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

#[test]
fn global_flags_translate_in_country_and_province_scopes_without_entity_state() {
    let schema = Schema::new(["unused"]).unwrap();
    let library = ScriptLibrary::new();
    for optimise in [false, true] {
        for scope in [EntityKind::Country, EntityKind::Province] {
            let mut world = World::new(schema.clone());
            world.add_entity("Actor", scope);
            world
                .set_global_markers(GlobalMarkers {
                    known: true,
                    ..Default::default()
                })
                .unwrap();
            let plan = ScriptCompiler::new(&schema, &library, optimise)
                .compile(
                    &document(
                        r#"
                if={limit={NOT={has_global_flag=gate}} log=absent}
                set_global_flag=gate
                set_global_flag=gate
                if={limit={has_global_flag=gate} log=present}
                clr_global_flag=gate
                clr_global_flag=missing
                if={limit={has_global_flag=gate} log=wrong} else={log=cleared}
            "#,
                    ),
                    scope,
                )
                .unwrap()
                .plan;
            world.execute(EntityId(0), &plan).unwrap();
            assert_eq!(
                world
                    .events()
                    .iter()
                    .map(|row| row.2.as_str())
                    .collect::<Vec<_>>(),
                ["absent", "present", "cleared"]
            );
            assert!(world.markers().is_empty());
            assert!(world.global_markers().values.is_empty());
        }
    }
    for text in [
        "set_global_flag={}",
        "clr_global_flag=\"\"",
        "set_global_flag=flag@ROOT",
        "if={limit={always=no} set_global_flag=$name$}",
        "if={limit={has_global_flag={flag=gate}} log=wrong}",
    ] {
        let error = ScriptCompiler::new(&schema, &library, true)
            .compile(&document(text), EntityKind::Country)
            .unwrap_err();
        assert!(error.location().is_some(), "{error}");
    }
}

#[test]
fn startup_and_later_events_share_global_state_with_condition_consumers() {
    let temp = fixture();
    write(
        temp.path(),
        "common/estates/global.txt",
        "estate_global={trigger={NOT={has_global_flag=gate}}}",
    );
    write(
        temp.path(),
        "common/ideas/global.txt",
        "global_ideas={free=yes start={global_tax_modifier=0.1} trigger={tag=AAA NOT={has_global_flag=gate}} bonus={} test={global_tax_modifier=0.1}}",
    );
    write(
        temp.path(),
        "common/on_actions/global.txt",
        r#"
        on_startup={if={limit={tag=AAA NOT={has_global_flag=gate}}
            set_global_flag=gate
            if={limit={has_global_flag=gate} country_event={id=global.later days=1}}
        }}
    "#,
    );
    write(
        temp.path(),
        "events/global.txt",
        r#"
        country_event={id=global.later is_triggered_only=yes trigger={has_global_flag=gate}
            immediate={capital_scope={clr_global_flag=gate} if={limit={NOT={has_global_flag=gate}} log=cleared}}
            option={name=continue set_global_flag=chosen}
        }
    "#,
    );
    let mut loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(
        loaded.report.estates.startup_unresolved.is_empty(),
        "{:?}",
        loaded.report.estates.startup_unresolved
    );
    assert_eq!(
        loaded.country("AAA").unwrap().estates.unwrap()["estate_global"].active,
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
            .contains_key("global_ideas")
    );
    assert!(loaded.initial.world().has_global_marker("gate").unwrap());
    let saved = loaded.save().unwrap();
    let mut resumed = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    resumed.restore(&saved).unwrap();
    for state in [&mut loaded, &mut resumed] {
        state.advance(1).unwrap();
        assert_eq!(state.initial.world().events().last().unwrap().2, "cleared");
        assert!(!state.initial.world().has_global_marker("gate").unwrap());
        state
            .initial
            .choose_event(&augustus_core::EventChoice {
                pending: augustus_core::PendingEventId(0),
                option: "0".into(),
            })
            .unwrap();
        assert!(state.initial.world().has_global_marker("chosen").unwrap());
    }
    assert_eq!(loaded.save().unwrap(), resumed.save().unwrap());
}

#[test]
fn active_global_history_and_failed_startup_preserve_unknown_state() {
    let temp = fixture();
    let name = "history/countries/AAA - Alpha.txt";
    let original = fs::read_to_string(temp.path().join(name)).unwrap();
    write(
        temp.path(),
        name,
        &(original.clone() + "\n1445.1.1={set_global_flag=future}"),
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(!loaded.initial.world().has_global_marker("future").unwrap());
    let later = VanillaWorld::load(temp.path(), "1445.1.1").unwrap();
    assert!(later.initial.world().has_global_marker("future").is_err());
    for effect in [
        "set_global_flag=history",
        "if={limit={always=yes} clr_global_flag=history}",
    ] {
        write(temp.path(), name, &(original.clone() + "\n" + effect));
        let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
        assert!(loaded.initial.world().has_global_marker("history").is_err());
    }
    write(temp.path(), name, &original);
    let province_name = "history/provinces/1 - North.txt";
    let province_history = fs::read_to_string(temp.path().join(province_name)).unwrap();
    write(
        temp.path(),
        province_name,
        &(province_history.clone() + "\nset_global_flag=province_history"),
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(
        loaded
            .initial
            .world()
            .has_global_marker("province_history")
            .is_err()
    );
    write(temp.path(), province_name, &province_history);
    write(
        temp.path(),
        "common/on_actions/global.txt",
        "on_startup={set_global_flag=prefix unavailable_effect=yes}",
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert_eq!(loaded.report.estates.pending_startup_effects.len(), 1);
    for name in ["prefix", "absent"] {
        assert!(loaded.initial.world().has_global_marker(name).is_err());
    }
    assert!(loaded.initial.world().events().is_empty());
}
