// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::{
    fs,
    path::{Path, PathBuf},
};

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
fn translated_attributes_explain_sources_and_keep_incomplete_state_explicit() {
    let loaded = VanillaWorld::load(fixture(), "1444.11.11").unwrap();
    let north = loaded.province(1).unwrap();
    let tax = &north.attributes["tax_basis"];
    assert_eq!(tax.base_micros, Some(2_125_000));
    assert_eq!(tax.known_subtotal_micros, Some(2_337_500));
    assert_eq!(tax.value_micros, None);
    assert_eq!(
        tax.steps[0].contributions[0].source,
        "static:same_culture_group"
    );
    assert_eq!(tax.steps[0].contributions[0].micros, -150_000);
    assert_eq!(tax.steps[0].contributions[1].source, "static:city");
    assert_eq!(
        north.attributes["manpower_contribution"].known_subtotal_micros,
        Some(831_250_000)
    );
    assert_eq!(
        loaded.country("AAA").unwrap().province_attribute_totals["tax_basis"].known_subtotal_micros,
        5_100_000
    );
    assert!(loaded.province(3).unwrap().attributes.is_empty());
    assert_eq!(
        loaded.report.attributes.manpower_per_base_micros,
        250_000_000
    );
    assert_eq!(
        loaded.report.attributes.definition_sources["define:manpower_per_base"].line,
        6
    );
    let future = VanillaWorld::load(fixture(), "1445.1.1").unwrap();
    assert!(
        future.province(1).unwrap().attributes["tax_basis"]
            .unresolved
            .contains("pending_modifier:province:1:test_harbor")
    );
    let temp = copy_fixture();
    let history = temp.path().join("history/provinces/1 - North.txt");
    let text = fs::read_to_string(&history).unwrap();
    fs::write(history, text + "\n1444.1.1 = { controller = AAA }\n").unwrap();
    let occupied = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert_eq!(
        occupied.province(1).unwrap().attributes["tax_basis"].known_subtotal_micros,
        Some(1_275_000)
    );
}

#[test]
fn translated_core_effects_join_existing_rates_before_the_autonomy_channel() {
    let temp = copy_fixture();
    let path = temp.path().join("common/static_modifiers/test_static.txt");
    let text = fs::read_to_string(&path).unwrap();
    fs::write(
        &path,
        text.replace(
            "core = {}",
            "core = { local_tax_modifier = 0.2 local_manpower_modifier = 0.3 }",
        ),
    )
    .unwrap();
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let north = loaded.province(1).unwrap();
    let tax = &north.attributes["tax_after_autonomy"];
    assert_eq!(tax.steps[0].output_micros, Some(2_762_500));
    assert_eq!(tax.steps[1].output_micros, Some(2_762_500));
    assert!(
        tax.steps[0]
            .contributions
            .iter()
            .any(|source| source.source == "static:core" && source.micros == 200_000)
    );
    assert!(!tax.unresolved.contains("state_and_core_effects"));
    assert_eq!(
        north.attributes["manpower_contribution"].known_subtotal_micros,
        Some(1_093_750_000)
    );
    assert!(tax.value_micros.is_none());
    let mut world = loaded.initial.world().clone();
    let province = loaded.initial.province_entity(1).unwrap();
    let owner = world.entity(province).unwrap().owner.unwrap();
    let mut administration = world.administration().clone();
    administration
        .country_areas
        .get_mut(&owner)
        .unwrap()
        .insert("north_area".into(), augustus_core::AreaStatus::Territory);
    world.set_administration(administration).unwrap();
    let changed = world
        .attributes()
        .evaluate(&world, province, "tax_basis")
        .unwrap();
    assert!(
        !changed.steps[0]
            .contributions
            .iter()
            .any(|source| source.source == "static:core")
    );
    assert_eq!(changed.known_subtotal_micros, Some(2_337_500));
}

