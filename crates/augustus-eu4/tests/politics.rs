// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{Action, Condition, DiplomaticKind, EntityKind, Schema};
use augustus_eu4::{
    ScriptCompiler, ScriptLibrary, TextDocument, TextEncoding, VanillaExternalState, VanillaWorld,
    read_vanilla_save_state,
};
use std::{
    fs,
    path::{Path, PathBuf},
};

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world")
}

fn copy_fixture() -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    for entry in walkdir::WalkDir::new(fixture()) {
        let entry = entry.unwrap();
        let path = temp
            .path()
            .join(entry.path().strip_prefix(fixture()).unwrap());
        if entry.file_type().is_dir() {
            fs::create_dir_all(path).unwrap();
        } else {
            fs::copy(entry.path(), path).unwrap();
        }
    }
    temp
}

fn write(root: &Path, relative_path: &str, text: &str) {
    let path = root.join(relative_path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn has_idea_group(world: &VanillaWorld, country: &str, group: &str) -> bool {
    world
        .country(country)
        .unwrap()
        .ideas
        .and_then(|ideas| ideas.groups.as_ref())
        .is_some_and(|groups| groups.contains_key(group))
}

fn country_condition(script: &str, optimise: bool) -> Condition {
    let source = format!("if={{limit={{{script}}} log=elector_condition_matched}}");
    let document = TextDocument::from_bytes(
        "self-authored-elector-condition.txt",
        source.into_bytes(),
        TextEncoding::Utf8,
    );
    let compiled = ScriptCompiler::new(
        &Schema::new([] as [&str; 0]).unwrap(),
        &ScriptLibrary::new(),
        optimise,
    )
    .compile(&document, EntityKind::Country)
    .unwrap();
    match compiled.plan.as_slice() {
        [Action::If { condition, .. }] => condition.clone(),
        _ => panic!("expected one conditional action"),
    }
}

fn elector_condition(script: &str, optimise: bool) -> Condition {
    country_condition(script, optimise)
}

#[test]
fn ruler_selection_preserves_optional_values_and_nested_source_diagnostics() {
    let loaded = VanillaWorld::load(fixture(), "1444.11.11").unwrap();
    let alpha = loaded.country("AAA").unwrap();
    let politics = alpha.politics.unwrap();
    let ruler = politics.ruler.as_ref().unwrap();
    assert_eq!(ruler.name.as_deref(), Some("Iris"));
    assert_eq!(
        (ruler.administration, ruler.diplomacy, ruler.military),
        (Some(2), Some(3), Some(4))
    );
    let heir = politics.heir.as_ref().unwrap();
    assert_eq!(heir.administration, Some(1));
    assert_eq!(heir.claim_milli, Some(87125));
    assert_eq!(heir.block_disinherit, Some(true));
    let consort = politics.consort.as_ref().unwrap();
    assert_eq!(
        consort.country_of_origin,
        loaded.initial.country_entity("BBB")
    );
    assert_eq!(consort.administration, None);
    assert_eq!(loaded.report.unapplied_country_fields["monarch.leader"], 1);
    assert!(loaded.report.unapplied_examples["country:monarch.leader"].line > 10);
    assert!(
        loaded.report.person_sources["AAA"]["ruler"]
            .path
            .ends_with("AAA - Alpha.txt")
    );
    assert_eq!(
        loaded
            .country("BBB")
            .unwrap()
            .politics
            .unwrap()
            .ruler
            .as_ref()
            .unwrap()
            .regent,
        Some(true)
    );
}

#[test]
fn start_is_included_end_is_excluded_and_future_history_is_not_scheduled() {
    let opening = VanillaWorld::load(fixture(), "1444.11.11").unwrap();
    let kinds: Vec<_> = opening.diplomacy().iter().map(|r| r.kind).collect();
    assert!(kinds.contains(&DiplomaticKind::Guarantee));
    assert!(!kinds.contains(&DiplomaticKind::RoyalMarriage));
    assert!(kinds.contains(&DiplomaticKind::TradeLeague));
    let tomorrow = VanillaWorld::load(fixture(), "1444.11.12").unwrap();
    assert!(
        tomorrow
            .country("AAA")
            .unwrap()
            .politics
            .unwrap()
            .heir
            .is_none()
    );
    assert!(tomorrow.report.expired_people.contains(&"AAA:heir".into()));
    assert!(
        !tomorrow
            .diplomacy()
            .iter()
            .any(|r| r.kind == DiplomaticKind::Warning)
    );
    let emperor = opening.initial.world().politics().offices["imperial_emperor"];
    assert_eq!(opening.initial.world().entity(emperor).unwrap().name, "AAA");
    assert_eq!(opening.initial.world().pending_tasks(), 0);
}

#[test]
fn elector_history_and_queries_follow_the_selected_date_and_preserve_unknowns() {
    let temp = copy_fixture();
    let country_history = temp.path().join("history/countries/AAA - Alpha.txt");
    let existing = fs::read_to_string(&country_history).unwrap();
    fs::write(
        &country_history,
        format!("{existing}\nelector = yes\n1450.1.1 = {{ elector = no }}\n"),
    )
    .unwrap();
    write(
        temp.path(),
        "common/ideas/elector.txt",
        "elector_ideas={free=yes trigger={is_elector=yes} one={}}",
    );

    let opening = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let alpha = opening.initial.country_entity("AAA").unwrap();
    let beta = opening.initial.country_entity("BBB").unwrap();
    let world = opening.initial.world();
    assert_eq!(world.politics().status(alpha, "elector"), Some(true));
    assert_eq!(world.politics().status(beta, "elector"), None);
    assert!(
        !opening
            .report
            .unapplied_country_fields
            .contains_key("elector")
    );
    assert!(opening.report.ideas.unknown_statuses["BBB"].contains("elector"));
    assert!(has_idea_group(&opening, "AAA", "elector_ideas"));
    assert!(!has_idea_group(&opening, "BBB", "elector_ideas"));

    for optimise in [false, true] {
        let yes = elector_condition("is_elector=yes", optimise);
        let no = elector_condition("is_elector=no", optimise);
        assert!(yes.evaluate(world, alpha).unwrap());
        assert!(!no.evaluate(world, alpha).unwrap());
        assert!(yes.evaluate(world, beta).is_err());
        assert!(no.evaluate(world, beta).is_err());
    }

    let later = VanillaWorld::load(temp.path(), "1450.1.1").unwrap();
    assert_eq!(
        later
            .initial
            .world()
            .politics()
            .status(later.initial.country_entity("AAA").unwrap(), "elector"),
        Some(false)
    );
    assert!(!later.report.ideas.unknown_statuses["AAA"].contains("elector"));
    assert!(!has_idea_group(&later, "AAA", "elector_ideas"));
    let no = elector_condition("is_elector=no", false);
    assert!(
        no.evaluate(
            later.initial.world(),
            later.initial.country_entity("AAA").unwrap()
        )
        .unwrap()
    );
}

#[test]
fn saved_elector_roster_overrides_history_and_drives_elector_queries() {
    let temp = copy_fixture();
    for (file, status) in [("AAA - Alpha.txt", "yes"), ("BBB - Beta.txt", "no")] {
        let path = temp.path().join("history/countries").join(file);
        let existing = fs::read_to_string(&path).unwrap();
        fs::write(&path, format!("{existing}\nelector = {status}\n")).unwrap();
    }

    let save_directory = tempfile::tempdir().unwrap();
    let save_path = save_directory.path().join("electors.eu4");
    fs::write(
        &save_path,
        r#"EU4txt
date=1444.11.11
empire={ electors={ BBB } }
countries={ AAA={} BBB={} }
provinces={}
"#,
    )
    .unwrap();

    let save = read_vanilla_save_state(&save_path).unwrap();
    let content = VanillaWorld::load(temp.path(), &save.date).unwrap();
    let external_state = save.state_for_world(&content).unwrap();
    let loaded =
        VanillaWorld::load_with_external_state(temp.path(), &save.date, &external_state).unwrap();
    let world = loaded.initial.world();
    let alpha = loaded.initial.country_entity("AAA").unwrap();
    let beta = loaded.initial.country_entity("BBB").unwrap();

    assert_eq!(world.politics().status(alpha, "elector"), Some(false));
    assert_eq!(world.politics().status(beta, "elector"), Some(true));
    for optimise in [false, true] {
        let yes = elector_condition("is_elector=yes", optimise);
        let no = elector_condition("is_elector=no", optimise);
        assert!(!yes.evaluate(world, alpha).unwrap());
        assert!(no.evaluate(world, alpha).unwrap());
        assert!(yes.evaluate(world, beta).unwrap());
        assert!(!no.evaluate(world, beta).unwrap());
    }
}

#[test]
fn saved_emperor_overrides_history_and_drives_emperor_queries() {
    let temp = copy_fixture();
    let save_directory = tempfile::tempdir().unwrap();
    let save_path = save_directory.path().join("emperor.eu4");
    fs::write(
        &save_path,
        r#"EU4txt
date=1444.11.11
empire={ emperor="BBB" }
countries={ AAA={} BBB={} }
provinces={}
"#,
    )
    .unwrap();

    let save = read_vanilla_save_state(&save_path).unwrap();
    let content = VanillaWorld::load(temp.path(), &save.date).unwrap();
    assert_eq!(
        content
            .initial
            .world()
            .politics()
            .offices
            .get("imperial_emperor"),
        Some(&content.initial.country_entity("AAA").unwrap())
    );
    let external_state = save.state_for_world(&content).unwrap();
    let loaded =
        VanillaWorld::load_with_external_state(temp.path(), &save.date, &external_state).unwrap();
    let world = loaded.initial.world();
    let alpha = loaded.initial.country_entity("AAA").unwrap();
    let beta = loaded.initial.country_entity("BBB").unwrap();

    assert_eq!(
        world.politics().offices.get("imperial_emperor"),
        Some(&beta)
    );
    for optimise in [false, true] {
        let yes = country_condition("is_emperor=yes", optimise);
        let no = country_condition("is_emperor=no", optimise);
        assert!(!yes.evaluate(world, alpha).unwrap());
        assert!(no.evaluate(world, alpha).unwrap());
        assert!(yes.evaluate(world, beta).unwrap());
        assert!(!no.evaluate(world, beta).unwrap());
    }

    let cleared_state = VanillaExternalState {
        political_office_holders: [("imperial_emperor".into(), None)].into(),
        ..Default::default()
    };
    let cleared =
        VanillaWorld::load_with_external_state(temp.path(), &save.date, &cleared_state).unwrap();
    assert!(
        !cleared
            .initial
            .world()
            .politics()
            .offices
            .contains_key("imperial_emperor")
    );
}

#[test]
fn nested_elector_effects_in_country_history_leave_the_status_unknown() {
    let temp = copy_fixture();
    let country_history = temp.path().join("history/countries/AAA - Alpha.txt");
    let existing = fs::read_to_string(&country_history).unwrap();
    fs::write(
        &country_history,
        format!("{existing}\nelector = yes\nif = {{ limit = {{ always = yes }} elector = no }}\n"),
    )
    .unwrap();

    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let alpha = loaded.initial.country_entity("AAA").unwrap();
    assert_eq!(
        loaded.initial.world().politics().status(alpha, "elector"),
        None
    );
    assert!(loaded.report.ideas.unknown_statuses["AAA"].contains("elector"));
}

#[test]
fn conflicting_country_histories_do_not_select_an_elector_status() {
    let temp = copy_fixture();
    let country_history = temp.path().join("history/countries/AAA - Alpha.txt");
    let existing = fs::read_to_string(&country_history).unwrap();
    fs::write(&country_history, format!("{existing}\nelector = yes\n")).unwrap();
    write(
        temp.path(),
        "history/countries/AAA - Alternate.txt",
        "elector = no\n",
    );

    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let alpha = loaded.initial.country_entity("AAA").unwrap();
    assert_eq!(
        loaded.initial.world().politics().status(alpha, "elector"),
        None
    );
    assert_eq!(loaded.report.ambiguous_country_histories["AAA"].len(), 2);
    assert!(loaded.report.ideas.unknown_statuses["AAA"].contains("elector"));
}

#[test]
fn elector_status_rejects_invalid_history_values_and_non_country_queries() {
    let temp = copy_fixture();
    let country_history = temp.path().join("history/countries/AAA - Alpha.txt");
    let existing = fs::read_to_string(&country_history).unwrap();
    fs::write(&country_history, format!("{existing}\nelector = maybe\n")).unwrap();
    let error = VanillaWorld::load(temp.path(), "1444.11.11")
        .unwrap_err()
        .to_string();
    assert!(error.contains("expected yes or no"), "{error}");
    assert!(error.contains("AAA - Alpha.txt"), "{error}");

    let source = TextDocument::from_bytes(
        "self-authored-elector-condition.txt",
        b"if={limit={is_elector=maybe} log=invalid}".to_vec(),
        TextEncoding::Utf8,
    );
    let schema = Schema::new([] as [&str; 0]).unwrap();
    let library = ScriptLibrary::new();
    assert!(
        ScriptCompiler::new(&schema, &library, false)
            .compile(&source, EntityKind::Country)
            .is_err()
    );
    assert!(
        ScriptCompiler::new(&schema, &library, false)
            .compile(&source, EntityKind::Province)
            .is_err()
    );
}

#[test]
fn duplicate_symmetric_records_keep_both_sources_but_one_core_relation() {
    let temp = copy_fixture();
    fs::write(
        temp.path().join("history/diplomacy/extra.txt"),
        "alliance = { first = AAA second = BBB start_date = 1444.1.1 end_date = 1450.1.1 }\n",
    )
    .unwrap();
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert_eq!(loaded.report.duplicate_relation_records, 1);
    assert_eq!(
        loaded
            .diplomacy()
            .iter()
            .filter(|r| r.kind == DiplomaticKind::Alliance)
            .count(),
        1
    );
    assert_eq!(
        loaded
            .report
            .relation_sources
            .iter()
            .filter(|r| r.relation.kind == DiplomaticKind::Alliance)
            .count(),
        2
    );
}

#[test]
fn office_selection_uses_chronology_across_files_and_can_be_cleared() {
    let temp = copy_fixture();
    fs::write(
        temp.path().join("history/diplomacy/z.txt"),
        "1420.1.1 = { emperor = BBB }\n1444.11.12 = { emperor = --- }\n",
    )
    .unwrap();
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert_eq!(
        loaded.initial.world().politics().offices["imperial_emperor"],
        loaded.initial.country_entity("AAA").unwrap()
    );
    let cleared = VanillaWorld::load(temp.path(), "1444.11.12").unwrap();
    assert!(
        !cleared
            .initial
            .world()
            .politics()
            .offices
            .contains_key("imperial_emperor")
    );
}

#[test]
fn invalid_people_relations_and_subject_references_fail_with_source_locations() {
    for (file, from, to, expected) in [
        (
            "history/countries/AAA - Alpha.txt",
            "adm = 2",
            "adm = 7",
            "skill outside",
        ),
        (
            "history/countries/AAA - Alpha.txt",
            "country_of_origin = BBB",
            "country_of_origin = ZZZ",
            "unknown person country",
        ),
        (
            "history/diplomacy/test.txt",
            "subject_type = test_vassal",
            "subject_type = missing",
            "unknown subject type",
        ),
        (
            "history/diplomacy/test.txt",
            "first = BBB second = AAA",
            "first = ZZZ second = AAA",
            "unknown diplomatic country",
        ),
    ] {
        let temp = copy_fixture();
        let path = temp.path().join(file);
        fs::write(&path, fs::read_to_string(&path).unwrap().replace(from, to)).unwrap();
        let error = VanillaWorld::load(temp.path(), "1444.11.11")
            .unwrap_err()
            .to_string();
        assert!(error.contains(expected), "{error}");
        assert!(
            error.contains(file.split('/').next_back().unwrap()),
            "{error}"
        );
    }
}
