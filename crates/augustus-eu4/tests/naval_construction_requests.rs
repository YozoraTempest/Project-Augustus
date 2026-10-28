// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::{
    fs,
    path::{Path, PathBuf},
};

use augustus_eu4::{
    ContentSet, TextEncoding, VanillaNavalConstructionReport, VanillaNavalConstructionRequest,
    VanillaWorld, read_vanilla_save_state,
};
use tempfile::TempDir;
use walkdir::WalkDir;

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
    map.push_str("\nforce_coastal = { 1 2 }\n");
    fs::write(map_path, map).unwrap();
    let content =
        ContentSet::from_installation(temp.path(), &[], None, None, TextEncoding::Utf8).unwrap();
    (temp, content)
}

fn write_save(directory: &Path, name: &str, province_one_orders: &str) -> PathBuf {
    let path = directory.join(name);
    let text = format!(
        "EU4txt\ndate = 1444.11.11\nplayer = AAA\ncountries = {{ AAA = {{ treasury = 20.000 }} BBB = {{ }} }}\nprovinces = {{ -1 = {{ owner = AAA controller = AAA {province_one_orders} }} -2 = {{ owner = AAA controller = AAA }} }}\n"
    );
    fs::write(&path, text).unwrap();
    path
}

fn saved_order(unit_type: &str) -> String {
    format!(
        "military_construction = {{ country = AAA envoy = -1 type = {unit_type} start_date = 1444.11.10 total = 3 original_total = 3 progress = 0.333 date = 1444.11.13 cost = 5.000 }}"
    )
}

fn request(province_id: u32, cost_milli: i64) -> VanillaNavalConstructionRequest {
    VanillaNavalConstructionRequest {
        country: "AAA".into(),
        province_id,
        unit_type: "sample_light_ship".into(),
        completion_day: 3,
        cost_milli,
    }
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

fn pending_count(world: &VanillaWorld) -> usize {
    world.initial.world().naval_constructions().len()
}

#[test]
fn explicit_paid_order_is_charged_once_and_survives_snapshot_restore() {
    let (directory, content) = fixture_with_content();
    let save_path = write_save(directory.path(), "baseline.eu4", "");
    let mut world = VanillaWorld::load_from_content_with_save(&content, &save_path).unwrap();

    let ids = world
        .submit_naval_constructions(&[request(1, 5_000)])
        .unwrap();
    assert_eq!(ids.len(), 1);
    assert_eq!(treasury(&world), Some(15_000));
    assert_eq!(pending_count(&world), 1);
    assert!(world.initial.naval_units().is_empty());

    let snapshot = world.save().unwrap();
    let mut resumed = VanillaWorld::load_from_content_with_save(&content, &save_path).unwrap();
    resumed.restore(&snapshot).unwrap();
    assert_eq!(treasury(&resumed), Some(15_000));
    assert_eq!(pending_count(&resumed), 1);

    world.advance(2).unwrap();
    resumed.advance(2).unwrap();
    for current in [&world, &resumed] {
        assert_eq!(pending_count(current), 1);
        assert!(current.initial.naval_units().is_empty());
        assert_eq!(treasury(current), Some(15_000));
    }

    world.advance(1).unwrap();
    resumed.advance(1).unwrap();
    for current in [&world, &resumed] {
        assert_eq!(pending_count(current), 0);
        assert_eq!(current.initial.naval_units().len(), 1);
        assert_eq!(treasury(current), Some(15_000));
    }
}

#[test]
fn rejected_batches_leave_treasury_and_orders_unchanged() {
    let (directory, content) = fixture_with_content();
    let save_path = write_save(directory.path(), "baseline.eu4", "");
    let mut world = VanillaWorld::load_from_content_with_save(&content, &save_path).unwrap();

    let baseline = world.save().unwrap();
    assert!(
        world
            .submit_naval_constructions(&[request(1, 5_000), request(999, 1_000)])
            .is_err()
    );
    assert_eq!(world.save().unwrap(), baseline);
    assert_eq!(treasury(&world), Some(20_000));
    assert_eq!(pending_count(&world), 0);

    assert!(
        world
            .submit_naval_constructions(&[request(1, 5_000), request(1, 5_000)])
            .is_err()
    );
    assert_eq!(world.save().unwrap(), baseline);

    assert!(
        world
            .submit_naval_constructions(&[request(1, 5_000), request(2, -1)])
            .is_err()
    );
    assert_eq!(world.save().unwrap(), baseline);

    world
        .submit_naval_constructions(&[request(1, 5_000)])
        .unwrap();
    let submitted = world.save().unwrap();
    assert!(
        world
            .submit_naval_constructions(&[request(1, 5_000)])
            .is_err()
    );
    assert_eq!(world.save().unwrap(), submitted);
    assert_eq!(treasury(&world), Some(15_000));
    assert_eq!(pending_count(&world), 1);
}

#[test]
fn unresolved_saved_military_orders_block_new_orders_in_the_same_province() {
    let (directory, content) = fixture_with_content();
    let cases = [
        ("unknown", saved_order("missing_ship")),
        ("land", saved_order("test_infantry")),
        (
            "multiple",
            format!(
                "{} {}",
                saved_order("sample_light_ship"),
                saved_order("sample_light_ship")
            ),
        ),
    ];

    for (name, orders) in cases {
        let save_path = write_save(directory.path(), &format!("{name}.eu4"), &orders);
        let mut world = VanillaWorld::load_from_content_with_save(&content, &save_path).unwrap();
        assert_eq!(pending_count(&world), 0);
        assert!(
            world
                .naval_construction_import_report()
                .unresolved
                .iter()
                .any(|entry| entry.province_id == Some(1))
        );

        let baseline = world.save().unwrap();
        assert!(
            world
                .submit_naval_constructions(&[request(1, 5_000)])
                .is_err()
        );
        assert_eq!(world.save().unwrap(), baseline);
        assert_eq!(treasury(&world), Some(20_000));
        assert_eq!(pending_count(&world), 0);
    }
}

#[test]
fn global_unresolved_import_state_blocks_new_orders() {
    let (directory, content) = fixture_with_content();
    let save_path = write_save(directory.path(), "baseline.eu4", "");
    let save = read_vanilla_save_state(&save_path).unwrap();
    let constructions = VanillaNavalConstructionReport {
        date: "1444.11.12".into(),
        ..VanillaNavalConstructionReport::default()
    };
    let mut world = VanillaWorld::load_from_content_with_save_state_and_naval_constructions(
        &content,
        &save,
        &constructions,
        None,
    )
    .unwrap();
    assert!(
        world
            .naval_construction_import_report()
            .unresolved
            .iter()
            .any(|entry| entry.province_id.is_none())
    );

    let baseline = world.save().unwrap();
    assert!(
        world
            .submit_naval_constructions(&[request(1, 5_000)])
            .is_err()
    );
    assert_eq!(world.save().unwrap(), baseline);
    assert_eq!(treasury(&world), Some(20_000));
    assert_eq!(pending_count(&world), 0);
}

#[test]
fn request_rejects_unknown_fields_during_deserialization() {
    let json = r#"{"country":"AAA","province_id":1,"unit_type":"sample_light_ship","completion_day":3,"cost_milli":5000,"price_quote":5000}"#;
    assert!(serde_json::from_str::<VanillaNavalConstructionRequest>(json).is_err());
}