#[test]
fn ambiguous_administrative_properties_only_taint_the_selected_category() {
    let temp = copy_fixture();
    let path = temp.path().join("common/static_modifiers/test_static.txt");
    let text = fs::read_to_string(&path).unwrap();
    fs::write(
        &path,
        text.replace(
            "territory_core = {",
            "territory_core = { local_tax_modifier = 0.2 local_tax_modifier = 0.6 ",
        ),
    )
    .unwrap();
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let mut world = loaded.initial.world().clone();
    let province = loaded.initial.province_entity(1).unwrap();
    let owner = world.entity(province).unwrap().owner.unwrap();
    let query = |world: &augustus_core::World| {
        world
            .attributes()
            .evaluate(world, province, "tax_basis")
            .unwrap()
    };
    assert!(
        !query(&world)
            .unresolved
            .iter()
            .any(|reason| reason.contains("territory_core:local_tax_modifier"))
    );
    let mut administration = world.administration().clone();
    administration
        .country_areas
        .get_mut(&owner)
        .unwrap()
        .insert("north_area".into(), augustus_core::AreaStatus::Territory);
    world.set_administration(administration).unwrap();
    assert!(
        query(&world)
            .unresolved
            .contains("administration:unresolved_property:territory_core:local_tax_modifier")
    );
    assert_eq!(query(&world).known_subtotal_micros, Some(2_337_500));
    assert_eq!(loaded.report.attributes.ambiguous_properties["static_modifiers:territory_core:local_tax_modifier"].len(), 2);
    let saved = world.save("administrative-effects").unwrap();
    let restored =
        augustus_core::World::restore(&saved, "administrative-effects", world.schema()).unwrap();
    assert_eq!(query(&world), query(&restored));
}

#[test]
fn unclassified_or_missing_administrative_effects_remain_unknown() {
    let later = VanillaWorld::load(fixture(), "1445.11.11").unwrap();
    assert!(
        later.province(1).unwrap().attributes["tax_basis"]
            .unresolved
            .contains("administration:unknown_category")
    );
    let temp = copy_fixture();
    let path = temp.path().join("common/static_modifiers/test_static.txt");
    let text = fs::read_to_string(&path).unwrap();
    fs::write(path, text + "\ncore = { local_tax_modifier = 0.2 }\n").unwrap();
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let tax = &loaded.province(1).unwrap().attributes["tax_basis"];
    assert!(
        tax.unresolved
            .contains("administration:missing_definition:core")
    );
    assert_eq!(tax.known_subtotal_micros, Some(2_337_500));
}

#[test]
fn duplicate_static_definitions_and_properties_preserve_ambiguity_without_guessing() {
    let temp = copy_fixture();
    let path = temp.path().join("common/static_modifiers/test_static.txt");
    let text = fs::read_to_string(&path).unwrap();
    fs::write(
        &path,
        text + "\ncity = { local_tax_modifier = 0.5 }\ncity = { local_tax_modifier = 0.75 }\n",
    )
    .unwrap();
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert_eq!(
        loaded.report.attributes.ambiguous_definitions["static_modifiers:city"].len(),
        3
    );
    assert!(
        !loaded
            .initial
            .world()
            .attributes()
            .static_modifiers
            .contains_key("city")
    );
    let tax = &loaded.province(1).unwrap().attributes["tax_basis"];
    assert_eq!(tax.known_subtotal_micros, Some(1_806_250));
    assert!(tax.unresolved.contains("ambiguous_static:city"));
    let temp = copy_fixture();
    let path = temp.path().join("common/buildings/test_buildings.txt");
    let text = fs::read_to_string(&path).unwrap();
    fs::write(
        path,
        text.replace(
            "local_tax_modifier = 0.125",
            "local_tax_modifier = 0.125 local_tax_modifier = 0.5",
        ),
    )
    .unwrap();
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let tax = &loaded.province(2).unwrap().attributes["tax_basis"];
    assert_eq!(tax.known_subtotal_micros, Some(4_600_000));
    assert!(
        tax.unresolved
            .contains("ambiguous:buildings:test_market:local_tax_modifier")
    );
}

