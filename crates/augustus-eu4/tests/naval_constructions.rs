// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use augustus_eu4::{
    ContentSet, TextEncoding, VanillaExternalState, VanillaNavalConstructionRecord,
    VanillaNavalConstructionReport, VanillaWorld, read_vanilla_naval_constructions,
    read_vanilla_naval_constructions_for_queue_import,
    read_vanilla_naval_constructions_for_queue_import_with_token_map,
    read_vanilla_naval_constructions_for_queue_import_with_token_resolver,
    read_vanilla_naval_constructions_with_token_map,
    read_vanilla_naval_constructions_with_token_resolver, read_vanilla_save_state,
};
use jomini::{
    binary::{Token, TokenResolver},
    common::Date,
};
use serde_json::Value;
use tempfile::TempDir;
use walkdir::WalkDir;
use zip::{ZipWriter, write::SimpleFileOptions};

fn source_fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world")
}

fn fixture_with_content() -> (TempDir, ContentSet) {
    let temp = tempfile::tempdir().unwrap();
    for entry in WalkDir::new(source_fixture()) {
        let entry = entry.unwrap();
        let target = temp
            .path()
            .join(entry.path().strip_prefix(source_fixture()).unwrap());
        if entry.file_type().is_dir() {
            fs::create_dir_all(target).unwrap();
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
    let map_path = temp.path().join("map/default.map");
    let mut map = fs::read_to_string(&map_path).unwrap();
    map.push_str("\nforce_coastal = { 1 }\n");
    fs::write(map_path, map).unwrap();
    let content =
        ContentSet::from_installation(temp.path(), &[], None, None, TextEncoding::Utf8).unwrap();
    (temp, content)
}

fn fixture_with_two_naval_types() -> (TempDir, ContentSet) {
    let (temp, _) = fixture_with_content();
    let light_ship =
        fs::read_to_string(temp.path().join("common/units/sample_light_ship.txt")).unwrap();
    fs::write(
        temp.path().join("common/units/sample_heavy_ship.txt"),
        light_ship.replace("type = light_ship", "type = heavy_ship"),
    )
    .unwrap();
    let map_path = temp.path().join("map/default.map");
    let mut map = fs::read_to_string(&map_path).unwrap();
    map.push_str("\nforce_coastal = { 2 }\n");
    fs::write(map_path, map).unwrap();
    let content =
        ContentSet::from_installation(temp.path(), &[], None, None, TextEncoding::Utf8).unwrap();
    (temp, content)
}

fn order(
    unit_type: &str,
    start_date: &str,
    total: u64,
    original_total: u64,
    date: &str,
    extra: &str,
) -> String {
    order_with_envoy(
        unit_type,
        start_date,
        total,
        original_total,
        date,
        Some("-1"),
        extra,
    )
}

fn order_with_envoy(
    unit_type: &str,
    start_date: &str,
    total: u64,
    original_total: u64,
    date: &str,
    envoy: Option<&str>,
    extra: &str,
) -> String {
    let envoy_field = envoy.map_or(String::new(), |value| format!("envoy = {value} "));
    format!(
        "military_construction = {{ country = AAA {envoy_field}type = {unit_type} start_date = {start_date} total = {total} original_total = {original_total} progress = 0.333 date = {date} cost = 5.000 {extra} }}"
    )
}

fn omit_progress(order: String) -> String {
    order.replace("progress = 0.333 ", "")
}

struct NavalQueueTestTokenResolver;

impl TokenResolver for NavalQueueTestTokenResolver {
    fn resolve(&self, token: u16) -> Option<&str> {
        match token {
            0x3000 => Some("date"),
            0x3001 => Some("provinces"),
            0x3002 => Some("military_construction"),
            0x3003 => Some("country"),
            0x3004 => Some("type"),
            0x3005 => Some("start_date"),
            0x3006 => Some("total"),
            0x3007 => Some("original_total"),
            0x3008 => Some("progress"),
            0x3009 => Some("cost"),
            _ => None,
        }
    }

    fn lookup(&self, index: u32) -> Option<&str> {
        match index {
            1 => Some("AAA"),
            2 => Some("sample_light_ship"),
            3 => Some("1444.11.10"),
            4 => Some("1444.11.11"),
            _ => None,
        }
    }
}

fn binary_save_with_missing_tail_progress() -> Vec<u8> {
    let save_date = Date::parse("1444.11.11").unwrap().to_binary();
    let first_date = Date::parse("1444.11.13").unwrap().to_binary();
    let second_date = Date::parse("1444.11.14").unwrap().to_binary();
    let mut bytes = b"EU4bin".to_vec();
    for token in [
        Token::Id(0x3000),
        Token::Equal,
        Token::I32(save_date),
        Token::Id(0x3001),
        Token::Equal,
        Token::Open,
        Token::I32(-1),
        Token::Equal,
        Token::Open,
        Token::Id(0x3002),
        Token::Equal,
        Token::Open,
        Token::Id(0x3003),
        Token::Equal,
        Token::Lookup(1),
        Token::Id(0x3004),
        Token::Equal,
        Token::Lookup(2),
        Token::Id(0x3005),
        Token::Equal,
        Token::Lookup(3),
        Token::Id(0x3006),
        Token::Equal,
        Token::I32(3),
        Token::Id(0x3007),
        Token::Equal,
        Token::I32(3),
        Token::Id(0x3008),
        Token::Equal,
        Token::F32(333_i32.to_le_bytes()),
        Token::Id(0x3000),
        Token::Equal,
        Token::I32(first_date),
        Token::Id(0x3009),
        Token::Equal,
        Token::F32(5_000_i32.to_le_bytes()),
        Token::Close,
        Token::Id(0x3002),
        Token::Equal,
        Token::Open,
        Token::Id(0x3003),
        Token::Equal,
        Token::Lookup(1),
        Token::Id(0x3004),
        Token::Equal,
        Token::Lookup(2),
        Token::Id(0x3005),
        Token::Equal,
        Token::Lookup(4),
        Token::Id(0x3006),
        Token::Equal,
        Token::I32(3),
        Token::Id(0x3007),
        Token::Equal,
        Token::I32(3),
        Token::Id(0x3000),
        Token::Equal,
        Token::I32(second_date),
        Token::Id(0x3009),
        Token::Equal,
        Token::F32(5_000_i32.to_le_bytes()),
        Token::Close,
        Token::Close,
        Token::Close,
    ] {
        token.write(&mut bytes).unwrap();
    }
    bytes
}

fn write_naval_queue_token_map(directory: &Path) -> PathBuf {
    let path = directory.join("naval-queue-tokens.txt");
    fs::write(
        &path,
        "0x3000 date\n0x3001 provinces\n0x3002 military_construction\n0x3003 country\n0x3004 type\n0x3005 start_date\n0x3006 total\n0x3007 original_total\n0x3008 progress\n0x3009 cost\nlookup 1 AAA\nlookup 2 sample_light_ship\nlookup 3 1444.11.10\nlookup 4 1444.11.11\n",
    )
    .unwrap();
    path
}

fn write_save(
    directory: &Path,
    name: &str,
    province_owner: &str,
    province_controller: &str,
    orders: &str,
    combat: &str,
) -> PathBuf {
    let path = directory.join(name);
    let text = format!(
        "EU4txt\ndate = 1444.11.11\nplayer = AAA\ncountries = {{ AAA = {{ treasury = 20.000 }} BBB = {{ }} }}\nprovinces = {{ -1 = {{ owner = {province_owner} controller = {province_controller} {orders} }} }}\n{combat}\n"
    );
    fs::write(&path, text).unwrap();
    path
}

fn write_save_with_two_provinces(
    directory: &Path,
    name: &str,
    first_orders: &str,
    second_orders: &str,
) -> PathBuf {
    let path = directory.join(name);
    let text = format!(
        "EU4txt\ndate = 1444.11.11\nplayer = AAA\ncountries = {{ AAA = {{ treasury = 20.000 }} BBB = {{ }} }}\nprovinces = {{ -1 = {{ owner = AAA controller = AAA {first_orders} }} -2 = {{ owner = AAA controller = AAA {second_orders} }} }}\n"
    );
    fs::write(&path, text).unwrap();
    path
}

fn pending_count(world: &VanillaWorld) -> usize {
    world.initial.world().naval_constructions().len()
}

fn treasury(world: &VanillaWorld) -> Option<i64> {
    let country = world.initial.country_entity("AAA")?;
    world
        .initial
        .world()
        .country_resources()
        .get(&country)
        .and_then(|resources| resources.treasury_milli)
}

fn fingerprint(world: &VanillaWorld) -> String {
    serde_json::from_str::<Value>(&world.save().unwrap()).unwrap()["content"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[test]
fn pending_order_query_translates_live_queue_state_and_preserves_snapshot() {
    let (directory, content) = fixture_with_two_naval_types();
    let queue = format!(
        "{} {}",
        order("sample_light_ship", "1444.11.10", 3, 3, "1444.11.13", ""),
        order("sample_heavy_ship", "1444.11.11", 1, 1, "1444.11.12", "")
    );
    let single = order("sample_light_ship", "1444.11.10", 6, 6, "1444.11.16", "");
    let path = write_save_with_two_provinces(directory.path(), "query.eu4", &queue, &single);
    let report = read_vanilla_naval_constructions(&path).unwrap();
    let mut loaded = VanillaWorld::load_from_content_with_save(&content, &path).unwrap();
    let default_orders = loaded.naval_construction_orders().unwrap();
    assert_eq!(default_orders.len(), 1);
    assert_eq!(default_orders[0].province_id, 2);
    assert_eq!(default_orders[0].queue_position, None);
    loaded.import_naval_construction_queues(&report).unwrap();
    let snapshot = loaded.save().unwrap();
    let orders = loaded.naval_construction_orders().unwrap();
    assert_eq!(orders.len(), 3);
    assert_eq!(
        orders.iter().map(|order| order.id.0).collect::<Vec<_>>(),
        [0, 1, 2]
    );
    assert_eq!(
        orders
            .iter()
            .map(|order| order.province_id)
            .collect::<Vec<_>>(),
        [2, 1, 1]
    );
    assert!(orders.iter().all(|order| order.country == "AAA"));
    assert_eq!(orders[1].unit_type, "sample_light_ship");
    assert_eq!(orders[2].unit_type, "sample_heavy_ship");
    assert_eq!(orders[1].queue_position, Some(1));
    assert_eq!(orders[2].queue_position, Some(2));
    assert_eq!(orders[2].completion_day, 2);
    assert_eq!(
        serde_json::to_value(&orders[2]).unwrap()["completion_date"],
        serde_json::json!({"year":1444,"month":11,"day":13})
    );
    assert_eq!(loaded.save().unwrap(), snapshot);

    loaded.advance(3).unwrap();
    let snapshot = loaded.save().unwrap();
    let mut restored = VanillaWorld::load_from_content_with_save(&content, &path).unwrap();
    restored.restore(&snapshot).unwrap();
    let current = restored.naval_construction_orders().unwrap();
    assert_eq!(current.len(), 2);
    assert_eq!(current[0].queue_position, None);
    assert_eq!(current[1].id, orders[2].id);
    assert_eq!(current[1].queue_position, Some(1));
    assert_eq!(current[1].completion_day, 4);
    assert_eq!(
        serde_json::to_value(&current[1]).unwrap()["completion_date"],
        serde_json::json!({"year":1444,"month":11,"day":15})
    );
    assert_eq!(restored.save().unwrap(), snapshot);
    assert_eq!(
        restored.naval_construction_orders().unwrap(),
        loaded.naval_construction_orders().unwrap()
    );
}

#[test]
fn saved_single_naval_order_resumes_and_completes_without_charging_twice() {
    let (directory, content) = fixture_with_content();
    let order = order("sample_light_ship", "1444.11.10", 3, 3, "1444.11.13", "");
    let path = write_save(directory.path(), "with-order.eu4", "AAA", "AAA", &order, "");
    let mut loaded = VanillaWorld::load_from_content_with_save(&content, &path).unwrap();
    assert_eq!(loaded.naval_construction_import_report().imported_count, 1);
    assert!(
        loaded
            .naval_construction_import_report()
            .unresolved
            .is_empty()
    );
    assert_eq!(pending_count(&loaded), 1);
    assert!(loaded.initial.naval_units().is_empty());
    assert_eq!(treasury(&loaded), Some(20_000));
    let overlay_loaded = VanillaWorld::load_from_content_with_save_and_external_state(
        &content,
        &path,
        &VanillaExternalState::default(),
    )
    .unwrap();
    assert_eq!(
        overlay_loaded
            .naval_construction_import_report()
            .imported_count,
        1
    );
    assert_eq!(pending_count(&overlay_loaded), 1);

    let missing_envoy_order = order_with_envoy(
        "sample_light_ship",
        "1444.11.10",
        3,
        3,
        "1444.11.13",
        None,
        "",
    );
    let missing_envoy_path = write_save(
        directory.path(),
        "missing-envoy.eu4",
        "AAA",
        "AAA",
        &missing_envoy_order,
        "",
    );
    let missing_envoy =
        VanillaWorld::load_from_content_with_save(&content, &missing_envoy_path).unwrap();
    assert_eq!(
        missing_envoy
            .naval_construction_import_report()
            .imported_count,
        1
    );

    let no_order_path = write_save(directory.path(), "without-order.eu4", "AAA", "AAA", "", "");
    let no_order = VanillaWorld::load_from_content_with_save(&content, &no_order_path).unwrap();
    assert_ne!(fingerprint(&loaded), fingerprint(&no_order));

    let snapshot = loaded.save().unwrap();
    let mut resumed = VanillaWorld::load_from_content_with_save(&content, &path).unwrap();
    resumed.restore(&snapshot).unwrap();
    assert_eq!(pending_count(&resumed), 1);

    loaded.advance(2).unwrap();
    resumed.advance(2).unwrap();
    assert_eq!(pending_count(&loaded), 1);
    assert_eq!(pending_count(&resumed), 1);
    assert!(loaded.initial.naval_units().is_empty());
    assert!(resumed.initial.naval_units().is_empty());

    loaded.advance(1).unwrap();
    resumed.advance(1).unwrap();
    assert_eq!(pending_count(&loaded), 0);
    assert_eq!(pending_count(&resumed), 0);
    for world in [&loaded, &resumed] {
        assert_eq!(world.initial.naval_units().len(), 1);
        assert_eq!(
            world.initial.naval_units()[0].unit_type,
            "sample_light_ship"
        );
        assert_eq!(
            world.initial.naval_units()[0].owner,
            world.initial.country_entity("AAA").unwrap()
        );
        assert_eq!(
            world.initial.naval_units()[0].location,
            world.initial.province_entity(1).unwrap()
        );
        assert_eq!(treasury(world), Some(20_000));
    }
}

#[test]
fn saved_fifo_naval_queue_preserves_record_order_and_survives_restore() {
    let (directory, content) = fixture_with_two_naval_types();
    let first = order("sample_light_ship", "1444.11.10", 3, 3, "1444.11.13", "");
    let second = omit_progress(order(
        "sample_heavy_ship",
        "1444.11.11",
        2,
        2,
        "1444.11.13",
        "",
    ));
    let path = write_save(
        directory.path(),
        "fifo-queue.eu4",
        "AAA",
        "AAA",
        &format!("{first} {second}"),
        "",
    );
    let mut continuous = VanillaWorld::load_from_content_with_save(&content, &path).unwrap();
    let strict_report = read_vanilla_naval_constructions(&path).unwrap();
    assert_eq!(strict_report.records.len(), 1);
    assert!(
        strict_report
            .unresolved
            .iter()
            .any(|entry| { entry.reason == "missing military_construction field progress" })
    );
    let report = read_vanilla_naval_constructions_for_queue_import(&path).unwrap();
    assert_eq!(report.records.len(), 2);
    assert_eq!(report.records[1].start_date, "1444.11.11");
    assert_eq!(report.records[1].date, "1444.11.13");
    assert_eq!(report.records[1].progress, "0");
    assert!(report.unresolved.is_empty());
    let no_queue_path = write_save(directory.path(), "fifo-baseline.eu4", "AAA", "AAA", "", "");
    let baseline = VanillaWorld::load_from_content_with_save(&content, &no_queue_path).unwrap();
    let baseline_fingerprint = fingerprint(&baseline);
    assert_eq!(fingerprint(&continuous), baseline_fingerprint);
    assert_eq!(pending_count(&continuous), 0);
    assert!(
        continuous
            .naval_construction_import_report()
            .unresolved
            .iter()
            .any(|entry| entry.reason.contains("require explicit FIFO import"))
    );
    continuous
        .import_naval_construction_queues(&report)
        .unwrap();
    assert_eq!(
        continuous.naval_construction_import_report().imported_count,
        2
    );
    assert!(
        continuous
            .naval_construction_import_report()
            .unresolved
            .is_empty()
    );
    assert_eq!(pending_count(&continuous), 2);
    assert_eq!(treasury(&continuous), Some(20_000));
    assert_eq!(fingerprint(&continuous), baseline_fingerprint);
    assert_eq!(
        continuous
            .initial
            .world()
            .naval_constructions()
            .iter()
            .map(|order| order.unit_type.as_str())
            .collect::<Vec<_>>(),
        ["sample_light_ship", "sample_heavy_ship"]
    );

    let initial_snapshot = continuous.save().unwrap();
    let mut old_snapshot_reloaded =
        VanillaWorld::load_from_content_with_save(&content, &path).unwrap();
    old_snapshot_reloaded
        .restore(&baseline.save().unwrap())
        .unwrap();
    assert_eq!(pending_count(&old_snapshot_reloaded), 0);
    let mut resumed = VanillaWorld::load_from_content_with_save(&content, &path).unwrap();
    resumed.restore(&initial_snapshot).unwrap();

    continuous.advance(3).unwrap();
    resumed.advance(3).unwrap();
    for world in [&continuous, &resumed] {
        assert_eq!(pending_count(world), 1);
        assert_eq!(world.initial.naval_units().len(), 1);
        assert_eq!(
            world.initial.naval_units()[0].unit_type,
            "sample_light_ship"
        );
        assert_eq!(treasury(world), Some(20_000));
        assert_eq!(
            world.initial.world().naval_constructions()[0].unit_type,
            "sample_heavy_ship"
        );
    }

    let middle_snapshot = resumed.save().unwrap();
    let mut resumed_after_middle =
        VanillaWorld::load_from_content_with_save(&content, &path).unwrap();
    resumed_after_middle.restore(&middle_snapshot).unwrap();
    continuous.advance(1).unwrap();
    resumed.advance(1).unwrap();
    resumed_after_middle.advance(1).unwrap();

    for world in [&continuous, &resumed, &resumed_after_middle] {
        assert_eq!(pending_count(world), 1);
        assert_eq!(world.initial.naval_units().len(), 1);
        assert_eq!(
            world.initial.naval_units()[0].unit_type,
            "sample_light_ship"
        );
        assert_eq!(
            world.initial.world().naval_constructions()[0].unit_type,
            "sample_heavy_ship"
        );
    }

    continuous.advance(1).unwrap();
    resumed.advance(1).unwrap();
    resumed_after_middle.advance(1).unwrap();

    let continuous_snapshot = continuous.save().unwrap();
    assert_eq!(resumed.save().unwrap(), continuous_snapshot);
    assert_eq!(resumed_after_middle.save().unwrap(), continuous_snapshot);
    for world in [&continuous, &resumed, &resumed_after_middle] {
        assert_eq!(pending_count(world), 0);
        assert_eq!(
            world
                .initial
                .naval_units()
                .iter()
                .map(|unit| unit.unit_type.as_str())
                .collect::<Vec<_>>(),
            ["sample_light_ship", "sample_heavy_ship"]
        );
        assert_eq!(treasury(world), Some(20_000));
    }
}

#[test]
fn queue_import_reader_only_defaults_missing_non_head_progress() {
    let (directory, content) = fixture_with_two_naval_types();
    let missing_progress = omit_progress(order(
        "sample_light_ship",
        "1444.11.11",
        3,
        3,
        "1444.11.14",
        "",
    ));

    let single_path = write_save(
        directory.path(),
        "single-missing-progress.eu4",
        "AAA",
        "AAA",
        &missing_progress,
        "",
    );
    let single = read_vanilla_naval_constructions_for_queue_import(&single_path).unwrap();
    assert_eq!(single.military_order_counts_by_province[&1], 1);
    assert!(single.records.is_empty());
    assert!(
        single
            .unresolved
            .iter()
            .any(|entry| { entry.reason == "missing military_construction field progress" })
    );

    let valid_tail = order("sample_heavy_ship", "1444.11.11", 3, 3, "1444.11.14", "");
    let missing_head_path = write_save(
        directory.path(),
        "head-missing-progress.eu4",
        "AAA",
        "AAA",
        &format!("{missing_progress} {valid_tail}"),
        "",
    );
    let missing_head =
        read_vanilla_naval_constructions_for_queue_import(&missing_head_path).unwrap();
    assert_eq!(missing_head.records.len(), 1);
    assert_eq!(missing_head.records[0].unit_type, "sample_heavy_ship");
    assert_eq!(missing_head.military_order_counts_by_province[&1], 2);
    assert!(
        missing_head
            .unresolved
            .iter()
            .any(|entry| { entry.reason == "missing military_construction field progress" })
    );

    let bad_head_path = write_save(
        directory.path(),
        "bad-head-counts-as-queue-position.eu4",
        "AAA",
        "AAA",
        &format!("military_construction = malformed {missing_progress}"),
        "",
    );
    let bad_head = read_vanilla_naval_constructions_for_queue_import(&bad_head_path).unwrap();
    assert_eq!(bad_head.military_order_counts_by_province[&1], 2);
    assert_eq!(bad_head.records.len(), 1);
    assert_eq!(bad_head.records[0].progress, "0");
    assert!(
        bad_head
            .unresolved
            .iter()
            .any(|entry| { entry.reason == "military_construction must be an object" })
    );
    let mut loaded = VanillaWorld::load_from_content_with_save(&content, &bad_head_path).unwrap();
    loaded.import_naval_construction_queues(&bad_head).unwrap();
    assert_eq!(pending_count(&loaded), 0);
    assert_eq!(loaded.naval_construction_import_report().imported_count, 0);
}

#[test]
fn queue_import_reader_keeps_invalid_tail_fields_and_planned_dates_unresolved() {
    let (directory, content) = fixture_with_two_naval_types();
    let head = order("sample_light_ship", "1444.11.10", 3, 3, "1444.11.13", "");
    let valid_tail = order("sample_heavy_ship", "1444.11.11", 3, 3, "1444.11.14", "");
    let invalid_progress = valid_tail.replace("progress = 0.333", "progress = invalid");
    let duplicate_progress = order(
        "sample_heavy_ship",
        "1444.11.11",
        3,
        3,
        "1444.11.14",
        "progress = 0.333",
    );
    let nonscalar_progress = valid_tail.replace("progress = 0.333", "progress = { }");
    let missing_cost = valid_tail.replace("cost = 5.000 ", "");
    let invalid_total = valid_tail.replace(" total = 3 ", " total = not_days ");
    let invalid_date = valid_tail.replace("date = 1444.11.14", "date = 1444.13.42");

    let malformed_tails = [
        (
            "invalid-progress",
            invalid_progress,
            "military_construction has an invalid progress",
        ),
        (
            "duplicate-progress",
            duplicate_progress,
            "duplicate military_construction field progress",
        ),
        (
            "nonscalar-progress",
            nonscalar_progress,
            "military_construction field progress must be scalar",
        ),
        (
            "missing-other-field",
            missing_cost,
            "missing military_construction field cost",
        ),
        (
            "invalid-total",
            invalid_total,
            "military_construction has an invalid total",
        ),
        (
            "invalid-date",
            invalid_date,
            "military_construction has an invalid date",
        ),
    ];
    for (name, tail, reason) in malformed_tails {
        let path = write_save(
            directory.path(),
            &format!("{name}.eu4"),
            "AAA",
            "AAA",
            &format!("{head} {tail}"),
            "",
        );
        let report = read_vanilla_naval_constructions_for_queue_import(&path).unwrap();
        assert_eq!(report.records.len(), 1, "{name}");
        assert!(
            report.unresolved.iter().any(|entry| entry.reason == reason),
            "{name}: {:?}",
            report.unresolved
        );
        let mut loaded = VanillaWorld::load_from_content_with_save(&content, &path).unwrap();
        loaded.import_naval_construction_queues(&report).unwrap();
        assert_eq!(pending_count(&loaded), 0, "{name}");
        assert_eq!(
            loaded.naval_construction_import_report().imported_count,
            0,
            "{name}"
        );
    }

    let mismatched_date_tail = omit_progress(order(
        "sample_heavy_ship",
        "1444.11.11",
        3,
        3,
        "1444.11.15",
        "",
    ));
    let path = write_save(
        directory.path(),
        "planned-date-mismatch.eu4",
        "AAA",
        "AAA",
        &format!("{head} {mismatched_date_tail}"),
        "",
    );
    let report = read_vanilla_naval_constructions_for_queue_import(&path).unwrap();
    assert_eq!(report.records.len(), 2);
    assert_eq!(report.records[1].progress, "0");
    assert!(report.unresolved.is_empty());
    let mut loaded = VanillaWorld::load_from_content_with_save(&content, &path).unwrap();
    loaded.import_naval_construction_queues(&report).unwrap();
    assert_eq!(pending_count(&loaded), 0);
    assert!(
        loaded
            .naval_construction_import_report()
            .unresolved
            .iter()
            .any(|entry| entry.reason
                == "start_date plus total does not equal the planned construction date")
    );
}

#[test]
fn queue_import_readers_support_binary_token_maps_and_resolvers() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("queue.eu4");
    fs::write(&path, binary_save_with_missing_tail_progress()).unwrap();
    let token_map = write_naval_queue_token_map(directory.path());

    let strict = read_vanilla_naval_constructions_with_token_map(&path, &token_map).unwrap();
    assert_eq!(strict.records.len(), 1);
    assert!(
        strict
            .unresolved
            .iter()
            .any(|entry| { entry.reason == "missing military_construction field progress" })
    );
    let strict_resolved =
        read_vanilla_naval_constructions_with_token_resolver(&path, NavalQueueTestTokenResolver)
            .unwrap();
    assert_eq!(strict_resolved.records.len(), 1);
    assert!(
        strict_resolved
            .unresolved
            .iter()
            .any(|entry| { entry.reason == "missing military_construction field progress" })
    );

    let mapped =
        read_vanilla_naval_constructions_for_queue_import_with_token_map(&path, &token_map)
            .unwrap();
    let resolved = read_vanilla_naval_constructions_for_queue_import_with_token_resolver(
        &path,
        NavalQueueTestTokenResolver,
    )
    .unwrap();
    assert_eq!(mapped.date, "1444.11.11");
    assert_eq!(mapped.records.len(), 2);
    assert_eq!(mapped.records[1].start_date, "1444.11.11");
    assert_eq!(mapped.records[1].progress, "0");
    assert!(mapped.unresolved.is_empty());
    assert_eq!(mapped.records, resolved.records);
    assert_eq!(mapped.unresolved, resolved.unresolved);
}

#[test]
fn zipped_text_queue_import_normalizes_missing_tail_progress() {
    let directory = tempfile::tempdir().unwrap();
    let source = write_save(
        directory.path(),
        "queue-source.eu4",
        "AAA",
        "AAA",
        &format!(
            "{} {}",
            order("sample_light_ship", "1444.11.10", 3, 3, "1444.11.13", ""),
            omit_progress(order(
                "sample_heavy_ship",
                "1444.11.11",
                3,
                3,
                "1444.11.14",
                "",
            ))
        ),
        "",
    );
    let path = directory.path().join("queue.zip.eu4");
    let mut archive = ZipWriter::new(fs::File::create(&path).unwrap());
    archive
        .start_file("gamestate", SimpleFileOptions::default())
        .unwrap();
    archive.write_all(&fs::read(source).unwrap()).unwrap();
    archive.finish().unwrap();

    let report = read_vanilla_naval_constructions_for_queue_import(&path).unwrap();
    assert_eq!(report.records.len(), 2);
    assert_eq!(report.records[1].progress, "0");
    assert!(report.unresolved.is_empty());
}

#[test]
fn unsupported_fifo_sibling_blocks_its_queue_but_not_another_province() {
    let (directory, content) = fixture_with_two_naval_types();
    let valid = order("sample_light_ship", "1444.11.10", 3, 3, "1444.11.13", "");
    let unknown = order("missing_ship", "1444.11.11", 3, 3, "1444.11.14", "");
    for (name, queue) in [
        ("unknown-suffix.eu4", format!("{valid} {unknown}")),
        ("unknown-prefix.eu4", format!("{unknown} {valid}")),
    ] {
        let path = write_save_with_two_provinces(
            directory.path(),
            name,
            &queue,
            &order("sample_heavy_ship", "1444.11.10", 3, 3, "1444.11.13", ""),
        );
        let report = read_vanilla_naval_constructions(&path).unwrap();
        let mut world = VanillaWorld::load_from_content_with_save(&content, &path).unwrap();
        assert_eq!(world.naval_construction_import_report().imported_count, 1);
        assert!(
            world
                .naval_construction_import_report()
                .unresolved
                .iter()
                .any(|entry| entry.province_id == Some(1)
                    && entry.reason.contains("require explicit FIFO import"))
        );
        world.import_naval_construction_queues(&report).unwrap();
        assert_eq!(pending_count(&world), 1);
        assert!(
            world
                .naval_construction_import_report()
                .unresolved
                .iter()
                .any(|entry| entry.province_id == Some(1)
                    && entry
                        .reason
                        .contains("unknown naval construction unit type"))
        );
        let queued = &world.initial.world().naval_constructions()[0];
        assert_eq!(queued.location, world.initial.province_entity(2).unwrap());
        assert_eq!(queued.unit_type, "sample_heavy_ship");
        assert_eq!(treasury(&world), Some(20_000));
        world.advance(3).unwrap();
        assert_eq!(pending_count(&world), 0);
        assert_eq!(world.initial.naval_units().len(), 1);
        assert_eq!(
            world.initial.naval_units()[0].unit_type,
            "sample_heavy_ship"
        );
    }
}

#[test]
fn public_report_recounts_records_and_rejects_a_mismatched_save_date() {
    let (directory, content) = fixture_with_two_naval_types();
    let path = write_save(directory.path(), "manual-report.eu4", "AAA", "AAA", "", "");
    let save = read_vanilla_save_state(&path).unwrap();
    let record = VanillaNavalConstructionRecord {
        province_id: 1,
        country: "AAA".into(),
        envoy: None,
        unit_type: "sample_light_ship".into(),
        start_date: "1444.11.10".into(),
        total: 3,
        original_total: 3,
        progress: "0.333".into(),
        date: "1444.11.13".into(),
        cost: "5.000".into(),
    };
    let second_record = VanillaNavalConstructionRecord {
        province_id: 1,
        country: "AAA".into(),
        envoy: None,
        unit_type: "sample_heavy_ship".into(),
        start_date: "1444.11.11".into(),
        total: 3,
        original_total: 3,
        progress: "0.000".into(),
        date: "1444.11.14".into(),
        cost: "5.000".into(),
    };
    let duplicate_report = VanillaNavalConstructionReport {
        date: save.date.clone(),
        records: vec![record.clone(), second_record.clone()],
        unresolved: Vec::new(),
        military_order_counts_by_province: Default::default(),
    };
    let duplicated = VanillaWorld::load_from_content_with_save_state_and_naval_constructions(
        &content,
        &save,
        &duplicate_report,
        None,
    )
    .unwrap();
    assert_eq!(
        duplicated.naval_construction_import_report().imported_count,
        0
    );
    assert_eq!(pending_count(&duplicated), 0);
    assert!(
        duplicated
            .naval_construction_import_report()
            .unresolved
            .iter()
            .any(|entry| entry.reason.contains("require explicit FIFO import"))
    );
    let mut duplicated = duplicated;
    let mut globally_unresolved = duplicate_report.clone();
    globally_unresolved
        .unresolved
        .push(augustus_eu4::VanillaNavalConstructionUnresolved {
            province_id: None,
            reason: "construction input is incomplete".into(),
        });
    let before_unresolved = duplicated.save().unwrap();
    duplicated
        .import_naval_construction_queues(&globally_unresolved)
        .unwrap();
    assert_eq!(pending_count(&duplicated), 0);
    assert_eq!(duplicated.save().unwrap(), before_unresolved);
    duplicated
        .import_naval_construction_queues(&duplicate_report)
        .unwrap();
    assert_eq!(
        duplicated.naval_construction_import_report().imported_count,
        2
    );
    assert_eq!(pending_count(&duplicated), 2);
    assert!(
        duplicated
            .naval_construction_import_report()
            .unresolved
            .is_empty()
    );

    let inconsistent_report = VanillaNavalConstructionReport {
        date: save.date.clone(),
        records: vec![record.clone(), second_record.clone()],
        unresolved: Vec::new(),
        military_order_counts_by_province: [(1, 3)].into(),
    };
    let mut inconsistent = VanillaWorld::load_from_content_with_save_state_and_naval_constructions(
        &content,
        &save,
        &inconsistent_report,
        None,
    )
    .unwrap();
    assert_eq!(
        inconsistent
            .naval_construction_import_report()
            .imported_count,
        0
    );
    assert_eq!(pending_count(&inconsistent), 0);
    assert!(
        inconsistent
            .naval_construction_import_report()
            .unresolved
            .iter()
            .any(|entry| entry.reason.contains("unparsed or inconsistent records"))
    );
    let inconsistent_snapshot = inconsistent.save().unwrap();
    inconsistent
        .import_naval_construction_queues(&inconsistent_report)
        .unwrap();
    assert_eq!(pending_count(&inconsistent), 0);
    assert_eq!(
        inconsistent
            .naval_construction_import_report()
            .imported_count,
        0
    );
    assert_eq!(inconsistent.save().unwrap(), inconsistent_snapshot);

    let mismatched_report = VanillaNavalConstructionReport {
        date: "1444.11.12".into(),
        records: vec![record],
        unresolved: Vec::new(),
        military_order_counts_by_province: [(1, 1)].into(),
    };
    let mismatched = VanillaWorld::load_from_content_with_save_state_and_naval_constructions(
        &content,
        &save,
        &mismatched_report,
        None,
    )
    .unwrap();
    assert_eq!(pending_count(&mismatched), 0);
    assert!(
        mismatched
            .naval_construction_import_report()
            .unresolved
            .iter()
            .any(|entry| entry.reason.contains("does not match the loaded save date"))
    );
}

#[test]
fn unsupported_or_ambiguous_orders_remain_unresolved_and_do_not_change_identity() {
    let (directory, content) = fixture_with_content();
    let no_order_path = write_save(directory.path(), "baseline.eu4", "AAA", "AAA", "", "");
    let no_order = VanillaWorld::load_from_content_with_save(&content, &no_order_path).unwrap();
    let baseline_fingerprint = fingerprint(&no_order);
    let valid = order("sample_light_ship", "1444.11.10", 3, 3, "1444.11.13", "");
    let unsupported = [
        (
            "unknown-type",
            "AAA",
            "AAA",
            order("missing_ship", "1444.11.10", 3, 3, "1444.11.13", ""),
            "unknown naval construction unit type",
            "",
        ),
        (
            "unsupported-envoy-value",
            "AAA",
            "AAA",
            order_with_envoy(
                "sample_light_ship",
                "1444.11.10",
                3,
                3,
                "1444.11.13",
                Some("0"),
                "",
            ),
            "envoy value other than -1 is unsupported",
            "",
        ),
        (
            "noninteger-envoy",
            "AAA",
            "AAA",
            order_with_envoy(
                "sample_light_ship",
                "1444.11.10",
                3,
                3,
                "1444.11.13",
                Some("0.5"),
                "",
            ),
            "invalid envoy",
            "",
        ),
        (
            "duplicate-envoy",
            "AAA",
            "AAA",
            order(
                "sample_light_ship",
                "1444.11.10",
                3,
                3,
                "1444.11.13",
                "envoy = -1",
            ),
            "duplicate military_construction field envoy",
            "",
        ),
        (
            "unknown-type-order-suffix",
            "AAA",
            "AAA",
            format!(
                "{valid} {}",
                order("missing_ship", "1444.11.11", 3, 3, "1444.11.14", "")
            ),
            "unknown naval construction unit type",
            "",
        ),
        (
            "unknown-type-order-prefix",
            "AAA",
            "AAA",
            format!(
                "{} {valid}",
                order("missing_ship", "1444.11.11", 3, 3, "1444.11.14", "")
            ),
            "unknown naval construction unit type",
            "",
        ),
        (
            "occupied-province",
            "BBB",
            "BBB",
            valid.clone(),
            "country, current owner, and controller must match",
            "",
        ),
        (
            "duration-mismatch",
            "AAA",
            "AAA",
            order("sample_light_ship", "1444.11.10", 2, 2, "1444.11.13", ""),
            "does not equal the planned construction date",
            "",
        ),
        (
            "invalid-date",
            "AAA",
            "AAA",
            order("sample_light_ship", "1444.11.10", 3, 3, "1444.13.42", ""),
            "invalid date",
            "",
        ),
        (
            "unsupported-field",
            "AAA",
            "AAA",
            order(
                "sample_light_ship",
                "1444.11.10",
                3,
                3,
                "1444.11.13",
                "queue_position = 1",
            ),
            "unsupported military_construction fields",
            "",
        ),
        (
            "future-start",
            "AAA",
            "AAA",
            order("sample_light_ship", "1444.11.13", 1, 1, "1444.11.14", ""),
            "start_date is after the saved world date",
            "",
        ),
        (
            "siege-paused",
            "AAA",
            "AAA",
            valid,
            "province has a siege combat",
            "combat = { siege_combat = { location = 1 active = yes } }",
        ),
    ];

    for (name, owner, controller, orders, reason, combat) in unsupported {
        let path = write_save(
            directory.path(),
            &format!("{name}.eu4"),
            owner,
            controller,
            &orders,
            combat,
        );
        let world = VanillaWorld::load_from_content_with_save(&content, &path).unwrap();
        assert_eq!(
            world.naval_construction_import_report().imported_count,
            0,
            "{name}"
        );
        assert_eq!(pending_count(&world), 0, "{name}");
        assert!(
            world
                .naval_construction_import_report()
                .unresolved
                .iter()
                .any(|entry| entry.reason.contains(reason)),
            "{name}: {:?}",
            world.naval_construction_import_report().unresolved
        );
        if name != "occupied-province" {
            assert_eq!(fingerprint(&world), baseline_fingerprint, "{name}");
        }
    }
}

#[test]
fn zipped_save_uses_metadata_date_when_gamestate_date_is_absent() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("meta-date.eu4");
    let file = fs::File::create(&path).unwrap();
    let mut archive = ZipWriter::new(file);
    archive
        .start_file("gamestate", SimpleFileOptions::default())
        .unwrap();
    archive.write_all(b"EU4txt\nprovinces = {}\n").unwrap();
    archive
        .start_file("meta", SimpleFileOptions::default())
        .unwrap();
    archive.write_all(b"EU4txt\ndate = 1444.11.11\n").unwrap();
    archive.finish().unwrap();

    let report = read_vanilla_naval_constructions(path).unwrap();
    assert_eq!(report.date, "1444.11.11");
    assert!(report.records.is_empty());
    assert!(report.unresolved.is_empty());
}
