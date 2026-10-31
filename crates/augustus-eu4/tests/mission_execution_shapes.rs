// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::CountryResources;
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

fn known_state() -> VanillaExternalState {
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
        eu4_random_state: Some(VanillaExternalEu4RandomState { seed: 91, count: 0 }),
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
        &known_state(),
    )
    .unwrap()
}

fn write_mission(root: &Path, trigger: &str, effect: &str) {
    fs::create_dir_all(root.join("missions")).unwrap();
    fs::write(
        root.join("descriptor.mod"),
        "name=\"Mission Execution Shapes\"\nsupported_version=\"1.37.*\"\n",
    )
    .unwrap();
    fs::write(
        root.join("missions/shapes.txt"),
        format!(
            "aaa_shape_series = {{\n    slot = 5\n    potential = {{ tag = AAA }}\n    aaa_shape = {{\n        position = 1\n        trigger = {trigger}\n        effect = {effect}\n    }}\n}}\n"
        ),
    )
    .unwrap();
}

fn write_raw_mod(root: &Path, missions: &str) {
    fs::create_dir_all(root.join("missions")).unwrap();
    fs::write(
        root.join("descriptor.mod"),
        "name=\"Mission Execution Shapes\"\nsupported_version=\"1.37.*\"\n",
    )
    .unwrap();
    fs::write(root.join("missions/shapes.txt"), missions).unwrap();
}

fn assert_shape_diagnostic(unresolved: &[String], field: &str, line: usize) {
    let expected = format!("mission {field} requires a non-mixed object block");
    assert_diagnostic_location(unresolved, &expected, line);
}

fn assert_diagnostic_location(unresolved: &[String], expected: &str, line: usize) {
    let diagnostic = unresolved
        .iter()
        .find(|reason| reason.contains(expected))
        .unwrap_or_else(|| panic!("missing diagnostic {expected:?}: {unresolved:?}"));
    let normalized = diagnostic.replace('\\', "/");
    assert!(
        normalized.contains(&format!("missions/shapes.txt:{line}:")),
        "diagnostic must point to the field on line {line}: {diagnostic}"
    );
}

#[test]
fn mixed_mission_blocks_are_diagnosed_and_cannot_be_claimed() {
    let cases = [
        ("trigger", "{ always = yes 7 }", "{ add_treasury = 9 }", 6),
        ("effect", "{ always = yes }", "{ add_treasury = 9 7 }", 7),
    ];

    for (field, trigger, effect, diagnostic_line) in cases {
        let fixture = tempfile::tempdir().unwrap();
        write_mission(fixture.path(), trigger, effect);
        let mut world = load_with_mod(fixture.path());
        let report = world.country_missions("AAA").unwrap();
        let mission = report
            .missions
            .iter()
            .find(|mission| mission.id == "aaa_shape")
            .unwrap();

        assert_eq!(mission.selected, Some(true), "{field}: {mission:?}");
        assert_eq!(mission.completed, Some(false), "{field}: {mission:?}");
        if field == "trigger" {
            assert_eq!(mission.requirements_met, None, "{field}: {mission:?}");
            assert!(mission.reward_supported, "{field}: {mission:?}");
        } else {
            assert_eq!(mission.requirements_met, Some(true), "{field}: {mission:?}");
            assert!(!mission.reward_supported, "{field}: {mission:?}");
        }
        assert_shape_diagnostic(&mission.unresolved, field, diagnostic_line);
        assert!(!mission.claimable, "{field}: {mission:?}");

        let before = world.save().unwrap();
        let random_before = world.initial.world().eu4_random_state();
        let entity = world.initial.country_entity("AAA").unwrap();
        assert!(world.claim_mission("AAA", "aaa_shape").is_err());
        assert_eq!(world.save().unwrap(), before, "{field}: claim wrote state");
        assert_eq!(
            world.initial.world().eu4_random_state(),
            random_before,
            "{field}: claim advanced random state"
        );
        assert!(world.initial.world().pending_events().is_empty());
        assert!(
            !world
                .initial
                .world()
                .completion_recorded(entity, "aaa_shape")
                .unwrap()
        );
    }
}

