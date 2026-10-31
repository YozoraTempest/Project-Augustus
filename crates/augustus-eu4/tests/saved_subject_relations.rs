// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::DiplomaticKind;
use augustus_eu4::{
    ContentRoot, ContentSet, ContentSource, TextEncoding, VanillaSaveStateImports,
    VanillaSavedSubjectRelation, VanillaSubjectRelationsImport, VanillaWorld,
    read_vanilla_save_state,
};
use std::{fs, path::Path};

const MISSIONS: &str = r#"
saved_subject_series = {
    slot = 1
    potential = { tag = AAA }
    saved_subject_task = {
        position = 1
        trigger = {
            is_subject_of = BBB
            is_subject_of_type = test_union
            overlord = { tag = BBB }
            BBB = { num_of_subjects = 1 any_subject_country = { tag = AAA is_subject_of_type = test_union } }
            NOT = { CCC = { num_of_subjects = 2 } }
        }
        effect = { add_treasury = 13 }
    }
}
"#;

const STARTUP: &str = r#"
on_startup = {
    if = {
        limit = { tag = AAA is_subject_of = BBB is_subject_of_type = test_union }
        add_treasury = 3
        log = saved_subject_startup
    }
}
"#;

fn fixture(startup: bool) -> (tempfile::TempDir, ContentSet) {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("descriptor.mod"),
        "name=\"Saved Subject Relations\"\n",
    )
    .unwrap();
    fs::create_dir_all(directory.path().join("missions")).unwrap();
    fs::create_dir_all(directory.path().join("common/subject_types")).unwrap();
    fs::create_dir_all(directory.path().join("common/country_tags")).unwrap();
    fs::create_dir_all(directory.path().join("common/countries")).unwrap();
    fs::create_dir_all(directory.path().join("history/countries")).unwrap();
    fs::write(
        directory.path().join("missions/saved_subjects.txt"),
        MISSIONS,
    )
    .unwrap();
    fs::write(
        directory
            .path()
            .join("common/subject_types/saved_subjects.txt"),
        "test_union = { copy_from = default }\n",
    )
    .unwrap();
    fs::write(
        directory
            .path()
            .join("common/country_tags/saved_subjects.txt"),
        "CCC = \"countries/Gamma.txt\"\n",
    )
    .unwrap();
    fs::write(
        directory.path().join("common/countries/Gamma.txt"),
        "color = { 40 120 70 }\n",
    )
    .unwrap();
    fs::write(
        directory.path().join("history/countries/CCC - Gamma.txt"),
        "capital = 1\nprimary_culture = alpha\nreligion = test_faith\ngovernment = monarchy\n",
    )
    .unwrap();
    if startup {
        fs::create_dir_all(directory.path().join("common/on_actions")).unwrap();
        fs::write(
            directory
                .path()
                .join("common/on_actions/saved_subjects.txt"),
            STARTUP,
        )
        .unwrap();
    }

    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world");
    let content = ContentSet::resolve(vec![
        ContentSource::base(ContentRoot::open(base).unwrap()),
        ContentSource::mod_directory(directory.path(), TextEncoding::Utf8).unwrap(),
    ])
    .unwrap();
    (directory, content)
}

fn write_save(root: &Path, name: &str, diplomacy: Option<&str>) -> std::path::PathBuf {
    write_save_at(root, name, diplomacy, "1444.11.11")
}

fn write_save_at(
    root: &Path,
    name: &str,
    diplomacy: Option<&str>,
    date: &str,
) -> std::path::PathBuf {
    let path = root.join(name);
    let diplomacy = diplomacy.unwrap_or("");
    fs::write(
        &path,
        format!(
            "EU4txt\ndate={date}\nplayer=AAA\n{diplomacy}\ncountries={{AAA={{human=yes treasury=5.000 completed_missions={{}} country_missions={{mission_slot={{saved_subject_series}}}}}} BBB={{human=no treasury=0}} CCC={{human=no treasury=0}}}}\nprovinces={{}}\n"
        ),
    )
    .unwrap();
    path
}

fn saved_subject_graph() -> &'static str {
    "diplomacy={ dependency={first=BBB second=AAA subject_type=test_union} dependency={first=CCC second=BBB subject_type=test_vassal} }"
}

