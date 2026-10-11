// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    Action, EstateDefinition, EstateEffect, EstateMetric, EstatePrivilege, EstateRules,
    EstateState, Estates, ModifierDefinition,
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

fn startup_fixture() -> tempfile::TempDir {
    let temp = land_fixture();
    let path = "history/countries/AAA - Alpha.txt";
    let text = fs::read_to_string(temp.path().join(path)).unwrap();
    write(
        temp.path(),
        path,
        &text.replace("set_estate_privilege = charter", ""),
    );
    write(
        temp.path(),
        "common/estate_privileges/test.txt",
        r#"
        charter = {
            benefits = { global_tax_modifier = 0.1 }
            is_valid = { tag = AAA }
            on_granted = {
                if = { limit = { has_estate_privilege = charter }
                    unreachable_effect = yes
                }
                else = { add_estate_loyalty = { estate = estate_guild loyalty = 13 } }
                change_estate_land_share = { estate = estate_guild share = 2 }
            }
            on_revoked = {
                if = { limit = { has_estate_privilege = charter }
                    add_estate_loyalty = { estate = estate_guild loyalty = -11 }
                }
                else = { unreachable_effect = yes }
                change_estate_land_share = { estate = estate_guild share = -2 }
            }
        }
    "#,
    );
    temp
}

fn loyalty(amount: i64) -> Action {
    Action::ChangeEstateMetric {
        estate: "estate_guild".into(),
        metric: EstateMetric::Loyalty,
        amount,
        minimum: Some(0),
    }
}

#[test]
fn startup_office_and_global_targets_share_core_state_and_restore() {
    let temp = startup_fixture();
    write(
        temp.path(),
        "common/on_actions/test.txt",
        r#"
        on_startup = { if = { limit = { tag = AAA }
            emperor = { save_global_event_target_as = council }
            event_target:council = { add_estate_loyalty = { estate = estate_guild loyalty = 3 } }
            BBB = { save_global_event_target_as = council }
            event_target:council = { add_estate_loyalty = { estate = estate_guild loyalty = 4 } }
            clear_global_event_target = council
            event_target:council = { unreachable_effect = yes }
            save_global_event_target_as = founder
        } }
    "#,
    );
    let mut loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(loaded.report.estates.pending_startup_effects.is_empty());
    assert_eq!(
        loaded.country("AAA").unwrap().estates.unwrap()["estate_guild"].loyalty_milli,
        Some(53_000)
    );
    assert_eq!(
        loaded.country("BBB").unwrap().estates.unwrap()["estate_guild"].loyalty_milli,
        Some(54_000)
    );
    assert_eq!(
        loaded.initial.world().global_scopes()["founder"],
        loaded.initial.country_entity("AAA").unwrap()
    );
    assert!(
        !loaded
            .initial
            .world()
            .global_scopes()
            .contains_key("council")
    );
    let saved = loaded.save().unwrap();
    let mut restored = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    restored.restore(&saved).unwrap();
    loaded.advance(3).unwrap();
    restored.advance(3).unwrap();
    assert_eq!(loaded.save().unwrap(), restored.save().unwrap());
}

