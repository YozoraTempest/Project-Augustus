// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use augustus_core::{
    EstateDefinition, EstateEffect, EstatePrivilege, EstateState, Estates,
    MONTHLY_AUTONOMY_DELTA_ATTRIBUTE, ModifierDefinition, ModifierInstance, World,
};
use augustus_eu4::{VanillaExternalState, VanillaWorld};

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
    assert_eq!(
        north.attributes["development"].value_micros,
        Some(7_625_000)
    );
    let tax = &north.attributes["tax_basis"];
    assert_eq!(tax.base_micros, Some(2_125_000));
    assert_eq!(tax.known_subtotal_micros, Some(212_000));
    assert_eq!(tax.value_micros, None);
    assert_eq!(tax.steps[0].rate_micros, -900_000);
    for (source, micros) in [
        ("metric:stability", 0),
        ("static:same_culture_group", -150_000),
        ("static:city", 250_000),
    ] {
        assert!(
            tax.steps[0]
                .contributions
                .iter()
                .any(|effect| effect.source == source && effect.micros == micros)
        );
    }
    assert_eq!(
        north.attributes["manpower_contribution"].known_subtotal_micros,
        Some(831_250_000)
    );
    assert_eq!(
        loaded.country("AAA").unwrap().province_attribute_totals["tax_basis"].known_subtotal_micros,
        1_100_000
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
    let entity = future.initial.province_entity(1).unwrap();
    assert!(
        future
            .initial
            .world()
            .has_modifier(entity, "test_harbor")
            .is_err()
    );
    assert!(!future.province(1).unwrap().modifier_attachments_known);
    assert!(
        future.province(1).unwrap().attributes["tax_basis"]
            .unresolved
            .contains(&format!(
                "modifier:{}:test_harbor:unknown_attachment",
                future.initial.world().entity(entity).unwrap().name
            ))
    );
    let temp = copy_fixture();
    let history = temp.path().join("history/provinces/1 - North.txt");
    let text = fs::read_to_string(&history).unwrap();
    fs::write(history, text + "\n1444.1.1 = { controller = AAA }\n").unwrap();
    let occupied = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert_eq!(
        occupied.province(1).unwrap().attributes["tax_basis"].known_subtotal_micros,
        Some(0)
    );
}

#[test]
fn development_attribute_stays_unknown_when_a_base_component_is_missing() {
    let root = copy_fixture();
    let path = root.path().join("history/provinces/1 - North.txt");
    let content = fs::read_to_string(&path).unwrap();
    let content = content.replace("base_manpower = 3.5", "");
    fs::write(path, content).unwrap();

    let loaded = VanillaWorld::load(root.path(), "1444.11.11").unwrap();
    let development = &loaded.province(1).unwrap().attributes["development"];
    assert!(development.value_micros.is_none());
    assert!(!development.unresolved.is_empty());
}

#[test]
fn monthly_autonomy_preserves_known_sources_when_war_status_is_unknown() {
    let loaded = VanillaWorld::load(fixture(), "1444.11.11").unwrap();
    let mut world = loaded.initial.world().clone();
    let country = loaded.initial.country_entity("AAA").unwrap();
    let province = loaded.initial.province_entity(2).unwrap();
    world.set_government_rank(country, Some(3)).unwrap();

    let mut modifiers = world.modifiers().clone();
    modifiers.definitions.insert(
        "test_national_autonomy".into(),
        ModifierDefinition {
            values_micros: [("global_autonomy".into(), -50_000)].into(),
            ..Default::default()
        },
    );
    modifiers.definitions.insert(
        "test_local_autonomy".into(),
        ModifierDefinition {
            values_micros: [("local_autonomy".into(), -100_000)].into(),
            ..Default::default()
        },
    );
    modifiers
        .active
        .entry(country)
        .or_default()
        .insert("test_national_autonomy".into(), ModifierInstance::default());
    modifiers
        .active
        .entry(province)
        .or_default()
        .insert("test_local_autonomy".into(), ModifierInstance::default());
    world.set_modifiers(modifiers).unwrap();

    let monthly = world
        .attributes()
        .evaluate(&world, province, MONTHLY_AUTONOMY_DELTA_ATTRIBUTE)
        .unwrap();
    assert_eq!(monthly.steps[0].flat_micros, -2_000);
    assert_eq!(monthly.value_micros, None);
    assert!(
        monthly
            .unresolved
            .iter()
            .any(|reason| reason.contains("unknown political status at_war"))
    );
    assert!(monthly.unresolved.contains("city_status"));
}