#[test]
fn arrays_scalars_and_nested_blocks_cannot_become_claimable_mission_rules() {
    let cases = [
        ("trigger", "{ yes no }", "{ add_treasury = 9 }", true),
        ("effect", "{ always = yes }", "{ 9 10 }", true),
        (
            "trigger",
            "{ always = yes 7 }",
            "{ add_treasury = 9 }",
            true,
        ),
        (
            "trigger",
            "{ 7 always = yes }",
            "{ add_treasury = 9 }",
            true,
        ),
        ("effect", "{ always = yes }", "{ add_treasury = 9 7 }", true),
        ("effect", "{ always = yes }", "{ 7 add_treasury = 9 }", true),
        ("trigger", "yes", "{ add_treasury = 9 }", true),
        ("effect", "{ always = yes }", "9", true),
        (
            "trigger",
            "{ AND = { always = yes 7 } }",
            "{ add_treasury = 9 }",
            false,
        ),
        (
            "trigger",
            "{ AND = { 7 always = yes } }",
            "{ add_treasury = 9 }",
            false,
        ),
        (
            "effect",
            "{ always = yes }",
            "{ if = { limit = { always = no } add_treasury = 9 7 } }",
            false,
        ),
    ];

    for (field, trigger, effect, requires_shape_diagnostic) in cases {
        let fixture = tempfile::tempdir().unwrap();
        write_mission(fixture.path(), trigger, effect);
        let mut world = load_with_mod(fixture.path());
        let report = world.country_missions("AAA").unwrap();
        let mission = report
            .missions
            .iter()
            .find(|mission| mission.id == "aaa_shape")
            .unwrap();

        assert_eq!(mission.selected, Some(true), "{field}: {mission:?}");
        assert_eq!(mission.completed, Some(false), "{field}: {mission:?}");
        assert!(!mission.unresolved.is_empty(), "{field}: {mission:?}");
        assert!(!mission.claimable, "{field}: {mission:?}");
        if field == "trigger" {
            assert_eq!(mission.requirements_met, None, "{field}: {mission:?}");
            assert!(mission.reward_supported, "{field}: {mission:?}");
        } else {
            assert_eq!(mission.requirements_met, Some(true), "{field}: {mission:?}");
            assert!(!mission.reward_supported, "{field}: {mission:?}");
        }
        if requires_shape_diagnostic {
            assert_shape_diagnostic(
                &mission.unresolved,
                field,
                if field == "trigger" { 6 } else { 7 },
            );
        }

        let before = world.save().unwrap();
        let random_before = world.initial.world().eu4_random_state();
        let entity = world.initial.country_entity("AAA").unwrap();
        assert!(world.claim_mission("AAA", "aaa_shape").is_err());
        assert_eq!(world.save().unwrap(), before, "{field}: claim wrote state");
        assert_eq!(world.initial.world().eu4_random_state(), random_before);
        assert!(world.initial.world().pending_events().is_empty());
        assert!(
            !world
                .initial
                .world()
                .completion_recorded(entity, "aaa_shape")
                .unwrap()
        );
    }
}

#[test]
fn legal_empty_and_regular_mission_blocks_remain_claimable() {
    for (trigger, effect, expected_treasury) in [
        ("{}", "{}", Some(0)),
        ("{ always = yes }", "{ add_treasury = 4 }", Some(4_000)),
    ] {
        let fixture = tempfile::tempdir().unwrap();
        write_mission(fixture.path(), trigger, effect);
        let mut world = load_with_mod(fixture.path());
        let report = world.country_missions("AAA").unwrap();
        let mission = report
            .missions
            .iter()
            .find(|mission| mission.id == "aaa_shape")
            .unwrap();
        assert_eq!(mission.selected, Some(true), "{mission:?}");
        assert_eq!(mission.requirements_met, Some(true), "{mission:?}");
        assert!(mission.reward_supported, "{mission:?}");
        assert!(mission.claimable, "{mission:?}");
        assert!(mission.unresolved.is_empty(), "{mission:?}");

        world.claim_mission("AAA", "aaa_shape").unwrap();
        assert_eq!(
            world
                .country("AAA")
                .unwrap()
                .resources
                .unwrap()
                .treasury_milli,
            expected_treasury
        );
        let entity = world.initial.country_entity("AAA").unwrap();
        assert!(
            world
                .initial
                .world()
                .completion_recorded(entity, "aaa_shape")
                .unwrap()
        );
    }
}

