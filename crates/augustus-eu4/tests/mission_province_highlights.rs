// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_eu4::{
    ContentRoot, ContentSet, ContentSource, TextEncoding, VanillaExternalEu4RandomState,
    VanillaExternalState, VanillaWorld,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

fn base_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world")
}

fn write_mod(root: &Path, missions: &str) {
    fs::create_dir_all(root.join("missions")).unwrap();
    fs::write(
        root.join("descriptor.mod"),
        "name=\"Mission Province Highlights Test\"\nsupported_version=\"1.37.*\"\n",
    )
    .unwrap();
    fs::write(root.join("missions/highlights.txt"), missions).unwrap();
}

fn load_with_mod(base: &Path, mod_root: &Path, state: &VanillaExternalState) -> VanillaWorld {
    let content = ContentSet::resolve(vec![
        ContentSource::base(ContentRoot::open(base).unwrap()),
        ContentSource::mod_directory(mod_root, TextEncoding::Utf8).unwrap(),
    ])
    .unwrap();
    VanillaWorld::load_from_content_with_players_and_external_state(
        &content,
        "1444.11.11",
        &BTreeSet::from(["AAA".into(), "BBB".into()]),
        state,
    )
    .unwrap()
}

fn known_mission_state() -> VanillaExternalState {
    VanillaExternalState {
        country_mission_completions: BTreeMap::from([
            ("AAA".into(), BTreeSet::new()),
            ("BBB".into(), BTreeSet::new()),
        ]),
        province_owners: BTreeMap::from([(1, Some("AAA".into())), (2, Some("BBB".into()))]),
        eu4_random_state: Some(VanillaExternalEu4RandomState { seed: 91, count: 0 }),
        ..Default::default()
    }
}