#[test]
fn expressions_overrides_and_invalid_attribute_shapes_are_rejected() {
    for (path, from, to, message) in [
        (
            "common/defines.lua",
            "0.25,",
            "0.25 * 2,",
            "nonnegative literal",
        ),
        (
            "common/defines.lua",
            "0.25,",
            "-0.25,",
            "nonnegative literal",
        ),
        (
            "common/defines.lua",
            "BASE_MP_TO_MANPOWER = 0.25,",
            "BASE_MP_TO_MANPOWER = 0.25,\nBASE_MP_TO_MANPOWER = 0.5,",
            "unique literal",
        ),
        (
            "common/buildings/test_buildings.txt",
            "modifier = { test_power",
            "modifier = { 1 } modifier = { test_power",
            "expected setup block",
        ),
        (
            "common/buildings/test_buildings.txt",
            "local_tax_modifier = 0.125",
            "local_tax_modifier = yes",
            "numeric property",
        ),
    ] {
        let temp = copy_fixture();
        let path = temp.path().join(path);
        let text = fs::read_to_string(&path).unwrap();
        fs::write(path, text.replace(from, to)).unwrap();
        let error = VanillaWorld::load(temp.path(), "1444.11.11").unwrap_err();
        assert!(error.to_string().contains(message), "{error}");
    }
    let temp = copy_fixture();
    fs::create_dir_all(temp.path().join("common/defines")).unwrap();
    fs::write(
        temp.path().join("common/defines/override.lua"),
        "NDefines.NMilitary.BASE_MP_TO_MANPOWER = 0.5\n",
    )
    .unwrap();
    let error = VanillaWorld::load(temp.path(), "1444.11.11").unwrap_err();
    assert!(
        error.to_string().contains("unsupported Lua evaluation"),
        "{error}"
    );
    assert!(error.location().is_some());
}

#[test]
fn accepted_cultures_unions_and_current_owners_select_cultural_sources() {
    use augustus_core::{AffiliationRelation, EntityId};
    let loaded = VanillaWorld::load(fixture(), "1444.11.11").unwrap();
    let mut world = loaded.initial.world().clone();
    let north = loaded.initial.province_entity(1).unwrap();
    let alpha = loaded.initial.country_entity("AAA").unwrap();
    let beta = loaded.initial.country_entity("BBB").unwrap();
    assert_eq!(
        world.affiliations().relation("culture", north, beta),
        Some(AffiliationRelation::SameGroup)
    );
    assert_eq!(
        world.affiliations().relation("culture", beta, alpha),
        Some(AffiliationRelation::Accepted)
    );
    world.set_owner(north, alpha).unwrap();
    assert_eq!(
        world
            .attributes()
            .evaluate(&world, north, "tax_basis")
            .unwrap()
            .known_subtotal_micros,
        Some(2_656_250)
    );
    let temp = copy_fixture();
    let path = temp.path().join("history/countries/BBB - Beta.txt");
    fs::write(&path, fs::read_to_string(&path).unwrap() + "\n1440.1.1 = { add_accepted_culture = alpha }\n1450.1.1 = { remove_accepted_culture = alpha }\n").unwrap();
    let accepted = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert_eq!(
        accepted.province(1).unwrap().attributes["tax_basis"].known_subtotal_micros,
        Some(2_656_250)
    );
    let future = VanillaWorld::load(temp.path(), "1450.1.1").unwrap();
    assert!(
        future.initial.world().affiliations().0["culture"].acceptance[&EntityId(1)]
            .members
            .as_ref()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        future
            .initial
            .world()
            .affiliations()
            .relation("culture", EntityId(2), EntityId(1)),
        Some(AffiliationRelation::Accepted)
    );
    fs::write(
        temp.path().join("common/cultures/test.txt"),
        "first = { alpha = {} } second = { beta = {} }",
    )
    .unwrap();
    fs::write(
        &path,
        fs::read_to_string(&path).unwrap() + "\n1444.11.11 = { remove_accepted_culture = alpha }\n",
    )
    .unwrap();
    let foreign = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert_eq!(
        foreign.province(1).unwrap().attributes["tax_basis"].known_subtotal_micros,
        Some(1_955_000)
    );
}

