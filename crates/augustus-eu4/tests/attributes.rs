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
use augustus_eu4::{
    ContentRoot, ContentSet, ContentSource, VanillaExternalState, VanillaWorld,
    read_vanilla_save_state, read_vanilla_save_state_with_state_edicts,
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

fn load_state_edict(identifier: Option<&str>) -> (tempfile::TempDir, VanillaWorld) {
    let temp = copy_fixture();
    let state_edicts = temp.path().join("common/state_edicts");
    fs::create_dir_all(&state_edicts).unwrap();
    fs::write(
        state_edicts.join("test.txt"),
        r#"edict_test = {
    allow = { always = yes }
    ai_will_do = { factor = 1 }
    modifier = { local_development_cost = -0.1 }
}
"#,
    )
    .unwrap();
    let active_edict = identifier
        .map(|identifier| format!("active_edict = {{ which={identifier} date=1444.11.11 }}"))
        .unwrap_or_default();
    let save_path = temp.path().join("state-edict.eu4");
    let save = r#"EU4txt
date=1444.11.11
map_area_data={
    north_area={
        state={
            area=north_area
            country_state={
                country=AAA
                ACTIVE_EDICT
            }
        }
    }
}
countries={ AAA={} BBB={} }
provinces={ -1={ owner=AAA } -2={ owner=BBB } }
"#
    .replace("ACTIVE_EDICT", &active_edict);
    fs::write(&save_path, save).unwrap();
    let parsed_save = read_vanilla_save_state_with_state_edicts(&save_path).unwrap();
    let content = ContentSet::resolve(vec![ContentSource::base(
        ContentRoot::open(temp.path()).unwrap(),
    )])
    .unwrap();
    let loaded = VanillaWorld::load_from_content_with_state_edicts(&content, &parsed_save).unwrap();
    (temp, loaded)
}

#[test]
fn saved_state_edict_becomes_a_modifier_on_its_current_state_provinces() {
    let (_temp, loaded) = load_state_edict(Some("edict_test"));
    let north = loaded.province(1).unwrap();
    assert_eq!(
        north.attributes["local_development_cost_modifier"].value_micros,
        Some(-100_000)
    );
    assert!(north.modifiers.contains(&"state_edict:edict_test"));
    assert_eq!(
        loaded.province(2).unwrap().attributes["local_development_cost_modifier"].value_micros,
        Some(0)
    );
    assert!(
        loaded
            .report
            .setup
            .definition_sources
            .contains_key("state_edict:edict_test")
    );
    assert_eq!(
        loaded.report.setup.unapplied_definition_fields["state_edict:edict_test:allow"],
        1
    );
    assert_eq!(
        loaded.report.setup.unapplied_definition_fields["state_edict:edict_test:ai_will_do"],
        1
    );
    let allow_source = &loaded.report.setup.unapplied_examples["state_edict:edict_test:allow"];
    assert!(allow_source.path.ends_with("common/state_edicts/test.txt"));
    assert_eq!(allow_source.line, 2);
}

#[test]
fn absent_state_edict_is_not_filled_with_a_default() {
    let (_temp, loaded) = load_state_edict(None);
    let north = loaded.province(1).unwrap();
    assert_eq!(
        north.attributes["local_development_cost_modifier"].value_micros,
        Some(0)
    );
    assert!(
        !north
            .modifiers
            .iter()
            .any(|name| name.starts_with("state_edict:"))
    );
}

#[test]
fn unknown_saved_state_edict_keeps_its_modifier_unresolved() {
    let (_temp, loaded) = load_state_edict(Some("edict_missing"));
    let north = loaded.province(1).unwrap();
    assert_eq!(
        north.attributes["local_development_cost_modifier"].value_micros,
        None
    );
    assert!(!north.modifier_attachments_known);
    assert_eq!(
        loaded.report.setup.unapplied_definition_fields["state_edict:edict_missing:unknown_active_definition"],
        1
    );
}

