// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::DiplomaticKind;
use augustus_eu4::VanillaWorld;
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