#[test]
fn religion_scopes_apply_direct_effects_without_secondary_or_tolerance_shortcuts() {
    let temp = copy_fixture();
    fs::write(temp.path().join("common/religions/test.txt"), r#"
        group = {
            test_faith = {
                country = { global_tax_modifier = 0.10 tolerance_heathen = 2 }
                province = { local_tax_modifier = 0.05 local_manpower_modifier = 0.20 tax_income = 1 }
                country_as_secondary = { global_tax_modifier = 0.90 }
            }
            other = { country = { global_tax_modifier = 0.30 } }
        }
    "#).unwrap();
    let country = temp.path().join("history/countries/BBB - Beta.txt");
    fs::write(
        &country,
        fs::read_to_string(&country).unwrap() + "\nreligion = other\n",
    )
    .unwrap();
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let north = loaded.province(1).unwrap();
    // (2.125 + 1) * (1 + .25 - .15 + .05 + .30)
    assert_eq!(
        north.attributes["tax_basis"].known_subtotal_micros,
        Some(4_531_250)
    );
    assert_eq!(
        north.attributes["manpower_contribution"].known_subtotal_micros,
        Some(1_006_250_000)
    );
    assert!(
        north.attributes["tax_basis"]
            .unresolved
            .contains("tolerance_and_religious_mechanics")
    );
    assert_eq!(loaded.report.attributes.religion_definitions, 2);
    assert!(
        loaded
            .report
            .attributes
            .definition_sources
            .contains_key("religions:test_faith:province")
    );
    assert_eq!(
        loaded.report.attributes.unmapped_properties["religions.country:tolerance_heathen"],
        1
    );
    let mut world = loaded.initial.world().clone();
    let beta = loaded.initial.country_entity("BBB").unwrap();
    let province = loaded.initial.province_entity(1).unwrap();
    let mut affiliations = world.affiliations().clone();
    affiliations
        .0
        .get_mut("religion")
        .unwrap()
        .members
        .insert(beta, Some("test_faith".into()));
    world.set_affiliations(affiliations).unwrap();
    assert_eq!(
        world
            .attributes()
            .evaluate(&world, province, "tax_basis")
            .unwrap()
            .known_subtotal_micros,
        Some(3_906_250)
    );
}

#[test]
fn ambiguous_religion_effects_and_unknown_culture_members_remain_unresolved() {
    let temp = copy_fixture();
    fs::write(temp.path().join("common/religions/test.txt"), "group = { test_faith = { country = { global_tax_modifier = 0.1 global_tax_modifier = 0.2 } } }").unwrap();
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let tax = &loaded.province(1).unwrap().attributes["tax_basis"];
    assert_eq!(tax.known_subtotal_micros, Some(2_337_500));
    assert!(
        tax.unresolved
            .contains("affiliation:religion:country:test_faith:ambiguous:global_tax_modifier")
    );
    fs::write(
        temp.path().join("common/religions/duplicate.txt"),
        "group = { test_faith = { country = { global_tax_modifier = 0.5 } } }",
    )
    .unwrap();
    fs::write(
        temp.path().join("common/cultures/duplicate.txt"),
        "other = { alpha = {} }",
    )
    .unwrap();
    let duplicate = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let tax = &duplicate.province(1).unwrap().attributes["tax_basis"];
    assert_eq!(tax.known_subtotal_micros, Some(2_656_250));
    assert!(
        tax.unresolved
            .contains("affiliation:culture:unknown_relation")
    );
    assert!(
        tax.unresolved
            .contains("affiliation:religion:country:test_faith:missing_definition")
    );
    assert_eq!(
        duplicate.report.attributes.ambiguous_definitions["religions:test_faith"].len(),
        2
    );
    assert!(
        !duplicate
            .report
            .attributes
            .definition_sources
            .contains_key("religions:test_faith:country")
    );
}