fn copy_dir(source: &Path, destination: &Path) {
    for entry in walkdir::WalkDir::new(source) {
        let entry = entry.unwrap();
        let target = destination.join(entry.path().strip_prefix(source).unwrap());
        if entry.file_type().is_dir() {
            fs::create_dir_all(target).unwrap();
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

#[test]
fn highlights_use_query_country_root_and_live_province_ownership_without_mutation() {
    let fixture = tempfile::tempdir().unwrap();
    write_mod(
        fixture.path(),
        r#"
aaa_highlight_series = {
    slot = 5
    potential = { tag = AAA }
    aaa_root = { position = 1 trigger = { always = yes } effect = { add_treasury = 1 } provinces_to_highlight = { AND = { OR = { province_id = 1 province_id = 2 } owned_by = ROOT } } }
    aaa_tag = { position = 2 trigger = { always = yes } effect = {} provinces_to_highlight = { AND = { province_id = 1 owned_by = AAA } } }
    aaa_geo = { position = 3 trigger = { always = yes } effect = {} provinces_to_highlight = { OR = { province_id = 1 area = east_area } } }
}
"#,
    );
    let mut world = load_with_mod(&base_root(), fixture.path(), &known_mission_state());
    let before_save = world.save().unwrap();
    let before_rng = world.initial.world().eu4_random_state();
    assert!(before_rng.is_some());

    let root_aaa = world
        .query_mission_province_highlights("AAA", "aaa_root")
        .unwrap();
    assert!(root_aaa.defined);
    assert_eq!(root_aaa.provinces, BTreeSet::from([1]));
    let root_bbb = world
        .query_mission_province_highlights("BBB", "aaa_root")
        .unwrap();
    assert_eq!(root_bbb.provinces, BTreeSet::from([2]));
    assert_eq!(
        world
            .query_mission_province_highlights("BBB", "aaa_tag")
            .unwrap()
            .provinces,
        BTreeSet::from([1])
    );
    assert_eq!(
        world
            .query_mission_province_highlights("AAA", "aaa_geo")
            .unwrap()
            .provinces,
        BTreeSet::from([1, 2])
    );
    assert!(
        world
            .country_missions("BBB")
            .unwrap()
            .missions
            .iter()
            .all(|mission| mission.id != "aaa_root" || mission.selected == Some(false))
    );
    assert_eq!(world.save().unwrap(), before_save);

    world.initial.set_province_owner(1, Some("BBB")).unwrap();
    assert_eq!(
        world
            .query_mission_province_highlights("AAA", "aaa_root")
            .unwrap()
            .provinces,
        BTreeSet::new()
    );
    assert_eq!(
        world
            .query_mission_province_highlights("BBB", "aaa_root")
            .unwrap()
            .provinces,
        BTreeSet::from([1, 2])
    );
    assert_eq!(
        world
            .query_mission_province_highlights("AAA", "aaa_tag")
            .unwrap()
            .provinces,
        BTreeSet::new()
    );
    assert_eq!(world.initial.world().eu4_random_state(), before_rng);
}

#[test]
fn known_highlight_targets_survive_an_unknown_province_evaluation() {
    let base_copy = tempfile::tempdir().unwrap();
    copy_dir(&base_root(), base_copy.path());
    fs::remove_file(base_copy.path().join("history/provinces/2 - East.txt")).unwrap();

    let mod_root = tempfile::tempdir().unwrap();
    write_mod(
        mod_root.path(),
        r#"aaa_mixed_series = { slot = 5 potential = { tag = AAA } aaa_mixed = { position = 1 trigger = { always = yes } effect = {} provinces_to_highlight = { AND = { OR = { province_id = 1 province_id = 2 } OR = { province_id = 1 owned_by = AAA } } } } }"#,
    );
    let state = VanillaExternalState {
        country_mission_completions: BTreeMap::from([
            ("AAA".into(), BTreeSet::new()),
            ("BBB".into(), BTreeSet::new()),
        ]),
        province_owners: BTreeMap::from([(1, Some("AAA".into()))]),
        ..Default::default()
    };
    let world = load_with_mod(base_copy.path(), mod_root.path(), &state);

    let report = world
        .query_mission_province_highlights("AAA", "aaa_mixed")
        .unwrap();
    assert_eq!(report.provinces, BTreeSet::from([1]));
    assert!(report.unknown_provinces.contains_key(&2));
    assert!(report.unresolved.is_empty());
}

#[test]
fn filter_directory_reports_malformed_and_duplicate_filters_without_changing_rewards() {
    let fixture = tempfile::tempdir().unwrap();
    write_mod(
        fixture.path(),
        r#"
aaa_filter_series = {
    slot = 5
    potential = { tag = AAA }
    aaa_missing = { position = 1 trigger = { always = yes } effect = {} highlight = yes }
    aaa_empty = { position = 2 trigger = { always = yes } effect = {} provinces_to_highlight = {} }
    aaa_all = { position = 3 trigger = { always = yes } effect = {} provinces_to_highlight = { always = yes } }
    aaa_duplicate = { position = 4 trigger = { always = yes } effect = {} provinces_to_highlight = { always = yes } provinces_to_highlight = { always = no } }
    aaa_malformed = { position = 5 trigger = { always = yes } effect = {} provinces_to_highlight = yes }
    aaa_unsupported = { position = 6 trigger = { always = yes } effect = { add_treasury = 1 } provinces_to_highlight = { unsupported_highlight = yes } }
    aaa_array = { position = 7 trigger = { always = yes } effect = {} provinces_to_highlight = { 1 2 } }
    aaa_mixed_array = { position = 8 trigger = { always = yes } effect = {} provinces_to_highlight = { 1 always = yes } }
    aaa_mixed_object = { position = 9 trigger = { always = yes } effect = {} provinces_to_highlight = { always = yes 1 } }
    aaa_non_equal = { position = 10 trigger = { always = yes } effect = { add_treasury = 1 } provinces_to_highlight > { always = yes } }
}
"#,
    );
    let world = load_with_mod(&base_root(), fixture.path(), &known_mission_state());

    assert_eq!(
        world.mission_highlight_ids(),
        BTreeSet::from([
            "aaa_all".into(),
            "aaa_array".into(),
            "aaa_duplicate".into(),
            "aaa_malformed".into(),
            "aaa_mixed_array".into(),
            "aaa_mixed_object".into(),
            "aaa_non_equal".into(),
            "aaa_unsupported".into(),
        ])
    );
    let missing = world
        .query_mission_province_highlights("AAA", "aaa_missing")
        .unwrap();
    assert!(!missing.defined);
    assert!(missing.provinces.is_empty());
    let empty = world
        .query_mission_province_highlights("AAA", "aaa_empty")
        .unwrap();
    assert!(!empty.defined);
    assert!(empty.provinces.is_empty());
    let all = world
        .query_mission_province_highlights("AAA", "aaa_all")
        .unwrap();
    assert!(all.defined);
    assert_eq!(all.provinces.len(), world.initial.provinces().len());

    let duplicate = world
        .query_mission_province_highlights("AAA", "aaa_duplicate")
        .unwrap();
    assert!(duplicate.defined);
    assert!(duplicate.provinces.is_empty());
    assert!(
        duplicate
            .unresolved
            .iter()
            .any(|reason| reason.contains("duplicate mission provinces_to_highlight fields"))
    );
    let malformed = world
        .query_mission_province_highlights("AAA", "aaa_malformed")
        .unwrap();
    assert!(malformed.defined);
    assert!(
        malformed
            .unresolved
            .iter()
            .any(|reason| reason
                .contains("provinces_to_highlight requires a non-mixed object block"))
    );
    for id in [
        "aaa_array",
        "aaa_mixed_array",
        "aaa_mixed_object",
        "aaa_non_equal",
    ] {
        let report = world.query_mission_province_highlights("AAA", id).unwrap();
        assert!(report.defined, "{id}");
        assert!(!report.unresolved.is_empty(), "{id}");
    }
    let non_equal = world
        .country_missions("AAA")
        .unwrap()
        .missions
        .into_iter()
        .find(|mission| mission.id == "aaa_non_equal")
        .unwrap();
    assert!(non_equal.reward_supported);
    assert!(non_equal.claimable, "{:?}", non_equal.unresolved);
    let unsupported = world
        .query_mission_province_highlights("AAA", "aaa_unsupported")
        .unwrap();
    assert!(unsupported.defined);
    assert!(!unsupported.unresolved.is_empty());
    let unsupported_reward = world
        .country_missions("AAA")
        .unwrap()
        .missions
        .into_iter()
        .find(|mission| mission.id == "aaa_unsupported")
        .unwrap();
    assert!(unsupported_reward.reward_supported);
    assert!(
        unsupported_reward.claimable,
        "{:?}",
        unsupported_reward.unresolved
    );

    assert!(
        world
            .query_mission_province_highlights("ZZZ", "aaa_all")
            .is_err()
    );
    assert!(
        world
            .query_mission_province_highlights("AAA", "not_a_mission")
            .is_err()
    );
}

#[test]
fn changing_only_the_visual_filter_keeps_existing_snapshots_compatible() {
    let first_mod = tempfile::tempdir().unwrap();
    let second_mod = tempfile::tempdir().unwrap();
    write_mod(
        first_mod.path(),
        r#"aaa_identity_series = { slot = 5 potential = { tag = AAA } aaa_identity = { position = 1 trigger = { always = yes } effect = { add_treasury = 1 } provinces_to_highlight = { always = yes } } }"#,
    );
    write_mod(
        second_mod.path(),
        r#"aaa_identity_series = { slot = 5 potential = { tag = AAA } aaa_identity = { position = 1 trigger = { always = yes } effect = { add_treasury = 1 } provinces_to_highlight = { province_id = 1 } } }"#,
    );
    let original = load_with_mod(&base_root(), first_mod.path(), &known_mission_state());
    let mut changed = load_with_mod(&base_root(), second_mod.path(), &known_mission_state());

    changed.restore(&original.save().unwrap()).unwrap();
    assert_eq!(
        changed
            .query_mission_province_highlights("AAA", "aaa_identity")
            .unwrap()
            .provinces,
        BTreeSet::from([1])
    );
}
