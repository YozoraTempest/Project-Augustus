// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::EventChoice;
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

fn example_mod_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/mission-claims")
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

fn sample_state() -> VanillaExternalState {
    serde_json::from_str(include_str!(
        "../../../examples/mission-claims/external_state.json"
    ))
    .unwrap()
}

fn write_mod(root: &Path, missions: &str, events: Option<&str>) {
    fs::create_dir_all(root.join("missions")).unwrap();
    fs::write(
        root.join("descriptor.mod"),
        "name=\"Mission Claims Test\"\nsupported_version=\"1.37.*\"\n",
    )
    .unwrap();
    fs::write(root.join("missions/claims.txt"), missions).unwrap();
    if let Some(events) = events {
        fs::create_dir_all(root.join("events")).unwrap();
        fs::write(root.join("events/claims.txt"), events).unwrap();
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
fn claim_chain_executes_reward_then_records_and_resumes_pending_event() {
    let mut world = load_with_mod(&base_root(), &example_mod_root(), &sample_state());
    let before_query = world.save().unwrap();
    let report = world.country_missions("AAA").unwrap();
    assert!(report.selection_known);
    assert!(report.completions_known);
    let first = report
        .missions
        .iter()
        .find(|item| item.id == "aaa_claim_first")
        .unwrap();
    assert_eq!(first.title, "Secure the Northern Province");
    assert_eq!(
        first.description.as_deref(),
        Some("Bring the northern province under our rule.")
    );
    assert_eq!(first.selected, Some(true));
    assert_eq!(first.completed, Some(false));
    assert_eq!(first.requirements_met, Some(true));
    assert!(first.claimable, "{:?}", first.unresolved);
    let second = report
        .missions
        .iter()
        .find(|item| item.id == "aaa_claim_second")
        .unwrap();
    assert_eq!(
        second.required_missions,
        BTreeSet::from(["aaa_claim_first".into()])
    );
    assert_eq!(second.requirements_met, Some(false));
    assert!(!second.claimable);
    assert_eq!(
        world.save().unwrap(),
        before_query,
        "queries must be read-only"
    );

    let bbb = world.country_missions("BBB").unwrap();
    assert_eq!(
        bbb.missions
            .iter()
            .find(|item| item.id == "bbb_claim_first")
            .unwrap()
            .selected,
        Some(true)
    );
    assert_eq!(
        bbb.missions
            .iter()
            .find(|item| item.id == "aaa_claim_first")
            .unwrap()
            .selected,
        Some(false)
    );

    world.claim_mission("AAA", "aaa_claim_first").unwrap();
    assert_eq!(
        world
            .country("AAA")
            .unwrap()
            .resources
            .unwrap()
            .treasury_milli,
        Some(5_000)
    );
    assert!(
        world
            .initial
            .world()
            .completion_recorded(
                world.initial.country_entity("AAA").unwrap(),
                "aaa_claim_first"
            )
            .unwrap()
    );
    let pending = world.initial.world().pending_events();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].event, "mission_claims.reward");
    let pending_id = pending[0].id;

    let saved_with_pending = world.save().unwrap();
    let mut resumed = load_with_mod(&base_root(), &example_mod_root(), &sample_state());
    resumed.restore(&saved_with_pending).unwrap();
    for state in [&mut world, &mut resumed] {
        state
            .initial
            .choose_event(&EventChoice {
                pending: pending_id,
                option: "0".into(),
            })
            .unwrap();
        let next = state
            .country_missions("AAA")
            .unwrap()
            .missions
            .into_iter()
            .find(|item| item.id == "aaa_claim_second")
            .unwrap();
        assert_eq!(next.requirements_met, Some(true));
        assert!(next.claimable, "{:?}", next.unresolved);
        state.claim_mission("AAA", "aaa_claim_second").unwrap();
        assert_eq!(
            state
                .country("AAA")
                .unwrap()
                .resources
                .unwrap()
                .treasury_milli,
            Some(12_000)
        );
        let completed = state.save().unwrap();
        assert!(state.claim_mission("AAA", "aaa_claim_second").is_err());
        assert_eq!(
            state.save().unwrap(),
            completed,
            "repeat claims must be no-ops"
        );
    }
    assert_eq!(world.save().unwrap(), resumed.save().unwrap());

    let final_report = world.country_missions("AAA").unwrap();
    let unknown = final_report
        .missions
        .iter()
        .find(|item| item.id == "aaa_claim_unknown")
        .unwrap();
    assert_eq!(unknown.requirements_met, None);
    assert!(!unknown.claimable);
    let unsupported = final_report
        .missions
        .iter()
        .find(|item| item.id == "aaa_claim_unsupported")
        .unwrap();
    assert!(!unsupported.reward_supported);
    assert!(!unsupported.claimable);
}