#[test]
fn malformed_required_missions_shapes_keep_their_strict_diagnostic() {
    for required in ["{ aaa_shape = yes }", "yes"] {
        let fixture = tempfile::tempdir().unwrap();
        write_raw_mod(
            fixture.path(),
            &format!(
                "aaa_required_series = {{\n    slot = 5\n    potential = {{ tag = AAA }}\n    aaa_shape = {{\n        position = 1\n        required_missions = {required}\n        trigger = {{ always = yes }}\n        effect = {{ add_treasury = 9 }}\n    }}\n}}\n"
            ),
        );
        let mut world = load_with_mod(fixture.path());
        let report = world.country_missions("AAA").unwrap();
        let mission = report
            .missions
            .iter()
            .find(|mission| mission.id == "aaa_shape")
            .unwrap();
        assert_eq!(mission.selected, Some(true), "{required}: {mission:?}");
        assert_eq!(mission.completed, Some(false), "{required}: {mission:?}");
        assert_eq!(
            mission.requirements_met,
            Some(true),
            "{required}: {mission:?}"
        );
        assert!(mission.reward_supported, "{required}: {mission:?}");
        assert!(!mission.claimable, "{required}: {mission:?}");
        assert_diagnostic_location(
            &mission.unresolved,
            "required_missions requires a scalar array",
            6,
        );

        let before = world.save().unwrap();
        let entity = world.initial.country_entity("AAA").unwrap();
        assert!(world.claim_mission("AAA", "aaa_shape").is_err());
        assert_eq!(world.save().unwrap(), before);
        assert!(
            !world
                .initial
                .world()
                .completion_recorded(entity, "aaa_shape")
                .unwrap()
        );
    }

    let fixture = tempfile::tempdir().unwrap();
    write_raw_mod(
        fixture.path(),
        "aaa_required_series = { slot = 5 potential = { tag = AAA } aaa_shape = { position = 1 required_missions = {} trigger = {} effect = {} } }",
    );
    let world = load_with_mod(fixture.path());
    let mission = world
        .country_missions("AAA")
        .unwrap()
        .missions
        .into_iter()
        .find(|mission| mission.id == "aaa_shape")
        .unwrap();
    assert!(mission.claimable, "{mission:?}");
    assert!(mission.unresolved.is_empty(), "{mission:?}");
}

