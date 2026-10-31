// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{CountryResources, EventChoice};
use augustus_eu4::{
    ContentRoot, ContentSet, ContentSource, TextEncoding, VanillaExternalState, VanillaWorld,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

fn base_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world")
}

fn state() -> VanillaExternalState {
    VanillaExternalState {
        country_resources: ["AAA", "BBB"]
            .into_iter()
            .map(|country| {
                (
                    country.into(),
                    CountryResources {
                        treasury_milli: Some(0),
                        ..Default::default()
                    },
                )
            })
            .collect(),
        country_mission_completions: BTreeMap::from([
            ("AAA".into(), BTreeSet::new()),
            ("BBB".into(), BTreeSet::new()),
        ]),
        ..Default::default()
    }
}

fn load_with_mod(mod_root: &Path) -> VanillaWorld {
    let content = ContentSet::resolve(vec![
        ContentSource::base(ContentRoot::open(base_root()).unwrap()),
        ContentSource::mod_directory(mod_root, TextEncoding::Utf8).unwrap(),
    ])
    .unwrap();
    VanillaWorld::load_from_content_with_players_and_external_state(
        &content,
        "1444.11.11",
        &BTreeSet::from(["AAA".into(), "BBB".into()]),
        &state(),
    )
    .unwrap()
}

fn write_mod(root: &Path, missions: &str, events: Option<&str>) {
    fs::create_dir_all(root.join("missions")).unwrap();
    fs::write(
        root.join("descriptor.mod"),
        "name=\"Mission Execution Boundaries\"\nsupported_version=\"1.37.*\"\n",
    )
    .unwrap();
    fs::write(root.join("missions/boundaries.txt"), missions).unwrap();
    if let Some(events) = events {
        fs::create_dir_all(root.join("events")).unwrap();
        fs::write(root.join("events/boundaries.txt"), events).unwrap();
    }
}

fn treasury(world: &VanillaWorld, country: &str) -> Option<i64> {
    world
        .country(country)
        .unwrap()
        .resources
        .unwrap()
        .treasury_milli
}

fn pending_id(world: &VanillaWorld, event: &str) -> augustus_core::PendingEventId {
    world
        .initial
        .world()
        .pending_events()
        .iter()
        .find(|pending| pending.event == event)
        .unwrap_or_else(|| panic!("missing pending event {event}"))
        .id
}

fn choose(world: &mut VanillaWorld, event: &str) {
    let pending = pending_id(world, event);
    world
        .initial
        .choose_event(&EventChoice {
            pending,
            option: "0".into(),
        })
        .unwrap();
}

#[test]
fn missing_event_root_does_not_disable_a_valid_mission_root() {
    let fixture = tempfile::tempdir().unwrap();
    write_mod(
        fixture.path(),
        r#"
aaa_boundary_series = {
    slot = 5
    potential = { tag = AAA }
    aaa_valid_root = {
        position = 1
        trigger = { always = yes }
        effect = {
            add_treasury = 3
            country_event = { id = boundary.valid }
        }
    }
    aaa_missing_root = {
        position = 2
        required_missions = { aaa_valid_root }
        trigger = { always = yes }
        effect = {
            add_treasury = 11
            country_event = { id = boundary.missing }
        }
    }
}
"#,
        Some(
            r#"
country_event = {
    id = boundary.valid
    is_triggered_only = yes
    immediate = { add_treasury = 4 }
    option = { name = done }
}
"#,
        ),
    );

    let mut world = load_with_mod(fixture.path());
    let report = world.country_missions("AAA").unwrap();
    let valid = report
        .missions
        .iter()
        .find(|mission| mission.id == "aaa_valid_root")
        .unwrap();
    assert!(valid.claimable, "{:?}", valid.unresolved);

    world.claim_mission("AAA", "aaa_valid_root").unwrap();
    assert_eq!(treasury(&world, "AAA"), Some(7_000));
    assert_eq!(world.initial.world().pending_events().len(), 1);
    assert_eq!(
        world.initial.world().pending_events()[0].event,
        "boundary.valid"
    );

    let report = world.country_missions("AAA").unwrap();
    let missing = report
        .missions
        .iter()
        .find(|mission| mission.id == "aaa_missing_root")
        .unwrap();
    assert!(missing.selected.unwrap_or(false));
    assert!(!missing.reward_supported);
    assert!(!missing.claimable);
    let before_failed_claim = world.save().unwrap();
    assert!(world.claim_mission("AAA", "aaa_missing_root").is_err());
    assert_eq!(world.save().unwrap(), before_failed_claim);
}

