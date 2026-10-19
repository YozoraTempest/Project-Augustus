// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{AttributeBase, AttributeFormula, MONTHLY_AUTONOMY_DELTA_ATTRIBUTE};
use augustus_eu4::{VanillaExternalState, VanillaWorld};
use std::{
    collections::BTreeMap,
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
    let target = root.join(path);
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(target, text).unwrap();
}

fn append(root: &Path, path: &str, text: &str) {
    let previous = fs::read_to_string(root.join(path)).unwrap();
    write(root, path, &(previous + "\n" + text));
}

fn raw(world: &VanillaWorld, province: u32) -> Option<i64> {
    world
        .province(province)
        .unwrap()
        .administration
        .unwrap()
        .raw_autonomy_micros
}

#[test]
fn external_raw_autonomy_binds_before_monthly_rules_run() {
    let state = VanillaExternalState {
        province_raw_autonomy_milli: BTreeMap::from([(1, Some(3_500))]),
        ..Default::default()
    };
    let loaded = VanillaWorld::load_with_external_state(fixture(), "1444.11.11", &state).unwrap();
    assert_eq!(raw(&loaded, 1), Some(35_000));
}

#[test]
fn adapter_monthly_autonomy_phase_applies_capital_bound_to_loaded_state() {
    let state = VanillaExternalState {
        province_raw_autonomy_milli: BTreeMap::from([(1, Some(50_000))]),
        ..Default::default()
    };
    let mut loaded =
        VanillaWorld::load_with_external_state(fixture(), "1444.11.11", &state).unwrap();
    let province = loaded.initial.province_entity(1).unwrap();
    let mut attributes = loaded.initial.world().attributes().clone();
    attributes.formulas.get_mut(&province).unwrap().insert(
        MONTHLY_AUTONOMY_DELTA_ATTRIBUTE.into(),
        AttributeFormula {
            base: AttributeBase::Constant { micros: -1_000 },
            channels: Vec::new(),
            minimum_micros: None,
            maximum_micros: None,
            result_quantum_micros: None,
            unresolved: Default::default(),
        },
    );
    loaded.initial.set_attributes(attributes).unwrap();
    let before = raw(&loaded, 1).unwrap();
    assert!(
        loaded
            .province(1)
            .unwrap()
            .administration
            .unwrap()
            .is_capital
    );

    assert!(loaded.update_monthly_autonomy().unwrap());

    let after = raw(&loaded, 1).unwrap();
    assert_eq!(before, 500_000);
    assert_eq!(after, 0);
}

fn identities(root: &Path) {
    write(
        root,
        "common/cultures/test.txt",
        "near = { alpha = {} beta = {} } far = { gamma = {} }",
    );
    write(
        root,
        "common/religions/test.txt",
        "familiar = { test_faith = {} heretic = {} } foreign = { other_faith = {} }",
    );
}

#[test]
fn cultural_religious_core_and_rank_inputs_generate_initial_values() {
    // Every value comes from this small self-authored world, including empty definitions.
    for (rank, culture, religion, cores, accepted, expected) in [
        (1, "alpha", "test_faith", "", "", 100_000),
        (
            1,
            "alpha",
            "test_faith",
            "",
            "add_accepted_culture = alpha",
            50_000,
        ),
        (2, "alpha", "test_faith", "", "", 50_000),
        (3, "alpha", "test_faith", "", "", 25_000),
        (3, "gamma", "test_faith", "", "", 50_000),
        (1, "beta", "heretic", "", "", 0),
        (1, "beta", "other_faith", "", "", 100_000),
        (2, "beta", "other_faith", "", "", 50_000),
        (1, "beta", "test_faith", "add_core = AAA", "", 20_000),
        (2, "beta", "test_faith", "add_core = AAA", "", 10_000),
        (
            2,
            "alpha",
            "test_faith",
            "",
            "1450.1.1 = { add_accepted_culture = alpha }",
            50_000,
        ),
        (
            2,
            "alpha",
            "test_faith",
            "",
            "add_accepted_culture = alpha 1440.1.1 = { remove_accepted_culture = alpha }",
            50_000,
        ),
    ] {
        let temp = copy_fixture();
        identities(temp.path());
        write(
            temp.path(),
            "history/countries/BBB - Beta.txt",
            &format!(
                "capital = 1 primary_culture = beta religion = test_faith government_rank = {rank} {accepted}"
            ),
        );
        write(
            temp.path(),
            "history/provinces/1 - North.txt",
            &format!(
                "owner = BBB controller = BBB add_core = BBB culture = {culture} religion = {religion} {cores}"
            ),
        );
        let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
        assert!(
            !loaded
                .report
                .unapplied_country_fields
                .contains_key("government_rank")
        );
        assert!(
            !loaded
                .report
                .unapplied_country_fields
                .contains_key("add_accepted_culture")
        );
        assert_eq!(
            raw(&loaded, 1),
            Some(expected),
            "rank={rank}, culture={culture}, religion={religion}, {accepted}"
        );
    }
}

