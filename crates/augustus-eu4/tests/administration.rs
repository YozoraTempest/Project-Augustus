// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

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
fn baseline_cores_states_and_history_deltas_reach_queries() {
    let loaded = VanillaWorld::load(fixture(), "1444.11.11").unwrap();
    let north = loaded.province(1).unwrap().administration.unwrap();
    assert_eq!(north.area.as_deref(), Some("north_area"));
    assert_eq!(north.area_status, Some(augustus_core::AreaStatus::State));
    assert_eq!(north.owner_core, Some(augustus_core::CoreType::Full));
    assert_eq!(north.minimum_autonomy_micros, Some(0));
    assert_eq!(north.raw_autonomy_micros, None);
    assert_eq!(north.history_autonomy_delta_micros, 250_000);
    assert_eq!(north.effective_autonomy_micros, Some(0));
    assert!(north.is_capital && north.is_capital_area);
    assert_eq!(loaded.report.administration.direct_autonomy_operations, 2);
    assert_eq!(loaded.report.administration.capital_states, 2);
    assert_eq!(loaded.report.administration.initial_states, 2);
    assert_eq!(loaded.report.administration.full_core_types, 2);
    assert_eq!(loaded.report.administration.unspecified_core_types, 0);
    assert_eq!(
        loaded.report.administration.unknown_country_area_statuses,
        0
    );
    assert_eq!(
        loaded.report.administration.rule_sources["capital_maximum"].line,
        9
    );
    assert_eq!(loaded.report.administration.autonomy_sources[&1].len(), 2);
    assert!(
        !loaded
            .report
            .unapplied_province_fields
            .contains_key("add_local_autonomy")
    );
    assert!(loaded.province(3).unwrap().administration.is_none());
    assert_eq!(
        VanillaWorld::load(fixture(), "1445.1.1")
            .unwrap()
            .province(1)
            .unwrap()
            .administration
            .unwrap()
            .history_autonomy_delta_micros,
        350_000
    );
    let temp = copy_fixture();
    let path = temp.path().join("history/countries/AAA - Alpha.txt");
    let text = fs::read_to_string(&path).unwrap();
    fs::write(path, text + "\n1435.1.1 = { capital = 2 }\n").unwrap();
    let earlier = VanillaWorld::load(temp.path(), "1435.1.1").unwrap();
    let north = earlier.province(1).unwrap().administration.unwrap();
    assert!(!north.is_capital);
    assert_eq!(north.owner_core, Some(augustus_core::CoreType::Unspecified));
    assert_eq!(north.history_autonomy_delta_micros, 301_250);
    assert_eq!(north.area_status, None);
    assert_eq!(north.effective_autonomy_micros, None);
    let after = &earlier.province(1).unwrap().attributes["tax_after_autonomy"];
    assert_eq!(after.known_subtotal_micros, None);
    assert!(after.unresolved.contains("administration:initial_autonomy"));
}

#[test]
fn baseline_states_follow_ownership_without_using_core_claims_or_replacing_capitals() {
    let temp = copy_fixture();
    let path = temp.path().join("history/countries/AAA - Alpha.txt");
    let text = fs::read_to_string(&path).unwrap();
    fs::write(path, text + "\n1444.11.11 = { capital = 1 }\n").unwrap();
    let path = temp.path().join("history/provinces/1 - North.txt");
    let text = fs::read_to_string(&path).unwrap();
    fs::write(path, text + "\n1444.11.11 = { add_core = AAA }\n").unwrap();
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert_eq!(loaded.initial.bound_capital("AAA"), None);
    let areas = loaded.country("AAA").unwrap().administrative_areas.unwrap();
    assert_eq!(areas.len(), 1);
    assert_eq!(areas["east_area"], augustus_core::AreaStatus::State);
    assert!(!areas.contains_key("north_area"));
    let east = loaded.province(2).unwrap().administration.unwrap();
    assert!(!east.is_capital);
    assert_eq!(east.area_status, Some(augustus_core::AreaStatus::State));
    assert_eq!(east.effective_autonomy_micros, None);
    assert_eq!(loaded.report.administration.initial_states, 2);
    assert_eq!(loaded.report.administration.capital_states, 1);
    assert_eq!(loaded.report.administration.full_core_types, 3);
    let core = loaded.initial.world().administration();
    let north = &core.provinces[&loaded.initial.province_entity(1).unwrap()];
    assert_eq!(
        north.cores[&loaded.initial.country_entity("AAA").unwrap()],
        augustus_core::CoreType::Full
    );
}