#[test]
fn claims_recheck_root_ownership_after_a_live_transfer() {
    let fixture = tempfile::tempdir().unwrap();
    let mission_text = r#"
aaa_owned_series = {
    slot = 5
    potential = { tag = AAA }
    aaa_owned = {
        position = 1
        trigger = { ROOT = { owns = 1 } }
        effect = { add_treasury = 1 }
    }
}
"#;
    write_mod(fixture.path(), mission_text, None);
    let state = VanillaExternalState {
        country_mission_completions: BTreeMap::from([
            ("AAA".into(), BTreeSet::new()),
            ("BBB".into(), BTreeSet::new()),
        ]),
        province_owners: BTreeMap::from([(1, Some("AAA".into()))]),
        ..Default::default()
    };
    let mut world = load_with_mod(&base_root(), fixture.path(), &state);
    let first_view = world.country_missions("AAA").unwrap();
    let owned = first_view
        .missions
        .iter()
        .find(|item| item.id == "aaa_owned")
        .unwrap();
    assert_eq!(owned.requirements_met, Some(true));
    assert!(owned.claimable, "{:?}", owned.unresolved);

    world.initial.set_province_owner(1, Some("BBB")).unwrap();
    let after_transfer = world.country_missions("AAA").unwrap();
    let owned = after_transfer
        .missions
        .iter()
        .find(|item| item.id == "aaa_owned")
        .unwrap();
    assert_eq!(owned.requirements_met, Some(false));
    assert!(!owned.claimable);
    let before_failed_claim = world.save().unwrap();
    assert!(world.claim_mission("AAA", "aaa_owned").is_err());
    assert_eq!(world.save().unwrap(), before_failed_claim);
}

#[test]
fn failed_reward_discards_prior_writes_and_random_draws() {
    let fixture = tempfile::tempdir().unwrap();
    write_mod(
        fixture.path(),
        r#"
aaa_atomic_series = {
    slot = 5
    potential = { tag = AAA }
    aaa_atomic = {
        position = 1
        trigger = { always = yes }
        effect = {
            set_variable = { which = before_failure value = 7 }
            random_owned_province = { limit = { always = yes } }
            divide_variable = { which = before_failure value = 0 }
        }
    }
}
"#,
        None,
    );
    let state = VanillaExternalState {
        eu4_random_state: Some(VanillaExternalEu4RandomState { seed: 91, count: 0 }),
        country_mission_completions: BTreeMap::from([
            ("AAA".into(), BTreeSet::new()),
            ("BBB".into(), BTreeSet::new()),
        ]),
        ..Default::default()
    };
    let mut world = load_with_mod(&base_root(), fixture.path(), &state);
    let view = world.country_missions("AAA").unwrap();
    let atomic = view
        .missions
        .iter()
        .find(|item| item.id == "aaa_atomic")
        .unwrap();
    assert!(atomic.reward_supported, "{:?}", atomic.unresolved);
    assert!(atomic.claimable, "{:?}", atomic.unresolved);
    let saved_before = world.save().unwrap();
    let random_before = world.initial.world().eu4_random_state();

    let error = world
        .claim_mission("AAA", "aaa_atomic")
        .unwrap_err()
        .to_string();
    assert!(error.contains("numeric division by zero"), "{error}");
    assert_eq!(world.initial.world().eu4_random_state(), random_before);
    assert_eq!(world.save().unwrap(), saved_before);
    assert!(
        world
            .country_missions("AAA")
            .unwrap()
            .missions
            .iter()
            .find(|item| item.id == "aaa_atomic")
            .unwrap()
            .claimable
    );
}

