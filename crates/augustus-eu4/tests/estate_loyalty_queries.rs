// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_eu4::{
    VanillaEstateLoyaltyQuery, VanillaExternalEstateState, VanillaExternalState, VanillaWorld,
};
use std::{collections::BTreeMap, fs, path::Path};

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
    let defines = fs::read_to_string(temp.path().join("common/defines.lua")).unwrap();
    fs::write(
        temp.path().join("common/defines.lua"),
        defines.replace(
            "NCountry = {",
            "NCountry = {\n        ESTATE_ANGRY_THRESHOLD = 30,\n        ESTATE_HAPPY_THRESHOLD = 60,\n        ESTATE_LOYALTY_DECAY_BASE_MIN = 2.0,\n        ESTATE_LOYALTY_DECAY_BASE_MAX = 5.0,",
        ),
    )
    .unwrap();
    fs::create_dir_all(temp.path().join("common/estates")).unwrap();
    fs::write(
        temp.path().join("common/estates/test.txt"),
        "estate_guild = { trigger = { tag = AAA } base_influence = 12 }",
    )
    .unwrap();
    temp
}

fn loaded() -> VanillaWorld {
    let temp = copy_fixture();
    let state = VanillaExternalState {
        estate_states: BTreeMap::from([(
            "AAA".into(),
            BTreeMap::from([(
                "estate_guild".into(),
                VanillaExternalEstateState {
                    loyalty_milli: Some(49_615),
                    land_share_micros: Some(150_000),
                    active_influences: None,
                },
            )]),
        )]),
        ..Default::default()
    };
    let mut world =
        VanillaWorld::load_with_external_state(temp.path(), "1444.11.11", &state).unwrap();
    world
        .set_estate_loyalty_targets(BTreeMap::from([(
            "AAA".into(),
            BTreeMap::from([("estate_guild".into(), Some(44_600))]),
        )]))
        .unwrap();
    world
}

#[test]
fn estate_loyalty_query_matches_daily_decay_and_is_read_only() {
    let world = loaded();
    let snapshot = world.save().unwrap();
    let report = world
        .query_vanilla_estate_loyalty(&VanillaEstateLoyaltyQuery {
            country: "AAA".into(),
            estate: Some("estate_guild".into()),
        })
        .unwrap();
    assert_eq!(report.entries.len(), 1);
    let entry = &report.entries[0];
    assert_eq!(entry.active, Some(true));
    assert!(entry.unresolved.is_empty());
    let evaluation = entry.evaluation.as_ref().unwrap();
    assert_eq!(evaluation.current_loyalty_milli, 49_615);
    assert_eq!(evaluation.target_loyalty_milli, 44_600);
    assert_eq!(evaluation.next_loyalty_milli, 49_424);
    assert_eq!(world.save().unwrap(), snapshot);
}

#[test]
fn estate_loyalty_query_reports_all_estates_and_unknown_boundaries() {
    let world = loaded();
    let report = world
        .query_vanilla_estate_loyalty(&VanillaEstateLoyaltyQuery {
            country: "AAA".into(),
            estate: None,
        })
        .unwrap();
    assert_eq!(
        report
            .entries
            .iter()
            .map(|entry| entry.estate.as_str())
            .collect::<Vec<_>>(),
        ["estate_guild"]
    );

    assert!(
        world
            .query_vanilla_estate_loyalty(&VanillaEstateLoyaltyQuery {
                country: "ZZZ".into(),
                estate: None,
            })
            .is_err()
    );
    assert!(
        world
            .query_vanilla_estate_loyalty(&VanillaEstateLoyaltyQuery {
                country: "AAA".into(),
                estate: Some("missing".into()),
            })
            .is_err()
    );

    let mut value = serde_json::json!({"country": "AAA", "unexpected": true});
    assert!(serde_json::from_value::<VanillaEstateLoyaltyQuery>(value.take()).is_err());
}
