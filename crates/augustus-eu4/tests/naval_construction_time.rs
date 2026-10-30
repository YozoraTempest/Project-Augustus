// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::{fs, path::Path};

use augustus_core::VanillaNavalConstructionTimeResult;
use augustus_eu4::{ContentSet, TextEncoding, VanillaNavalConstructionTimeQuery, VanillaWorld};
use tempfile::TempDir;
use walkdir::WalkDir;

fn source_fixture() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world")
}

fn fixture() -> TempDir {
    let temp = tempfile::tempdir().unwrap();
    let game = temp.path().join("game");
    for entry in WalkDir::new(source_fixture()) {
        let entry = entry.unwrap();
        let target = game.join(entry.path().strip_prefix(source_fixture()).unwrap());
        if entry.file_type().is_dir() {
            fs::create_dir_all(target).unwrap();
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }

    let light_ship = fs::read_to_string(game.join("common/units/sample_light_ship.txt")).unwrap();
    assert!(light_ship.contains("type = light_ship"));
    fs::write(
        game.join("common/units/sample_galley.txt"),
        light_ship.replace("type = light_ship", "type = galley"),
    )
    .unwrap();
    temp
}

fn write(root: &Path, path: &str, contents: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

fn naval_time_mod(temp: &TempDir, contents: &str) -> std::path::PathBuf {
    let path = temp.path().join("naval-time-mod");
    write(&path, "descriptor.mod", "name = \"Naval Time Test\"\n");
    write(&path, "common/defines/20_naval_time.lua", contents);
    path
}

fn load_world(
    temp: &TempDir,
    mods: &[std::path::PathBuf],
    controller: Option<&str>,
) -> (ContentSet, VanillaWorld) {
    let game = temp.path().join("game");
    let content =
        ContentSet::from_installation(&game, mods, None, None, TextEncoding::Utf8).unwrap();
    let controller = controller
        .map(|tag| format!("controller={tag}"))
        .unwrap_or_default();
    let save = temp.path().join("world.eu4");
    fs::write(
        &save,
        format!(
            "EU4txt\ndate=1444.11.11\nplayer=AAA\ncountries={{ AAA={{ treasury=20.000 }} BBB={{}} }}\nprovinces={{ -1={{ owner=AAA {controller} }} -2={{ owner=AAA controller=AAA }} }}\n"
        ),
    )
    .unwrap();
    let world = VanillaWorld::load_from_content_with_save(&content, &save).unwrap();
    (content, world)
}

fn query(country: &str) -> VanillaNavalConstructionTimeQuery {
    VanillaNavalConstructionTimeQuery {
        country: country.into(),
        province_id: 1,
        unit_type: "sample_galley".into(),
        country_modifier_milli: Some(0),
        province_modifier_milli: Some(-30),
        additional_province_modifier_milli: Some(0),
    }
}

fn valid_defines(galley_time: &str, minimum: &str) -> String {
    format!(
        "NDefines.NMilitary.GALLEY_TIME={galley_time}\nNDefines.NMilitary.MIN_RECRUIT_TIME_MODIFIER={minimum}\n"
    )
}

#[test]
fn galley_time_query_uses_mod_defines_and_caller_resolved_modifiers() {
    let temp = fixture();
    let game = temp.path().join("game");
    write(
        &game,
        "common/defines/20_naval_time.lua",
        &valid_defines("400", "0.1"),
    );
    let patch = naval_time_mod(&temp, &valid_defines("365", "0.2"));
    let (content, world) = load_world(&temp, std::slice::from_ref(&patch), Some("AAA"));

    assert_eq!(
        world.report.setup.naval_construction_time_defines,
        std::collections::BTreeMap::from([
            ("GALLEY_TIME".into(), 365),
            ("MIN_RECRUIT_TIME_MODIFIER".into(), 200),
        ])
    );
    for name in ["GALLEY_TIME", "MIN_RECRUIT_TIME_MODIFIER"] {
        let source =
            &world.report.setup.definition_sources[&format!("naval_construction_time:{name}")];
        assert!(source.path.starts_with(fs::canonicalize(&patch).unwrap()));
    }

    let before = world.save().unwrap();
    let first = world.query_naval_construction_time(&query("AAA")).unwrap();
    assert_eq!(
        first.evaluation,
        Some(VanillaNavalConstructionTimeResult {
            modifier_factor_milli: 970,
            duration_days: 354,
        })
    );
    assert!(first.unresolved.is_empty());

    let mut faster = query("AAA");
    faster.province_modifier_milli = Some(-40);
    let second = world.query_naval_construction_time(&faster).unwrap();
    assert_eq!(
        second.evaluation,
        Some(VanillaNavalConstructionTimeResult {
            modifier_factor_milli: 960,
            duration_days: 350,
        })
    );
    assert_eq!(world.save().unwrap(), before);

    let mut restored =
        VanillaWorld::load_from_content_with_save(&content, temp.path().join("world.eu4")).unwrap();
    restored.restore(&before).unwrap();
    assert_eq!(
        restored
            .query_naval_construction_time(&query("AAA"))
            .unwrap(),
        first
    );
}

#[test]
fn exact_thousandth_defines_accept_decimal_values_without_rounding_finer_values() {
    for (minimum, expected) in [("1.001", 1_001), ("1.003", 1_003)] {
        let temp = fixture();
        let patch = naval_time_mod(&temp, &valid_defines("365", minimum));
        let (_, world) = load_world(&temp, &[patch], Some("AAA"));
        assert_eq!(
            world.report.setup.naval_construction_time_defines["MIN_RECRUIT_TIME_MODIFIER"],
            expected
        );
        assert!(
            world
                .report
                .setup
                .naval_construction_time_define_errors
                .is_empty()
        );
    }

    for minimum in ["0.0000001", "-0.0000001"] {
        let temp = fixture();
        let patch = naval_time_mod(&temp, &valid_defines("365", minimum));
        let (_, world) = load_world(&temp, &[patch], Some("AAA"));
        assert!(
            world
                .report
                .setup
                .naval_construction_time_define_errors
                .contains_key("MIN_RECRUIT_TIME_MODIFIER")
        );
        let error = world
            .query_naval_construction_time(&query("AAA"))
            .unwrap_err();
        assert!(error.to_string().contains("MIN_RECRUIT_TIME_MODIFIER"));
    }
}

#[test]
fn missing_defines_and_caller_modifiers_stay_unresolved() {
    let temp = fixture();
    let (_, world) = load_world(&temp, &[], Some("AAA"));
    let mut incomplete = query("AAA");
    incomplete.country_modifier_milli = None;
    incomplete.province_modifier_milli = None;
    incomplete.additional_province_modifier_milli = None;

    let report = world.query_naval_construction_time(&incomplete).unwrap();
    assert_eq!(report.evaluation, None);
    for unresolved in [
        "naval_construction_time:missing_define:GALLEY_TIME",
        "naval_construction_time:missing_define:MIN_RECRUIT_TIME_MODIFIER",
        "naval_construction_time:missing_input:country_modifier_milli",
        "naval_construction_time:missing_input:province_modifier_milli",
        "naval_construction_time:missing_input:additional_province_modifier_milli",
    ] {
        assert!(report.unresolved.iter().any(|entry| entry == unresolved));
    }
}

#[test]
fn invalid_defines_are_deferred_and_unknown_identities_take_priority() {
    let temp = fixture();
    let patch = naval_time_mod(&temp, &valid_defines("365.5", "0.2"));
    let (_, world) = load_world(&temp, std::slice::from_ref(&patch), Some("AAA"));
    assert!(
        world
            .report
            .setup
            .naval_construction_time_define_errors
            .contains_key("GALLEY_TIME")
    );

    let unknown_country = query("MISSING");
    let error = world
        .query_naval_construction_time(&unknown_country)
        .unwrap_err();
    assert!(error.to_string().contains("unknown country MISSING"));
    assert!(!error.to_string().contains("GALLEY_TIME"));

    let mut unknown_province = query("AAA");
    unknown_province.province_id = u32::MAX;
    let error = world
        .query_naval_construction_time(&unknown_province)
        .unwrap_err();
    assert!(error.to_string().contains("unknown province"));
    assert!(!error.to_string().contains("GALLEY_TIME"));

    let mut unknown_template = query("AAA");
    unknown_template.unit_type = "missing_galley".into();
    let error = world
        .query_naval_construction_time(&unknown_template)
        .unwrap_err();
    assert!(error.to_string().contains("unknown unit template"));
    assert!(!error.to_string().contains("GALLEY_TIME"));

    let mut non_naval = query("AAA");
    non_naval.unit_type = "test_infantry".into();
    let error = world.query_naval_construction_time(&non_naval).unwrap_err();
    assert!(error.to_string().contains("is not naval"));
    assert!(!error.to_string().contains("GALLEY_TIME"));

    let error = world
        .query_naval_construction_time(&query("AAA"))
        .unwrap_err();
    assert!(error.to_string().contains("GALLEY_TIME"));
    assert!(error.to_string().contains("20_naval_time.lua"));
    assert!(error.location().is_some());
}

#[test]
fn other_naval_categories_are_unresolved_and_owner_controller_are_live() {
    let temp = fixture();
    let patch = naval_time_mod(&temp, &valid_defines("365", "0.2"));
    let (_, mut world) = load_world(&temp, &[patch], Some("AAA"));

    let mut light_ship = query("AAA");
    light_ship.unit_type = "sample_light_ship".into();
    let unsupported = world.query_naval_construction_time(&light_ship).unwrap();
    assert_eq!(unsupported.evaluation, None);
    assert_eq!(
        unsupported.unresolved,
        ["naval_construction_time:unsupported_category"]
    );

    world
        .initial
        .set_province_controller(1, Some("BBB"))
        .unwrap();
    let owner_mismatch = world.query_naval_construction_time(&query("BBB")).unwrap();
    assert_eq!(
        owner_mismatch.unresolved,
        ["naval_construction_time:owner_mismatch"]
    );
    let controller_mismatch = world.query_naval_construction_time(&query("AAA")).unwrap();
    assert_eq!(
        controller_mismatch.unresolved,
        ["naval_construction_time:controller_mismatch"]
    );

    let snapshot = world.save().unwrap();
    let resumed_content = ContentSet::from_installation(
        temp.path().join("game"),
        &[temp.path().join("naval-time-mod")],
        None,
        None,
        TextEncoding::Utf8,
    )
    .unwrap();
    let mut resumed =
        VanillaWorld::load_from_content_with_save(&resumed_content, temp.path().join("world.eu4"))
            .unwrap();
    resumed.restore(&snapshot).unwrap();
    assert_eq!(
        resumed
            .query_naval_construction_time(&query("BBB"))
            .unwrap(),
        owner_mismatch
    );
    assert_eq!(
        resumed
            .query_naval_construction_time(&query("AAA"))
            .unwrap(),
        controller_mismatch
    );
}

#[test]
fn unknown_and_known_absent_controllers_do_not_produce_a_duration() {
    let temp = fixture();
    let patch = naval_time_mod(&temp, &valid_defines("365", "0.2"));
    let content = ContentSet::from_installation(
        temp.path().join("game"),
        &[patch],
        None,
        None,
        TextEncoding::Utf8,
    )
    .unwrap();
    let unknown = VanillaWorld::load_from_content(&content, "1444.11.11").unwrap();
    let unknown = unknown
        .query_naval_construction_time(&query("AAA"))
        .unwrap();
    assert!(unknown.evaluation.is_none());
    assert!(
        unknown
            .unresolved
            .contains(&"naval_construction_time:controller_unknown".into())
    );

    let temp = fixture();
    let patch = naval_time_mod(&temp, &valid_defines("365", "0.2"));
    let (_, mut absent) = load_world(&temp, &[patch], Some("AAA"));
    absent.initial.set_province_controller(1, None).unwrap();
    let absent = absent.query_naval_construction_time(&query("AAA")).unwrap();
    assert!(absent.evaluation.is_none());
    assert!(
        absent
            .unresolved
            .contains(&"naval_construction_time:controller_mismatch".into())
    );

    let temp = fixture();
    let patch = naval_time_mod(&temp, &valid_defines("365", "0.2"));
    let (_, mut ownerless) = load_world(&temp, &[patch], Some("AAA"));
    ownerless.initial.set_province_owner(1, None).unwrap();
    let ownerless = ownerless
        .query_naval_construction_time(&query("AAA"))
        .unwrap();
    assert!(ownerless.evaluation.is_none());
    assert!(
        ownerless
            .unresolved
            .contains(&"naval_construction_time:owner_unknown_or_absent".into())
    );
}