#[test]
fn external_country_statuses_resolve_live_political_conditions() {
    let state = VanillaExternalState {
        country_statuses: BTreeMap::from([(
            "AAA".into(),
            BTreeMap::from([("at_war".into(), Some(false))]),
        )]),
        ..Default::default()
    };
    let loaded = VanillaWorld::load_with_external_state(fixture(), "1444.11.11", &state).unwrap();
    let country = loaded.initial.country_entity("AAA").unwrap();
    let province = loaded.initial.province_entity(2).unwrap();
    assert_eq!(
        loaded.initial.world().politics().status(country, "at_war"),
        Some(false)
    );
    let monthly = loaded
        .initial
        .world()
        .attributes()
        .evaluate(
            loaded.initial.world(),
            province,
            MONTHLY_AUTONOMY_DELTA_ATTRIBUTE,
        )
        .unwrap();
    assert!(
        !monthly
            .unresolved
            .iter()
            .any(|reason| reason.contains("unknown political status at_war"))
    );
}

#[test]
fn monthly_autonomy_combines_rank_peace_and_estate_sources() {
    let loaded = VanillaWorld::load(fixture(), "1444.11.11").unwrap();
    let mut world = loaded.initial.world().clone();
    let country = loaded.initial.country_entity("AAA").unwrap();
    let province = loaded.initial.province_entity(2).unwrap();
    world.set_government_rank(country, Some(3)).unwrap();

    let mut politics = world.politics().clone();
    politics
        .countries
        .entry(country)
        .or_default()
        .statuses
        .insert("at_war".into(), Some(false));
    world.set_politics(politics).unwrap();

    let privilege = "test_autonomy_favor";
    let mut estates = Estates::default();
    estates.definitions.insert(
        "estate_nobles".into(),
        EstateDefinition {
            privileges: [privilege.into()].into_iter().collect(),
            ..Default::default()
        },
    );
    estates.privileges.insert(
        privilege.into(),
        EstatePrivilege {
            effect: EstateEffect {
                modifier: ModifierDefinition {
                    values_micros: [("global_autonomy".into(), 50_000)].into_iter().collect(),
                    ..Default::default()
                },
                ..Default::default()
            },
            ..Default::default()
        },
    );
    estates.countries.insert(
        country,
        [(
            "estate_nobles".into(),
            EstateState {
                active: Some(true),
                privileges: Some([privilege.into()].into_iter().collect()),
                ..Default::default()
            },
        )]
        .into_iter()
        .collect(),
    );
    world.set_estates(estates).unwrap();

    let monthly = world
        .attributes()
        .evaluate(&world, province, MONTHLY_AUTONOMY_DELTA_ATTRIBUTE)
        .unwrap();
    let contribution = |source: &str| {
        monthly
            .steps
            .iter()
            .flat_map(|step| &step.contributions)
            .find(|effect| effect.source == source && effect.property == "global_autonomy")
            .map(|effect| effect.micros)
    };

    let rank = contribution("government_rank:3").unwrap();
    let peace = contribution("static:peace").unwrap();
    let estate = contribution("estates:estate_nobles:privilege:test_autonomy_favor").unwrap();
    assert_eq!((rank, peace, estate), (-500, -1_000, 500));
    assert_eq!(rank + peace + estate, -1_000);
}

