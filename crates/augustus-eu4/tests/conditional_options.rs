// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{CountryController, EntityId};
use augustus_eu4::VanillaWorld;
use std::{collections::BTreeSet, fs, path::Path};

fn copy(source: &Path, target: &Path) {
    for entry in walkdir::WalkDir::new(source) {
        let entry = entry.unwrap();
        let path = target.join(entry.path().strip_prefix(source).unwrap());
        if entry.file_type().is_dir() {
            fs::create_dir_all(path).unwrap();
        } else {
            fs::copy(entry.path(), path).unwrap();
        }
    }
}

fn fixture() -> tempfile::TempDir {
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let temp = tempfile::tempdir().unwrap();
    copy(&examples.join("world"), temp.path());
    copy(&examples.join("conditional-options/eu4"), temp.path());
    let actions = temp.path().join("common/on_actions");
    fs::create_dir_all(&actions).unwrap();
    fs::write(actions.join("test.txt"),
        "on_startup={events={choices.start} if={limit={ai=no} log=human_start} else={log=ai_start}}"
    ).unwrap();
    temp
}

#[test]
fn player_roster_is_bound_before_startup_and_saved_control_is_restored() {
    let temp = fixture();
    let players = BTreeSet::from(["AAA".into()]);
    let mut loaded = VanillaWorld::load_with_players(temp.path(), "1444.11.11", &players).unwrap();
    assert!(
        loaded.report.estates.pending_startup_effects.is_empty(),
        "{:?}",
        loaded.report.estates.startup_unresolved
    );
    let world = loaded.initial.world();
    assert_eq!(
        world.country_controller(EntityId(0)).unwrap(),
        Some(CountryController::Human)
    );
    assert_eq!(
        world.country_controller(EntityId(1)).unwrap(),
        Some(CountryController::Ai)
    );
    assert!(
        world
            .events()
            .iter()
            .any(|entry| entry.1 == EntityId(0) && entry.2 == "human_start")
    );
    assert!(
        world
            .events()
            .iter()
            .any(|entry| entry.1 == EntityId(1) && entry.2 == "ai_start")
    );
    let human = world
        .pending_events()
        .iter()
        .find(|pending| pending.current == EntityId(0))
        .unwrap()
        .id;
    assert_eq!(
        world
            .available_event_options(human)
            .unwrap()
            .iter()
            .map(|option| option.id.as_str())
            .collect::<Vec<_>>(),
        ["1"]
    );
    loaded
        .initial
        .set_country_controller(EntityId(0), CountryController::Ai)
        .unwrap();
    let saved = loaded.save().unwrap();
    let mut restored =
        VanillaWorld::load_with_players(temp.path(), "1444.11.11", &players).unwrap();
    restored.restore(&saved).unwrap();
    assert_eq!(
        restored
            .initial
            .world()
            .country_controller(EntityId(0))
            .unwrap(),
        Some(CountryController::Ai)
    );
    assert_eq!(restored.save().unwrap(), saved);
    let different_roster =
        VanillaWorld::load_with_players(temp.path(), "1444.11.11", &BTreeSet::new()).unwrap();
    assert_ne!(different_roster.save().unwrap(), saved);
    let mut different = different_roster;
    assert!(different.restore(&saved).is_err());
    assert!(
        VanillaWorld::load_with_players(temp.path(), "1444.11.11", &BTreeSet::from(["ZZZ".into()]))
            .is_err()
    );
}

#[test]
fn absent_roster_reports_unknown_control_and_rolls_back_startup_writes() {
    let temp = fixture();
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(!loaded.report.estates.pending_startup_effects.is_empty());
    assert!(
        loaded
            .report
            .estates
            .startup_unresolved
            .iter()
            .any(|reason| reason.contains("unknown_country_controller"))
    );
    assert!(loaded.initial.world().country_controllers().is_empty());
    assert!(loaded.initial.world().pending_events().is_empty());
    assert!(loaded.initial.world().events().is_empty());
}

#[test]
fn changing_only_an_option_guard_changes_content_identity() {
    let temp = fixture();
    let players = BTreeSet::from(["AAA".into()]);
    let first = VanillaWorld::load_with_players(temp.path(), "1444.11.11", &players).unwrap();
    let path = temp.path().join("events/demo.txt");
    fs::write(
        &path,
        fs::read_to_string(&path)
            .unwrap()
            .replace("trigger = { ai = no }", "trigger = { always = yes }"),
    )
    .unwrap();
    let mut changed = VanillaWorld::load_with_players(temp.path(), "1444.11.11", &players).unwrap();
    assert!(changed.restore(&first.save().unwrap()).is_err());
}