#[test]
fn scoped_country_operations_are_reported_and_kept_out_of_confirmed_values() {
    let temp = copy_fixture();
    let path = temp.path().join("history/countries/AAA - Alpha.txt");
    let text = fs::read_to_string(&path).unwrap();
    fs::write(&path, text + "\n1444.1.1 = { east_area = { add_local_autonomy = 20 } }\n1445.1.1 = { if = { limit = { always = yes } every_owned_province = { add_local_autonomy = 30 } } }\n").unwrap();
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert_eq!(
        loaded.report.administration.pending_country_autonomy["AAA"].len(),
        1
    );
    assert!(
        loaded
            .province(2)
            .unwrap()
            .administration
            .unwrap()
            .unresolved
            .contains("scoped_country_history")
    );
    assert_eq!(
        loaded
            .province(2)
            .unwrap()
            .administration
            .unwrap()
            .history_autonomy_delta_micros,
        0
    );
    assert_eq!(
        VanillaWorld::load(temp.path(), "1445.1.1")
            .unwrap()
            .report
            .administration
            .pending_country_autonomy["AAA"]
            .len(),
        2
    );
}

#[test]
fn repeated_membership_is_idempotent_and_absent_map_input_remains_unknown() {
    let temp = copy_fixture();
    let path = temp.path().join("map/area.txt");
    let text = fs::read_to_string(&path)
        .unwrap()
        .replace("20 60 80 } 1", "20 60 80 } 1 1 1");
    fs::write(path, text + "\nunused_area = {}\n").unwrap();
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert_eq!(
        loaded.report.administration.repeated_province_memberships[&1],
        3
    );
    assert_eq!(loaded.report.administration.mapped_provinces, 3);
    assert!(
        loaded
            .report
            .administration
            .empty_areas
            .contains("unused_area")
    );
    let path = temp.path().join("map/default.map");
    let text = fs::read_to_string(&path)
        .unwrap()
        .replace("area = \"area.txt\"", "");
    fs::write(path, text).unwrap();
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert_eq!(
        loaded.province(1).unwrap().administration.unwrap().area,
        None
    );
    assert_eq!(loaded.report.administration.missing_land_areas, [1, 2]);
    assert_eq!(loaded.report.administration.capital_states, 0);
}

#[test]
fn invalid_area_membership_color_rules_and_literal_history_fail_with_sources() {
    for (relative, extra, message) in [
        ("map/area.txt", "other = { 1 }", "more than one area"),
        ("map/area.txt", "other = { 99 }", "unknown province"),
        (
            "map/area.txt",
            "other = { color = { 1 2 } 4 }",
            "three byte",
        ),
        ("map/area.txt", "other = { nonsense = yes 4 }", "metadata"),
        ("map/area.txt", "other = { 0 }", "positive area"),
        (
            "history/provinces/1 - North.txt",
            "add_local_autonomy = 0.00001",
            "autonomy percentage",
        ),
        (
            "history/provinces/1 - North.txt",
            "add_local_autonomy = { 20 }",
            "autonomy percentage",
        ),
        (
            "common/static_modifiers/test_static.txt",
            "colonial_core = { min_local_autonomy = yes }",
            "",
        ),
    ] {
        let temp = copy_fixture();
        let path = temp.path().join(relative);
        let text = fs::read_to_string(&path).unwrap();
        fs::write(path, text + "\n" + extra).unwrap();
        // A duplicate whole definition is reported as ambiguity, never selected.
        if message.is_empty() {
            let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
            assert_eq!(
                loaded
                    .initial
                    .world()
                    .administration()
                    .rules
                    .state_territorial_minimum_micros,
                None
            );
        } else {
            let error = VanillaWorld::load(temp.path(), "1444.11.11").unwrap_err();
            assert!(error.to_string().contains(message), "{error}");
        }
    }
    let temp = copy_fixture();
    let path = temp.path().join("common/static_modifiers/test_static.txt");
    let text = fs::read_to_string(&path)
        .unwrap()
        .replace("min_local_autonomy = 35", "min_local_autonomy = 101");
    fs::write(path, text).unwrap();
    assert!(
        VanillaWorld::load(temp.path(), "1444.11.11")
            .unwrap_err()
            .to_string()
            .contains("autonomy ratio")
    );
}
