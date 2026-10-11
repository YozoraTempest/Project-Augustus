// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    EstateDefinition, EstateEffect, EstatePrivilege, EstateRules, EstateState, Estates,
    ModifierDefinition,
};
use augustus_eu4::VanillaWorld;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world")
}
fn write(root: &Path, path: &str, text: &str) {
    fs::create_dir_all(root.join(path).parent().unwrap()).unwrap();
    fs::write(root.join(path), text).unwrap();
}
fn append(root: &Path, path: &str, text: &str) {
    write(
        root,
        path,
        &(fs::read_to_string(root.join(path)).unwrap() + "\n" + text),
    );
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
    let path = "common/defines.lua";
    let defines = fs::read_to_string(temp.path().join(path)).unwrap();
    write(
        temp.path(),
        path,
        &defines.replace(
            "NCountry = {",
            "NCountry = {
        ESTATE_ANGRY_THRESHOLD = 30,
        ESTATE_HAPPY_THRESHOLD = 60,",
        ),
    );
    write(
        temp.path(),
        "common/estates/test.txt",
        "guild = { trigger = { tag = AAA } country_modifier_neutral = { global_tax_modifier = 0.2 } privileges = { charter } } inactive = { trigger = { always = no } }",
    );
    write(
        temp.path(),
        "common/estate_privileges/test.txt",
        "charter = { benefits = { global_tax_modifier = 0.125 } penalties = { global_tax_modifier = -0.025 } is_valid = { always = yes } on_granted = {} }",
    );
    append(
        temp.path(),
        "history/countries/AAA - Alpha.txt",
        "set_estate_privilege = charter",
    );
    temp
}