#[test]
fn direct_self_completion_reward_is_blocked_but_preview_is_ignored() {
    let fixture = tempfile::tempdir().unwrap();
    write_mod(
        fixture.path(),
        r#"
aaa_self_status_series = {
    slot = 4
    potential = { tag = AAA }
    aaa_direct_self_status = {
        position = 1
        trigger = { always = yes }
        effect = {
            add_treasury = 1
            if = {
                limit = { mission_completed = aaa_direct_self_status }
                add_treasury = 100
            }
        }
    }
}
bbb_preview_status_series = {
    slot = 5
    potential = { tag = BBB }
    bbb_preview_status = {
        position = 1
        trigger = { always = yes }
        effect = {
            tooltip = {
                country_event = { id = boundary.preview }
                if = {
                    limit = { mission_completed = bbb_preview_status }
                    add_treasury = 100
                }
            }
            add_treasury = 2
        }
    }
}
"#,
        Some(
            r#"
country_event = {
    id = boundary.preview
    is_triggered_only = yes
    immediate = {
        if = {
            limit = { mission_completed = bbb_preview_status }
            add_treasury = 500
        }
    }
    option = { name = done }
}
"#,
        ),
    );

    let mut world = load_with_mod(fixture.path());
    let report = world.country_missions("AAA").unwrap();
    let direct = report
        .missions
        .iter()
        .find(|mission| mission.id == "aaa_direct_self_status")
        .unwrap();
    assert!(!direct.reward_supported);
    assert!(!direct.claimable);
    let before_failed_claim = world.save().unwrap();
    assert!(
        world
            .claim_mission("AAA", "aaa_direct_self_status")
            .is_err()
    );
    assert_eq!(world.save().unwrap(), before_failed_claim);

    let report = world.country_missions("BBB").unwrap();
    let preview = report
        .missions
        .iter()
        .find(|mission| mission.id == "bbb_preview_status")
        .unwrap();
    assert!(preview.claimable, "{:?}", preview.unresolved);
    assert!(
        world
            .initial
            .world()
            .event_catalog()
            .definitions()
            .contains_key("boundary.preview")
    );
    world.claim_mission("BBB", "bbb_preview_status").unwrap();
    assert_eq!(treasury(&world, "BBB"), Some(2_000));
    assert!(world.initial.world().pending_events().is_empty());
    assert!(
        world
            .initial
            .world()
            .completion_recorded(
                world.initial.country_entity("BBB").unwrap(),
                "bbb_preview_status"
            )
            .unwrap()
    );
}