#[test]
fn initial_autonomy_uses_rank_global_autonomy_value_not_rank_number() {
    let temp = copy_fixture();
    identities(temp.path());
    write(
        temp.path(),
        "history/countries/BBB - Beta.txt",
        "capital = 1 primary_culture = beta religion = test_faith government_rank = 3 add_accepted_culture = alpha",
    );
    write(
        temp.path(),
        "history/provinces/1 - North.txt",
        "owner = BBB controller = BBB add_core = BBB add_core = AAA culture = alpha religion = test_faith",
    );

    let negative_rank_modifier = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert_eq!(
        negative_rank_modifier
            .report
            .administration
            .initial_country_ranks["BBB"],
        3
    );
    assert_eq!(raw(&negative_rank_modifier, 1), Some(35_000));
    assert_eq!(
        negative_rank_modifier
            .report
            .attributes
            .government_rank_definitions,
        2
    );
    assert!(
        !negative_rank_modifier
            .report
            .attributes
            .unmapped_properties
            .contains_key("government_ranks:global_autonomy")
    );

    write(
        temp.path(),
        "common/government_ranks/test.txt",
        "2 = { global_autonomy = -0.025 } 3 = { global_autonomy = 0 }",
    );
    let zero_rank_modifier = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert_eq!(
        zero_rank_modifier
            .report
            .administration
            .initial_country_ranks["BBB"],
        3
    );
    assert_eq!(raw(&zero_rank_modifier, 1), Some(70_000));

    write(
        temp.path(),
        "common/government_ranks/test.txt",
        "2 = { global_autonomy = -0.025 } 3 = { global_autonomy = 0.05 }",
    );
    let positive_rank_modifier = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert_eq!(
        positive_rank_modifier
            .report
            .administration
            .initial_country_ranks["BBB"],
        3
    );
    assert_eq!(raw(&positive_rank_modifier, 1), Some(70_000));
}

#[test]
fn duplicate_government_rank_definition_keeps_initial_autonomy_unknown() {
    let temp = copy_fixture();
    identities(temp.path());
    write(
        temp.path(),
        "history/countries/BBB - Beta.txt",
        "capital = 1 primary_culture = beta religion = test_faith government_rank = 3 add_accepted_culture = alpha",
    );
    write(
        temp.path(),
        "history/provinces/1 - North.txt",
        "owner = BBB controller = BBB add_core = BBB add_core = AAA culture = alpha religion = test_faith",
    );
    write(
        temp.path(),
        "common/government_ranks/duplicate.txt",
        "3 = { global_autonomy = 0 }",
    );

    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert_eq!(loaded.report.attributes.government_rank_definitions, 1);
    assert_eq!(
        loaded.report.attributes.ambiguous_definitions["government_ranks:3"].len(),
        2
    );
    assert_eq!(raw(&loaded, 1), None);
    assert!(
        loaded
            .report
            .administration
            .unknown_initial_autonomy
            .contains(&1)
    );
}

