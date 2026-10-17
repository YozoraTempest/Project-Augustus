// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::TechnologyProperty;
use augustus_eu4::VanillaWorld;
use std::{
    fs,
    path::{Path, PathBuf},
};

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
fn baseline_technology_resources_and_permanent_modifiers_reach_the_shared_state() {
    let loaded = VanillaWorld::load(fixture(), "1444.11.11").unwrap();
    let alpha = loaded.country("AAA").unwrap();
    assert_eq!(alpha.technology.unwrap()["administrative"], 3);
    assert_eq!(alpha.resources.unwrap().mercantilism_milli, Some(18500));
    assert_eq!(alpha.resources.unwrap().treasury_milli, None);
    assert_eq!(alpha.modifiers, ["test_state"]);
    assert_eq!(
        alpha.declared_modifier_totals_micros["test_tax_modifier"],
        12500
    );
    assert!(loaded.country("BBB").unwrap().technology.is_none());
    let north = loaded.province(1).unwrap();
    assert_eq!(north.modifiers, ["test_harbor", "test_river"]);
    assert_eq!(
        north.declared_modifier_totals_micros["test_trade_value"],
        8250000
    );
    assert_eq!(north.declared_modifier_totals_micros["test_growth"], -2500);
    assert!(loaded.province(2).unwrap().modifiers.is_empty());
    assert_eq!(loaded.report.setup.technology_sources["AAA"].line, 4);
    assert_eq!(
        loaded.report.setup.history_sources["province:2:test_river"].line,
        12
    );
    let future = VanillaWorld::load(fixture(), "1445.1.1").unwrap();
    assert!(future.country("AAA").unwrap().technology.is_none());
    assert_eq!(future.province(1).unwrap().modifiers, ["test_river"]);
    assert_eq!(future.report.setup.pending_timed_modifiers.len(), 1);
    assert_eq!(
        future.report.setup.pending_timed_modifiers["province:1:test_harbor"].line,
        23
    );
    let later = VanillaWorld::load(fixture(), "1446.1.1").unwrap();
    assert!(later.report.setup.pending_timed_modifiers.is_empty());
    assert_eq!(later.province(1).unwrap().modifiers.len(), 2);
    let end = VanillaWorld::load(fixture(), "1450.1.1").unwrap();
    assert!(end.country("AAA").unwrap().modifiers.is_empty());
}

#[test]
fn technology_property_query_preserves_unparsed_fields() {
    let temp = copy_fixture();
    let path = temp.path().join("common/technologies/test_adm.txt");
    let text = fs::read_to_string(&path).unwrap().replace(
        "technology = { year = 1430 test_capability = yes }",
        "technology = { year = 1430 test_capability = yes production_efficiency = 0.02 native_only = text }",
    );
    fs::write(path, text).unwrap();

    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let country = loaded.initial.country_entity("AAA").unwrap();
    let technology = loaded.initial.world().technology();
    assert_eq!(
        technology.country_property(country, "administrative", "production_efficiency"),
        TechnologyProperty::Value(20_000)
    );
    assert_eq!(
        technology.country_property(country, "administrative", "test_capability"),
        TechnologyProperty::Flag(true)
    );
    assert_eq!(
        technology.country_property(country, "administrative", "missing"),
        TechnologyProperty::Absent
    );
    assert_eq!(
        technology.country_property(country, "administrative", "native_only"),
        TechnologyProperty::Unknown
    );
}

#[test]
fn ambiguous_properties_keep_all_sources_without_selecting_a_value() {
    let temp = copy_fixture();
    let path = temp
        .path()
        .join("common/event_modifiers/test_modifiers.txt");
    let text = fs::read_to_string(&path).unwrap().replace(
        "test_trade_value = 7.125",
        "test_trade_value = 7.125 test_trade_value = 8 test_trade_value = 9",
    );
    fs::write(path, text).unwrap();
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert_eq!(
        loaded.report.setup.ambiguous_properties["event_modifier:test_river:test_trade_value"]
            .len(),
        3
    );
    assert!(
        !loaded.initial.world().modifiers().definitions["test_river"]
            .values_micros
            .contains_key("test_trade_value")
    );
    assert_eq!(
        loaded.province(1).unwrap().declared_modifier_totals_micros["test_trade_value"],
        1125000
    );
}

#[test]
fn malformed_tables_modifier_references_and_numeric_input_fail_with_sources() {
    for (file, from, to, message) in [
        (
            "common/technology.txt",
            "start_level = 3",
            "start_level = 9",
            "exceeds a table",
        ),
        (
            "common/technology.txt",
            "groups = { test_group = { start_level = 3 } }",
            "groups = { 1 2 }",
            "expected setup block",
        ),
        (
            "common/technologies/test_adm.txt",
            "monarch_power = ADM",
            "monarch_power = MIL",
            "power or levels do not match",
        ),
        (
            "common/technologies/test_adm.txt",
            "year = 1300",
            "removed_year = 1300",
            "missing technology year",
        ),
        (
            "common/technologies/test_adm.txt",
            "0.0125",
            "0.0000001",
            "supported precision",
        ),
        (
            "common/event_modifiers/test_modifiers.txt",
            "test_empty = { }",
            "test_empty = { 1 }",
            "expected setup block",
        ),
        (
            "common/event_modifiers/test_modifiers.txt",
            "test_empty =",
            "test_river =",
            "duplicate event modifier definition",
        ),
        (
            "history/countries/AAA - Alpha.txt",
            "technology_group = test_group",
            "technology_group = missing",
            "unknown technology group",
        ),
        (
            "history/countries/AAA - Alpha.txt",
            "mercantilism = 18.5",
            "mercantilism = 101",
            "between 0 and 100",
        ),
        (
            "history/provinces/1 - North.txt",
            "name = test_river",
            "name = missing",
            "unknown event modifier missing",
        ),
        (
            "history/provinces/1 - North.txt",
            "duration = -1",
            "duration = -2",
            "invalid modifier duration",
        ),
    ] {
        let temp = copy_fixture();
        let path = temp.path().join(file);
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains(from));
        fs::write(path, text.replace(from, to)).unwrap();
        let error = VanillaWorld::load(temp.path(), "1444.11.11").unwrap_err();
        assert!(error.to_string().contains(message), "{error}");
        if !matches!(
            message,
            "unknown technology group" | "power or levels do not match"
        ) {
            assert!(error.location().is_some(), "{error}");
        }
    }
}