#[test]
fn delayed_event_can_read_completion_and_resume_with_pending_choice() {
    let fixture = tempfile::tempdir().unwrap();
    write_mod(
        fixture.path(),
        r#"
aaa_delayed_status_series = {
    slot = 5
    potential = { tag = AAA }
    aaa_delayed_status = {
        position = 1
        trigger = { always = yes }
        effect = {
            add_treasury = 1
            country_event = { id = boundary.delayed days = 1 }
        }
    }
}
"#,
        Some(
            r#"
country_event = {
    id = boundary.delayed
    trigger = { mission_completed = aaa_delayed_status }
    is_triggered_only = yes
    immediate = {
        if = {
            limit = { mission_completed = aaa_delayed_status }
            add_treasury = 2
        }
    }
    option = { name = done }
}
"#,
        ),
    );

    let mut world = load_with_mod(fixture.path());
    let mission = world
        .country_missions("AAA")
        .unwrap()
        .missions
        .into_iter()
        .find(|mission| mission.id == "aaa_delayed_status")
        .unwrap();
    assert!(mission.claimable, "{:?}", mission.unresolved);
    world.claim_mission("AAA", "aaa_delayed_status").unwrap();
    assert_eq!(treasury(&world, "AAA"), Some(1_000));
    assert!(world.initial.world().pending_events().is_empty());

    world.advance(1).unwrap();
    assert_eq!(treasury(&world, "AAA"), Some(3_000));
    assert_eq!(world.initial.world().pending_events().len(), 1);
    assert_eq!(
        world.initial.world().pending_events()[0].event,
        "boundary.delayed"
    );
    let snapshot = world.save().unwrap();
    let mut resumed = load_with_mod(fixture.path());
    resumed.restore(&snapshot).unwrap();

    for loaded in [&mut world, &mut resumed] {
        choose(loaded, "boundary.delayed");
        assert!(loaded.initial.world().pending_events().is_empty());
        assert_eq!(treasury(loaded, "AAA"), Some(3_000));
    }
    assert_eq!(world.save().unwrap(), resumed.save().unwrap());
}

#[test]
fn pending_option_after_and_child_event_observe_recorded_completion() {
    let fixture = tempfile::tempdir().unwrap();
    write_mod(
        fixture.path(),
        r#"
aaa_pending_status_series = {
    slot = 5
    potential = { tag = AAA }
    aaa_pending_status = {
        position = 1
        trigger = { always = yes }
        effect = {
            add_treasury = 1
            country_event = { id = boundary.pending }
        }
    }
}
"#,
        Some(
            r#"
country_event = {
    id = boundary.pending
    is_triggered_only = yes
    option = {
        name = accept
        if = {
            limit = { mission_completed = aaa_pending_status }
            add_treasury = 2
        }
        country_event = { id = boundary.child }
    }
    after = {
        if = {
            limit = { mission_completed = aaa_pending_status }
            add_treasury = 3
        }
    }
}
country_event = {
    id = boundary.child
    trigger = { mission_completed = aaa_pending_status }
    is_triggered_only = yes
    immediate = {
        if = {
            limit = { mission_completed = aaa_pending_status }
            add_treasury = 4
        }
    }
    option = { name = done }
}
"#,
        ),
    );

    let mut world = load_with_mod(fixture.path());
    let mission = world
        .country_missions("AAA")
        .unwrap()
        .missions
        .into_iter()
        .find(|mission| mission.id == "aaa_pending_status")
        .unwrap();
    assert!(mission.claimable, "{:?}", mission.unresolved);
    world.claim_mission("AAA", "aaa_pending_status").unwrap();
    assert_eq!(treasury(&world, "AAA"), Some(1_000));
    assert!(
        world
            .initial
            .world()
            .completion_recorded(
                world.initial.country_entity("AAA").unwrap(),
                "aaa_pending_status"
            )
            .unwrap()
    );
    assert_eq!(world.initial.world().pending_events().len(), 1);
    assert_eq!(
        world.initial.world().pending_events()[0].event,
        "boundary.pending"
    );

    let snapshot = world.save().unwrap();
    let mut resumed = load_with_mod(fixture.path());
    resumed.restore(&snapshot).unwrap();
    for loaded in [&mut world, &mut resumed] {
        choose(loaded, "boundary.pending");
        assert_eq!(treasury(loaded, "AAA"), Some(10_000));
        assert_eq!(loaded.initial.world().pending_events().len(), 1);
        assert_eq!(
            loaded.initial.world().pending_events()[0].event,
            "boundary.child"
        );
        choose(loaded, "boundary.child");
        assert!(loaded.initial.world().pending_events().is_empty());
        assert_eq!(treasury(loaded, "AAA"), Some(10_000));
    }
    assert_eq!(world.save().unwrap(), resumed.save().unwrap());
}