#[test]
fn ordered_history_clamps_each_operation_before_country_effects() {
    let temp = copy_fixture();
    write(
        temp.path(),
        "history/provinces/1 - North.txt",
        "owner = BBB add_core = BBB culture = beta religion = test_faith add_local_autonomy = -10 1440.1.1 = { add_local_autonomy = 5 }",
    );
    append(
        temp.path(),
        "history/countries/BBB - Beta.txt",
        "every_owned_province = { add_local_autonomy = 3 }",
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert_eq!(raw(&loaded, 1), Some(80_000));
    assert_eq!(
        loaded
            .province(1)
            .unwrap()
            .administration
            .unwrap()
            .effective_autonomy_micros,
        Some(0)
    );
    write(
        temp.path(),
        "history/provinces/1 - North.txt",
        "owner = BBB add_core = BBB culture = beta religion = test_faith add_local_autonomy = 150 1440.1.1 = { add_local_autonomy = -20 }",
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert_eq!(raw(&loaded, 1), Some(830_000));
    assert_eq!(
        loaded.report.administration.applied_country_autonomy[&1].len(),
        1
    );
    // Unowned land still receives its own ordered history, but no owned iterator.
    write(
        temp.path(),
        "history/provinces/2 - East.txt",
        "add_local_autonomy = 150 add_local_autonomy = -20",
    );
    assert_eq!(
        raw(&VanillaWorld::load(temp.path(), "1444.11.11").unwrap(), 2),
        Some(800_000)
    );
}

#[test]
fn geographic_scopes_include_foreign_owners_and_iterators_keep_ownership() {
    let temp = copy_fixture();
    append(temp.path(), "map/default.map", "region = \"region.txt\"");
    write(
        temp.path(),
        "map/region.txt",
        "unused = {} north_region = { areas = { north_area } } east_region = { areas = { east_area } }",
    );
    append(
        temp.path(),
        "history/countries/AAA - Alpha.txt",
        "every_owned_province = { add_local_autonomy = 3 } north_area = { add_local_autonomy = 7 } if = { limit = { has_dlc = \"Domination\" } every_owned_province = { limit = { region = north_region } add_local_autonomy = 90 } }",
    );
    append(
        temp.path(),
        "history/countries/BBB - Beta.txt",
        "every_owned_province = { if = { limit = { OR = { area = north_area region = east_region } NOT = { always = no } } add_local_autonomy = 2 } }",
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert_eq!(raw(&loaded, 1), Some(390_000));
    assert_eq!(raw(&loaded, 2), Some(30_000));
    assert_eq!(loaded.report.administration.country_autonomy_operations, 4);
    assert_eq!(
        loaded.report.administration.applied_country_autonomy[&1].len(),
        2
    );
    assert!(
        loaded
            .report
            .administration
            .pending_country_autonomy
            .is_empty()
    );
}

#[test]
fn fixed_reforms_subject_inheritance_and_emperor_rank_affect_initialization() {
    let temp = copy_fixture();
    identities(temp.path());
    write(
        temp.path(),
        "common/government_reforms/test.txt",
        "test_empire = { fixed_rank = 3 } test_small = { fixed_rank = 1 }",
    );
    write(
        temp.path(),
        "common/subject_types/test.txt",
        "default = { max_government_rank = 0 } intermediate = { copy_from = default max_government_rank = 1 } test_vassal = {} test_vassal = { copy_from = intermediate }",
    );
    append(
        temp.path(),
        "history/countries/BBB - Beta.txt",
        "government_rank = 3 add_government_reform = test_empire",
    );
    // Subject caps apply to the province owner, even when its reform fixes a rank.
    assert_eq!(
        raw(&VanillaWorld::load(temp.path(), "1444.11.11").unwrap(), 1),
        Some(350_000)
    );
    append(
        temp.path(),
        "history/diplomacy/test.txt",
        "1444.11.11 = { emperor = BBB }",
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert_eq!(raw(&loaded, 1), Some(275_000));
    assert_eq!(loaded.report.administration.initial_country_ranks["BBB"], 3);
    assert_eq!(
        loaded.country("BBB").unwrap().government.unwrap().rank,
        Some(3)
    );
    // Conflicting fixed ranks remain ambiguous rather than selecting an arbitrary reform.
    append(
        temp.path(),
        "history/countries/BBB - Beta.txt",
        "add_government_reform = test_small",
    );
    assert_eq!(
        raw(&VanillaWorld::load(temp.path(), "1444.11.11").unwrap(), 1),
        None
    );
}

#[test]
fn uncertain_scoped_history_does_not_claim_values_for_foreign_provinces() {
    for body in [
        "north_area = { if = { limit = { unsupported = yes } add_local_autonomy = 10 } }",
        "north_area = { if = { add_local_autonomy = 10 } }",
        "north_area = { if = { limit = { has_dlc = \"Unknown expansion\" } add_local_autonomy = 10 } }",
        "north_area = { if = { limit = { NOT = { always = yes always = no } } add_local_autonomy = 10 } }",
    ] {
        let temp = copy_fixture();
        append(temp.path(), "history/countries/AAA - Alpha.txt", body);
        let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
        assert_eq!(raw(&loaded, 1), None, "{body}");
        assert_eq!(raw(&loaded, 2), Some(0));
        assert!(
            loaded
                .province(1)
                .unwrap()
                .administration
                .unwrap()
                .unresolved
                .contains("scoped_country_history")
        );
        assert!(
            !loaded
                .report
                .administration
                .applied_country_autonomy
                .contains_key(&1)
        );
    }
}

#[test]
fn duplicate_groups_and_nested_country_identity_preserve_unknown_inputs() {
    let temp = copy_fixture();
    identities(temp.path());
    append(
        temp.path(),
        "history/diplomacy/test.txt",
        "1444.11.11 = { emperor = BBB }",
    );
    append(
        temp.path(),
        "common/cultures/test.txt",
        "duplicate = { alpha = {} }",
    );
    assert_eq!(
        raw(&VanillaWorld::load(temp.path(), "1444.11.11").unwrap(), 1),
        None
    );
    let temp = copy_fixture();
    append(
        temp.path(),
        "history/countries/BBB - Beta.txt",
        "monarch = { name = Test religion = other_faith } queen = { name = Test religion = other_faith }",
    );
    assert_eq!(
        raw(&VanillaWorld::load(temp.path(), "1444.11.11").unwrap(), 1),
        Some(300_000)
    );
    append(
        temp.path(),
        "history/countries/BBB - Beta.txt",
        "if = { limit = { always = yes } primary_culture = alpha }",
    );
    assert_eq!(
        raw(&VanillaWorld::load(temp.path(), "1444.11.11").unwrap(), 1),
        None
    );
}

#[test]
fn nested_province_initialization_is_reported_without_using_partial_history() {
    let temp = copy_fixture();
    append(
        temp.path(),
        "history/provinces/1 - North.txt",
        "if = { limit = { always = yes } add_local_autonomy = 10 }",
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert_eq!(raw(&loaded, 1), None);
    assert_eq!(
        loaded.report.administration.pending_province_initialization[&1].len(),
        1
    );
    assert_eq!(raw(&loaded, 2), Some(0));
}

#[test]
fn constant_false_country_branches_and_missing_union_rules_remain_distinct() {
    let temp = copy_fixture();
    append(
        temp.path(),
        "history/countries/BBB - Beta.txt",
        "if = { limit = { always = no } every_owned_province = { add_local_autonomy = 90 } } every_owned_province = { if = { limit = { always = no } add_local_autonomy = 20 } }",
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert_eq!(raw(&loaded, 1), Some(300_000));
    assert!(
        !loaded
            .report
            .administration
            .applied_country_autonomy
            .contains_key(&1)
    );
    identities(temp.path());
    append(
        temp.path(),
        "history/diplomacy/test.txt",
        "1444.11.11 = { emperor = BBB }",
    );
    let path = temp.path().join("common/defines.lua");
    fs::write(
        &path,
        fs::read_to_string(&path)
            .unwrap()
            .replace("CULTURAL_UNION_MIN_RANK = 3,", ""),
    )
    .unwrap();
    assert_eq!(
        raw(&VanillaWorld::load(temp.path(), "1444.11.11").unwrap(), 1),
        None
    );
}

#[test]
fn initialized_noncapital_autonomy_reduces_shared_attribute_subtotals() {
    let temp = copy_fixture();
    append(
        temp.path(),
        "history/countries/BBB - Beta.txt",
        "1444.11.11 = { capital = 2 }",
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let north = loaded.province(1).unwrap();
    let administration = north.administration.unwrap();
    assert!(!administration.is_capital);
    assert_eq!(administration.effective_autonomy_micros, Some(300_000));
    assert_eq!(
        north.attributes["tax_after_autonomy"].known_subtotal_micros,
        Some(148_000)
    );
    assert_eq!(
        north.attributes["manpower_after_autonomy"].known_subtotal_micros,
        Some(581_875_000)
    );
    assert_eq!(north.attributes["tax_after_autonomy"].value_micros, None);
    assert!(
        !north.attributes["tax_after_autonomy"]
            .unresolved
            .contains("administration:initial_autonomy")
    );
}

#[test]
fn baseline_cores_states_and_history_deltas_reach_queries() {
    let loaded = VanillaWorld::load(fixture(), "1444.11.11").unwrap();
    let north = loaded.province(1).unwrap().administration.unwrap();
    assert_eq!(north.area.as_deref(), Some("north_area"));
    assert_eq!(north.area_status, Some(augustus_core::AreaStatus::State));
    assert_eq!(north.owner_core, Some(augustus_core::CoreType::Full));
    assert_eq!(north.minimum_autonomy_micros, Some(0));
    assert_eq!(north.raw_autonomy_micros, Some(300_000));
    assert_eq!(north.history_autonomy_delta_micros, 0);
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
    assert_eq!(east.effective_autonomy_micros, Some(0));
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
fn unknown_country_conditions_keep_affected_values_unknown() {
    let temp = copy_fixture();
    let path = temp.path().join("history/countries/AAA - Alpha.txt");
    let text = fs::read_to_string(&path).unwrap();
    fs::write(&path, text + "\n1444.1.1 = { east_area = { if = { limit = { unsupported = yes } add_local_autonomy = 20 } } }\n1445.1.1 = { if = { limit = { unsupported = yes } every_owned_province = { add_local_autonomy = 30 } } }\n").unwrap();
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
