// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::NumericSlot;
use augustus_eu4::{VanillaWorld, read_vanilla_save_state};
use std::{collections::BTreeMap, fs, path::Path};

fn tradition_fixture(modifier: Option<&str>) -> tempfile::TempDir {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world");
    let root = tempfile::tempdir().unwrap();
    for entry in walkdir::WalkDir::new(&source) {
        let entry = entry.unwrap();
        let target = root
            .path()
            .join(entry.path().strip_prefix(&source).unwrap());
        if entry.file_type().is_dir() {
            fs::create_dir_all(target).unwrap();
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }

    let static_path = root.path().join("common/static_modifiers/test_static.txt");
    let static_text = fs::read_to_string(&static_path).unwrap();
    let static_text = static_text.replace(
        "base_values = { global_manpower = 8 }",
        "base_values = { global_manpower = 8 navy_tradition_decay = 0.05 }\nnavy_tradition = { trade_steering = 1.0 }",
    );
    assert!(static_text.contains("navy_tradition_decay = 0.05"));
    fs::write(static_path, static_text).unwrap();

    if let Some(modifier) = modifier {
        fs::create_dir_all(root.path().join("common/event_modifiers")).unwrap();
        fs::write(
            root.path()
                .join("common/event_modifiers/navy_tradition.txt"),
            format!("navy_tradition_control = {{ {modifier} }}"),
        )
        .unwrap();
        fs::create_dir_all(root.path().join("common/on_actions")).unwrap();
        fs::write(
            root.path().join("common/on_actions/navy_tradition.txt"),
            "on_startup = { if = { limit = { tag = AAA } add_country_modifier = { name = navy_tradition_control duration = -1 } } }",
        )
        .unwrap();
    }
    root
}

fn startup_effect_fixture(effect: &str) -> tempfile::TempDir {
    let root = tradition_fixture(None);
    fs::create_dir_all(root.path().join("common/on_actions")).unwrap();
    fs::write(
        root.path()
            .join("common/on_actions/navy_tradition_effect.txt"),
        format!("on_startup={{if={{limit={{tag=AAA}} {effect}}}}}"),
    )
    .unwrap();
    root
}

fn initial_values() -> BTreeMap<String, BTreeMap<String, Option<i64>>> {
    known_navy_tradition(9_000)
}

fn known_navy_tradition(value: i64) -> BTreeMap<String, BTreeMap<String, Option<i64>>> {
    BTreeMap::from([(
        "AAA".into(),
        BTreeMap::from([("navy_tradition".into(), Some(value))]),
    )])
}

fn load_with_startup_navy_effect(effect: &str, initial: Option<i64>) -> VanillaWorld {
    let root = startup_effect_fixture(effect);
    match initial {
        Some(value) => VanillaWorld::load_with_country_metric_values(
            root.path(),
            "1444.11.11",
            &known_navy_tradition(value),
        ),
        None => VanillaWorld::load(root.path(), "1444.11.11"),
    }
    .unwrap()
}

#[test]
fn save_import_reads_navy_tradition_in_milli_units() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("state.eu4");
    fs::write(
        &path,
        "EU4txt\ndate=1444.11.11\nplayer=AAA\ncountries={ AAA={ navy_tradition=9.000 } }\nprovinces={}\n",
    )
    .unwrap();

    let save = read_vanilla_save_state(path).unwrap();
    assert_eq!(
        save.external_state.country_metric_values["AAA"]["navy_tradition"],
        Some(9_000)
    );
}

