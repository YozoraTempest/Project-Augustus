// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::{fs, path::Path};

use augustus_core::{EntityKind, Schema, World};
use augustus_eu4::{ScriptCompiler, ScriptLibrary, TextDocument, TextEncoding, VanillaWorld};

fn document(text: &str) -> TextDocument {
    TextDocument::from_bytes(
        "self-authored-transition.txt",
        text.as_bytes().to_vec(),
        TextEncoding::Utf8,
    )
}

fn fixture() -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world");
    for entry in walkdir::WalkDir::new(&source) {
        let entry = entry.unwrap();
        let target = temp
            .path()
            .join(entry.path().strip_prefix(&source).unwrap());
        if entry.file_type().is_dir() {
            fs::create_dir_all(target).unwrap();
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
    temp
}

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

#[test]
fn startup_callbacks_read_old_state_clear_reforms_and_follow_event_dependencies() {
    let temp = fixture();
    write(
        temp.path(),
        "common/on_actions/transition.txt",
        r#"
        on_government_change={
            if={limit={government=monarchy has_reform=test_charter legitimacy=72.125} log=old_monarchy}
            add_legitimacy=-3
            country_event={id=transition.callback}
        }
        on_native_change_government={log=native_callback}
        on_startup={if={limit={tag=AAA}
            change_government=monarchy
            if={limit={legitimacy=72.125 NOT={legitimacy=72.126} has_reform=test_charter} log=same}
            change_government=republic
            if={limit={government=republic has_reform=test_council NOT={has_reform=test_charter}
                legitimacy=100 republican_tradition=50 NOT={republican_tradition=50.001}} log=converted}
            change_government=monarchy
            if={limit={legitimacy=50 NOT={legitimacy=50.001} republican_tradition=50} log=returned}
        }}
    "#,
    );
    write(
        temp.path(),
        "events/transition.txt",
        r#"
        country_event={id=transition.callback is_triggered_only=yes immediate={
            if={limit={government=monarchy has_reform=test_charter} log=callback_event_old}
        } option={name=accept}}
    "#,
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(
        loaded.report.estates.startup_unresolved.is_empty(),
        "{:?}",
        loaded.report.estates.startup_unresolved
    );
    let world = loaded.initial.world();
    let country = loaded.initial.country_entity("AAA").unwrap();
    assert_eq!(world.government_kind(country).unwrap(), "monarchy");
    assert_eq!(
        world
            .government_metric_value(country, "legitimacy")
            .unwrap(),
        50_000
    );
    assert_eq!(
        world
            .government_metric_stored_value(country, "republican_tradition")
            .unwrap(),
        50_000
    );
    assert_eq!(
        world
            .events()
            .iter()
            .map(|(_, _, text)| text.as_str())
            .collect::<Vec<_>>(),
        [
            "old_monarchy",
            "callback_event_old",
            "same",
            "old_monarchy",
            "callback_event_old",
            "converted",
            "returned"
        ]
    );
    assert!(
        world
            .event_catalog()
            .definitions()
            .contains_key("transition.callback")
    );
    assert!(
        World::restore(
            &world.save("loaded-transitions").unwrap(),
            "loaded-transitions",
            world.schema()
        )
        .is_err()
    );
    let saved = loaded.save().unwrap();
    let mut restored = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    restored.restore(&saved).unwrap();
    assert_eq!(restored.initial.world(), world);
}

#[test]
fn native_exit_callback_is_selected_before_same_or_cross_type_application() {
    let temp = fixture();
    write(
        temp.path(),
        "common/governments/native.txt",
        "native={basic_reform=test_native} theocracy={basic_reform=test_theocracy} tribal={basic_reform=test_tribal}",
    );
    write(
        temp.path(),
        "common/government_reforms/native.txt",
        "test_native={native_mechanic=yes} test_theocracy={religion=yes has_devotion=yes} test_tribal={tribal=yes monarchy=yes}",
    );
    write(
        temp.path(),
        "common/on_actions/native.txt",
        r#"
        on_government_change={log=ordinary}
        on_native_change_government={if={limit={government=native has_reform=test_native} log=native_old}}
        on_startup={if={limit={tag=AAA}
            change_government=native change_government=native change_government=theocracy
            if={limit={devotion=50 NOT={devotion=50.001}} log=devotion}
            change_government=tribal
            if={limit={legitimacy=50 devotion=50} log=retained}
        }}
    "#,
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(
        loaded.report.estates.startup_unresolved.is_empty(),
        "{:?}",
        loaded.report.estates.startup_unresolved
    );
    let world = loaded.initial.world();
    assert_eq!(loaded.report.governments.transition_forms, 5);
    assert_eq!(
        world
            .events()
            .iter()
            .map(|(_, _, text)| text.as_str())
            .collect::<Vec<_>>(),
        [
            "ordinary",
            "native_old",
            "native_old",
            "devotion",
            "ordinary",
            "retained"
        ]
    );
    let country = loaded.initial.country_entity("AAA").unwrap();
    assert_eq!(world.government_kind(country).unwrap(), "tribal");
    assert!(!world.government_metric_active(country, "devotion").unwrap());
    assert_eq!(
        world
            .government_metric_stored_value(country, "devotion")
            .unwrap(),
        50_000
    );
}

#[test]
fn callback_sources_and_unsupported_requests_are_checked_before_optimisation() {
    let schema = Schema::new(["unused"]).unwrap();
    for optimise in [false, true] {
        for script in [
            "change_government=unknown",
            "change_government=ROOT",
            "change_government={type=monarchy}",
        ] {
            let library = ScriptLibrary::new();
            let error = ScriptCompiler::new(&schema, &library, optimise)
                .with_government_form_ids(["monarchy".into()].into())
                .compile(
                    &document(&format!("if={{limit={{always=no}}\n{script}}}")),
                    EntityKind::Country,
                )
                .unwrap_err()
                .to_string();
            assert!(error.contains("self-authored-transition.txt:2:"), "{error}");
        }
        for body in [
            "on_government_change={unknown=yes}",
            "on_government_change={change_government=monarchy}",
            "on_government_change={} on_government_change={}",
        ] {
            let mut library = ScriptLibrary::new();
            library
                .add_on_actions(&TextDocument::from_bytes(
                    "callback.txt",
                    body.as_bytes().to_vec(),
                    TextEncoding::Utf8,
                ))
                .unwrap();
            let error = ScriptCompiler::new(&schema, &library, optimise)
                .with_government_form_ids(["monarchy".into()].into())
                .compile(&document("change_government=monarchy"), EntityKind::Country)
                .unwrap_err()
                .to_string();
            assert!(error.contains("callback.txt:"), "{error}");
        }
        let library = ScriptLibrary::new();
        assert!(
            ScriptCompiler::new(&schema, &library, optimise)
                .with_government_form_ids(["monarchy".into()].into())
                .compile(
                    &document("change_government=monarchy"),
                    EntityKind::Province
                )
                .unwrap_err()
                .to_string()
                .contains("country scope")
        );
    }
}

#[test]
fn uncertain_startup_metrics_and_unverified_forms_are_not_filled_with_defaults() {
    let temp = fixture();
    write(
        temp.path(),
        "common/governments/unverified.txt",
        "custom={basic_reform=test_custom}",
    );
    write(
        temp.path(),
        "common/government_reforms/unverified.txt",
        "test_custom={monarchy=yes}",
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let country = loaded.initial.country_entity("BBB").unwrap();
    assert!(
        loaded
            .initial
            .world()
            .government_metric_active(country, "republican_tradition")
            .unwrap()
    );
    assert!(
        loaded
            .initial
            .world()
            .government_metric_stored_value(country, "republican_tradition")
            .is_err()
    );
    assert!(
        !loaded
            .initial
            .world()
            .governments()
            .forms
            .contains_key("custom")
    );
    assert!(
        loaded
            .report
            .governments
            .unavailable_transitions
            .contains_key("custom")
    );
}

#[test]
fn startup_republic_entry_retains_higher_values_and_same_form_skips_the_entry_floor() {
    let temp = fixture();
    write(
        temp.path(),
        "common/on_actions/retained.txt",
        r#"
        on_startup={if={limit={tag=AAA}
            change_government=republic add_republican_tradition=26
            change_government=monarchy change_government=republic
            if={limit={republican_tradition=76 NOT={republican_tradition=76.001}} log=retained}
            add_republican_tradition=-1000000 add_republican_tradition=7.125
            change_government=republic
            if={limit={republican_tradition=7.125 NOT={republican_tradition=7.126}} log=same}
            change_government=monarchy change_government=republic
            if={limit={republican_tradition=50 NOT={republican_tradition=50.001}} log=floor}
        }}
    "#,
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(
        loaded.report.estates.startup_unresolved.is_empty(),
        "{:?}",
        loaded.report.estates.startup_unresolved
    );
    assert_eq!(
        loaded
            .initial
            .world()
            .events()
            .iter()
            .map(|row| row.2.as_str())
            .collect::<Vec<_>>(),
        ["retained", "same", "floor"]
    );
    assert!(
        loaded.initial.world().governments().forms["republic"]
            .retain_higher_metrics
            .contains("republican_tradition")
    );
    let mut resumed = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    resumed.restore(&loaded.save().unwrap()).unwrap();
    assert_eq!(loaded.initial.world(), resumed.initial.world());
}
