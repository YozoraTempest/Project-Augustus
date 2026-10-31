// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::{collections::BTreeSet, fs, path::Path};

use augustus_eu4::{
    ContentRoot, ContentSet, ContentSource, TextEncoding, VanillaExternalState,
    VanillaMissionSelectionImport, VanillaWorld, read_vanilla_mission_selection,
    read_vanilla_save_state,
};

const MISSIONS: &str = r#"
archived_series = {
    slot = 1
    potential = { always = no }
    archived_first = { position = 1 trigger = { always = yes } effect = {} }
    archived_next = {
        position = 2
        required_missions = { archived_first }
        trigger = { always = yes }
        effect = { add_treasury = 1 }
    }
}
current_series = {
    slot = 1
    potential = { always = yes }
    current_task = { trigger = { always = yes } effect = {} }
}
"#;

fn fixture(missions: &str, startup: Option<&str>) -> (tempfile::TempDir, ContentSet) {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("descriptor.mod"),
        "name=\"Saved Missions\"",
    )
    .unwrap();
    fs::create_dir(directory.path().join("missions")).unwrap();
    fs::write(directory.path().join("missions/saved.txt"), missions).unwrap();
    if let Some(startup) = startup {
        fs::create_dir_all(directory.path().join("common/on_actions")).unwrap();
        fs::write(
            directory.path().join("common/on_actions/saved.txt"),
            startup,
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

fn write_save(root: &Path, date: &str, alpha_selection: &str) -> std::path::PathBuf {
    let path = root.join("saved.eu4");
    fs::write(
        &path,
        format!(
            "EU4txt\ndate={date}\nplayer=AAA\ncountries={{AAA={{treasury=0 completed_missions={{archived_first}} {alpha_selection}}} BBB={{treasury=0 country_missions={{mission_slot={{}}}}}} ZZZ={{country_missions={{mission_slot={{missing_series}}}}}}}} provinces={{}}"
        ),
    )
    .unwrap();
    path
}

#[test]
fn saved_series_preserve_cached_selection_claims_and_snapshot_until_explicit_refresh() {
    let (directory, content) = fixture(MISSIONS, None);
    let path = write_save(
        directory.path(),
        "1445.1.1",
        "country_missions={mission_slot={archived_series}}",
    );
    let mut loaded = VanillaWorld::load_from_content_with_save(&content, &path).unwrap();
    let alpha = loaded.initial.country_entity("AAA").unwrap();
    let beta = loaded.initial.country_entity("BBB").unwrap();
    let before = loaded.save().unwrap();
    let report = loaded.country_missions("AAA").unwrap();
    assert!(report.selection_known && report.completions_known);
    let next = report
        .missions
        .iter()
        .find(|m| m.id == "archived_next")
        .unwrap();
    assert_eq!(next.selected, Some(true));
    assert!(next.claimable, "{:?}", next.unresolved);
    assert_eq!(
        loaded.initial.world().objectives().entities[&beta],
        BTreeSet::new()
    );
    assert_eq!(
        loaded.save().unwrap(),
        before,
        "mission queries remain read-only"
    );
    assert_eq!(loaded.report.missions.known_countries, 2);

    loaded.claim_mission("AAA", "archived_next").unwrap();
    assert_eq!(
        loaded
            .country("AAA")
            .unwrap()
            .resources
            .unwrap()
            .treasury_milli,
        Some(1_000)
    );
    let claimed = loaded.save().unwrap();
    loaded.initial.refresh_objectives(alpha).unwrap();
    assert!(
        !loaded
            .initial
            .world()
            .objective_available(alpha, "archived_next")
            .unwrap()
    );
    assert!(
        loaded
            .initial
            .world()
            .objective_available(alpha, "current_task")
            .unwrap()
    );
    assert!(
        loaded
            .initial
            .world()
            .completion_recorded(alpha, "archived_next")
            .unwrap()
    );
    loaded.restore(&claimed).unwrap();
    assert!(
        loaded
            .initial
            .world()
            .objective_available(alpha, "archived_next")
            .unwrap()
    );
    assert!(loaded.claim_mission("AAA", "archived_next").is_err());
    assert_eq!(loaded.save().unwrap(), claimed);
}

#[test]
fn saved_selection_is_bound_before_startup_and_external_resource_overlay() {
    let (directory, content) = fixture(
        MISSIONS,
        Some(
            "on_startup={if={limit={tag=AAA has_mission=archived_next} add_treasury=3 log=saved_selection_visible}}",
        ),
    );
    let path = write_save(
        directory.path(),
        "1445.1.1",
        "country_missions={mission_slot={archived_series}}",
    );
    let overlay: VanillaExternalState =
        serde_json::from_str(r#"{"country_resources":{"AAA":{"treasury_milli":7000}}}"#).unwrap();
    let loaded =
        VanillaWorld::load_from_content_with_save_and_external_state(&content, &path, &overlay)
            .unwrap();
    assert!(
        loaded.report.estates.startup_unresolved.is_empty(),
        "{:?}",
        loaded.report.estates.startup_unresolved
    );
    assert_eq!(
        loaded
            .country("AAA")
            .unwrap()
            .resources
            .unwrap()
            .treasury_milli,
        Some(10_000)
    );
    assert!(
        loaded
            .initial
            .world()
            .events()
            .iter()
            .any(|entry| entry.2 == "saved_selection_visible")
    );
    assert!(loaded.country_missions("AAA").unwrap().selection_known);
}

#[test]
fn unknown_saved_series_make_the_entire_selection_unknown_even_on_the_fixed_start_date() {
    let (directory, content) = fixture(MISSIONS, None);
    let path = write_save(
        directory.path(),
        "1444.11.11",
        "country_missions={mission_slot={archived_series missing_series}}",
    );
    let loaded = VanillaWorld::load_from_content_with_save(&content, &path).unwrap();
    let alpha = loaded.country_missions("AAA").unwrap();
    assert!(!alpha.selection_known);
    assert!(
        alpha
            .missions
            .iter()
            .all(|mission| mission.selected.is_none() && !mission.claimable)
    );
    assert!(loaded.report.missions.unresolved["AAA"].contains("missing_series"));
    assert!(loaded.country_missions("BBB").unwrap().selection_known);
    assert_eq!(loaded.report.missions.known_countries, 1);
}

#[test]
fn missing_saved_selection_preserves_existing_translated_state_api_and_date_is_checked() {
    let (directory, content) = fixture(MISSIONS, None);
    let path = write_save(directory.path(), "1445.1.1", "");
    let save = read_vanilla_save_state(&path).unwrap();
    let selection = read_vanilla_mission_selection(&path).unwrap();
    assert!(!selection.countries.contains_key("AAA"));
    let loaded = VanillaWorld::load_from_content_with_save_state_and_mission_selection(
        &content, &save, &selection, None, None,
    )
    .unwrap();
    assert!(!loaded.country_missions("AAA").unwrap().selection_known);
    assert!(loaded.country_missions("BBB").unwrap().selection_known);
    let old_api = VanillaWorld::load_from_content_with_save_state(&content, &save).unwrap();
    assert!(!old_api.country_missions("BBB").unwrap().selection_known);
    let mismatch = VanillaMissionSelectionImport {
        date: "1445.1.2".into(),
        ..selection
    };
    let error = VanillaWorld::load_from_content_with_save_state_and_mission_selection(
        &content, &save, &mismatch, None, None,
    )
    .err()
    .unwrap();
    assert!(error.to_string().contains("does not match"));
}

#[test]
fn saved_selection_does_not_claim_support_for_unresolved_load_conditions() {
    let missions = "archived_series={slot=1 potential_on_load={always=no} archived_task={trigger={always=yes} effect={}}}";
    let (directory, content) = fixture(missions, None);
    let path = write_save(
        directory.path(),
        "1445.1.1",
        "country_missions={mission_slot={archived_series}}",
    );
    let mut loaded = VanillaWorld::load_from_content_with_save(&content, &path).unwrap();
    let report = loaded.country_missions("AAA").unwrap();
    assert!(report.selection_known);
    assert_eq!(report.missions[0].selected, Some(true));
    assert_eq!(report.missions[0].completed, Some(false));
    assert_eq!(report.missions[0].requirements_met, Some(true));
    assert!(!report.missions[0].claimable);
    assert!(
        report.missions[0]
            .unresolved
            .iter()
            .any(|reason| reason.contains("potential_on_load"))
    );
    let before = loaded.save().unwrap();
    assert!(loaded.claim_mission("AAA", "archived_task").is_err());
    assert_eq!(loaded.save().unwrap(), before);
    let alpha = loaded.initial.country_entity("AAA").unwrap();
    assert!(loaded.initial.refresh_objectives(alpha).is_err());
    assert_eq!(
        loaded.save().unwrap(),
        before,
        "failed refresh preserves the saved selection"
    );
}