#[test]
fn saved_relations_replace_history_and_drive_queries_startup_missions_and_restore() {
    let (directory, content) = fixture(true);
    let path = write_save_at(
        directory.path(),
        "saved-subjects.eu4",
        Some(saved_subject_graph()),
        "1445.1.1",
    );
    let mut loaded = VanillaWorld::load_from_content_with_save(&content, &path).unwrap();
    let alpha = loaded.initial.country_entity("AAA").unwrap();
    let beta = loaded.initial.country_entity("BBB").unwrap();
    let gamma = loaded.initial.country_entity("CCC").unwrap();
    let world = loaded.initial.world();

    assert!(
        loaded.report.estates.startup_unresolved.is_empty(),
        "startup unresolved: {:?}",
        loaded.report.estates.startup_unresolved
    );
    assert!(world.subjects().known);
    assert!(world.is_subject_of(alpha, beta).unwrap());
    assert!(!world.is_subject_of(beta, alpha).unwrap());
    assert_eq!(world.subject_overlord(alpha).unwrap(), Some(beta));
    assert!(world.is_subject_of_type(alpha, "test_union").unwrap());
    assert!(!world.is_subject_of_type(alpha, "test_vassal").unwrap());
    assert_eq!(world.subject_count(alpha).unwrap(), 0);
    assert_eq!(world.subject_count(beta).unwrap(), 1);
    assert_eq!(world.subject_countries(beta).unwrap(), [alpha]);
    assert_eq!(world.subject_count(gamma).unwrap(), 1);
    assert_eq!(world.subject_countries(gamma).unwrap(), [beta]);
    assert_eq!(
        world
            .politics()
            .relations
            .iter()
            .filter(|relation| relation.kind == DiplomaticKind::Subject)
            .count(),
        2
    );
    assert!(world.politics().relations.iter().any(|relation| {
        relation.kind == DiplomaticKind::Alliance
            && ((relation.first == beta && relation.second == alpha)
                || (relation.first == alpha && relation.second == beta))
    }));
    assert!(
        loaded
            .initial
            .world()
            .events()
            .iter()
            .any(|event| event.2 == "saved_subject_startup")
    );
    assert_eq!(
        loaded
            .country("AAA")
            .unwrap()
            .resources
            .unwrap()
            .treasury_milli,
        Some(8_000)
    );

    let before_query = loaded.save().unwrap();
    let report = loaded.country_missions("AAA").unwrap();
    assert!(report.selection_known);
    let task = report
        .missions
        .iter()
        .find(|mission| mission.id == "saved_subject_task")
        .unwrap();
    assert_eq!(task.selected, Some(true));
    assert_eq!(task.requirements_met, Some(true));
    assert!(task.claimable, "{:?}", task.unresolved);
    assert_eq!(
        loaded.save().unwrap(),
        before_query,
        "queries remain read-only"
    );

    loaded.claim_mission("AAA", "saved_subject_task").unwrap();
    assert_eq!(
        loaded
            .country("AAA")
            .unwrap()
            .resources
            .unwrap()
            .treasury_milli,
        Some(21_000)
    );
    let claimed = loaded.save().unwrap();
    loaded.restore(&claimed).unwrap();
    assert!(
        loaded
            .country_missions("AAA")
            .unwrap()
            .missions
            .iter()
            .find(|mission| mission.id == "saved_subject_task")
            .unwrap()
            .completed
            .unwrap()
    );
    assert!(loaded.claim_mission("AAA", "saved_subject_task").is_err());
    assert_eq!(loaded.save().unwrap(), claimed);
}