#[test]
fn legacy_save_reader_does_not_require_state_edict_fields() {
    let temp = tempfile::tempdir().unwrap();
    let save_path = temp.path().join("incomplete-state-edict.eu4");
    fs::write(
        &save_path,
        r#"EU4txt
date=1444.11.11
map_area_data={ north_area={ state={ area=north_area country_state={ country=AAA active_edict={ which=edict_test } } } } }
countries={ AAA={} BBB={} }
provinces={ -1={ owner=AAA } -2={ owner=BBB } }
"#,
    )
    .unwrap();

    assert!(read_vanilla_save_state(&save_path).is_ok());
    assert!(read_vanilla_save_state_with_state_edicts(&save_path).is_err());
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
fn country_trade_steering_combines_idea_and_metric_sources_but_stays_incomplete() {
    let temp = copy_fixture();
    let ideas_path = temp.path().join("common/ideas/test_ideas.txt");
    let ideas = fs::read_to_string(&ideas_path).unwrap();
    let updated_ideas = ideas.replace(
        "start = { test_tradition = 0.125 }",
        "start = { test_tradition = 0.125 trade_steering = 0.25 }",
    );
    assert_ne!(updated_ideas, ideas);
    fs::write(ideas_path, updated_ideas).unwrap();

    let static_modifiers_path = temp.path().join("common/static_modifiers/test_static.txt");
    let static_modifiers = fs::read_to_string(&static_modifiers_path).unwrap();
    fs::write(
        static_modifiers_path,
        format!("{static_modifiers}\nnavy_tradition = {{ trade_steering = 1.0 }}\n"),
    )
    .unwrap();

    let external = VanillaExternalState {
        country_metric_values: BTreeMap::from([(
            "AAA".into(),
            BTreeMap::from([("navy_tradition".into(), Some(8_852))]),
        )]),
        ..Default::default()
    };

    let loaded =
        VanillaWorld::load_with_external_state(temp.path(), "1444.11.11", &external).unwrap();
    let world = loaded.initial.world();
    let country = loaded.initial.country_entity("AAA").unwrap();
    let steering = world
        .attributes()
        .evaluate(world, country, "country_trade_steering")
        .unwrap();

    assert_eq!(steering.known_subtotal_micros, Some(338_520));
    assert_eq!(steering.value_micros, None);
    assert!(
        steering
            .unresolved
            .contains("country_trade_steering:remaining_sources_not_bound")
    );
    assert!(steering.steps.iter().any(|step| {
        step.contributions.iter().any(|contribution| {
            contribution.source == "ideas:traditions:test_alpha"
                && contribution.property == "trade_steering"
                && contribution.micros == 250_000
        })
    }));
    assert!(steering.steps.iter().any(|step| {
        step.contributions.iter().any(|contribution| {
            contribution.source == "metric:navy_tradition"
                && contribution.property == "trade_steering"
                && contribution.micros == 88_520
        })
    }));
}

#[test]
fn country_fort_maintenance_modifier_reads_known_idea_value_and_keeps_unknown_sources() {
    let temp = copy_fixture();
    let ideas_path = temp.path().join("common/ideas/test_ideas.txt");
    let ideas = fs::read_to_string(&ideas_path).unwrap();
    let updated_ideas = ideas.replace(
        "start = { test_tradition = 0.125 }",
        "start = { test_tradition = 0.125 fort_maintenance_modifier = -0.2 }",
    );
    assert_ne!(updated_ideas, ideas);
    fs::write(ideas_path, updated_ideas).unwrap();

    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let modifiers = loaded.country_attributes("AAA").unwrap();
    let fort = &modifiers["fort_maintenance_modifier"];

    assert_eq!(fort.known_subtotal_micros, Some(-200_000));
    assert_eq!(fort.value_micros, None);
    assert!(
        fort.unresolved
            .contains("fort_maintenance_modifier:remaining_sources_not_bound")
    );
    assert!(fort.steps.iter().any(|step| {
        step.contributions.iter().any(|contribution| {
            contribution.source == "ideas:traditions:test_alpha"
                && contribution.property == "fort_maintenance_modifier"
                && contribution.micros == -200_000
        })
    }));
}

#[test]
fn country_force_limit_modifier_sources_are_exposed_without_claiming_a_complete_total() {
    let temp = copy_fixture();
    let ideas_path = temp.path().join("common/ideas/test_ideas.txt");
    let ideas = fs::read_to_string(&ideas_path).unwrap();
    let updated_ideas = ideas.replace(
        "start = { test_tradition = 0.125 }",
        "start = { test_tradition = 0.125 land_forcelimit = 2 naval_forcelimit = 3 land_forcelimit_modifier = 0.25 naval_forcelimit_modifier = -0.1 }",
    );
    assert_ne!(updated_ideas, ideas);
    fs::write(ideas_path, updated_ideas).unwrap();

    let static_modifiers_path = temp.path().join("common/static_modifiers/test_static.txt");
    let static_modifiers = fs::read_to_string(&static_modifiers_path).unwrap();
    fs::write(
        static_modifiers_path,
        format!(
            "{static_modifiers}\ntest_force_limit = {{ land_forcelimit = 1 naval_forcelimit = 1 land_forcelimit_modifier = 0.1 naval_forcelimit_modifier = 0.1 }}\n"
        ),
    )
    .unwrap();

    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let attributes = loaded.country_attributes("AAA").unwrap();
    for (name, property, expected) in [
        ("country_land_forcelimit_flat", "land_forcelimit", 2_000_000),
        (
            "country_naval_forcelimit_flat",
            "naval_forcelimit",
            3_000_000,
        ),
        (
            "country_land_forcelimit_modifier",
            "land_forcelimit_modifier",
            250_000,
        ),
        (
            "country_naval_forcelimit_modifier",
            "naval_forcelimit_modifier",
            -100_000,
        ),
    ] {
        let attribute = &attributes[name];
        assert_eq!(attribute.known_subtotal_micros, Some(expected), "{name}");
        assert_eq!(attribute.value_micros, None, "{name}");
        assert!(
            attribute
                .unresolved
                .contains(&format!("{name}:remaining_sources_not_bound")),
            "{name}"
        );
        assert!(attribute.steps.iter().any(|step| {
            step.contributions.iter().any(|contribution| {
                contribution.source == "ideas:traditions:test_alpha"
                    && contribution.property == property
                    && contribution.micros == expected
            })
        }));
    }

    for property in [
        "land_forcelimit",
        "naval_forcelimit",
        "land_forcelimit_modifier",
        "naval_forcelimit_modifier",
    ] {
        assert!(
            loaded
                .report
                .attributes
                .unmapped_properties
                .contains_key(&format!("static_modifiers:{property}")),
            "{property}"
        );
    }
}

#[test]
fn province_force_limit_raw_sources_include_development_buildings_and_owned_totals() {
    let temp = copy_fixture();
    let static_path = temp.path().join("common/static_modifiers/test_static.txt");
    let static_modifiers = fs::read_to_string(&static_path).unwrap();
    fs::write(
        static_path,
        format!(
            "{static_modifiers}\ndevelopment = {{ land_forcelimit = 0.1 naval_forcelimit = 0.1 }}\n"
        ),
    )
    .unwrap();

    let buildings_path = temp.path().join("common/buildings/test_buildings.txt");
    let buildings = fs::read_to_string(&buildings_path).unwrap();
    fs::write(
        buildings_path,
        format!(
            "{buildings}\ntest_land_force = {{ cost = 100 time = 12 modifier = {{ land_forcelimit = 1 }} }}\ntest_naval_force = {{ cost = 100 time = 12 build_trigger = {{ has_port = yes }} modifier = {{ naval_forcelimit = 2 }} }}\n"
        ),
    )
    .unwrap();

    let province_path = temp.path().join("history/provinces/1 - North.txt");
    let province = fs::read_to_string(&province_path).unwrap();
    fs::write(
        province_path,
        format!("{province}\n1441.1.1 = {{ test_land_force = yes test_naval_force = yes }}\n"),
    )
    .unwrap();

    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let world = loaded.initial.world();
    let province = loaded.initial.province_entity(1).unwrap();
    for (name, property, expected, required_unknown) in [
        (
            "province_land_forcelimit_local_raw",
            "land_forcelimit",
            1_762_500,
            "province_land_forcelimit_local_raw:autonomy_not_applied",
        ),
        (
            "province_naval_forcelimit_local_raw",
            "naval_forcelimit",
            2_762_500,
            "province_naval_forcelimit_local_raw:port_gate_not_applied",
        ),
    ] {
        let attribute = world.attributes().evaluate(world, province, name).unwrap();
        assert_eq!(attribute.known_subtotal_micros, Some(expected), "{name}");
        assert_eq!(attribute.value_micros, None, "{name}");
        assert!(attribute.unresolved.contains(required_unknown), "{name}");
        assert!(attribute.steps.iter().any(|step| {
            step.contributions.iter().any(|contribution| {
                (contribution.source == "building:test_land_force"
                    || contribution.source == "building:test_naval_force")
                    && contribution.property == property
            })
        }));
        assert!(attribute.steps.iter().any(|step| {
            step.contributions.iter().any(|contribution| {
                contribution.source == format!("province_development:{property}")
                    && contribution.property == "composition_adjustment"
            })
        }));
    }

    let mut coastal_world = world.clone();
    coastal_world.set_province_has_port(province, true).unwrap();
    let autonomy = coastal_world
        .administration()
        .evaluate(&coastal_world, province)
        .unwrap()
        .effective_autonomy_micros
        .unwrap();
    for (raw_name, local_name) in [
        (
            "province_land_forcelimit_local_raw",
            "province_land_forcelimit_local",
        ),
        (
            "province_naval_forcelimit_local_raw",
            "province_naval_forcelimit_local",
        ),
    ] {
        let raw = coastal_world
            .attributes()
            .evaluate(&coastal_world, province, raw_name)
            .unwrap()
            .known_subtotal_micros
            .unwrap();
        let local = coastal_world
            .attributes()
            .evaluate(&coastal_world, province, local_name)
            .unwrap();
        let expected = (i128::from(raw) * i128::from(1_000_000 - autonomy) / 1_000_000) as i64;
        assert_eq!(local.known_subtotal_micros, Some(expected), "{local_name}");
        assert_eq!(local.value_micros, None, "{local_name}");
        assert!(
            local
                .unresolved
                .contains(&format!("{local_name}:remaining_local_sources_not_bound"))
        );
    }

    let mut inland_world = coastal_world.clone();
    inland_world.set_province_has_port(province, false).unwrap();
    let inland_naval = inland_world
        .attributes()
        .evaluate(&inland_world, province, "province_naval_forcelimit_local")
        .unwrap();
    assert_eq!(inland_naval.known_subtotal_micros, Some(0));
    assert_eq!(inland_naval.value_micros, None);

    let country = loaded.initial.country_entity("AAA").unwrap();
    for name in [
        "province_land_forcelimit_local_raw",
        "province_naval_forcelimit_local_raw",
    ] {
        let total = world.attributes().sum_owned(world, country, name).unwrap();
        let expected: i64 = loaded
            .initial
            .owned_provinces("AAA")
            .into_iter()
            .map(|id| {
                world
                    .attributes()
                    .evaluate(world, loaded.initial.province_entity(id).unwrap(), name)
                    .unwrap()
                    .known_subtotal_micros
                    .unwrap()
            })
            .sum();
        assert_eq!(total.known_subtotal_micros, expected, "{name}");
    }

    let direct = world
        .attributes()
        .evaluate(world, country, "country_land_forcelimit_flat")
        .unwrap();
    let province_total = world
        .attributes()
        .sum_owned(world, country, "province_land_forcelimit_local")
        .unwrap();
    let combined = loaded.country_attributes("AAA").unwrap();
    let expected = direct.known_subtotal_micros.map(|value| {
        value
            .checked_add(province_total.known_subtotal_micros)
            .unwrap()
    });
    assert_eq!(
        combined["country_land_forcelimit_flat"].known_subtotal_micros,
        expected
    );
    assert!(
        combined["country_land_forcelimit_flat"]
            .steps
            .iter()
            .any(|step| {
                step.contributions.iter().any(|contribution| {
                    contribution.source == "owned_provinces"
                        && contribution.property == "province_land_forcelimit_local"
                        && contribution.micros == province_total.known_subtotal_micros
                })
            })
    );
}

#[test]
fn province_local_fort_maintenance_modifier_reads_building_value_and_keeps_unknown_sources() {
    let temp = copy_fixture();
    let buildings_path = temp.path().join("common/buildings/test_buildings.txt");
    let buildings = fs::read_to_string(&buildings_path).unwrap();
    fs::write(
        buildings_path,
        format!(
            "{buildings}\ntest_fort_support = {{ cost = 50 time = 12 modifier = {{ local_fort_maintenance_modifier = 0.25 }} }}\n"
        ),
    )
    .unwrap();

    let history_path = temp.path().join("history/provinces/1 - North.txt");
    let history = fs::read_to_string(&history_path).unwrap();
    fs::write(
        history_path,
        format!("{history}\n1439.1.1 = {{ test_fort_support = yes }}\n"),
    )
    .unwrap();

    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let fort = &loaded.province(1).unwrap().attributes["local_fort_maintenance_modifier"];

    assert_eq!(fort.known_subtotal_micros, Some(250_000));
    assert_eq!(fort.value_micros, None);
    assert!(
        fort.unresolved
            .contains("local_fort_maintenance_modifier:remaining_sources_not_bound")
    );
    assert!(fort.steps.iter().any(|step| {
        step.contributions.iter().any(|contribution| {
            contribution.source == "building:test_fort_support"
                && contribution.property == "local_fort_maintenance_modifier"
                && contribution.micros == 250_000
        })
    }));
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
    assert!(
        !loaded
            .report
            .attributes
            .unmapped_properties
            .contains_key("religions.country:tolerance_heathen")
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
fn religious_tolerance_scales_base_and_legitimacy_by_live_religion_relation() {
    let temp = copy_fixture();
    let static_dir = temp.path().join("common/static_modifiers");
    let modifier_path = static_dir.join("test_governments.txt");
    let source = fs::read_to_string(&modifier_path).unwrap();
    let revised = source.replace(
        "legitimacy = { test_authority = 0.75 }",
        "legitimacy = { test_authority = 0.75 tolerance_heathen = 2 }",
    );
    assert_ne!(source, revised);
    fs::write(modifier_path, revised).unwrap();

    let base_values_path = static_dir.join("test_static.txt");
    let base_values_source = fs::read_to_string(&base_values_path).unwrap();
    let base_values_with_tolerance = base_values_source.replace(
        "base_values = { global_manpower = 8 }",
        "base_values = { global_manpower = 8 tolerance_own = 3 tolerance_heretic = -2 tolerance_heathen = -3 }",
    );
    assert_ne!(base_values_source, base_values_with_tolerance);
    let base_values_revised = base_values_with_tolerance.replace(
        "occupied = { local_tax_modifier = -0.5 local_manpower_modifier = -0.5 }",
        "occupied = { local_tax_modifier = -0.5 local_manpower_modifier = -0.5 trade_goods_size_modifier = -0.5 }",
    );
    assert_ne!(base_values_with_tolerance, base_values_revised);
    fs::write(base_values_path, base_values_revised).unwrap();

    let effects_path = static_dir.join("religious_tolerance.txt");
    let write_effects = |tolerance_tax: &str, intolerance_tax: &str| {
        fs::write(
            &effects_path,
            format!(
                "tolerance = {{ local_tax_modifier = {tolerance_tax} trade_goods_size_modifier = 0.05 }}\n\
                 intolerance = {{ local_tax_modifier = {intolerance_tax} trade_goods_size_modifier = 0.1 }}\n"
            ),
        )
        .unwrap();
    };
    write_effects("0", "0");

    fs::write(
        temp.path().join("common/religions/religious_tolerance.txt"),
        "christian = { state_faith = { country = { tolerance_own = 1 tolerance_heathen = 0.5 } } heretic_faith = {} } heathen = { heathen_faith = {} }",
    )
    .unwrap();
    let country_history = temp.path().join("history/countries/BBB - Beta.txt");
    fs::create_dir_all(country_history.parent().unwrap()).unwrap();
    fs::write(
        &country_history,
        "government = monarchy capital = 1 primary_culture = beta religion = state_faith",
    )
    .unwrap();
    let province_history = temp.path().join("history/provinces/1 - North.txt");
    let province_source = fs::read_to_string(&province_history).unwrap();

    let set_province_religion = |religion: &str| {
        let updated =
            province_source.replace("religion = test_faith", &format!("religion = {religion}"));
        assert_ne!(province_source, updated);
        fs::write(&province_history, updated).unwrap();
    };
    let load = || VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let tax_rate = |loaded: &VanillaWorld| {
        loaded.province(1).unwrap().attributes["tax_basis"].steps[0].rate_micros
    };
    let trade_goods_size_modifier = |loaded: &VanillaWorld| {
        let world = loaded.initial.world();
        let province = loaded.initial.province_entity(1).unwrap();
        world
            .attributes()
            .evaluate(world, province, "trade_goods_size_modifier")
            .unwrap()
    };

    set_province_religion("heathen_faith");
    let heathen_baseline = tax_rate(&load());
    write_effects("0.05", "0.1");
    let heathen = load();
    let country_entity = heathen.initial.country_entity("BBB").unwrap();
    let country_tolerance = heathen
        .initial
        .world()
        .attributes()
        .evaluate(heathen.initial.world(), country_entity, "tolerance_heathen")
        .unwrap();
    assert_eq!(
        tax_rate(&heathen) - heathen_baseline,
        -150_000,
        "tolerance effects must use the current province and owner religions"
    );
    assert_eq!(country_tolerance.known_subtotal_micros, Some(-1_500_000));
    assert!(country_tolerance.value_micros.is_none());
    let heathen_goods_size = trade_goods_size_modifier(&heathen);
    assert_eq!(heathen_goods_size.known_subtotal_micros, Some(-150_000));
    assert!(heathen_goods_size.value_micros.is_none());
    assert!(
        !heathen
            .report
            .attributes
            .unmapped_properties
            .contains_key("static_modifiers:trade_goods_size_modifier")
    );

    set_province_religion("heretic_faith");
    write_effects("0", "0");
    let heretic_baseline = tax_rate(&load());
    write_effects("0.05", "0.1");
    let heretic = load();
    let heretic_country = heretic.initial.country_entity("BBB").unwrap();
    let heretic_province = heretic.initial.province_entity(1).unwrap();
    assert_eq!(
        heretic.initial.world().affiliations().relation(
            "religion",
            heretic_province,
            heretic_country,
        ),
        Some(augustus_core::AffiliationRelation::SameGroup)
    );
    assert_eq!(
        heretic
            .initial
            .world()
            .attributes()
            .evaluate(
                heretic.initial.world(),
                heretic_country,
                "tolerance_heretic",
            )
            .unwrap()
            .known_subtotal_micros,
        Some(-2_000_000)
    );
    assert_eq!(tax_rate(&heretic) - heretic_baseline, -200_000);
    assert_eq!(
        trade_goods_size_modifier(&heretic).known_subtotal_micros,
        Some(-200_000)
    );

    set_province_religion("state_faith");
    write_effects("0", "0");
    let own_baseline = tax_rate(&load());
    write_effects("0.05", "0.1");
    let own = load();
    assert_eq!(tax_rate(&own) - own_baseline, 200_000);
    assert_eq!(
        trade_goods_size_modifier(&own).known_subtotal_micros,
        Some(200_000)
    );

    let mut global_world = own.initial.world().clone();
    let owner = own.initial.country_entity("BBB").unwrap();
    let province = own.initial.province_entity(1).unwrap();
    let mut modifiers = global_world.modifiers().clone();
    modifiers.definitions.insert(
        "test_global_goods_size".into(),
        ModifierDefinition {
            values_micros: [("global_trade_goods_size_modifier".into(), 250_000)].into(),
            ..Default::default()
        },
    );
    modifiers
        .active
        .entry(owner)
        .or_default()
        .insert("test_global_goods_size".into(), ModifierInstance::default());
    global_world.set_modifiers(modifiers).unwrap();
    assert_eq!(
        global_world
            .attributes()
            .evaluate(&global_world, province, "trade_goods_size_modifier")
            .unwrap()
            .known_subtotal_micros,
        Some(450_000),
        "country-wide modifiers add to the province-local goods-size modifier"
    );

    let occupied_source = province_source
        .replace("controller = BBB", "controller = AAA")
        .replace("religion = test_faith", "religion = state_faith");
    assert_ne!(province_source, occupied_source);
    fs::write(&province_history, occupied_source).unwrap();
    let occupied = load();
    let occupied_goods_size = trade_goods_size_modifier(&occupied);
    assert_eq!(
        occupied_goods_size.known_subtotal_micros,
        Some(-300_000),
        "owner={:?}, controller={:?}, evaluation={occupied_goods_size:#?}",
        occupied.province(1).unwrap().state.owner,
        occupied.province(1).unwrap().state.controller
    );
}

#[test]
fn devastation_scales_trade_goods_size_before_quantity_calculation() {
    let temp = copy_fixture();
    let static_path = temp.path().join("common/static_modifiers/test_static.txt");
    let source = fs::read_to_string(&static_path).unwrap();
    let revised = source.replace(
        "devastation = { local_manpower_modifier = -1 }",
        "devastation = { local_manpower_modifier = -1 trade_goods_size_modifier = -2 }",
    );
    assert_ne!(source, revised);
    fs::write(static_path, revised).unwrap();

    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let province = loaded.initial.province_entity(1).unwrap();
    let mut world = loaded.initial.world().clone();
    let mut metrics = world.metrics().clone();
    metrics
        .entities
        .entry(province)
        .or_default()
        .insert("devastation".into(), Some(25_000));
    world.set_metrics(metrics).unwrap();

    let modifier = world
        .attributes()
        .evaluate(&world, province, "trade_goods_size_modifier")
        .unwrap();
    assert_eq!(modifier.known_subtotal_micros, Some(-500_000));
    assert!(modifier.value_micros.is_none());

    let quantity = augustus_core::calculate_vanilla_trade_goods_size(
        augustus_core::VanillaTradeGoodsSizeInput {
            province_base_milli: 1_000,
            trade_good_base_milli: 0,
            external_base_milli: 0,
            province_modifier_milli: i32::try_from(modifier.known_subtotal_micros.unwrap() / 1_000)
                .unwrap(),
            trade_good_modifier_milli: 0,
            external_modifier_milli: 0,
        },
    );
    assert_eq!(quantity.quantity_milli, 500);
}

#[test]
fn prosperity_goods_size_effect_stays_unresolved_until_its_scale_is_verified() {
    let temp = copy_fixture();
    let static_path = temp.path().join("common/static_modifiers/test_static.txt");
    let source = fs::read_to_string(&static_path).unwrap();
    fs::write(
        &static_path,
        format!("{source}\nprosperity = {{ trade_goods_size_modifier = 0.25 }}\n"),
    )
    .unwrap();

    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let province = loaded.initial.province_entity(1).unwrap();
    let mut world = loaded.initial.world().clone();
    let before = world
        .attributes()
        .evaluate(&world, province, "trade_goods_size_modifier")
        .unwrap();
    let mut metrics = world.metrics().clone();
    metrics
        .entities
        .entry(province)
        .or_default()
        .insert("prosperity".into(), Some(50_000));
    world.set_metrics(metrics).unwrap();

    let after = world
        .attributes()
        .evaluate(&world, province, "trade_goods_size_modifier")
        .unwrap();

    assert_eq!(after.known_subtotal_micros, before.known_subtotal_micros);
    assert!(
        after
            .unresolved
            .contains("metric:prosperity:unresolved:trade_goods_size_modifier")
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