#[test]
fn malformed_rule_identity_is_stable_and_distinct_from_valid_rules() {
    let invalid_first = tempfile::tempdir().unwrap();
    let invalid_second = tempfile::tempdir().unwrap();
    let empty = tempfile::tempdir().unwrap();
    let prefix_first = tempfile::tempdir().unwrap();
    let prefix_second = tempfile::tempdir().unwrap();
    let invalid_effect_first = tempfile::tempdir().unwrap();
    let invalid_effect_second = tempfile::tempdir().unwrap();
    let empty_effect = tempfile::tempdir().unwrap();

    write_mission(
        invalid_first.path(),
        "{ always = yes 7 }",
        "{ add_treasury = 1 }",
    );
    write_mission(
        invalid_second.path(),
        "{ always = yes 7 }",
        "{ add_treasury = 1 }",
    );
    write_mission(empty.path(), "{}", "{ add_treasury = 1 }");
    write_mission(
        prefix_first.path(),
        "{ always = yes }",
        "{ add_treasury = 1 }",
    );
    write_raw_mod(
        prefix_second.path(),
        "aaa_shape_series={slot=5 potential={tag=AAA} aaa_shape={position=1 trigger={always=yes} effect={add_treasury=1}}}",
    );
    write_mission(
        invalid_effect_first.path(),
        "{ always = yes }",
        "{ add_treasury = 1 7 }",
    );
    write_mission(
        invalid_effect_second.path(),
        "{ always = yes }",
        "{ add_treasury = 1 7 }",
    );
    write_mission(empty_effect.path(), "{ always = yes }", "{}");

    let invalid_a = load_with_mod(invalid_first.path());
    let invalid_b = load_with_mod(invalid_second.path());
    let mut empty_world = load_with_mod(empty.path());
    let mut prefix_a = load_with_mod(prefix_first.path());
    let prefix_b = load_with_mod(prefix_second.path());
    let invalid_effect_a = load_with_mod(invalid_effect_first.path());
    let invalid_effect_b = load_with_mod(invalid_effect_second.path());
    let mut empty_effect_world = load_with_mod(empty_effect.path());
    let invalid_snapshot = invalid_a.save().unwrap();
    let empty_snapshot = empty_world.save().unwrap();
    let prefix_snapshot = prefix_a.save().unwrap();
    let invalid_effect_snapshot = invalid_effect_a.save().unwrap();
    let empty_effect_snapshot = empty_effect_world.save().unwrap();

    assert_eq!(invalid_snapshot, invalid_b.save().unwrap());
    assert_eq!(prefix_snapshot, prefix_b.save().unwrap());
    assert_eq!(invalid_effect_snapshot, invalid_effect_b.save().unwrap());
    assert_ne!(invalid_snapshot, empty_snapshot);
    assert_ne!(invalid_snapshot, prefix_snapshot);
    assert_ne!(invalid_effect_snapshot, empty_effect_snapshot);
    assert_ne!(invalid_effect_snapshot, prefix_snapshot);
    assert!(empty_world.restore(&invalid_snapshot).is_err());
    assert!(prefix_a.restore(&invalid_snapshot).is_err());
    assert!(
        empty_effect_world
            .restore(&invalid_effect_snapshot)
            .is_err()
    );
    assert!(prefix_a.restore(&invalid_effect_snapshot).is_err());

    let invalid_view = invalid_a
        .country_missions("AAA")
        .unwrap()
        .missions
        .into_iter()
        .find(|mission| mission.id == "aaa_shape")
        .unwrap();
    let empty_view = empty_world
        .country_missions("AAA")
        .unwrap()
        .missions
        .into_iter()
        .find(|mission| mission.id == "aaa_shape")
        .unwrap();
    let prefix_view = prefix_a
        .country_missions("AAA")
        .unwrap()
        .missions
        .into_iter()
        .find(|mission| mission.id == "aaa_shape")
        .unwrap();
    let invalid_effect_view = invalid_effect_a
        .country_missions("AAA")
        .unwrap()
        .missions
        .into_iter()
        .find(|mission| mission.id == "aaa_shape")
        .unwrap();
    let empty_effect_view = empty_effect_world
        .country_missions("AAA")
        .unwrap()
        .missions
        .into_iter()
        .find(|mission| mission.id == "aaa_shape")
        .unwrap();
    assert!(!invalid_view.claimable, "{invalid_view:?}");
    assert!(empty_view.claimable, "{empty_view:?}");
    assert!(prefix_view.claimable, "{prefix_view:?}");
    assert!(!invalid_effect_view.claimable, "{invalid_effect_view:?}");
    assert!(empty_effect_view.claimable, "{empty_effect_view:?}");
}

#[test]
fn non_equal_trigger_and_effect_operators_still_fail_content_loading() {
    for field in ["trigger", "effect"] {
        let fixture = tempfile::tempdir().unwrap();
        let (trigger, effect) = if field == "trigger" {
            ("> { always = yes }", "= { add_treasury = 1 }")
        } else {
            ("= { always = yes }", "> { add_treasury = 1 }")
        };
        write_raw_mod(
            fixture.path(),
            &format!(
                "aaa_operator_series = {{ slot = 5 potential = {{ tag = AAA }} aaa_operator = {{ position = 1 trigger {trigger} effect {effect} }} }}"
            ),
        );
        let content = ContentSet::resolve(vec![
            ContentSource::base(ContentRoot::open(base_root()).unwrap()),
            ContentSource::mod_directory(fixture.path(), TextEncoding::Utf8).unwrap(),
        ])
        .unwrap();
        assert!(
            VanillaWorld::load_from_content_with_players_and_external_state(
                &content,
                "1444.11.11",
                &BTreeSet::from(["AAA".into(), "BBB".into()]),
                &known_state(),
            )
            .is_err()
        );
    }
}
