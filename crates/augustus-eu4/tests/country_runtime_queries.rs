// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_eu4::{
    VanillaCountryRuntimeQuery, VanillaExternalState, VanillaSaveState, VanillaWorld,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

fn copy_fixture() -> tempfile::TempDir {
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
    temp
}

#[test]
fn country_runtime_query_exposes_bound_missions_and_variables_without_mutation() {
    let fixture = copy_fixture();
    fs::create_dir_all(fixture.path().join("missions")).unwrap();
    fs::write(
        fixture.path().join("missions/test.txt"),
        "test_series={slot=1 sample_mission={position=1}}",
    )
    .unwrap();
    let baseline = VanillaWorld::load(fixture.path(), "1444.11.11").unwrap();
    let save = VanillaSaveState {
        date: "1444.11.11".into(),
        player: Some("AAA".into()),
        players: BTreeSet::from(["AAA".into()]),
        external_state: VanillaExternalState {
            country_mission_completions: BTreeMap::from([(
                "AAA".into(),
                BTreeSet::from(["sample_mission".into()]),
            )]),
            country_variables: BTreeMap::from([(
                "AAA".into(),
                BTreeMap::from([("saved_counter".into(), 2_375)]),
            )]),
            ..Default::default()
        },
    };
    let state = save.state_for_world(&baseline).unwrap();
    let loaded =
        VanillaWorld::load_with_external_state(fixture.path(), "1444.11.11", &state).unwrap();
    let before = loaded.save().unwrap();

    let report = loaded
        .query_vanilla_country_runtime(&VanillaCountryRuntimeQuery {
            country: "AAA".into(),
        })
        .unwrap();
    assert_eq!(report.country, "AAA");
    assert!(report.mission_ids.contains("sample_mission"));
    assert_eq!(
        report.completed_missions,
        Some(BTreeSet::from(["sample_mission".into()]))
    );
    assert_eq!(
        report.available_missions,
        Some(BTreeSet::from(["sample_mission".into()]))
    );
    assert_eq!(
        report.variables,
        Some(BTreeMap::from([("saved_counter".into(), 2_375)]))
    );
    assert!(report.variables_known);
    assert_eq!(loaded.save().unwrap(), before);
}

#[test]
fn country_runtime_query_distinguishes_unknown_and_known_empty_namespaces() {
    let fixture = copy_fixture();
    fs::create_dir_all(fixture.path().join("missions")).unwrap();
    fs::write(
        fixture.path().join("missions/test.txt"),
        "test_series={slot=1 sample_mission={position=1}}",
    )
    .unwrap();
    let unknown = VanillaWorld::load(fixture.path(), "1445.1.1").unwrap();
    let report = unknown
        .query_vanilla_country_runtime(&VanillaCountryRuntimeQuery {
            country: "AAA".into(),
        })
        .unwrap();
    assert_eq!(report.completed_missions, None);
    assert_eq!(report.available_missions, None);
    assert_eq!(report.variables, None);
    assert!(!report.variables_known);

    let state = VanillaExternalState {
        country_mission_completions: BTreeMap::from([("AAA".into(), BTreeSet::new())]),
        country_variables: BTreeMap::from([("AAA".into(), BTreeMap::new())]),
        ..Default::default()
    };
    let known = VanillaWorld::load_with_external_state(fixture.path(), "1445.1.1", &state).unwrap();
    let report = known
        .query_vanilla_country_runtime(&VanillaCountryRuntimeQuery {
            country: "AAA".into(),
        })
        .unwrap();
    assert_eq!(report.completed_missions, Some(BTreeSet::new()));
    assert_eq!(report.variables, Some(BTreeMap::new()));
    assert!(report.variables_known);
}

#[test]
fn country_runtime_query_rejects_unknown_country_and_fields() {
    let fixture = copy_fixture();
    let loaded = VanillaWorld::load(fixture.path(), "1444.11.11").unwrap();
    let error = loaded
        .query_vanilla_country_runtime(&VanillaCountryRuntimeQuery {
            country: "ZZZ".into(),
        })
        .unwrap_err()
        .to_string();
    assert!(error.contains("unknown EU4 country ZZZ"));

    let error = serde_json::from_str::<VanillaCountryRuntimeQuery>(
        r#"{"country":"AAA","unexpected":true}"#,
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("unknown field"));
}