#[test]
fn monthly_settlement_applies_the_estate_global_autonomy_delta_to_province_state() {
    let build = |include_privilege| {
        let temp = copy_fixture();
        let country_history = temp.path().join("history/countries/AAA - Alpha.txt");
        let country = fs::read_to_string(&country_history).unwrap();
        fs::write(
            &country_history,
            country.replace("1440.1.1 = { capital = 2 }", "1440.1.1 = { capital = 1 }"),
        )
        .unwrap();
        let capital_history = temp.path().join("history/provinces/1 - North.txt");
        let history = fs::read_to_string(&capital_history).unwrap();
        fs::write(
            &capital_history,
            history.replace(
                "1440.1.1 = { owner = BBB controller = BBB remove_core = AAA }",
                "1440.1.1 = { owner = AAA controller = AAA }",
            ),
        )
        .unwrap();
        let province_history = temp.path().join("history/provinces/2 - East.txt");
        let history = fs::read_to_string(&province_history).unwrap();
        fs::write(&province_history, format!("{history}\nis_city = yes\n")).unwrap();

        let privilege = "estate_nobles_habsburg_disunity";
        let estate_dir = temp.path().join("common/estates");
        let privilege_dir = temp.path().join("common/estate_privileges");
        fs::create_dir_all(&estate_dir).unwrap();
        fs::create_dir_all(&privilege_dir).unwrap();
        fs::write(
            estate_dir.join("nobles.txt"),
            format!(
                "estate_nobles = {{ trigger = {{ tag = AAA }} base_influence = 10 privileges = {{ {privilege} }} }}"
            ),
        )
        .unwrap();
        fs::write(
            privilege_dir.join("nobles.txt"),
            format!(
                "{privilege} = {{ benefits = {{ global_autonomy = 0.05 }} is_valid = {{ always = yes }} on_granted = {{}} }}"
            ),
        )
        .unwrap();
        if include_privilege {
            let history = fs::read_to_string(&country_history).unwrap();
            fs::write(
                &country_history,
                format!("{history}\nset_estate_privilege = {privilege}\n"),
            )
            .unwrap();
        }

        let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
        let country = loaded.initial.country_entity("AAA").unwrap();
        let province = loaded.initial.province_entity(2).unwrap();
        let mut world = loaded.initial.world().clone();
        world.set_government_rank(country, Some(3)).unwrap();

        let mut politics = world.politics().clone();
        politics
            .countries
            .entry(country)
            .or_default()
            .statuses
            .insert("at_war".into(), Some(false));
        world.set_politics(politics).unwrap();
        assert_eq!(
            world.estates().countries[&country]["estate_nobles"]
                .privileges
                .as_ref()
                .unwrap()
                .contains(privilege),
            include_privilege
        );
        assert_eq!(
            world.estates().privileges[privilege]
                .effect
                .modifier
                .values_micros["global_autonomy"],
            50_000
        );

        let mut administration = world.administration().clone();
        administration
            .provinces
            .get_mut(&province)
            .unwrap()
            .raw_autonomy_micros = Some(35_000);
        world.set_administration(administration).unwrap();
        assert!(
            !world
                .administration()
                .evaluate(&world, province)
                .unwrap()
                .is_capital
        );
        let monthly = world
            .attributes()
            .evaluate(&world, province, MONTHLY_AUTONOMY_DELTA_ATTRIBUTE)
            .unwrap();
        assert_eq!(
            monthly.value_micros,
            Some(if include_privilege { -1_000 } else { -1_500 }),
            "unresolved={:?}, steps={:?}",
            monthly.unresolved,
            monthly.steps
        );
        (world, country, province)
    };

    let (mut without_privilege, country, province) = build(false);
    let (mut with_privilege, _, _) = build(true);
    let (mut resumed_with_privilege, _, _) = build(true);
    without_privilege.advance_to(51).unwrap();
    with_privilege.advance_to(51).unwrap();
    resumed_with_privilege.advance_to(20).unwrap();
    let snapshot = resumed_with_privilege
        .save("autonomy-tax-save-resume:1")
        .unwrap();
    resumed_with_privilege = World::restore(
        &snapshot,
        "autonomy-tax-save-resume:1",
        resumed_with_privilege.schema(),
    )
    .unwrap();
    resumed_with_privilege.advance_to(51).unwrap();
    assert_eq!(resumed_with_privilege, with_privilege);

    assert_eq!(
        without_privilege.administration().provinces[&province].raw_autonomy_micros,
        Some(32_000)
    );
    assert_eq!(
        with_privilege.administration().provinces[&province].raw_autonomy_micros,
        Some(33_000)
    );
    let without_tax = without_privilege
        .taxation()
        .unwrap()
        .last_settlement
        .as_ref()
        .unwrap();
    let with_tax = with_privilege
        .taxation()
        .unwrap()
        .last_settlement
        .as_ref()
        .unwrap();
    assert_eq!(with_tax.day, 51);
    assert!(with_tax.accounts.contains_key(&country));
    let without_effective_autonomy = without_privilege
        .administration()
        .evaluate(&without_privilege, province)
        .unwrap()
        .effective_autonomy_micros;
    let with_effective_autonomy = with_privilege
        .administration()
        .evaluate(&with_privilege, province)
        .unwrap()
        .effective_autonomy_micros;
    let without_province_tax_micros = without_privilege
        .attributes()
        .evaluate(&without_privilege, province, "tax_after_autonomy")
        .unwrap()
        .known_subtotal_micros
        .expect("known provincial tax subtotal");
    let with_province_tax_micros = with_privilege
        .attributes()
        .evaluate(&with_privilege, province, "tax_after_autonomy")
        .unwrap()
        .known_subtotal_micros
        .expect("known provincial tax subtotal");
    let without_country_tax_micros = without_privilege
        .attributes()
        .sum_owned(&without_privilege, country, "tax_after_autonomy")
        .unwrap()
        .known_subtotal_micros;
    let with_country_tax_micros = with_privilege
        .attributes()
        .sum_owned(&with_privilege, country, "tax_after_autonomy")
        .unwrap()
        .known_subtotal_micros;
    assert!(
        with_province_tax_micros < without_province_tax_micros,
        "higher core-generated autonomy should reduce the tax subtotal: with={with_province_tax_micros}, without={without_province_tax_micros}, effective=({with_effective_autonomy:?}, {without_effective_autonomy:?}), monthly=({}, {})",
        with_tax.accounts[&country].known_subtotal_milli,
        without_tax.accounts[&country].known_subtotal_milli
    );
    assert_eq!(
        with_tax.accounts[&country].known_subtotal_milli,
        with_country_tax_micros / 12_000
    );
    assert_eq!(
        without_tax.accounts[&country].known_subtotal_milli,
        without_country_tax_micros / 12_000
    );
}