#[test]
fn mission_rule_identity_tracks_semantics_without_installation_paths() {
    let example = example_mod_root();
    let first_copy = tempfile::tempdir().unwrap();
    let second_copy = tempfile::tempdir().unwrap();
    copy_dir(&example, first_copy.path());
    copy_dir(&example, second_copy.path());
    let original = load_with_mod(&base_root(), first_copy.path(), &sample_state());
    let same_content = load_with_mod(&base_root(), second_copy.path(), &sample_state());
    let snapshot = original.save().unwrap();
    assert_eq!(snapshot, same_content.save().unwrap());

    let mission_file = second_copy.path().join("missions/mission_claims.txt");
    let modified = fs::read_to_string(&mission_file)
        .unwrap()
        .replace("add_treasury = 5", "add_treasury = 6");
    fs::write(mission_file, modified).unwrap();
    let mut changed = load_with_mod(&base_root(), second_copy.path(), &sample_state());
    assert!(changed.restore(&snapshot).is_err());

    let metadata_only = tempfile::tempdir().unwrap();
    let explicit_empty = tempfile::tempdir().unwrap();
    let metadata = r#"aaa_identity_series = { slot = 5 potential = { tag = AAA } aaa_identity = { position = 1 } }"#;
    let empty = r#"aaa_identity_series = { slot = 5 potential = { tag = AAA } aaa_identity = { position = 1 trigger = {} effect = {} } }"#;
    write_mod(metadata_only.path(), metadata, None);
    write_mod(explicit_empty.path(), empty, None);
    let metadata_world = load_with_mod(&base_root(), metadata_only.path(), &sample_state());
    let mut empty_world = load_with_mod(&base_root(), explicit_empty.path(), &sample_state());
    let metadata_view = metadata_world.country_missions("AAA").unwrap();
    assert!(
        !metadata_view
            .missions
            .iter()
            .find(|item| item.id == "aaa_identity")
            .unwrap()
            .claimable
    );
    let explicit_view = empty_world.country_missions("AAA").unwrap();
    assert!(
        explicit_view
            .missions
            .iter()
            .find(|item| item.id == "aaa_identity")
            .unwrap()
            .claimable
    );
    let metadata_snapshot = metadata_world.save().unwrap();
    assert_ne!(metadata_snapshot, empty_world.save().unwrap());
    assert!(empty_world.restore(&metadata_snapshot).is_err());
}

#[test]
fn unknown_owned_province_state_does_not_become_a_claimable_false() {
    let base_copy = tempfile::tempdir().unwrap();
    copy_dir(&base_root(), base_copy.path());
    let province = base_copy.path().join("history/provinces/2 - East.txt");
    fs::remove_file(province).unwrap();

    let mod_root = tempfile::tempdir().unwrap();
    write_mod(
        mod_root.path(),
        r#"aaa_unknown_owner_series = { slot = 5 potential = { tag = AAA } aaa_unknown_owner = { position = 1 trigger = { owns = 2 } effect = {} } }"#,
        None,
    );
    let state = VanillaExternalState {
        country_mission_completions: BTreeMap::from([
            ("AAA".into(), BTreeSet::new()),
            ("BBB".into(), BTreeSet::new()),
        ]),
        ..Default::default()
    };
    let world = load_with_mod(base_copy.path(), mod_root.path(), &state);
    let report = world.country_missions("AAA").unwrap();
    let item = report
        .missions
        .iter()
        .find(|item| item.id == "aaa_unknown_owner")
        .unwrap();
    assert_eq!(item.requirements_met, None);
    assert!(!item.claimable);
}