#[test]
fn startup_effect_adds_quantized_navy_tradition_with_bounds_and_unknown_base() {
    let precise = load_with_startup_navy_effect("add_navy_tradition=1.234", Some(9_000));
    assert!(
        precise.report.estates.startup_unresolved.is_empty(),
        "{:?}",
        precise.report.estates.startup_unresolved
    );
    let precise_country = precise.initial.country_entity("AAA").unwrap();
    let metric = NumericSlot::Metric("navy_tradition".into());
    assert_eq!(
        precise
            .initial
            .world()
            .numeric_value(precise_country, &metric)
            .unwrap(),
        10_234
    );

    let lower_bound = load_with_startup_navy_effect("add_navy_tradition=-20", Some(9_000));
    let lower_country = lower_bound.initial.country_entity("AAA").unwrap();
    assert_eq!(
        lower_bound
            .initial
            .world()
            .numeric_value(lower_country, &metric)
            .unwrap(),
        0
    );

    let upper_bound = load_with_startup_navy_effect("add_navy_tradition=2", Some(99_000));
    let upper_country = upper_bound.initial.country_entity("AAA").unwrap();
    assert_eq!(
        upper_bound
            .initial
            .world()
            .numeric_value(upper_country, &metric)
            .unwrap(),
        100_000
    );

    let unknown_base = load_with_startup_navy_effect("add_navy_tradition=1.234", None);
    let unknown_country = unknown_base.initial.country_entity("AAA").unwrap();
    assert!(
        unknown_base
            .initial
            .world()
            .numeric_value(unknown_country, &metric)
            .is_err()
    );
    assert_eq!(
        unknown_base
            .initial
            .world()
            .unbased_numeric_delta(unknown_country, &metric),
        Some(1_234)
    );
}

#[test]
fn annual_gain_decay_floor_and_save_resume_match_expected_monthly_values() {
    let baseline_root = tradition_fixture(None);
    let gain_root = tradition_fixture(Some("navy_tradition = 1"));
    let floor_root = tradition_fixture(Some("navy_tradition = 1 navy_tradition_decay = -0.06"));
    let values = initial_values();
    let mut baseline =
        VanillaWorld::load_with_country_metric_values(baseline_root.path(), "1444.11.11", &values)
            .unwrap();
    let gain =
        VanillaWorld::load_with_country_metric_values(gain_root.path(), "1444.11.11", &values)
            .unwrap();
    let floor =
        VanillaWorld::load_with_country_metric_values(floor_root.path(), "1444.11.11", &values)
            .unwrap();
    let baseline_aaa = baseline.initial.country_entity("AAA").unwrap();
    let gain_aaa = gain.initial.country_entity("AAA").unwrap();
    let floor_aaa = floor.initial.country_entity("AAA").unwrap();

    let annual_change = |world: &VanillaWorld, country| {
        world
            .initial
            .world()
            .attributes()
            .evaluate(
                world.initial.world(),
                country,
                "yearly_navy_tradition_change",
            )
            .unwrap()
            .value_micros
    };
    assert_eq!(annual_change(&baseline, baseline_aaa), Some(-450_000));
    assert_eq!(annual_change(&gain, gain_aaa), Some(550_000));
    assert_eq!(annual_change(&floor, floor_aaa), Some(1_000_000));
    assert_eq!(
        gain.initial
            .world()
            .numeric_value(gain_aaa, &NumericSlot::Metric("navy_tradition".into()))
            .unwrap(),
        9_000
    );

    let snapshot = gain.save().unwrap();
    let mut resumed =
        VanillaWorld::load_with_country_metric_values(gain_root.path(), "1444.11.11", &values)
            .unwrap();
    resumed.restore(&snapshot).unwrap();
    let mut uninterrupted = gain;
    baseline.advance(20).unwrap();
    resumed.advance(20).unwrap();
    uninterrupted.advance(20).unwrap();
    let mut clamped = floor;
    clamped.advance(20).unwrap();

    let tradition = NumericSlot::Metric("navy_tradition".into());
    assert_eq!(
        baseline
            .initial
            .world()
            .numeric_value(baseline_aaa, &tradition)
            .unwrap(),
        8_963
    );
    assert_eq!(
        resumed
            .initial
            .world()
            .numeric_value(gain_aaa, &tradition)
            .unwrap(),
        9_045
    );
    assert_eq!(
        uninterrupted
            .initial
            .world()
            .numeric_value(gain_aaa, &tradition)
            .unwrap(),
        9_045
    );
    assert_eq!(
        clamped
            .initial
            .world()
            .numeric_value(floor_aaa, &tradition)
            .unwrap(),
        9_083
    );

    let income = |world: &VanillaWorld, country| {
        world
            .initial
            .world()
            .monthly_accruals()
            .unwrap()
            .last_settlement
            .as_ref()
            .unwrap()
            .accounts[&country]["navy_tradition"]
            .income
    };
    assert_eq!(income(&baseline, baseline_aaa), Some(-37));
    assert_eq!(income(&resumed, gain_aaa), Some(45));
    assert_eq!(income(&clamped, floor_aaa), Some(83));
}