#[test]
fn monthly_autonomy_selects_war_or_peace_modifier_from_dated_war_history() {
    let temp = copy_fixture();
    let war_history = temp.path().join("history/wars/test_war.txt");
    fs::create_dir_all(war_history.parent().unwrap()).unwrap();
    fs::write(
        war_history,
        "1444.1.1 = { add_attacker = AAA add_defender = BBB }\n1444.12.1 = { rem_attacker = AAA rem_defender = BBB }\n",
    )
    .unwrap();
    let static_modifiers = temp.path().join("common/static_modifiers/test_static.txt");
    let text = fs::read_to_string(&static_modifiers).unwrap();
    assert!(text.contains("war = { global_autonomy = 0 }"));
    fs::write(
        &static_modifiers,
        text.replace(
            "war = { global_autonomy = 0 }",
            "war = { global_autonomy = 0.2 }",
        ),
    )
    .unwrap();

    let at_war = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let aaa = at_war.initial.country_entity("AAA").unwrap();
    let bbb = at_war.initial.country_entity("BBB").unwrap();
    assert_eq!(
        at_war.initial.world().politics().status(aaa, "at_war"),
        Some(true)
    );
    assert_eq!(
        at_war.initial.world().politics().status(bbb, "at_war"),
        Some(true)
    );
    let province = at_war.initial.province_entity(2).unwrap();
    let monthly = at_war
        .initial
        .world()
        .attributes()
        .evaluate(
            at_war.initial.world(),
            province,
            MONTHLY_AUTONOMY_DELTA_ATTRIBUTE,
        )
        .unwrap();
    assert!(
        monthly.steps[0].contributions.iter().any(|effect| {
            effect.source == "static:war" && effect.property == "global_autonomy"
        })
    );

    let at_peace = VanillaWorld::load(temp.path(), "1444.12.1").unwrap();
    let aaa = at_peace.initial.country_entity("AAA").unwrap();
    assert_eq!(
        at_peace.initial.world().politics().status(aaa, "at_war"),
        Some(false)
    );
    let province = at_peace.initial.province_entity(2).unwrap();
    let monthly = at_peace
        .initial
        .world()
        .attributes()
        .evaluate(
            at_peace.initial.world(),
            province,
            MONTHLY_AUTONOMY_DELTA_ATTRIBUTE,
        )
        .unwrap();
    assert!(
        monthly.steps[0].contributions.iter().any(|effect| {
            effect.source == "static:peace" && effect.property == "global_autonomy"
        })
    );
}

#[test]
fn translated_core_effects_join_direct_tax_efficiency_before_autonomy() {
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
    assert_eq!(tax.steps.len(), 2);
    assert_eq!(tax.steps[0].output_micros, Some(637_500));
    assert_eq!(tax.steps[1].rate_micros, 0);
    assert_eq!(tax.steps[1].output_micros, Some(637_000));
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
    assert_eq!(changed.known_subtotal_micros, Some(212_000));
}

