// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::{
    fs,
    path::{Path, PathBuf},
};

use augustus_core::{EntityKind, ProvinceClass, Schema, World};
use augustus_eu4::VanillaWorld;

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world")
}

fn copy_fixture() -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    for entry in walkdir::WalkDir::new(fixture()) {
        let entry = entry.unwrap();
        let target = temp
            .path()
            .join(entry.path().strip_prefix(fixture()).unwrap());
        if entry.file_type().is_dir() {
            fs::create_dir_all(target).unwrap();
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
    temp
}

#[test]
fn dated_history_reaches_shared_native_state_and_preserves_provenance() {
    let loaded = VanillaWorld::load(fixture(), "1444.11.11").unwrap();
    let province = loaded.initial.province(1).unwrap();
    assert_eq!(province.owner.as_deref(), Some("BBB"));
    assert_eq!(province.controller.as_deref(), Some("BBB"));
    assert_eq!(province.cores, ["BBB"]);
    assert_eq!(province.tax_milli, Some(2125));
    assert_eq!(province.manpower_milli, Some(3500));
    assert_eq!(loaded.initial.bound_capital("AAA"), Some(2));
    assert_eq!(loaded.initial.bound_capital("BBB"), Some(1));
    assert_eq!(loaded.report.ruleset, "eu4-final-all-dlc");
    assert_eq!(
        loaded.report.unapplied_country_fields["set_country_flag"],
        1
    );
    assert_eq!(loaded.report.unapplied_province_fields["discovered_by"], 1);
    let origin = &loaded.report.unapplied_examples["province:discovered_by"];
    assert_eq!(origin.line, 16);
    assert!(origin.path.ends_with("history/provinces/1 - North.txt"));
    assert_eq!(loaded.province_sources[&1][0].line, 2);
    assert_eq!(loaded.province_sources[&1].len(), 2);

    // The same independent native input produces the same simulation entities,
    // relations and fixed-point values, without retaining an EU4 runtime.
    let schema = Schema::new([
        "province.tax_base",
        "province.production_base",
        "province.manpower_base",
    ])
    .unwrap();
    let mut native = World::new(schema);
    let alpha = native.add_entity("AAA", EntityKind::Country);
    let beta = native.add_entity("BBB", EntityKind::Country);
    let north = native.add_entity("1", EntityKind::Province);
    let east = native.add_entity("2", EntityKind::Province);
    for id in 3..=6 {
        native.add_entity(id.to_string(), EntityKind::Province);
    }
    native.set_owner(north, beta).unwrap();
    native.set_owner(east, alpha).unwrap();
    native.set_capital(alpha, east).unwrap();
    native.set_capital(beta, north).unwrap();
    for (entity, values) in [(north, [2125, 2000, 3500]), (east, [4000, 5000, 6000])] {
        for (name, value) in [
            "province.tax_base",
            "province.production_base",
            "province.manpower_base",
        ]
        .into_iter()
        .zip(values)
        {
            native
                .set(entity, native.schema().field(name).unwrap(), value)
                .unwrap();
        }
    }
    let politics = serde_json::from_str(r#"{
        "countries": {
            "0": {"ruler":{"name":"Iris","dynasty":"Sun","birth_date":{"year":1410,"month":1,"day":1},"administration":2,"diplomacy":3,"military":4},
                "heir":{"name":"Rowan","regnal_name":"Rowan I","dynasty":"Sun","birth_date":{"year":1440,"month":1,"day":1},"historical_death_date":{"year":1444,"month":11,"day":12},"administration":1,"diplomacy":2,"military":3,"claim_milli":87125,"block_disinherit":true},
                "consort":{"name":"Vale","country_of_origin":1,"female":true,"birth_date":{"year":1415,"month":1,"day":1},"historical_death_date":{"year":1450,"month":1,"day":1}}},
            "1": {"ruler":{"name":"Council","regent":true,"administration":0,"diplomacy":0,"military":0}}
        },
        "relations": [
            {"kind":"alliance","first":0,"second":1},
            {"kind":"guarantee","first":1,"second":0},
            {"kind":"warning","first":0,"second":1},
            {"kind":"trade_league","first":0,"second":1},
            {"kind":"subject","first":0,"second":1,"subject_type":"test_vassal"}
        ],
        "offices":{"imperial_emperor":0,"celestial_emperor":1}
    }"#).unwrap();
    native.set_politics(politics).unwrap();
    assert_eq!(loaded.initial.world(), &native);
    let earlier = VanillaWorld::load(fixture(), "1435.1.1").unwrap();
    assert_eq!(
        earlier.initial.province(1).unwrap().owner.as_deref(),
        Some("AAA")
    );
    assert_eq!(earlier.initial.bound_capital("BBB"), None);
    assert_eq!(earlier.report.unbound_active_capitals, Vec::<String>::new());
}

#[test]
fn map_categories_and_absent_history_are_explicit() {
    let loaded = VanillaWorld::load(fixture(), "1444.11.11").unwrap();
    assert_eq!(
        loaded.initial.province(3).unwrap().class,
        ProvinceClass::Sea
    );
    assert_eq!(
        loaded.initial.province(4).unwrap().class,
        ProvinceClass::Wasteland
    );
    assert_eq!(
        loaded.initial.province(5).unwrap().class,
        ProvinceClass::Unused
    );
    assert_eq!(loaded.report.unmapped_definition_ids, [5, 6]);
    assert_eq!(loaded.report.missing_province_history, [3, 5, 6]);
    assert_eq!(loaded.initial.province(3).unwrap().tax_milli, None);
    assert_eq!(loaded.palette().len(), 4);
    assert_eq!(loaded.report.unknown_map_colors, Vec::<[u8; 3]>::new());
    let temp = copy_fixture();
    let path = temp.path().join("map/definition.csv");
    let text = fs::read_to_string(&path)
        .unwrap()
        .replace("1;240;10;10", "1;241;10;10");
    fs::write(path, text).unwrap();
    let changed = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert_eq!(changed.report.unknown_map_colors, [[240, 10, 10]]);
}

#[test]
fn ambiguous_country_history_is_reported_without_selecting_a_winner() {
    let temp = copy_fixture();
    fs::write(
        temp.path().join("history/countries/AAA - Other.txt"),
        "capital = 1\n",
    )
    .unwrap();
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert_eq!(loaded.report.ambiguous_country_histories["AAA"].len(), 2);
    assert_eq!(loaded.report.country_history_files, 1);
    assert_eq!(
        loaded.initial.country("AAA").unwrap().declared_capital,
        None
    );
    assert_eq!(loaded.initial.country("AAA").unwrap().religion, None);
    assert_eq!(loaded.report.unbound_active_capitals, ["AAA"]);
}

#[test]
fn invalid_relationships_and_used_colors_fail_world_construction() {
    for (from, to, expected) in [
        ("owner = BBB", "owner = ZZZ", "unknown country ZZZ"),
        (
            "base_tax = 2.125",
            "base_tax = -1",
            "nonnegative base value",
        ),
        ("1440.1.1", "1440.13.1", "invalid history date"),
    ] {
        let temp = copy_fixture();
        let path = temp.path().join("history/provinces/1 - North.txt");
        let text = fs::read_to_string(&path).unwrap().replace(from, to);
        fs::write(&path, text).unwrap();
        let error = VanillaWorld::load(temp.path(), "1444.11.11")
            .unwrap_err()
            .to_string();
        assert!(error.contains(expected), "{error}");
    }
    let temp = copy_fixture();
    let path = temp.path().join("map/definition.csv");
    fs::write(
        &path,
        fs::read_to_string(&path)
            .unwrap()
            .replace("2;10;240;10", "2;240;10;10"),
    )
    .unwrap();
    assert!(
        VanillaWorld::load(temp.path(), "1444.11.11")
            .unwrap_err()
            .to_string()
            .contains("duplicate map color")
    );
}

#[test]
fn wrong_version_dimensions_and_dates_fail_before_publishing_a_world() {
    assert!(VanillaWorld::load(fixture(), "1444.2.30").is_err());
    let temp = copy_fixture();
    fs::write(
        temp.path().join("launcher-settings.json"),
        r#"{"rawVersion":"v1.36.0.0"}"#,
    )
    .unwrap();
    assert!(
        VanillaWorld::load(temp.path(), "1444.11.11")
            .unwrap_err()
            .to_string()
            .contains("1.37.5")
    );
    fs::copy(
        fixture().join("launcher-settings.json"),
        temp.path().join("launcher-settings.json"),
    )
    .unwrap();
    let path = temp.path().join("map/default.map");
    fs::write(
        &path,
        fs::read_to_string(&path)
            .unwrap()
            .replace("width = 4", "width = 5"),
    )
    .unwrap();
    assert!(
        VanillaWorld::load(temp.path(), "1444.11.11")
            .unwrap_err()
            .to_string()
            .contains("dimensions")
    );
}