#[test]
fn translated_content_matches_native_core_and_keeps_startup_metrics_unknown() {
    let temp = copy_fixture();
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let mut native = VanillaWorld::load(fixture(), "1444.11.11")
        .unwrap()
        .initial
        .world()
        .clone();
    let alpha = loaded.initial.country_entity("AAA").unwrap();
    let beta = loaded.initial.country_entity("BBB").unwrap();
    let numeric = |value| EstateEffect {
        modifier: ModifierDefinition {
            values_micros: BTreeMap::from([("global_tax_modifier".into(), value)]),
            ..Default::default()
        },
        ..Default::default()
    };
    native
        .set_estates(Estates {
            definitions: BTreeMap::from([
                (
                    "guild".into(),
                    EstateDefinition {
                        neutral: numeric(200_000),
                        privileges: ["charter".into()].into(),
                        ..Default::default()
                    },
                ),
                ("inactive".into(), EstateDefinition::default()),
            ]),
            privileges: BTreeMap::from([(
                "charter".into(),
                EstatePrivilege {
                    effect: numeric(100_000),
                    ..Default::default()
                },
            )]),
            rules: EstateRules {
                angry_below_milli: Some(30_000),
                happy_from_milli: Some(60_000),
                influence_bands: None,
            },
            countries: BTreeMap::from([
                (
                    alpha,
                    BTreeMap::from([
                        (
                            "guild".into(),
                            EstateState {
                                active: Some(true),
                                privileges: Some(["charter".into()].into()),
                                ..Default::default()
                            },
                        ),
                        (
                            "inactive".into(),
                            EstateState {
                                active: Some(false),
                                privileges: Some(Default::default()),
                                ..Default::default()
                            },
                        ),
                    ]),
                ),
                (
                    beta,
                    BTreeMap::from([
                        (
                            "guild".into(),
                            EstateState {
                                active: Some(false),
                                privileges: Some(Default::default()),
                                ..Default::default()
                            },
                        ),
                        (
                            "inactive".into(),
                            EstateState {
                                active: Some(false),
                                privileges: Some(Default::default()),
                                ..Default::default()
                            },
                        ),
                    ]),
                ),
            ]),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(
        native.save("test-estates").unwrap(),
        loaded.initial.world().save("test-estates").unwrap()
    );
    let country = loaded.country("AAA").unwrap();
    assert_eq!(
        country.declared_estate_effects.known_totals_micros["global_tax_modifier"],
        100_000
    );
    assert!(
        country
            .declared_estate_effects
            .unresolved
            .contains("estates:guild:unknown_loyalty_band")
    );
    assert!(
        country.province_attribute_totals["tax_basis"]
            .unresolved
            .iter()
            .any(|r| r.starts_with("estates:guild:"))
    );
    assert_eq!(loaded.report.estates.historical_privileges, 1);
}

#[test]
fn dependency_conditions_macros_and_unknowns_select_without_order_guesses() {
    let temp = copy_fixture();
    write(
        temp.path(),
        "common/scripted_triggers/test.txt",
        "has_enabled_estate = { has_country_flag = allow_$estate$ } has_disabled_estate = { has_country_flag = deny_$estate$ }",
    );
    append(
        temp.path(),
        "history/countries/AAA - Alpha.txt",
        "set_country_flag = allow_favored",
    );
    append(
        temp.path(),
        "common/estates/test.txt",
        r#"
        dependent = { trigger = { has_estate = later } }
        later = { trigger = { has_dlc = "Emperor" NOT = { is_nomad = yes } } }
        conditional = { trigger = { if = { limit = { tag = BBB } always = no } } }
        favored = { trigger = { has_enabled_estate = { estate = favored } NOT = { has_disabled_estate = { estate = favored } } } }
        unbound = { trigger = { has_enabled_estate = yes } }
        missing = { trigger = { has_estate = absent } }
        cycle_a = { trigger = { has_estate = cycle_b } }
        cycle_b = { trigger = { has_estate = cycle_a } }
        false_known = { trigger = { always = no unsupported_trigger = yes } }
        true_known = { trigger = { OR = { always = yes unsupported_trigger = yes } } }
        unknown = { trigger = { unsupported_trigger = yes } }
    "#,
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let country = loaded.country("AAA").unwrap();
    let states = country.estates.unwrap();
    for id in ["dependent", "later", "conditional", "favored", "true_known"] {
        assert_eq!(states[id].active, Some(true), "{id}");
    }
    assert_eq!(states["false_known"].active, Some(false));
    for id in ["unbound", "missing", "cycle_a", "cycle_b", "unknown"] {
        assert_eq!(states[id].active, None, "{id}");
    }
    assert_eq!(
        loaded.country("BBB").unwrap().estates.unwrap()["conditional"].active,
        Some(false)
    );
    assert!(
        loaded
            .report
            .estates
            .definition_sources
            .contains_key("condition:has_enabled_estate")
    );
    append(
        temp.path(),
        "history/countries/AAA - Alpha.txt",
        "clr_country_flag = allow_favored",
    );
    assert_eq!(
        VanillaWorld::load(temp.path(), "1444.11.11")
            .unwrap()
            .country("AAA")
            .unwrap()
            .estates
            .unwrap()["favored"]
            .active,
        Some(false)
    );
}

#[test]
fn history_dates_scopes_and_land_deltas_do_not_invent_final_state() {
    let temp = copy_fixture();
    append(
        temp.path(),
        "history/countries/AAA - Alpha.txt",
        "1450.1.1 = { set_estate_privilege = charter } 1440.1.1 = { remove_estate_privilege = charter change_estate_land_share = { estate = guild share = 5 } }",
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let country = loaded.country("AAA").unwrap();
    let state = &country.estates.unwrap()["guild"];
    assert!(state.privileges.as_ref().unwrap().is_empty());
    assert_eq!(
        (
            state.loyalty_milli,
            state.influence_milli,
            state.land_share_micros
        ),
        (None, None, None)
    );
    assert_eq!(loaded.report.estates.pending_land_changes["AAA"].len(), 1);
    assert_eq!(
        VanillaWorld::load(temp.path(), "1450.1.1")
            .unwrap()
            .country("AAA")
            .unwrap()
            .estates
            .unwrap()["guild"]
            .privileges
            .as_ref()
            .unwrap()
            .len(),
        1
    );
    append(
        temp.path(),
        "history/countries/AAA - Alpha.txt",
        "if = { limit = { unsupported_trigger = yes } set_estate_privilege = charter }",
    );
    assert!(
        VanillaWorld::load(temp.path(), "1444.11.11")
            .unwrap()
            .country("AAA")
            .unwrap()
            .estates
            .unwrap()["guild"]
            .privileges
            .is_none()
    );
}

#[test]
fn conditional_properties_and_unexecuted_callbacks_preserve_missing_sources() {
    let temp = copy_fixture();
    write(
        temp.path(),
        "common/estate_privileges/test.txt",
        "charter = { conditional_modifier = { trigger = { tag = AAA } modifier = { global_tax_modifier = 0.9 } } benefits = { global_tax_modifier = 0.1 global_manpower_modifier = 0.2 } }",
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let alpha = loaded.initial.country_entity("AAA").unwrap();
    let effects = loaded
        .initial
        .world()
        .estates()
        .property_effects(alpha, "global_manpower_modifier")
        .unwrap();
    assert_eq!(
        effects.known_totals_micros["global_manpower_modifier"],
        200_000
    );
    assert!(effects.unresolved.is_empty());
    let tax = loaded
        .initial
        .world()
        .estates()
        .property_effects(alpha, "global_tax_modifier")
        .unwrap();
    assert!(!tax.known_totals_micros.contains_key("global_tax_modifier"));
    assert!(
        tax.unresolved
            .iter()
            .any(|r| r.contains("global_tax_modifier"))
    );
    write(
        temp.path(),
        "common/estate_privileges/test.txt",
        "charter = { benefits = { global_tax_modifier = 0.1 global_tax_modifier = 0.2 global_manpower_modifier = 0.2 } on_granted = { add_country_modifier = { name = not_loaded duration = -1 } } is_valid = { tag = AAA } }",
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let effects = &loaded.country("AAA").unwrap().declared_estate_effects;
    assert_eq!(
        effects.known_totals_micros["global_manpower_modifier"],
        200_000
    );
    assert!(
        !effects
            .known_totals_micros
            .contains_key("global_tax_modifier")
    );
    assert!(
        effects
            .unresolved
            .iter()
            .any(|r| r.ends_with(":on_granted"))
    );
    assert!(effects.unresolved.iter().any(|r| r.ends_with(":is_valid")));
    assert_eq!(loaded.report.estates.ambiguous_properties.len(), 1);
}

#[test]
fn ambiguous_or_unbound_grants_are_unknown_and_never_counted_twice() {
    let temp = copy_fixture();
    append(
        temp.path(),
        "common/estates/test.txt",
        "other = { trigger = { always = yes } privileges = { charter } }",
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let country = loaded.country("AAA").unwrap();
    for id in ["guild", "other"] {
        assert!(country.estates.unwrap()[id].privileges.is_none());
    }
    assert_eq!(loaded.report.estates.historical_privileges, 0);
    assert!(
        !country
            .declared_estate_effects
            .known_totals_micros
            .contains_key("global_tax_modifier")
    );
    assert!(
        country
            .declared_estate_effects
            .unresolved
            .iter()
            .any(|r| r.contains("unresolved_privilege_binding"))
    );
    append(
        temp.path(),
        "history/countries/AAA - Alpha.txt",
        "set_estate_privilege = absent",
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(
        loaded.report.estates.unresolved_countries["AAA"]
            .iter()
            .any(|r| r == "unresolved_privilege_binding:absent")
    );
}

#[test]
fn invalid_catalogs_and_conditions_report_their_source() {
    for text in [
        "guild = { trigger = { always = yes } } guild = {}",
        "guild = { trigger = { always = yes } trigger = {} }",
        "guild = { privileges = { charter charter } }",
        "guild = { trigger = { has_estate = \"\" } }",
        "guild = { trigger = { if = { always = yes } } }",
    ] {
        let temp = copy_fixture();
        write(temp.path(), "common/estates/test.txt", text);
        assert!(
            VanillaWorld::load(temp.path(), "1444.11.11")
                .unwrap_err()
                .to_string()
                .replace(char::from(92), "/")
                .contains("common/estates/test.txt")
        );
    }
    let temp = copy_fixture();
    append(
        temp.path(),
        "common/estate_privileges/test.txt",
        "charter = {}",
    );
    assert!(
        VanillaWorld::load(temp.path(), "1444.11.11")
            .unwrap_err()
            .to_string()
            .replace(char::from(92), "/")
            .contains("common/estate_privileges/test.txt")
    );
}
