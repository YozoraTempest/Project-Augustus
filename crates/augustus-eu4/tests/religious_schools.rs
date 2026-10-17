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
fn startup_school_effects_use_first_matching_branch_and_live_attribute_definitions() {
    let temp = fixture("religion=test_faith");
    write(
        temp.path(),
        "common/religions/test.txt",
        "test_religions={test_faith={} related_faith={} religious_schools={first={global_tax_modifier=0.25 picture=icon on_invite_scholar={log=unused}} second={global_tax_modifier=-0.5}}} remote_group={remote_faith={}}",
    );
    write(
        temp.path(),
        "common/on_actions/test.txt",
        r#"on_startup={
        if={limit={tag=AAA} set_religious_school={group=test_religions school=first} country_event={id=schools.change}}
        else_if={limit={always=yes} set_religious_school={group=test_religions school=second}}
    }"#,
    );
    write(
        temp.path(),
        "events/test.txt",
        r#"
        country_event={id=schools.change is_triggered_only=yes trigger={tag=AAA}
            immediate={
                if={limit={religious_school={group=test_religions school=first}} set_religious_school={group=test_religions school=second}}
                else_if={limit={religious_school={group=test_religions school=second}} set_religious_school={group=test_religions school=first}}
            } option={name=Continue country_event={id=schools.later days=1}}}
        country_event={id=schools.later is_triggered_only=yes trigger={religious_school={group=test_religions school=second}}
            immediate={set_religious_school={group=test_religions school=first}} option={name=Continue}}
    "#,
    );
    let mut loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(
        loaded.report.estates.pending_startup_effects.is_empty(),
        "{:?}",
        loaded.report.estates.startup_unresolved
    );
    assert_eq!(loaded.report.attributes.religious_school_definitions, 2);
    let country = loaded.initial.country_entity("AAA").unwrap();
    let province = loaded.initial.province_entity(2).unwrap();
    assert_eq!(
        loaded
            .initial
            .world()
            .affiliations()
            .member("religious_school", country),
        Some("second")
    );
    let query = |world: &augustus_core::World| {
        world
            .attributes()
            .evaluate(world, province, "tax_basis")
            .unwrap()
    };
    let second = query(loaded.initial.world());
    assert!(second.steps[0].contributions.iter().any(|c| c.source
        == "affiliation:religious_school:country:second"
        && c.micros == -500_000));
    loaded
        .initial
        .choose_event(&augustus_core::EventChoice {
            pending: augustus_core::PendingEventId(0),
            option: "0".into(),
        })
        .unwrap();
    let mut resumed = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    resumed.restore(&loaded.save().unwrap()).unwrap();
    for state in [&mut loaded, &mut resumed] {
        state.advance(1).unwrap();
        let first = query(state.initial.world());
        assert_eq!(
            state
                .initial
                .world()
                .affiliations()
                .member("religious_school", country),
            Some("first")
        );
        assert!(first.steps[0].contributions.iter().any(|c| c.source
            == "affiliation:religious_school:country:first"
            && c.micros == 250_000));
        assert!(
            !first
                .unresolved
                .iter()
                .any(|reason| reason.contains("on_invite_scholar"))
        );
        assert_eq!(
            first.steps[0].rate_micros - second.steps[0].rate_micros,
            750_000
        );
    }
    assert_eq!(loaded.save().unwrap(), resumed.save().unwrap());
}

#[test]
fn school_effects_validate_every_branch_and_reject_unknown_or_ambiguous_definitions() {
    let schema = Schema::new(["value"]).unwrap();
    let library = ScriptLibrary::new();
    for effect in [
        "set_religious_school=first",
        "set_religious_school={group=family school=missing}",
        "set_religious_school={group=other school=first}",
        "set_religious_school={group=family school=ambiguous}",
        "set_religious_school={group=family school=first extra=yes}",
        "set_religious_school={group=family school=ROOT}",
    ] {
        let document = TextDocument::from_bytes(
            "self-authored-effects.txt",
            format!(
                "if={{limit={{always=yes}} log=selected}} else_if={{limit={{always=no}} {effect}}}"
            )
            .into_bytes(),
            TextEncoding::Utf8,
        );
        let error = ScriptCompiler::new(&schema, &library, true)
            .with_religious_school_groups(
                [
                    ("first".into(), Some("family".into())),
                    ("ambiguous".into(), None),
                ]
                .into(),
            )
            .compile(&document, EntityKind::Country)
            .unwrap_err()
            .to_string();
        assert!(error.contains("self-authored-effects.txt:1:"), "{error}");
    }
    let document = TextDocument::from_bytes(
        "self-authored-effects.txt",
        b"set_religious_school={group=family school=first}".to_vec(),
        TextEncoding::Utf8,
    );
    assert!(
        ScriptCompiler::new(&schema, &library, true)
            .with_religious_school_groups([("first".into(), Some("family".into()))].into())
            .compile(&document, EntityKind::Province)
            .is_err()
    );
}

