// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{Condition, EntityKind, Schema, Target};
use augustus_eu4::{ScriptCompiler, ScriptLibrary, TextDocument, TextEncoding, VanillaWorld};
use std::{fs, path::Path};

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn fixture(history: &str) -> tempfile::TempDir {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world");
    let temp = tempfile::tempdir().unwrap();
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
    write(
        temp.path(),
        "common/religions/test.txt",
        "test_religions={test_faith={} related_faith={} religious_schools={first={} second={}}} remote_group={remote_faith={}}",
    );
    write(
        temp.path(),
        "history/countries/AAA - Alpha.txt",
        &format!("capital=1 primary_culture=alpha government=monarchy {history}"),
    );
    temp
}

fn presence() -> Condition {
    Condition::AffiliationPresent {
        target: Target::Current,
        dimension: "religious_school".into(),
    }
}

#[test]
fn history_order_dates_and_group_changes_match_live_school_state() {
    for (history, school) in [
        ("religious_school=first religion=test_faith", None),
        ("religion=test_faith religious_school=first", Some("first")),
        (
            "religion=test_faith religious_school=first 1444.1.1={religion=related_faith}",
            Some("first"),
        ),
        (
            "religion=test_faith religious_school=first 1444.1.1={religion=remote_faith}",
            None,
        ),
        (
            "religion=test_faith religious_school=first 1445.1.1={religion=remote_faith}",
            Some("first"),
        ),
    ] {
        let temp = fixture(history);
        let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
        let id = loaded.initial.country_entity("AAA").unwrap();
        let affiliations = loaded.initial.world().affiliations();
        assert_eq!(
            affiliations.member("religious_school", id),
            school,
            "{history}"
        );
        assert_eq!(
            presence().evaluate(loaded.initial.world(), id).unwrap(),
            school.is_some()
        );
        assert!(
            !affiliations.0["religion"]
                .groups
                .contains_key("religious_schools")
        );
        assert_eq!(loaded.report.attributes.religion_definitions, 3);
        assert!(
            !loaded
                .report
                .unapplied_country_fields
                .contains_key("religious_school")
        );
        let saved = loaded.save().unwrap();
        let mut resumed = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
        resumed.restore(&saved).unwrap();
        assert_eq!(resumed.save().unwrap(), saved);
    }
}

#[test]
fn missing_ambiguous_or_conditional_school_inputs_remain_unknown() {
    for history in [
        "religion=test_faith religious_school=missing",
        "religion=test_faith if={limit={always=yes} religious_school=first}",
    ] {
        let temp = fixture(history);
        let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
        let id = loaded.initial.country_entity("AAA").unwrap();
        assert!(presence().evaluate(loaded.initial.world(), id).is_err());
        assert!(
            loaded
                .report
                .religious_schools
                .unknown_countries
                .contains(&"AAA".into())
        );
        assert!(!loaded.report.religious_schools.history_sources["AAA"].is_empty());
    }
    let temp = fixture("religion=test_faith religious_school=first");
    write(
        temp.path(),
        "common/religions/duplicate.txt",
        "test_religions={religious_schools={first={}}}",
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let id = loaded.initial.country_entity("AAA").unwrap();
    assert!(presence().evaluate(loaded.initial.world(), id).is_err());
    assert_eq!(
        loaded.report.religious_schools.definition_sources["first"].len(),
        2
    );
}

#[test]
fn startup_candidates_school_queries_and_saved_delays_use_loaded_memberships() {
    let temp = fixture("religion=test_faith religious_school=first");
    write(
        temp.path(),
        "common/on_actions/test.txt",
        "on_startup={events={schools.selected schools.absent}}",
    );
    write(
        temp.path(),
        "events/test.txt",
        r#"
        country_event={id=schools.selected is_triggered_only=yes trigger={tag=AAA has_religious_school=yes religious_school={group=test_religions school=first}}
            immediate={log=selected} option={name=Continue country_event={id=schools.later days=1}}}
        country_event={id=schools.absent is_triggered_only=yes trigger={tag=BBB has_religious_school=no}
            immediate={log=absent} option={name=Continue}}
        country_event={id=schools.later is_triggered_only=yes trigger={has_religious_school=yes religious_school={group=test_religions school=first}}
            immediate={log=later} option={name=Continue}}
    "#,
    );
    let mut loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(
        loaded.report.estates.pending_startup_effects.is_empty(),
        "{:?}",
        loaded.report.estates.startup_unresolved
    );
    assert_eq!(
        loaded
            .initial
            .world()
            .events()
            .iter()
            .map(|event| event.2.as_str())
            .collect::<Vec<_>>(),
        ["selected", "absent"]
    );
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
        state.advance(1).unwrap();
        assert_eq!(state.initial.world().events().last().unwrap().2, "later");
        assert_eq!(state.initial.world().pending_events().len(), 2);
    }
    assert_eq!(loaded.save().unwrap(), resumed.save().unwrap());
}

#[test]
fn school_trigger_shapes_scope_and_literals_are_checked_before_folding() {
    let schema = Schema::new(["value"]).unwrap();
    let library = ScriptLibrary::new();
    for text in [
        "has_religious_school=maybe",
        "religious_school=first",
        "religious_school={school=first}",
        "religious_school={group=test school=first school=second}",
        "religious_school={group=test school=first extra=yes}",
        "religious_school={group=ROOT school=first}",
        "religious_school={group=test school=\"\"}",
    ] {
        let document = TextDocument::from_bytes(
            "self-authored-school.txt",
            format!("if={{limit={{always=no {text}}} log=unused}}").into_bytes(),
            TextEncoding::Utf8,
        );
        let error = ScriptCompiler::new(&schema, &library, true)
            .compile(&document, EntityKind::Country)
            .unwrap_err()
            .to_string();
        assert!(error.contains("self-authored-school.txt:1:"), "{error}");
    }
    let document = TextDocument::from_bytes(
        "self-authored-school.txt",
        b"if={limit={has_religious_school=yes} log=unused}".to_vec(),
        TextEncoding::Utf8,
    );
    assert!(
        ScriptCompiler::new(&schema, &library, true)
            .compile(&document, EntityKind::Province)
            .is_err()
    );
}