#[test]
fn missing_and_explicitly_empty_diplomacy_keep_unknown_distinct_from_independence() {
    let (directory, content) = fixture(false);
    let missing_path = write_save(directory.path(), "missing-diplomacy.eu4", None);
    let missing = VanillaWorld::load_from_content_with_save(&content, &missing_path).unwrap();
    let alpha = missing.initial.country_entity("AAA").unwrap();
    assert!(!missing.initial.world().subjects().known);
    assert!(missing.initial.world().subjects().counts.is_empty());
    assert!(
        !missing
            .initial
            .world()
            .politics()
            .relations
            .iter()
            .any(|relation| relation.kind == DiplomaticKind::Subject)
    );
    assert!(missing.initial.world().subject_count(alpha).is_err());
    assert_eq!(
        missing
            .initial
            .world()
            .politics()
            .status(alpha, "colonial_nation"),
        None
    );
    let missing_task = missing
        .country_missions("AAA")
        .unwrap()
        .missions
        .into_iter()
        .find(|mission| mission.id == "saved_subject_task")
        .unwrap();
    assert_eq!(missing_task.requirements_met, None);
    assert!(!missing_task.claimable);

    let empty_path = write_save(
        directory.path(),
        "empty-diplomacy.eu4",
        Some("diplomacy={}"),
    );
    let empty = VanillaWorld::load_from_content_with_save(&content, &empty_path).unwrap();
    let alpha = empty.initial.country_entity("AAA").unwrap();
    assert!(empty.initial.world().subjects().known);
    assert_eq!(empty.initial.world().subject_count(alpha).unwrap(), 0);
    assert!(
        !empty
            .initial
            .world()
            .politics()
            .relations
            .iter()
            .any(|relation| relation.kind == DiplomaticKind::Subject)
    );
    assert_eq!(
        empty
            .initial
            .world()
            .politics()
            .status(alpha, "colonial_nation"),
        Some(false)
    );
    let empty_task = empty
        .country_missions("AAA")
        .unwrap()
        .missions
        .into_iter()
        .find(|mission| mission.id == "saved_subject_task")
        .unwrap();
    assert_eq!(empty_task.requirements_met, Some(false));
    assert!(!empty_task.claimable);
}

#[test]
fn translated_save_state_without_subject_import_preserves_history_graph_behavior() {
    let (directory, content) = fixture(false);
    let path = write_save(directory.path(), "translated-save.eu4", None);
    let save = read_vanilla_save_state(&path).unwrap();
    let loaded = VanillaWorld::load_from_content_with_save_state(&content, &save).unwrap();
    let alpha = loaded.initial.country_entity("AAA").unwrap();
    let beta = loaded.initial.country_entity("BBB").unwrap();
    assert!(loaded.initial.world().subjects().known);
    assert!(loaded.initial.world().is_subject_of(beta, alpha).unwrap());
    assert!(
        loaded
            .initial
            .world()
            .is_subject_of_type(beta, "test_vassal")
            .unwrap()
    );
}

#[test]
fn sidecar_date_and_invalid_subject_graphs_are_rejected() {
    let (directory, content) = fixture(false);
    let path = write_save(directory.path(), "validation-save.eu4", None);
    let save = read_vanilla_save_state(&path).unwrap();

    let relation =
        |overlord: &str, subject: &str, subject_type: &str| VanillaSavedSubjectRelation {
            overlord: overlord.into(),
            subject: subject.into(),
            subject_type: subject_type.into(),
        };
    let rejected = |date: &str, relations: Vec<VanillaSavedSubjectRelation>| {
        let imported = VanillaSubjectRelationsImport {
            date: date.into(),
            relations: Some(relations),
        };
        VanillaWorld::load_from_content_with_save_state_and_imports(
            &content,
            &save,
            VanillaSaveStateImports {
                subject_relations: Some(&imported),
                ..Default::default()
            },
            None,
        )
    };

    assert!(rejected("1444.11.12", vec![]).is_err());
    assert!(rejected("1444.11.11", vec![relation("AAA", "AAA", "test_union")]).is_err());
    assert!(
        rejected(
            "1444.11.11",
            vec![
                relation("BBB", "AAA", "test_union"),
                relation("BBB", "AAA", "test_union")
            ]
        )
        .is_err()
    );
    assert!(
        rejected(
            "1444.11.11",
            vec![
                relation("BBB", "AAA", "test_union"),
                relation("CCC", "AAA", "test_union")
            ]
        )
        .is_err()
    );
    assert!(
        rejected(
            "1444.11.11",
            vec![
                relation("BBB", "AAA", "test_union"),
                relation("AAA", "BBB", "test_vassal")
            ]
        )
        .is_err()
    );
    assert!(rejected("1444.11.11", vec![relation("ZZZ", "AAA", "test_union")]).is_err());
    assert!(rejected("1444.11.11", vec![relation("BBB", "AAA", "missing_type")]).is_err());
}
