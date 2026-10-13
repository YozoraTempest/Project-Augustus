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
fn write(root: &Path, path: &str, text: &str) {
    fs::write(root.join(path), text).unwrap();
}
fn append(root: &Path, path: &str, text: &str) {
    write(
        root,
        path,
        &(fs::read_to_string(root.join(path)).unwrap() + "\n" + text),
    );
}

#[test]
fn selected_reforms_and_live_ownership_feed_the_shared_attribute_calculator() {
    let temp = copy_fixture();
    append(
        temp.path(),
        "common/government_reforms/test_governments.txt",
        "levy = { modifiers = { global_tax_modifier = 0.125 global_manpower_modifier = 0.25 } } unselected = { modifiers = { global_tax_modifier = 0.9 } }",
    );
    append(
        temp.path(),
        "history/countries/AAA - Alpha.txt",
        "1440.1.1 = { add_government_reform = levy } 1450.1.1 = { remove_government_reform = levy }",
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let country = loaded.country("AAA").unwrap();
    assert!(
        country
            .government
            .unwrap()
            .reforms
            .as_ref()
            .unwrap()
            .contains("test_court")
    );
    assert_eq!(
        country.government.unwrap().metrics.as_ref().unwrap()["legitimacy"],
        Some(75_125)
    );
    assert_eq!(
        country.declared_government_effects.known_totals_micros["test_authority"],
        188_437
    );
    assert_eq!(
        country.province_attribute_totals["tax_basis"].known_subtotal_micros,
        5_600_000
    );
    let mut world = loaded.initial.world().clone();
    let province = loaded.initial.province_entity(2).unwrap();
    let alpha = loaded.initial.country_entity("AAA").unwrap();
    let beta = loaded.initial.country_entity("BBB").unwrap();
    world.set_owner(province, beta).unwrap();
    let value = world
        .attributes()
        .evaluate(&world, province, "tax_basis")
        .unwrap();
    assert!(
        !value.steps[0]
            .contributions
            .iter()
            .any(|c| c.source == "government:reform:levy")
    );
    world.set_owner(province, alpha).unwrap();
    let mut governments = world.governments().clone();
    governments
        .countries
        .get_mut(&alpha)
        .unwrap()
        .reforms
        .as_mut()
        .unwrap()
        .remove("levy");
    world.set_governments(governments).unwrap();
    assert_eq!(
        world
            .attributes()
            .evaluate(&world, province, "tax_basis")
            .unwrap()
            .known_subtotal_micros,
        Some(5_100_000)
    );
    let later = VanillaWorld::load(temp.path(), "1450.1.1").unwrap();
    assert!(
        !later
            .country("AAA")
            .unwrap()
            .government
            .unwrap()
            .reforms
            .as_ref()
            .unwrap()
            .contains("levy")
    );
}

#[test]
fn metric_history_keeps_precision_date_order_and_known_activation_separate() {
    let temp = copy_fixture();
    append(
        temp.path(),
        "history/countries/AAA - Alpha.txt",
        "1450.1.1 = { legitimacy = 90 } 1443.1.1 = { legitimacy = 51.125 } 1443.1.1 = { legitimacy = 37.5 }",
    );
    write(
        temp.path(),
        "history/countries/BBB - Beta.txt",
        "government = monarchy capital = 1 primary_culture = beta religion = test_faith",
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert_eq!(
        loaded
            .country("AAA")
            .unwrap()
            .government
            .unwrap()
            .metrics
            .as_ref()
            .unwrap()["legitimacy"],
        Some(37_500)
    );
    assert_eq!(
        loaded
            .country("BBB")
            .unwrap()
            .government
            .unwrap()
            .metrics
            .as_ref()
            .unwrap()["legitimacy"],
        Some(100_000)
    );
    assert_eq!(
        loaded.report.governments.default_legitimacy_countries,
        ["BBB"]
    );
    let later = VanillaWorld::load(temp.path(), "1450.1.1").unwrap();
    assert_eq!(
        later
            .country("AAA")
            .unwrap()
            .government
            .unwrap()
            .metrics
            .as_ref()
            .unwrap()["legitimacy"],
        Some(90_000)
    );
    assert_eq!(
        later
            .country("BBB")
            .unwrap()
            .government
            .unwrap()
            .metrics
            .as_ref()
            .unwrap()["legitimacy"],
        None
    );
    assert!(
        later
            .report
            .governments
            .default_legitimacy_countries
            .is_empty()
    );
}

#[test]
fn special_mechanics_and_unknown_flags_do_not_enable_monarchy_metrics() {
    for flags in [
        "nomad = yes",
        "has_devotion = yes",
        "conditional = { allow = { has_dlc = \"Mandate of Heaven\" } has_meritocracy = yes }",
    ] {
        let temp = copy_fixture();
        append(
            temp.path(),
            "common/government_reforms/test_governments.txt",
            &format!("special = {{ {flags} }}"),
        );
        append(
            temp.path(),
            "history/countries/AAA - Alpha.txt",
            "add_government_reform = special",
        );
        let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
        let metrics = loaded
            .country("AAA")
            .unwrap()
            .government
            .unwrap()
            .metrics
            .as_ref()
            .unwrap();
        assert!(!metrics.contains_key("legitimacy"), "{flags}");
        if flags == "has_devotion = yes" {
            assert_eq!(metrics.get("devotion"), Some(&None));
        } else {
            assert!(metrics.is_empty(), "{flags}");
        }
    }
    let temp = copy_fixture();
    append(
        temp.path(),
        "common/government_reforms/test_governments.txt",
        "special = { conditional = { allow = { tag = AAA } has_meritocracy = yes } }",
    );
    append(
        temp.path(),
        "history/countries/AAA - Alpha.txt",
        "add_government_reform = special",
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(
        loaded
            .country("AAA")
            .unwrap()
            .government
            .unwrap()
            .metrics
            .is_none()
    );
    assert_eq!(
        loaded
            .country("BBB")
            .unwrap()
            .government
            .unwrap()
            .metrics
            .as_ref()
            .unwrap()
            .get("republican_tradition"),
        Some(&None)
    );
}

#[test]
fn unsupported_conditions_and_duplicate_effects_remain_property_specific_unknowns() {
    let temp = copy_fixture();
    append(
        temp.path(),
        "common/government_reforms/test_governments.txt",
        "levy = { modifiers = { global_tax_modifier = 0.125 } conditional = { allow = { tag = AAA } modifiers = { global_tax_modifier = 0.9 } } }",
    );
    append(
        temp.path(),
        "history/countries/AAA - Alpha.txt",
        "add_government_reform = levy if = { limit = { has_dlc = \"Emperor\" } legitimacy = 60.125 } if = { limit = { always = no } remove_government_reform = levy }",
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let country = loaded.country("AAA").unwrap();
    assert_eq!(
        country.government.unwrap().metrics.as_ref().unwrap()["legitimacy"],
        Some(60_125)
    );
    assert_eq!(
        country.province_attribute_totals["tax_basis"].known_subtotal_micros,
        5_600_000
    );
    assert!(
        country.province_attribute_totals["tax_basis"]
            .unresolved
            .contains("government:reform:levy:unresolved:global_tax_modifier")
    );
    assert!(
        !country.province_attribute_totals["manpower_contribution"]
            .unresolved
            .iter()
            .any(|r| r.starts_with("government:reform:levy"))
    );
    append(
        temp.path(),
        "history/countries/AAA - Alpha.txt",
        "if = { limit = { tag = AAA } legitimacy = 12 remove_government_reform = levy }",
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(
        loaded
            .country("AAA")
            .unwrap()
            .government
            .unwrap()
            .reforms
            .is_none()
    );
    assert!(
        loaded
            .country("AAA")
            .unwrap()
            .government
            .unwrap()
            .metrics
            .is_none()
    );
    assert!(!loaded.report.governments.pending_history["AAA"].is_empty());
}

#[test]
fn ambiguous_reform_definitions_and_properties_are_reported_without_selection_guesses() {
    let temp = copy_fixture();
    append(
        temp.path(),
        "common/government_reforms/test_governments.txt",
        "levy = { modifiers = { global_tax_modifier = 0.1 global_tax_modifier = 0.3 global_manpower_modifier = 0.2 } }",
    );
    append(
        temp.path(),
        "history/countries/AAA - Alpha.txt",
        "add_government_reform = levy",
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert_eq!(
        loaded
            .country("AAA")
            .unwrap()
            .declared_government_effects
            .known_totals_micros["global_manpower_modifier"],
        200_000
    );
    assert_eq!(
        loaded.report.governments.ambiguous_properties["reform:levy:global_tax_modifier"].len(),
        2
    );
    assert!(
        loaded
            .country("AAA")
            .unwrap()
            .declared_government_effects
            .unresolved
            .contains("government:reform:levy:unresolved:global_tax_modifier")
    );
    append(
        temp.path(),
        "common/government_reforms/test_governments.txt",
        "levy = { modifiers = { global_tax_modifier = 0.7 } }",
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert_eq!(
        loaded.report.governments.ambiguous_definitions["reform:levy"].len(),
        2
    );
    assert!(
        loaded
            .country("AAA")
            .unwrap()
            .declared_government_effects
            .unresolved
            .contains("government:reform:levy:missing_definition")
    );
}

#[test]
fn invalid_metric_values_and_effect_shapes_report_their_input_source() {
    for value in ["-1", "100.001", "75.0001", "wrong"] {
        let temp = copy_fixture();
        append(
            temp.path(),
            "history/countries/AAA - Alpha.txt",
            &format!("legitimacy = {value}"),
        );
        let error = VanillaWorld::load(temp.path(), "1444.11.11")
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("AAA - Alpha.txt") && error.contains("legitimacy"),
            "{error}"
        );
    }
    let temp = copy_fixture();
    append(
        temp.path(),
        "common/government_reforms/test_governments.txt",
        "malformed = { modifiers = 1 }",
    );
    assert!(
        VanillaWorld::load(temp.path(), "1444.11.11")
            .unwrap_err()
            .to_string()
            .contains("test_governments.txt")
    );
}

#[test]
fn later_reforms_replace_the_previous_selection_at_the_same_level() {
    let temp = copy_fixture();
    write(
        temp.path(),
        "common/governments/test_governments.txt",
        "monarchy = { basic_reform = test_court reform_levels = { first = { reforms = { test_charter replacement } } second = { reforms = { other } } } } republic = { basic_reform = test_council }",
    );
    append(
        temp.path(),
        "common/government_reforms/test_governments.txt",
        "replacement = { modifiers = { global_tax_modifier = 0.125 } } other = { modifiers = { global_manpower_modifier = 0.25 } }",
    );
    append(
        temp.path(),
        "history/countries/AAA - Alpha.txt",
        "1440.1.1 = { add_government_reform = replacement add_government_reform = other } 1450.1.1 = { remove_government_reform = replacement }",
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let reform_levels = &loaded.initial.world().governments().reform_levels["monarchy"];
    assert_eq!(reform_levels["test_charter"].as_deref(), Some("first"));
    assert_eq!(reform_levels["replacement"].as_deref(), Some("first"));
    assert_eq!(reform_levels["other"].as_deref(), Some("second"));
    let country = loaded.country("AAA").unwrap();
    assert_eq!(
        country
            .government
            .unwrap()
            .reforms
            .as_ref()
            .unwrap()
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["other", "replacement", "test_court"]
    );
    assert!(
        !country
            .declared_government_effects
            .known_totals_micros
            .contains_key("test_charter_effect")
    );
    let later = VanillaWorld::load(temp.path(), "1450.1.1").unwrap();
    assert_eq!(
        later
            .country("AAA")
            .unwrap()
            .government
            .unwrap()
            .reforms
            .as_ref()
            .unwrap()
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["other", "test_court"]
    );
}

#[test]
fn unsupported_numeric_properties_and_ambiguous_level_membership_remain_unknown() {
    let temp = copy_fixture();
    append(
        temp.path(),
        "common/government_reforms/test_governments.txt",
        "levy = { modifiers = { global_tax_modifier = variable_value global_manpower_modifier = 0.25 } }",
    );
    append(
        temp.path(),
        "history/countries/AAA - Alpha.txt",
        "add_government_reform = levy",
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(
        loaded
            .country("AAA")
            .unwrap()
            .declared_government_effects
            .unresolved
            .contains("government:reform:levy:unresolved:global_tax_modifier")
    );
    assert_eq!(
        loaded
            .country("AAA")
            .unwrap()
            .declared_government_effects
            .known_totals_micros["global_manpower_modifier"],
        250_000
    );
    write(
        temp.path(),
        "common/governments/test_governments.txt",
        "monarchy = { basic_reform = test_court reform_levels = { first = { reforms = { levy } } second = { reforms = { levy } } } } republic = { basic_reform = test_council }",
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(
        loaded
            .country("AAA")
            .unwrap()
            .government
            .unwrap()
            .reforms
            .is_none()
    );
    let temp = copy_fixture();
    write(
        temp.path(),
        "common/static_modifiers/test_governments.txt",
        "legitimacy = { test_authority = variable_value }",
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(
        loaded
            .country("AAA")
            .unwrap()
            .declared_government_effects
            .unresolved
            .contains("government:metric:legitimacy:unresolved:test_authority")
    );
}

#[test]
fn initialization_uses_the_current_tier_selection_for_fixed_government_ranks() {
    let temp = copy_fixture();
    write(
        temp.path(),
        "common/governments/test_governments.txt",
        "monarchy = { basic_reform = test_court reform_levels = { first = { reforms = { older newer } } } } republic = { basic_reform = test_council }",
    );
    append(
        temp.path(),
        "common/government_reforms/test_governments.txt",
        "older = { fixed_rank = 3 } newer = { fixed_rank = 1 }",
    );
    write(
        temp.path(),
        "history/countries/BBB - Beta.txt",
        "government = monarchy capital = 1 primary_culture = beta religion = test_faith add_government_reform = older 1440.1.1 = { add_government_reform = newer }",
    );
    // Retain both countries' base government identities; use no subject or emperor override.
    write(temp.path(), "history/diplomacy/test.txt", "");
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert_eq!(loaded.report.administration.initial_country_ranks["BBB"], 1);
}