#[test]
fn fort_buildings_keep_their_role_but_skip_standard_province_attributes() {
    use augustus_core::BuildingRole;

    let temp = copy_fixture();
    let path = temp.path().join("common/buildings/test_buildings.txt");
    let text = fs::read_to_string(&path).unwrap();
    let updated = text.replace(
        "test_fort = { cost = 200 time = 18 }",
        "test_fort = { cost = 200 time = 18 influencing_fort = yes modifier = { fort_level = 3 tax_income = 7 local_tax_modifier = 0.5 } }",
    );
    assert_ne!(updated, text);
    fs::write(path, updated).unwrap();

    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let fort = &loaded.initial.world().economy().buildings["test_fort"];
    assert_eq!(fort.role, BuildingRole::Fortification);
    assert_eq!(fort.fort_level_milli, Some(3_000));
    assert!(
        !loaded
            .report
            .economy
            .unapplied_definition_fields
            .contains_key("buildings:influencing_fort")
    );
    assert!(
        !loaded
            .report
            .attributes
            .unmapped_properties
            .contains_key("buildings:fort_level")
    );

    let province = loaded.initial.province_entity(1).unwrap();
    let tax = loaded
        .initial
        .world()
        .attributes()
        .evaluate(loaded.initial.world(), province, "tax_basis")
        .unwrap();
    assert_eq!(tax.known_subtotal_micros, Some(212_000));
    assert!(
        tax.steps[0]
            .contributions
            .iter()
            .all(|contribution| contribution.source != "building:test_fort")
    );

    let save = loaded.save().unwrap();
    let mut restored = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    restored.restore(&save).unwrap();
    assert_eq!(
        restored.initial.world().economy().buildings["test_fort"],
        *fort
    );
}

#[test]
fn non_fort_buildings_remain_in_standard_province_attributes() {
    use augustus_core::BuildingRole;

    let temp = copy_fixture();
    let path = temp.path().join("common/buildings/test_buildings.txt");
    let text = fs::read_to_string(&path).unwrap();
    let updated = text.replace(
        "test_fort = { cost = 200 time = 18 }",
        "test_fort = { cost = 200 time = 18 influencing_fort = no modifier = { fort_level = 3 tax_income = 7 local_tax_modifier = 0.5 } }",
    );
    assert_ne!(updated, text);
    fs::write(path, updated).unwrap();

    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert_eq!(
        loaded.initial.world().economy().buildings["test_fort"].role,
        BuildingRole::Standard
    );
    let province = loaded.initial.province_entity(1).unwrap();
    let tax = loaded
        .initial
        .world()
        .attributes()
        .evaluate(loaded.initial.world(), province, "tax_basis")
        .unwrap();
    assert!(tax.steps[0].contributions.iter().any(|contribution| {
        contribution.source == "building:test_fort" && contribution.property == "tax_income"
    }));
    assert!(tax.steps[0].contributions.iter().any(|contribution| {
        contribution.source == "building:test_fort" && contribution.property == "local_tax_modifier"
    }));
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
    assert_eq!(query(&world).known_subtotal_micros, Some(212_000));
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
    assert_eq!(tax.known_subtotal_micros, Some(212_000));
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
    assert_eq!(tax.known_subtotal_micros, Some(0));
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
    assert_eq!(tax.known_subtotal_micros, Some(600_000));
    assert!(
        tax.unresolved
            .contains("ambiguous:buildings:test_market:local_tax_modifier")
    );
}

#[test]
fn invalid_define_values_and_attribute_shapes_are_rejected() {
    for (path, from, to, message) in [
        (
            "common/defines.lua",
            "0.25,",
            "-0.25,",
            "nonnegative define",
        ),
        (
            "common/defines.lua",
            "BASE_MP_TO_MANPOWER = 0.25,",
            "BASE_MP_TO_MANPOWER = '0.25',",
            "numeric define",
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
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert_eq!(
        loaded.report.attributes.manpower_per_base_micros,
        500_000_000
    );
    assert!(
        loaded.report.attributes.definition_sources["define:manpower_per_base"]
            .path
            .ends_with("common/defines/override.lua")
    );
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
        Some(531_000)
    );
    let temp = copy_fixture();
    let path = temp.path().join("history/countries/BBB - Beta.txt");
    fs::write(&path, fs::read_to_string(&path).unwrap() + "\n1440.1.1 = { add_accepted_culture = alpha }\n1450.1.1 = { remove_accepted_culture = alpha }\n").unwrap();
    let accepted = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert_eq!(
        accepted.province(1).unwrap().attributes["tax_basis"].known_subtotal_micros,
        Some(531_000)
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
        Some(0)
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
    // (2.125 + 1) * (.25 - .15 + .05 + .30)
    assert_eq!(
        north.attributes["tax_basis"].known_subtotal_micros,
        Some(1_406_000)
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
        Some(781_000)
    );
}

#[test]
fn ambiguous_religion_effects_and_unknown_culture_members_remain_unresolved() {
    let temp = copy_fixture();
    fs::write(temp.path().join("common/religions/test.txt"), "group = { test_faith = { country = { global_tax_modifier = 0.1 global_tax_modifier = 0.2 } } }").unwrap();
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let tax = &loaded.province(1).unwrap().attributes["tax_basis"];
    assert_eq!(tax.known_subtotal_micros, Some(212_000));
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
    assert_eq!(tax.known_subtotal_micros, Some(531_000));
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