#[test]
fn incomplete_startup_discards_global_bindings_with_other_prefix_writes() {
    let temp = startup_fixture();
    write(
        temp.path(),
        "common/on_actions/test.txt",
        "on_startup = { emperor = { save_global_event_target_as = partial } unsupported_effect = yes }",
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert_eq!(loaded.report.estates.pending_startup_effects.len(), 1);
    assert!(loaded.initial.world().global_scopes().is_empty());
    assert_eq!(loaded.report.estates.applied_startup_effects, 0);
}

#[test]
fn startup_macros_scopes_and_live_callback_conditions_match_native_execution() {
    let temp = startup_fixture();
    write(
        temp.path(),
        "common/scripted_effects/test.txt",
        r#"
        apply_charter = { set_estate_privilege = charter add_estate_loyalty = { estate = estate_guild loyalty = 3 } }
        unrelated = { unsupported_effect = { arbitrary = { data = 1 } } }
    "#,
    );
    let startup_script = r#"
        on_startup = { if = { limit = { tag = AAA }
            if = { limit = { always = no } unreachable_effect = yes }
            else_if = { limit = { has_dlc = "Emperor" } hidden_effect = { every_country = { limit = { tag = AAA } apply_charter = yes } } }
            else = { unreachable_effect = yes }
            BBB = { add_estate_loyalty = { estate = estate_guild loyalty = -100 } }
            every_country = { limit = { tag = AAA }
                if = { limit = { has_estate_privilege = charter } add_estate_loyalty = { estate = estate_guild loyalty = 1 } }
            }
        } }
    "#;
    write(temp.path(), "common/on_actions/test.txt", startup_script);
    let mut loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let states = loaded.country("AAA").unwrap().estates.unwrap();
    assert_eq!(states["estate_guild"].loyalty_milli, Some(67_000));
    assert_eq!(states["estate_guild"].land_share_micros, Some(170_000));
    assert_eq!(
        loaded.country("BBB").unwrap().estates.unwrap()["estate_guild"].loyalty_milli,
        Some(0)
    );
    assert_eq!(loaded.report.estates.applied_startup_effects, 6);
    assert!(loaded.report.estates.pending_startup_effects.is_empty());
    let effects = loaded.country("AAA").unwrap().declared_estate_effects;
    assert_eq!(effects.known_totals_micros["global_tax_modifier"], 100_000);
    assert!(effects.unresolved.is_empty());

    write(temp.path(), "common/on_actions/test.txt", "on_startup = {}");
    let mut native = VanillaWorld::load(temp.path(), "1444.11.11")
        .unwrap()
        .initial;
    let alpha = native.country_entity("AAA").unwrap();
    let beta = native.country_entity("BBB").unwrap();
    native
        .execute(
            alpha,
            &[
                loyalty(13_000),
                Action::ChangeEstateMetric {
                    estate: "estate_guild".into(),
                    metric: EstateMetric::LandShare,
                    amount: 20_000,
                    minimum: None,
                },
                Action::SetEstatePrivilege {
                    estate: "estate_guild".into(),
                    privilege: "charter".into(),
                    granted: true,
                },
                Action::ResolveEstatePrivilegeEffect {
                    privilege: "charter".into(),
                    effect: "is_valid".into(),
                },
                Action::ResolveEstatePrivilegeEffect {
                    privilege: "charter".into(),
                    effect: "on_granted".into(),
                },
                loyalty(3_000),
                loyalty(1_000),
            ],
        )
        .unwrap();
    native.execute(beta, &[loyalty(-100_000)]).unwrap();
    assert_eq!(
        loaded.initial.world().save("same").unwrap(),
        native.world().save("same").unwrap()
    );

    write(temp.path(), "common/on_actions/test.txt", startup_script);
    let saved = loaded.save().unwrap();
    let mut restored = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    restored.restore(&saved).unwrap();
    loaded.advance(45).unwrap();
    restored.restore(&loaded.save().unwrap()).unwrap();
    loaded.advance(320).unwrap();
    restored.advance(320).unwrap();
    assert_eq!(loaded.save().unwrap(), restored.save().unwrap());
    assert_eq!(
        loaded.country("AAA").unwrap().estates.unwrap()["estate_guild"].loyalty_milli,
        Some(67_000)
    );
}

#[test]
fn startup_revocation_runs_callback_and_clears_country_resolution() {
    let temp = startup_fixture();
    write(
        temp.path(),
        "common/on_actions/test.txt",
        "on_startup = { if = { limit = { tag = AAA } set_estate_privilege = charter remove_estate_privilege = charter } }",
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let state = &loaded.country("AAA").unwrap().estates.unwrap()["estate_guild"];
    assert_eq!(state.loyalty_milli, Some(52_000));
    assert_eq!(state.land_share_micros, Some(150_000));
    assert!(state.privileges.as_ref().unwrap().is_empty());
    assert!(
        loaded
            .initial
            .world()
            .estates()
            .resolved_privilege_effects
            .is_empty()
    );
    assert_eq!(loaded.report.estates.applied_startup_effects, 6);

    let temp = land_fixture();
    write(
        temp.path(),
        "common/estate_privileges/test.txt",
        "charter = { on_revoked = { add_estate_loyalty = { estate = estate_guild loyalty = 2 } } }",
    );
    append(
        temp.path(),
        "history/countries/AAA - Alpha.txt",
        "remove_estate_privilege = charter",
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(
        loaded.country("AAA").unwrap().estates.unwrap()["estate_guild"]
            .loyalty_milli
            .is_none()
    );
    assert!(
        loaded.report.estates.unresolved_initialization["AAA"]
            .contains("unresolved_privilege_initialization")
    );
}

#[test]
fn incomplete_startup_discards_prefix_and_preserves_callback_source() {
    let temp = startup_fixture();
    write(
        temp.path(),
        "common/scripted_effects/test.txt",
        "bad_callback = { unsupported_effect = yes }",
    );
    write(
        temp.path(),
        "common/estate_privileges/test.txt",
        "charter = { benefits = { global_tax_modifier = 0.1 } on_granted = { bad_callback = yes } }",
    );
    write(
        temp.path(),
        "common/on_actions/test.txt",
        "on_startup = { if = { limit = { tag = AAA } add_estate_loyalty = { estate = estate_guild loyalty = 2 } set_estate_privilege = charter } }",
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert_eq!(loaded.report.estates.applied_startup_effects, 0);
    assert!(loaded.report.estates.startup_effect_sources.is_empty());
    assert!(
        loaded
            .report
            .estates
            .startup_unresolved
            .contains("unsupported_effect:unsupported_effect")
    );
    assert!(
        loaded.report.estates.pending_startup_effects[0]
            .path
            .ends_with("common/scripted_effects/test.txt")
    );
    assert!(
        loaded
            .initial
            .world()
            .estates()
            .resolved_privilege_effects
            .is_empty()
    );
    for state in loaded.country("AAA").unwrap().estates.unwrap().values() {
        assert!(
            state.loyalty_milli.is_none()
                && state.land_share_micros.is_none()
                && state.privileges.is_none()
        );
    }
    assert!(
        loaded
            .country("AAA")
            .unwrap()
            .declared_estate_effects
            .known_totals_micros
            .is_empty()
    );
    assert!(loaded.save().is_ok());
}

#[test]
fn unknown_conditions_repeated_grants_and_numeric_bounds_block_startup() {
    for body in [
        "if = { limit = { unknown_trigger = yes } add_estate_loyalty = { estate = estate_guild loyalty = 3 } }",
        "set_estate_privilege = charter set_estate_privilege = charter",
        "add_estate_loyalty = { estate = estate_guild loyalty = 100 }",
        "change_estate_land_share = { estate = estate_guild share = -100 }",
    ] {
        let temp = startup_fixture();
        write(
            temp.path(),
            "common/on_actions/test.txt",
            &format!("on_startup = {{ if = {{ limit = {{ tag = AAA }} {body} }} }}"),
        );
        let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
        assert_eq!(loaded.report.estates.applied_startup_effects, 0);
        assert_eq!(loaded.report.estates.pending_startup_effects.len(), 1);
        assert!(
            loaded.country("AAA").unwrap().estates.unwrap()["estate_guild"]
                .loyalty_milli
                .is_none()
        );
        assert!(loaded.save().is_ok());
    }
}

#[test]
fn ambiguous_recursive_and_parameterized_effects_are_pending_with_sources() {
    for declaration in [
        "test_effect = {} test_effect = {}",
        "test_effect = { test_effect = yes }",
        "test_effect = { add_estate_loyalty = { estate = estate_guild loyalty = 1 } }",
    ] {
        let temp = startup_fixture();
        write(temp.path(), "common/scripted_effects/test.txt", declaration);
        let call = if declaration.contains("loyalty") {
            "test_effect = { estate = estate_guild }"
        } else {
            "test_effect = yes"
        };
        write(
            temp.path(),
            "common/on_actions/test.txt",
            &format!("on_startup = {{ if = {{ limit = {{ tag = AAA }} {call} }} }}"),
        );
        let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
        assert_eq!(loaded.report.estates.pending_startup_effects.len(), 1);
        assert_eq!(loaded.report.estates.applied_startup_effects, 0);
    }
    for script in [
        "on_startup = { else = {} }",
        "on_startup = { if = { limit = { always = yes } } else = {} else = {} }",
        "on_startup = { if = { limit = { always = yes } } else = {} else_if = { limit = { always = no } } }",
        "on_startup = {} on_startup = {}",
    ] {
        let temp = startup_fixture();
        write(temp.path(), "common/on_actions/test.txt", script);
        assert!(
            VanillaWorld::load(temp.path(), "1444.11.11")
                .unwrap_err()
                .to_string()
                .replace(char::from(92), "/")
                .contains("common/on_actions/test.txt")
        );
    }
}

#[test]
fn startup_uses_each_living_country_as_its_initial_scope() {
    let temp = startup_fixture();
    write(
        temp.path(),
        "common/on_actions/test.txt",
        "on_startup = { add_estate_loyalty = { estate = estate_guild loyalty = 1 } }",
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    for tag in ["AAA", "BBB"] {
        assert_eq!(
            loaded.country(tag).unwrap().estates.unwrap()["estate_guild"].loyalty_milli,
            Some(51_000)
        );
    }
    assert_eq!(loaded.report.estates.applied_startup_effects, 2);
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
fn translated_content_matches_native_core_with_known_loyalty_and_unknown_influence() {
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
                                loyalty_milli: Some(50_000),
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
            .contains("estates:guild:neutral:unknown_scale")
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
        (Some(50_000), None, None)
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
        "guild = { base_influence = 1 base_influence = 2 }",
        "guild = { base_influence = invalid }",
        "guild = { influence_modifier = { trigger = { always = yes } } }",
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

fn land_fixture() -> tempfile::TempDir {
    let temp = copy_fixture();
    let path = "common/defines.lua";
    let text = fs::read_to_string(temp.path().join(path)).unwrap();
    write(temp.path(), path, &text.replace("NCountry = {", "NCountry = {\n ESTATE_START_CROWNLAND_INFLUENCE = 12,\n ESTATE_MIN_DISTRIBUTED_CROWNLAND = 40,\n"));
    write(
        temp.path(),
        "common/estates/test.txt",
        r#"
        estate_guild = { trigger = { always = yes } base_influence = 12 privileges = { charter }
            influence_modifier = { desc = test trigger = { tag = BBB } influence = -12 } }
        estate_council = { trigger = { always = yes } base_influence = 24 }
        estate_order = { trigger = { always = yes } base_influence = 12 }
    "#,
    );
    temp
}

#[test]
fn land_distribution_and_dated_history_generate_resumable_core_state() {
    let temp = land_fixture();
    append(
        temp.path(),
        "history/countries/AAA - Alpha.txt",
        r#"
        change_estate_land_share = { estate = estate_council share = 5 }
        change_estate_land_share = { estate = estate_guild share = -2 }
        change_estate_land_share = { estate = estate_order share = -3 }
        add_estate_loyalty = { estate = estate_guild loyalty = 7 }
        1450.1.1 = { add_estate_loyalty = { estate = estate_guild loyalty = -100 } }
    "#,
    );
    let mut loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let states = loaded.country("AAA").unwrap().estates.unwrap();
    assert_eq!(states["estate_guild"].land_share_micros, Some(130_000));
    assert_eq!(states["estate_council"].land_share_micros, Some(350_000));
    assert_eq!(states["estate_order"].land_share_micros, Some(120_000));
    assert_eq!(states["estate_guild"].loyalty_milli, Some(57_000));
    assert_eq!(
        loaded.country("BBB").unwrap().estates.unwrap()["estate_guild"].land_share_micros,
        Some(0)
    );
    assert_eq!(loaded.report.estates.initial_land_countries, 2);
    assert!(loaded.report.estates.pending_land_changes.is_empty());
    let saved = loaded.save().unwrap();
    let mut restored = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    restored.restore(&saved).unwrap();
    loaded.advance(365).unwrap();
    restored.advance(365).unwrap();
    assert_eq!(loaded.save().unwrap(), restored.save().unwrap());
    let later = VanillaWorld::load(temp.path(), "1450.1.1").unwrap();
    assert_eq!(
        later.country("AAA").unwrap().estates.unwrap()["estate_guild"].loyalty_milli,
        Some(0)
    );
}

#[test]
fn uncertain_influence_scoped_history_and_startup_effects_preserve_unknowns() {
    let temp = land_fixture();
    append(
        temp.path(),
        "common/estates/test.txt",
        "uncertain = { trigger = { always = yes } base_influence = 3 influence_modifier = { trigger = { unknown_condition = yes } influence = 2 } }",
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(
        loaded
            .country("AAA")
            .unwrap()
            .estates
            .unwrap()
            .values()
            .all(|s| s.land_share_micros.is_none())
    );
    assert!(
        loaded.report.estates.unresolved_initialization["AAA"]
            .iter()
            .any(|r| r.contains("unknown_condition"))
    );
    write(
        temp.path(),
        "common/on_actions/test.txt",
        "on_startup = { effect = { add_estate_loyalty = { estate = estate_guild loyalty = 9 } } }",
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(
        loaded
            .country("AAA")
            .unwrap()
            .estates
            .unwrap()
            .values()
            .all(|s| s.loyalty_milli.is_none())
    );
    assert!(
        loaded.report.estates.unresolved_initialization["AAA"]
            .contains("unexecuted_startup_estate_effects")
    );
    assert!(
        loaded
            .country("AAA")
            .unwrap()
            .estates
            .unwrap()
            .values()
            .all(|s| s.land_share_micros.is_none())
    );
    write(temp.path(), "common/on_actions/test.txt", "on_startup = {}");
    append(
        temp.path(),
        "history/countries/AAA - Alpha.txt",
        "if = { limit = { always = yes } add_estate_loyalty = { estate = estate_guild loyalty = 9 } }",
    );
    assert!(
        VanillaWorld::load(temp.path(), "1444.11.11")
            .unwrap()
            .country("AAA")
            .unwrap()
            .estates
            .unwrap()
            .values()
            .all(|s| s.loyalty_milli.is_none())
    );
}

#[test]
fn unvalidated_land_boundaries_and_privilege_land_effects_are_reported() {
    let temp = land_fixture();
    append(
        temp.path(),
        "history/countries/AAA - Alpha.txt",
        "change_estate_land_share = { estate = estate_guild share = 100 }",
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(
        loaded
            .country("AAA")
            .unwrap()
            .estates
            .unwrap()
            .values()
            .all(|s| s.land_share_micros.is_none())
    );
    assert_eq!(loaded.report.estates.pending_land_changes["AAA"].len(), 1);
    let temp = land_fixture();
    append(
        temp.path(),
        "common/estate_privileges/test.txt",
        "other = { land_share = 5 }",
    );
    write(
        temp.path(),
        "common/estate_privileges/test.txt",
        "charter = { land_share = 5 is_valid = { always = yes } }",
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(
        loaded
            .country("AAA")
            .unwrap()
            .estates
            .unwrap()
            .values()
            .all(|s| s.land_share_micros.is_none())
    );
}

#[test]
fn government_and_attached_modifiers_contribute_before_historical_privileges() {
    let temp = land_fixture();
    append(
        temp.path(),
        "common/government_reforms/test_governments.txt",
        "estate_levy = { modifiers = { guild_influence_modifier = 0.12 } }",
    );
    append(
        temp.path(),
        "history/countries/AAA - Alpha.txt",
        "add_government_reform = estate_levy",
    );
    append(
        temp.path(),
        "common/event_modifiers/test_modifiers.txt",
        "estate_weights = { guild_exclusive_influence_modifier = 0.12 all_estate_influence_modifier = 0.12 }",
    );
    append(
        temp.path(),
        "history/countries/AAA - Alpha.txt",
        "add_country_modifier = { name = estate_weights duration = -1 }",
    );
    write(
        temp.path(),
        "common/estate_privileges/test.txt",
        "charter = { influence = 0.9 loyalty = 0.8 is_valid = { always = yes } on_granted = {} }",
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let states = loaded.country("AAA").unwrap().estates.unwrap();
    assert_eq!(states["estate_guild"].land_share_micros, Some(266_660));
    assert_eq!(states["estate_council"].land_share_micros, Some(200_000));
    assert_eq!(states["estate_order"].land_share_micros, Some(133_340));
    assert_eq!(states["estate_guild"].loyalty_milli, Some(50_000));
    assert!(states["estate_guild"].influence_milli.is_none());
}

#[test]
fn unknown_numeric_boundaries_and_invalid_history_remain_diagnostic() {
    let temp = land_fixture();
    append(
        temp.path(),
        "history/countries/AAA - Alpha.txt",
        "add_estate_loyalty = { estate = estate_guild loyalty = 100 }",
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(
        loaded
            .country("AAA")
            .unwrap()
            .estates
            .unwrap()
            .values()
            .all(|s| s.loyalty_milli.is_none())
    );
    assert_eq!(
        loaded.report.estates.pending_loyalty_changes["AAA"].len(),
        1
    );
    assert!(loaded.report.estates.pending_land_changes.is_empty());
    assert!(
        loaded
            .country("AAA")
            .unwrap()
            .estates
            .unwrap()
            .values()
            .all(|s| s.land_share_micros.is_some())
    );

    let temp = land_fixture();
    append(
        temp.path(),
        "common/estates/test.txt",
        "negative = { trigger = { always = yes } base_influence = -1 }",
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(
        loaded
            .country("AAA")
            .unwrap()
            .estates
            .unwrap()
            .values()
            .all(|s| s.land_share_micros.is_none())
    );
    assert!(
        loaded.report.estates.unresolved_initialization["AAA"]
            .contains("negative:unsupported_negative_initial_influence")
    );

    for history in [
        "change_estate_land_share = { estate = estate_guild share = 0.00001 }",
        "add_estate_loyalty = { estate = estate_guild loyalty = 1 loyalty = 2 }",
        "add_estate_loyalty = { loyalty = 1 }",
    ] {
        let temp = land_fixture();
        append(temp.path(), "history/countries/AAA - Alpha.txt", history);
        let message = VanillaWorld::load(temp.path(), "1444.11.11")
            .unwrap_err()
            .to_string()
            .replace(char::from(92), "/");
        assert!(
            message.contains("history/countries/AAA - Alpha.txt"),
            "{message}"
        );
    }
}

#[test]
fn removed_or_unbound_privileges_cannot_hide_unexecuted_initialization() {
    for operation in [
        "remove_estate_privilege = charter",
        "set_estate_privilege = absent",
    ] {
        let temp = land_fixture();
        write(
            temp.path(),
            "common/estate_privileges/test.txt",
            "charter = { on_granted = { add_estate_loyalty = { estate = estate_guild loyalty = 8 } } }",
        );
        append(temp.path(), "history/countries/AAA - Alpha.txt", operation);
        let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
        let states = loaded.country("AAA").unwrap().estates.unwrap();
        assert!(
            states
                .values()
                .all(|s| s.land_share_micros.is_none() && s.loyalty_milli.is_none())
        );
        assert!(
            loaded.report.estates.unresolved_initialization["AAA"]
                .contains("unresolved_privilege_initialization")
        );
        if operation.starts_with("remove") {
            assert!(
                states["estate_guild"]
                    .privileges
                    .as_ref()
                    .unwrap()
                    .is_empty()
            );
        }
    }
}