#[test]
fn school_attribute_ambiguities_taint_only_the_referenced_property() {
    let temp = fixture("religion=test_faith religious_school=first");
    write(
        temp.path(),
        "common/religions/test.txt",
        "test_religions={test_faith={} related_faith={} religious_schools={first={global_tax_modifier=0.1 global_tax_modifier=0.2 global_manpower_modifier=0.3}}}",
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let province = loaded.initial.province_entity(2).unwrap();
    let query = |name| {
        loaded
            .initial
            .world()
            .attributes()
            .evaluate(loaded.initial.world(), province, name)
            .unwrap()
    };
    assert!(
        query("tax_basis")
            .unresolved
            .iter()
            .any(|reason| reason.contains("global_tax_modifier"))
    );
    assert!(
        !query("manpower_contribution")
            .unresolved
            .iter()
            .any(|reason| reason.contains("global_tax_modifier"))
    );
    write(
        temp.path(),
        "common/religions/duplicate.txt",
        "test_religions={religious_schools={first={global_tax_modifier=0.9}}}",
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(
        loaded
            .report
            .attributes
            .ambiguous_definitions
            .contains_key("religious_schools:first")
    );
    assert!(
        !loaded.initial.world().attributes().affiliation_modifiers["religious_school"]["country"]
            .contains_key("first")
    );
}

#[test]
fn unsupported_school_property_values_are_not_silently_treated_as_zero() {
    let temp = fixture("religion=test_faith religious_school=first");
    write(
        temp.path(),
        "common/religions/test.txt",
        "test_religions={test_faith={} related_faith={} religious_schools={first={global_tax_modifier=unknown global_manpower_modifier=0.3}}}",
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let province = loaded.initial.province_entity(2).unwrap();
    let query = |name| {
        loaded
            .initial
            .world()
            .attributes()
            .evaluate(loaded.initial.world(), province, name)
            .unwrap()
    };
    assert!(
        query("tax_basis")
            .unresolved
            .iter()
            .any(|reason| reason.contains("global_tax_modifier"))
    );
    assert!(
        query("manpower_contribution").steps[0]
            .contributions
            .iter()
            .any(
                |c| c.source == "affiliation:religious_school:country:first" && c.micros == 300_000
            )
    );
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
fn random_country_scopes_set_school_opinion_against_the_previous_country_school() {
    let temp = fixture("religion=test_faith religious_school=first");
    write(
        temp.path(),
        "history/countries/BBB - Beta.txt",
        "capital=2 primary_culture=beta religion=test_faith religious_school=second",
    );
    write(
        temp.path(),
        "history/countries/BBB - Beta alternate.txt",
        "capital=2 primary_culture=beta religion=test_faith religious_school=second",
    );
    write(
        temp.path(),
        "common/on_actions/test.txt",
        "on_startup={if={limit={tag=AAA} random_country={limit={religious_school={group=test_religions school=second}} set_school_opinion={who=PREV opinion=-1}}}}",
    );

    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(loaded.report.estates.startup_unresolved.is_empty());
    let beta = loaded.initial.country_entity("BBB").unwrap();
    assert_eq!(
        loaded
            .initial
            .world()
            .school_opinion(beta, "first")
            .unwrap(),
        -1
    );
}

#[test]
fn duplicate_country_histories_keep_agreed_school_state_and_reject_disagreement() {
    let temp = fixture("religion=test_faith religious_school=first");
    write(
        temp.path(),
        "history/countries/AAA - Alternate.txt",
        "capital=1 primary_culture=alpha religion=test_faith religious_school=first",
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let id = loaded.initial.country_entity("AAA").unwrap();
    assert_eq!(
        loaded
            .initial
            .world()
            .affiliations()
            .member("religious_school", id),
        Some("first")
    );
    assert!(
        loaded
            .report
            .ambiguous_country_histories
            .contains_key("AAA")
    );
    assert!(
        !loaded
            .report
            .religious_schools
            .unknown_countries
            .contains(&"AAA".into())
    );

    let temp = fixture("religion=test_faith religious_school=first");
    write(
        temp.path(),
        "history/countries/AAA - Alternate.txt",
        "capital=1 primary_culture=alpha religion=test_faith religious_school=second",
    );
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
