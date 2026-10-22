// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use augustus_core::{
    Action, COUNTRY_LEDGER_INCOME_GOLD_CATEGORY, COUNTRY_LEDGER_INCOME_PRODUCTION_CATEGORY,
    COUNTRY_LEDGER_INCOME_TAX_CATEGORY, COUNTRY_LEDGER_INCOME_TRADE_CATEGORY,
    COUNTRY_LEDGER_SEND_OFFICERS_CATEGORY, CashflowBreakdown, CountryResources, EntityId,
    EntityKind, Error, MapKnowledge, MonthlyEconomy, MonthlyPipeline, MonthlySystem, NationalFocus,
    ProvinceClass, ProvinceDiscovery, RulerSkill, Schema, VanillaProductionInput,
    VanillaProductionInputState, VanillaTradeIncomeInput, VanillaTradeIncomePassInput, World,
    YearlyPipeline, YearlySystem,
};
use augustus_eu4::{
    ContentRoot, ContentSet, ContentSource, TextEncoding, VanillaExternalAdvisor,
    VanillaExternalArmy, VanillaExternalEstateState, VanillaExternalFleet,
    VanillaExternalNavalUnit, VanillaExternalState, VanillaWorld, read_vanilla_save_state,
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

fn append_fixture(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    let previous = fs::read_to_string(&path).unwrap();
    fs::write(path, format!("{previous}\n{text}")).unwrap();
}

fn selected_idea(loaded: &VanillaWorld, tag: &str) -> Option<String> {
    loaded
        .country(tag)
        .unwrap()
        .ideas
        .unwrap()
        .groups
        .as_ref()
        .and_then(|groups| groups.keys().next().cloned())
}

struct PipelinePhase {
    id: &'static str,
    country: EntityId,
    message: &'static str,
}

impl MonthlySystem for PipelinePhase {
    fn id(&self) -> &str {
        self.id
    }

    fn validate(&self, world: &World) -> Result<(), Error> {
        if world.entity(self.country)?.kind != EntityKind::Country {
            return Err(Error::Invalid("pipeline phase requires a country".into()));
        }
        Ok(())
    }

    fn settle(&self, world: &mut World) -> Result<(), Error> {
        world.execute(self.country, &[Action::Emit(self.message.into())])
    }
}

struct MonthlyRelationExpensePhase {
    country: EntityId,
    amount_milli: i64,
    message: &'static str,
}

impl MonthlySystem for MonthlyRelationExpensePhase {
    fn id(&self) -> &str {
        "vanilla-relation-expense:v1"
    }

    fn validate(&self, world: &World) -> Result<(), Error> {
        if world.entity(self.country)?.kind != EntityKind::Country {
            return Err(Error::Invalid(
                "relation expense phase requires a country".into(),
            ));
        }
        Ok(())
    }

    fn settle(&self, world: &mut World) -> Result<(), Error> {
        world.record_country_expense(
            self.country,
            COUNTRY_LEDGER_SEND_OFFICERS_CATEGORY,
            self.amount_milli,
        )?;
        world.execute(self.country, &[Action::Emit(self.message.into())])
    }
}

impl YearlySystem for PipelinePhase {
    fn id(&self) -> &str {
        self.id
    }

    fn validate(&self, world: &World) -> Result<(), Error> {
        if world.entity(self.country)?.kind != EntityKind::Country {
            return Err(Error::Invalid("pipeline phase requires a country".into()));
        }
        Ok(())
    }

    fn settle(&self, world: &mut World) -> Result<(), Error> {
        world.execute(self.country, &[Action::Emit(self.message.into())])
    }
}

#[test]
fn world_loader_reads_mod_files_through_resolved_content() {
    let fixture = copy_fixture();
    let mod_root = fixture.path().join("mod");
    fs::create_dir_all(mod_root.join("common/ideas")).unwrap();
    fs::write(mod_root.join("descriptor.mod"), "name=\"Test Mod\"").unwrap();
    fs::write(
        mod_root.join("common/ideas/99_mod.txt"),
        "mod_group = { free = yes first_idea = {} }",
    )
    .unwrap();
    let content = ContentSet::resolve(vec![
        ContentSource::base(ContentRoot::open(fixture.path()).unwrap()),
        ContentSource::mod_directory(&mod_root, TextEncoding::Windows1252).unwrap(),
    ])
    .unwrap();

    let loaded = VanillaWorld::load_from_content(&content, "1444.11.11").unwrap();
    let source = &loaded.report.ideas.definition_sources["mod_group"];
    assert!(source.path.starts_with(mod_root.canonicalize().unwrap()));
    assert!(
        content
            .sources()
            .iter()
            .any(|source| source.name() == "Test Mod")
    );
}

#[test]
fn country_tax_uses_all_nations_when_no_free_city_is_in_the_hre() {
    let fixture = copy_fixture();
    append_fixture(
        fixture.path(),
        "common/static_modifiers/test_static.txt",
        "all_nations = { global_tax_income = 13 }\n\
         free_cities_in_hre = { global_tax_income = 3 }",
    );

    let loaded = VanillaWorld::load(fixture.path(), "1444.11.11").unwrap();
    let country = loaded.initial.country_entity("AAA").unwrap();
    let world = loaded.initial.world();
    let tax = world
        .attributes()
        .evaluate(world, country, "country_annual_tax")
        .unwrap();

    assert_eq!(
        world
            .taxation()
            .unwrap()
            .country_annual_attribute
            .as_deref(),
        Some("country_annual_tax")
    );
    assert_eq!(tax.known_subtotal_micros, Some(13_000_000));
    assert_eq!(tax.value_micros, Some(13_000_000));
    assert!(
        !loaded
            .report
            .attributes
            .unmapped_properties
            .contains_key("static_modifiers:global_tax_income")
    );
}

#[test]
fn country_tax_does_not_assume_landless_countries_receive_all_nations() {
    let fixture = copy_fixture();
    append_fixture(
        fixture.path(),
        "common/static_modifiers/test_static.txt",
        "all_nations = { global_tax_income = 13 }",
    );
    for (id, name) in [(1, "North"), (2, "East"), (3, "South"), (4, "Highland")] {
        fs::write(
            fixture
                .path()
                .join(format!("history/provinces/{id} - {name}.txt")),
            "owner = AAA",
        )
        .unwrap();
    }

    let loaded = VanillaWorld::load(fixture.path(), "1444.11.11").unwrap();
    let landowner = loaded.initial.country_entity("AAA").unwrap();
    let landless = loaded.initial.country_entity("BBB").unwrap();
    let world = loaded.initial.world();
    assert!(!world.has_owned_province(landless).unwrap());
    assert_eq!(
        world
            .attributes()
            .evaluate(world, landowner, "country_annual_tax")
            .unwrap()
            .value_micros,
        Some(13_000_000)
    );
    let landless_tax = world
        .attributes()
        .evaluate(world, landless, "country_annual_tax")
        .unwrap();
    assert_eq!(landless_tax.value_micros, None);
    assert!(
        landless_tax
            .unresolved
            .contains("all_nations_landless_country_type_unresolved")
    );
}

#[test]
fn country_tax_counts_free_city_reforms_in_the_hre_for_the_current_emperor() {
    let fixture = copy_fixture();
    append_fixture(
        fixture.path(),
        "common/static_modifiers/test_static.txt",
        "all_nations = { global_tax_income = 13 }\n\
         free_cities_in_hre = { global_tax_income = 3 }",
    );
    append_fixture(
        fixture.path(),
        "common/government_reforms/test_governments.txt",
        "free_city = { republic = yes }",
    );
    append_fixture(
        fixture.path(),
        "history/countries/BBB - Beta.txt",
        "add_government_reform = free_city",
    );
    append_fixture(
        fixture.path(),
        "history/provinces/2 - East.txt",
        "1444.1.1 = { owner = BBB hre = yes }",
    );
    append_fixture(
        fixture.path(),
        "history/provinces/4 - Highland.txt",
        "owner = AAA",
    );

    let mut loaded = VanillaWorld::load(fixture.path(), "1444.11.11").unwrap();
    let emperor = loaded.initial.country_entity("AAA").unwrap();
    let city = loaded.initial.country_entity("BBB").unwrap();
    let province = loaded.initial.province_entity(2).unwrap();
    let world = loaded.initial.world();
    assert_eq!(
        world.politics().offices.get("imperial_emperor"),
        Some(&emperor)
    );
    assert!(world.has_owned_province(emperor).unwrap());
    assert!(world.government_reform_selected(city, "free_city").unwrap());
    assert_eq!(
        world
            .affiliations()
            .belongs_to("imperial_membership", province, "hre"),
        Some(true)
    );
    assert_eq!(world.entity(province).unwrap().owner, Some(city));
    assert!(world.owned_provinces(city).unwrap().contains(&province));
    let emperor_tax = world
        .attributes()
        .evaluate(world, emperor, "country_annual_tax")
        .unwrap();
    assert_eq!(emperor_tax.value_micros, Some(16_000_000));
    assert_eq!(
        world
            .attributes()
            .evaluate(world, city, "country_annual_tax")
            .unwrap()
            .value_micros,
        Some(13_000_000)
    );

    let mut affiliations = loaded.initial.world().affiliations().clone();
    let imperial = affiliations.0.get_mut("imperial_membership").unwrap();
    imperial.members.remove(&province);
    imperial.absent_members.insert(province);
    loaded.initial.set_affiliations(affiliations).unwrap();
    assert_eq!(
        loaded
            .initial
            .world()
            .attributes()
            .evaluate(loaded.initial.world(), emperor, "country_annual_tax")
            .unwrap()
            .value_micros,
        Some(13_000_000)
    );

    let mut affiliations = loaded.initial.world().affiliations().clone();
    let imperial = affiliations.0.get_mut("imperial_membership").unwrap();
    imperial.absent_members.remove(&province);
    imperial.members.insert(province, Some("hre".into()));
    loaded.initial.set_affiliations(affiliations).unwrap();
    let mut governments = loaded.initial.world().governments().clone();
    governments
        .countries
        .get_mut(&city)
        .unwrap()
        .reforms
        .as_mut()
        .unwrap()
        .remove("free_city");
    loaded.initial.set_governments(governments).unwrap();
    assert_eq!(
        loaded
            .initial
            .world()
            .attributes()
            .evaluate(loaded.initial.world(), emperor, "country_annual_tax")
            .unwrap()
            .value_micros,
        Some(13_000_000)
    );
}

#[test]
fn external_province_city_status_reaches_the_loaded_province_state() {
    let state = VanillaExternalState {
        province_is_city: BTreeMap::from([(2, Some(true))]),
        province_in_empire: BTreeMap::from([(2, Some(true))]),
        ..Default::default()
    };
    let loaded = VanillaWorld::load_with_external_state(fixture(), "1444.11.11", &state).unwrap();
    assert_eq!(loaded.province(2).unwrap().state.is_city, Some(true));
    let province = loaded.initial.province_entity(2).unwrap();
    assert_eq!(
        loaded
            .initial
            .world()
            .affiliations()
            .belongs_to("imperial_membership", province, "hre"),
        Some(true)
    );

    let error = VanillaWorld::load_with_external_state(
        fixture(),
        "1444.11.11",
        &VanillaExternalState {
            province_is_city: BTreeMap::from([(999, Some(true))]),
            ..Default::default()
        },
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("unknown EU4 city-status province 999"));
}

#[test]
fn external_province_owners_replace_history_before_ownership_is_indexed() {
    let state = VanillaExternalState {
        province_owners: BTreeMap::from([(1, Some("BBB".into())), (2, None)]),
        ..Default::default()
    };
    let loaded = VanillaWorld::load_with_external_state(fixture(), "1444.11.11", &state).unwrap();
    let north = loaded.initial.province_entity(1).unwrap();
    let east = loaded.initial.province_entity(2).unwrap();
    let aaa = loaded.initial.country_entity("AAA").unwrap();
    let bbb = loaded.initial.country_entity("BBB").unwrap();

    assert_eq!(
        loaded.province(1).unwrap().state.owner.as_deref(),
        Some("BBB")
    );
    assert_eq!(
        loaded.initial.world().entity(north).unwrap().owner,
        Some(bbb)
    );
    assert_eq!(loaded.province(2).unwrap().state.owner, None);
    assert_eq!(loaded.initial.world().entity(east).unwrap().owner, None);
    assert!(
        !loaded
            .initial
            .world()
            .owned_provinces(aaa)
            .unwrap()
            .contains(&north)
    );
    assert!(
        loaded
            .initial
            .world()
            .owned_provinces(bbb)
            .unwrap()
            .contains(&north)
    );
    assert!(
        !loaded
            .initial
            .world()
            .owned_provinces(bbb)
            .unwrap()
            .contains(&east)
    );

    let error = VanillaWorld::load_with_external_state(
        fixture(),
        "1444.11.11",
        &VanillaExternalState {
            province_owners: BTreeMap::from([(1, Some("ZZZ".into()))]),
            ..Default::default()
        },
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("province 1 references unknown saved owner ZZZ"));

    let error = VanillaWorld::load_with_external_state(
        fixture(),
        "1444.11.11",
        &VanillaExternalState {
            province_owners: BTreeMap::from([(999, None)]),
            ..Default::default()
        },
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("external province ownership references unknown province 999"));
}

#[test]
fn imported_save_ownership_replaces_content_history_before_startup() {
    let temp = tempfile::tempdir().unwrap();
    let save_path = temp.path().join("state.eu4");
    fs::write(
        &save_path,
        b"EU4txt\ndate=1444.11.11\ncountries={ AAA={} BBB={} }\nprovinces={ -1={ owner=BBB } -2={} }\n",
    )
    .unwrap();
    let save = read_vanilla_save_state(&save_path).unwrap();
    let content_world = VanillaWorld::load(fixture(), &save.date).unwrap();
    let external_state = save.state_for_world(&content_world).unwrap();
    let loaded =
        VanillaWorld::load_with_external_state(fixture(), &save.date, &external_state).unwrap();
    let north = loaded.initial.province_entity(1).unwrap();
    let east = loaded.initial.province_entity(2).unwrap();
    let bbb = loaded.initial.country_entity("BBB").unwrap();

    assert_eq!(
        loaded.initial.world().entity(north).unwrap().owner,
        Some(bbb)
    );
    assert_eq!(loaded.initial.world().entity(east).unwrap().owner, None);
    assert!(
        loaded
            .initial
            .world()
            .owned_provinces(bbb)
            .unwrap()
            .contains(&north)
    );
    assert!(
        !loaded
            .initial
            .world()
            .owned_provinces(bbb)
            .unwrap()
            .contains(&east)
    );
}

#[test]
fn startup_actions_from_separate_files_are_all_applied() {
    let root = copy_fixture();
    let on_actions = root.path().join("common/on_actions");
    fs::create_dir_all(&on_actions).unwrap();
    fs::write(
        on_actions.join("00_base.txt"),
        "on_startup = { log = base_startup }",
    )
    .unwrap();
    fs::write(
        on_actions.join("advisor_replace.txt"),
        "on_startup = { log = mod_startup }",
    )
    .unwrap();

    let loaded = VanillaWorld::load(root.path(), "1444.11.11").unwrap();
    let messages = loaded
        .initial
        .world()
        .events()
        .iter()
        .map(|event| event.2.as_str())
        .collect::<BTreeSet<_>>();
    assert!(messages.contains("base_startup"));
    assert!(messages.contains("mod_startup"));

    let duplicate = copy_fixture();
    let path = duplicate.path().join("common/on_actions/repeated.txt");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(
        &path,
        "on_startup = { log = first } on_startup = { log = second }",
    )
    .unwrap();
    let error = VanillaWorld::load(duplicate.path(), "1444.11.11")
        .unwrap_err()
        .to_string();
    assert!(error.contains("duplicate startup action"));
}

#[test]
fn startup_region_scope_discovers_each_member_province_for_the_root_country() {
    let root = copy_fixture();
    append_fixture(root.path(), "map/area.txt", "highland_area = { 4 }");
    append_fixture(
        root.path(),
        "map/default.map",
        "region = \"region.txt\"\nsuperregion = \"superregion.txt\"\ncontinent = \"continent.txt\"",
    );
    fs::write(
        root.path().join("map/region.txt"),
        "discovery_region = { areas = { north_area east_area } }",
    )
    .unwrap();
    fs::write(
        root.path().join("map/superregion.txt"),
        "discovery_superregion = { discovery_region }",
    )
    .unwrap();
    fs::write(
        root.path().join("map/continent.txt"),
        "discovery_continent = { 4 }",
    )
    .unwrap();
    let on_actions = root.path().join("common/on_actions");
    fs::create_dir_all(&on_actions).unwrap();
    fs::write(
        on_actions.join("discovery.txt"),
        "on_startup = { if = { limit = { tag = BBB } discovery_region = { log = discovery_scope discover_country = ROOT } discovery_superregion = { discover_country = ROOT } discovery_continent = { discover_country = ROOT } } }",
    )
    .unwrap();

    let loaded = VanillaWorld::load(root.path(), "1444.11.11").unwrap();
    let world = loaded.initial.world();
    let beta = loaded.initial.country_entity("BBB").unwrap();
    let north = loaded.initial.province_entity(1).unwrap();
    let east = loaded.initial.province_entity(2).unwrap();
    let sea = loaded.initial.province_entity(3).unwrap();
    let highland = loaded.initial.province_entity(4).unwrap();

    assert_eq!(
        world.province_discovered_by(north, beta).unwrap(),
        Some(true)
    );
    assert_eq!(
        world.province_discovered_by(east, beta).unwrap(),
        Some(true)
    );
    assert_eq!(world.province_discovered_by(sea, beta).unwrap(), None);
    assert_eq!(
        world.province_discovered_by(highland, beta).unwrap(),
        Some(true)
    );
    let region_visits: Vec<_> = world
        .events()
        .iter()
        .filter(|event| event.2 == "discovery_scope")
        .map(|event| event.1)
        .collect();
    assert_eq!(region_visits, [north, east]);
    assert!(loaded.report.estates.pending_startup_effects.is_empty());
}

#[test]
fn delayed_mod_event_replaces_advisor_after_save_resume() {
    let root = copy_fixture();
    let advisor_types = root.path().join("common/advisortypes");
    fs::create_dir_all(&advisor_types).unwrap();
    fs::write(
        advisor_types.join("advisor_replace.txt"),
        "army_reformer = { monarch_power = MIL }",
    )
    .unwrap();
    let on_actions = root.path().join("common/on_actions");
    fs::create_dir_all(&on_actions).unwrap();
    fs::write(
        on_actions.join("00_base.txt"),
        "on_startup = { log = base_startup }",
    )
    .unwrap();
    fs::write(
        on_actions.join("advisor_replace.txt"),
        "on_startup = { if = { limit = { tag = AAA } country_event = { id = probe.1 days = 25 } } }",
    )
    .unwrap();
    let events = root.path().join("events");
    fs::create_dir_all(&events).unwrap();
    fs::write(
        events.join("advisor_replace.txt"),
        "namespace = probe country_event = { id = probe.1 hidden = yes is_triggered_only = yes immediate = { log = PKREP_SYR_BEGIN remove_advisor_by_category = MIL hire_advisor = { type = army_reformer skill = 1 cost_multiplier = 1.0 } log = PKREP_SYR_END } }",
    )
    .unwrap();

    let mut continuous = VanillaWorld::load(root.path(), "1444.11.11").unwrap();
    assert!(continuous.report.estates.pending_startup_effects.is_empty());
    assert!(
        continuous
            .initial
            .world()
            .events()
            .iter()
            .any(|event| event.2 == "base_startup")
    );
    assert!(
        continuous
            .initial
            .world()
            .events()
            .iter()
            .all(|event| { event.2 != "PKREP_SYR_BEGIN" && event.2 != "PKREP_SYR_END" })
    );

    let mut segmented = VanillaWorld::load(root.path(), "1444.11.11").unwrap();
    segmented.advance(20).unwrap();
    let snapshot = segmented.save().unwrap();
    let mut resumed = VanillaWorld::load(root.path(), "1444.11.11").unwrap();
    resumed.restore(&snapshot).unwrap();
    resumed.advance(5).unwrap();
    continuous.advance(25).unwrap();

    let events = resumed.initial.world().events();
    assert_eq!(
        events
            .iter()
            .filter(|event| event.2 == "PKREP_SYR_BEGIN" || event.2 == "PKREP_SYR_END")
            .map(|event| (event.0, event.2.as_str()))
            .collect::<Vec<_>>(),
        [(25, "PKREP_SYR_BEGIN"), (25, "PKREP_SYR_END")]
    );
    assert!(resumed.initial.world().pending_events().is_empty());
    assert_eq!(continuous.save().unwrap(), resumed.save().unwrap());

    let country = resumed.country("AAA").unwrap();
    assert!(!country.advisors_known);
    assert!(country.advisors.is_none());
    let slots = country.advisor_slots.unwrap();
    assert_eq!(slots.len(), 1);
    let military = slots[&augustus_core::RulerSkill::Military]
        .as_ref()
        .unwrap();
    assert_eq!(military.type_name, "army_reformer");
    assert_eq!(military.skill, 1);
    assert_eq!(military.cost_multiplier_milli, Some(1_000));
}

#[test]
fn external_hre_membership_survives_pending_startup_fallback() {
    let root = copy_fixture();
    fs::create_dir_all(root.path().join("common/on_actions")).unwrap();
    fs::write(
        root.path().join("common/on_actions/pending.txt"),
        "on_startup={unsupported_effect=yes}",
    )
    .unwrap();
    let state = VanillaExternalState {
        country_metric_values: BTreeMap::from([(
            "AAA".into(),
            BTreeMap::from([("stability".into(), Some(2))]),
        )]),
        province_in_empire: BTreeMap::from([(2, Some(true))]),
        ..Default::default()
    };
    let loaded = VanillaWorld::load_with_external_state(root.path(), "1444.11.11", &state).unwrap();
    assert!(!loaded.report.estates.pending_startup_effects.is_empty());
    assert_eq!(
        loaded.country("AAA").unwrap().metrics.as_ref().unwrap()["stability"],
        Some(2)
    );
    let province = loaded.initial.province_entity(2).unwrap();
    assert_eq!(
        loaded
            .initial
            .world()
            .affiliations()
            .belongs_to("imperial_membership", province, "hre"),
        Some(true)
    );
}

#[test]
fn external_estate_state_reaches_live_loyalty_and_land_effects() {
    let temp = copy_fixture();
    fs::create_dir_all(temp.path().join("common/estates")).unwrap();
    fs::write(
        temp.path().join("common/estates/test.txt"),
        "estate_guild = { trigger = { always = yes } base_influence = 12 }",
    )
    .unwrap();
    let state = VanillaExternalState {
        estate_states: BTreeMap::from([(
            "AAA".into(),
            BTreeMap::from([(
                "estate_guild".into(),
                VanillaExternalEstateState {
                    loyalty_milli: Some(44_600),
                    land_share_micros: Some(400_000),
                    active_influences: None,
                },
            )]),
        )]),
        ..Default::default()
    };
    let loaded = VanillaWorld::load_with_external_state(temp.path(), "1444.11.11", &state).unwrap();
    let country = loaded.initial.country_entity("AAA").unwrap();
    let estate = &loaded.initial.world().estates().countries[&country]["estate_guild"];
    assert_eq!(estate.loyalty_milli, Some(44_600));
    assert_eq!(estate.land_share_micros, Some(400_000));
}

#[test]
fn estate_state_overlay_merges_each_snapshot_field() {
    let mut base = VanillaExternalState {
        estate_states: BTreeMap::from([(
            "AAA".into(),
            BTreeMap::from([(
                "estate_guild".into(),
                VanillaExternalEstateState {
                    loyalty_milli: Some(44_600),
                    land_share_micros: Some(400_000),
                    active_influences: Some(BTreeSet::from([1])),
                },
            )]),
        )]),
        ..Default::default()
    };
    let overlay = VanillaExternalState {
        estate_states: BTreeMap::from([(
            "AAA".into(),
            BTreeMap::from([(
                "estate_guild".into(),
                VanillaExternalEstateState {
                    loyalty_milli: None,
                    land_share_micros: None,
                    active_influences: Some(BTreeSet::from([4])),
                },
            )]),
        )]),
        ..Default::default()
    };
    base.apply_overlay(&overlay);
    assert_eq!(
        base.estate_states["AAA"]["estate_guild"].loyalty_milli,
        Some(44_600)
    );
    assert_eq!(
        base.estate_states["AAA"]["estate_guild"].land_share_micros,
        Some(400_000)
    );
    assert_eq!(
        base.estate_states["AAA"]["estate_guild"].active_influences,
        Some(BTreeSet::from([4]))
    );
}

#[test]
fn technology_levels_keep_enabled_unit_names_in_order_with_sources() {
    let fixture = copy_fixture();
    fs::write(
        fixture.path().join("common/technology.txt"),
        "groups = { test_group = { start_level = 1 } }\ntables = {\n\
         adm_tech = \"technologies/test_adm.txt\"\n\
         dip_tech = \"technologies/test_dip.txt\"\n\
         mil_tech = \"technologies/test_mil.txt\"\n}",
    )
    .unwrap();
    fs::write(
        fixture.path().join("common/technologies/test_dip.txt"),
        "monarch_power = DIP\n\
         technology = { year = 1410 enable = barque trade_range = 100 enable = cog }\n\
         technology = { year = 1500 enable = caravel }",
    )
    .unwrap();
    fs::create_dir_all(fixture.path().join("common/units")).unwrap();
    fs::write(
        fixture.path().join("common/units/barque.txt"),
        "type = light_ship\nunit_type = western\nhull_size = 8",
    )
    .unwrap();

    let loaded = VanillaWorld::load(fixture.path(), "1444.11.11").unwrap();
    let levels = &loaded.initial.world().technology().tables["diplomatic"];
    assert_eq!(levels[0].enabled_units, ["barque", "cog"]);
    assert_eq!(levels[1].enabled_units, ["caravel"]);
    let template = &loaded.initial.world().unit_templates().definitions["barque"];
    assert_eq!(template.category, "light_ship");
    assert_eq!(
        template
            .properties
            .iter()
            .map(|property| (property.name.as_str(), property.value.as_str()))
            .collect::<Vec<_>>(),
        [("unit_type", "western"), ("hull_size", "8")]
    );
    assert_eq!(loaded.report.setup.unit_templates, 5);
    assert!(
        !loaded
            .report
            .setup
            .unapplied_definition_fields
            .contains_key("technology.level:enable")
    );
    assert_eq!(
        loaded.report.setup.enabled_unit_sources["diplomatic:0:barque"][0].line,
        2
    );
    assert_eq!(loaded.report.setup.unit_template_sources["barque"].line, 1);

    let saved = loaded.save().unwrap();
    let mut same = VanillaWorld::load(fixture.path(), "1444.11.11").unwrap();
    same.restore(&saved).unwrap();
    assert_eq!(
        same.initial.world().unit_templates(),
        loaded.initial.world().unit_templates()
    );
    fs::write(
        fixture.path().join("common/units/barque.txt"),
        "type = light_ship\nunit_type = western\nhull_size = 9",
    )
    .unwrap();
    let mut changed = VanillaWorld::load(fixture.path(), "1444.11.11").unwrap();
    assert!(changed.restore(&saved).is_err());
}

#[test]
fn country_history_keeps_unit_type_separate_and_applies_dated_changes() {
    let fixture = copy_fixture();
    append_fixture(
        fixture.path(),
        "history/countries/AAA - Alpha.txt",
        "technology_group = test_group\nunit_type = sample_army_group\n\
         1445.1.1 = { unit_type = later_army_group }",
    );

    let start = VanillaWorld::load(fixture.path(), "1444.11.11").unwrap();
    let country = start.initial.country("AAA").unwrap();
    assert_eq!(country.technology_group.as_deref(), Some("test_group"));
    assert_eq!(country.unit_type.as_deref(), Some("sample_army_group"));
    let start_source = &start.report.setup.history_sources["country:AAA:unit_type"];
    assert!(start_source.path.ends_with("AAA - Alpha.txt"));

    let later = VanillaWorld::load(fixture.path(), "1450.1.1").unwrap();
    let country = later.initial.country("AAA").unwrap();
    assert_eq!(country.technology_group.as_deref(), Some("test_group"));
    assert_eq!(country.unit_type.as_deref(), Some("later_army_group"));
    assert!(later.report.setup.history_sources["country:AAA:unit_type"].line > start_source.line);
    assert!(
        !start
            .report
            .unapplied_country_fields
            .contains_key("unit_type")
    );
}

#[test]
fn country_without_unit_type_inherits_its_technology_group() {
    let loaded = VanillaWorld::load(fixture(), "1444.11.11").unwrap();

    let country = loaded.initial.country("AAA").unwrap();
    assert_eq!(country.technology_group.as_deref(), Some("test_group"));
    assert_eq!(country.unit_type.as_deref(), Some("test_group"));
    assert!(
        !loaded
            .report
            .setup
            .history_sources
            .contains_key("country:AAA:unit_type")
    );

    let country_without_group = loaded.initial.country("BBB").unwrap();
    assert_eq!(country_without_group.technology_group, None);
    assert_eq!(country_without_group.unit_type, None);
}

#[test]
fn country_without_unit_type_inherits_its_final_dated_technology_group() {
    let fixture = copy_fixture();
    let technology_path = fixture.path().join("common/technology.txt");
    let technology = fs::read_to_string(&technology_path).unwrap();
    fs::write(
        technology_path,
        technology.replace(
            "groups = { test_group = { start_level = 3 } }",
            "groups = { test_group = { start_level = 3 } later_group = { start_level = 2 } }",
        ),
    )
    .unwrap();
    append_fixture(
        fixture.path(),
        "history/countries/AAA - Alpha.txt",
        "1445.1.1 = { technology_group = later_group }",
    );

    let before_change = VanillaWorld::load(fixture.path(), "1444.11.11").unwrap();
    let country = before_change.initial.country("AAA").unwrap();
    assert_eq!(country.technology_group.as_deref(), Some("test_group"));
    assert_eq!(country.unit_type.as_deref(), Some("test_group"));

    let after_change = VanillaWorld::load(fixture.path(), "1450.1.1").unwrap();
    let country = after_change.initial.country("AAA").unwrap();
    assert_eq!(country.technology_group.as_deref(), Some("later_group"));
    assert_eq!(country.unit_type.as_deref(), Some("later_group"));
}

#[test]
fn historical_federation_membership_is_not_a_generated_nation_and_later_origin_is_unknown() {
    let fixture = copy_fixture();
    append_fixture(
        fixture.path(),
        "history/countries/AAA - Alpha.txt",
        "federation = { name = test_union members = { AAA BBB } }",
    );
    fs::write(fixture.path().join("common/ideas/00_conditions.txt"),
        "historical = { free = yes trigger = { is_client_nation = no is_federation_nation = no } one = {} }").unwrap();
    let loaded = VanillaWorld::load(fixture.path(), "1444.11.11").unwrap();
    assert_eq!(selected_idea(&loaded, "AAA").as_deref(), Some("historical"));
    let later = VanillaWorld::load(fixture.path(), "1450.1.1").unwrap();
    assert_eq!(selected_idea(&later, "AAA"), None);
    assert!(later.report.ideas.unknown_statuses["AAA"].contains("federation_nation"));
    append_fixture(
        fixture.path(),
        "history/countries/AAA - Alpha.txt",
        "if = { limit = { always = yes } create_client_state = {} }",
    );
    let pending = VanillaWorld::load(fixture.path(), "1444.11.11").unwrap();
    assert_eq!(selected_idea(&pending, "BBB"), None);
    assert!(pending.report.ideas.unknown_statuses["BBB"].contains("client_nation"));
}

#[test]
fn colonial_identity_uses_the_current_subject_definition_and_ordered_inheritance() {
    let fixture = copy_fixture();
    fs::write(
        fixture.path().join("common/ideas/00_conditions.txt"),
        "colonial = { free = yes trigger = { is_colonial_nation = yes } one = {} }",
    )
    .unwrap();
    append_fixture(
        fixture.path(),
        "history/countries/AAA - Alpha.txt",
        "colonial_parent = BBB",
    );
    let subjects = fixture.path().join("common/subject_types/test.txt");
    fs::write(&subjects, "default = { has_colonial_parent = no } colony = {} colony = { copy_from = default has_colonial_parent = yes } test_vassal = { copy_from = colony }").unwrap();
    let loaded = VanillaWorld::load(fixture.path(), "1444.11.11").unwrap();
    assert_eq!(selected_idea(&loaded, "BBB").as_deref(), Some("colonial"));
    assert_eq!(selected_idea(&loaded, "AAA").as_deref(), Some("test_alpha"));
    fs::write(&subjects, "default = { has_colonial_parent = no } colony = { copy_from = default has_colonial_parent = yes } test_vassal = { has_colonial_parent = no copy_from = colony }").unwrap();
    let inherited = VanillaWorld::load(fixture.path(), "1444.11.11").unwrap();
    assert_eq!(
        selected_idea(&inherited, "BBB").as_deref(),
        Some("colonial")
    );
    fs::write(&subjects, "default = { has_colonial_parent = no } colony = { copy_from = default has_colonial_parent = yes } test_vassal = { copy_from = colony has_colonial_parent = no }").unwrap();
    let overridden = VanillaWorld::load(fixture.path(), "1444.11.11").unwrap();
    assert_eq!(
        selected_idea(&overridden, "BBB").as_deref(),
        Some("test_common")
    );
    let released = VanillaWorld::load(fixture.path(), "1450.1.1").unwrap();
    let beta = released.initial.country_entity("BBB").unwrap();
    assert_eq!(
        released
            .initial
            .world()
            .politics()
            .status(beta, "colonial_nation"),
        Some(false)
    );
}

#[test]
fn cyclic_or_duplicate_subject_definitions_keep_colonial_selection_unknown() {
    let fixture = copy_fixture();
    fs::write(
        fixture.path().join("common/ideas/00_conditions.txt"),
        "colonial = { free = yes trigger = { is_colonial_nation = yes } one = {} }",
    )
    .unwrap();
    for definitions in [
        "default = { has_colonial_parent = no } test_vassal = { copy_from = other } other = { copy_from = test_vassal }",
        "default = { has_colonial_parent = no } test_vassal = { has_colonial_parent = yes } test_vassal = { has_colonial_parent = no }",
    ] {
        fs::write(
            fixture.path().join("common/subject_types/test.txt"),
            definitions,
        )
        .unwrap();
        let loaded = VanillaWorld::load(fixture.path(), "1444.11.11").unwrap();
        assert_eq!(selected_idea(&loaded, "BBB"), None);
        assert!(loaded.report.ideas.unknown_statuses["BBB"].contains("colonial_nation"));
    }
}

#[test]
fn loader_distinguishes_an_explicit_zero_seed_from_runtime_clock_mode() {
    let runtime_seed = VanillaWorld::load(fixture(), "1444.11.11").unwrap();
    let fixed_zero = VanillaWorld::load_with_seed(fixture(), "1444.11.11", 0).unwrap();

    assert_eq!(runtime_seed.initial.world().eu4_event_seed(), None);
    assert_eq!(fixed_zero.initial.world().eu4_event_seed(), Some(0));
    assert_eq!(
        runtime_seed.initial.world().eu4_random_state().unwrap().0,
        0
    );
    assert_eq!(fixed_zero.initial.world().eu4_random_state().unwrap().0, 0);
}

#[test]
fn startup_random_choices_do_not_change_content_identity_or_block_cross_seed_restore() {
    let fixture = copy_fixture();
    fs::create_dir_all(fixture.path().join("common/on_actions")).unwrap();
    fs::create_dir_all(fixture.path().join("events")).unwrap();
    fs::write(
        fixture.path().join("common/on_actions/seeded_startup.txt"),
        "on_startup={if={limit={tag=AAA} country_event={id=seeded.1}}}",
    )
    .unwrap();
    fs::write(
        fixture.path().join("events/seeded_startup.txt"),
        "country_event={id=seeded.1 is_triggered_only=yes option={name=left ai_chance={factor=1} set_country_flag=seed_left log=seed_left} option={name=right ai_chance={factor=1} set_country_flag=seed_right log=seed_right}}",
    )
    .unwrap();

    let worlds = [0, 1, 2, 3, 4, 5, 6, 7, 42]
        .into_iter()
        .map(|seed| {
            let mut world = VanillaWorld::load_with_players_and_seed(
                fixture.path(),
                "1444.11.11",
                &BTreeSet::new(),
                seed,
            )
            .unwrap();
            world.advance(1).unwrap();
            world
        })
        .collect::<Vec<_>>();
    let outcomes = worlds
        .iter()
        .filter_map(|world| {
            world
                .initial
                .world()
                .events()
                .iter()
                .find(|event| event.2 == "seed_left" || event.2 == "seed_right")
                .map(|event| event.2.as_str())
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(
        outcomes.len(),
        2,
        "unresolved={:?}, events={:?}, pending={}",
        worlds[0].report.estates.startup_unresolved,
        worlds[0].initial.world().events(),
        worlds[0].initial.world().pending_events().len()
    );

    let saved = worlds[0].save().unwrap();
    let content = serde_json::from_str::<serde_json::Value>(&saved).unwrap()["content"]
        .as_str()
        .unwrap()
        .to_owned();
    for world in &worlds[1..] {
        let snapshot = serde_json::from_str::<serde_json::Value>(&world.save().unwrap()).unwrap();
        assert_eq!(snapshot["content"], content);
    }

    let mut resumed = VanillaWorld::load_with_players_and_seed(
        fixture.path(),
        "1444.11.11",
        &BTreeSet::new(),
        42,
    )
    .unwrap();
    resumed.restore(&saved).unwrap();
}

#[test]
fn revolutionary_reforms_and_revolution_target_history_are_distinct_conditions() {
    let fixture = copy_fixture();
    append_fixture(
        fixture.path(),
        "common/government_reforms/test_governments.txt",
        "test_revolution = { revolutionary = yes custom_attributes = { test_charter = yes } }",
    );
    append_fixture(
        fixture.path(),
        "history/countries/AAA - Alpha.txt",
        "1443.1.1 = { revolution_target = yes add_government_reform = test_revolution }\n1445.1.1 = { revolution_target = no }",
    );
    fs::write(fixture.path().join("common/ideas/00_conditions.txt"),
        "revolution = { free = yes trigger = { is_revolution_target = yes is_revolutionary = yes has_government_attribute = test_charter } one = {} }").unwrap();
    let loaded = VanillaWorld::load(fixture.path(), "1444.11.11").unwrap();
    assert_eq!(selected_idea(&loaded, "AAA").as_deref(), Some("revolution"));
    let later = VanillaWorld::load(fixture.path(), "1450.1.1").unwrap();
    assert_eq!(selected_idea(&later, "AAA").as_deref(), Some("test_alpha"));
    append_fixture(
        fixture.path(),
        "history/countries/BBB - Beta.txt",
        "revolution_target = yes",
    );
    let error = VanillaWorld::load(fixture.path(), "1444.11.11")
        .err()
        .unwrap()
        .to_string();
    assert!(error.contains("multiple revolution targets"));
}

#[test]
fn uncertain_reform_features_and_scoped_target_changes_do_not_become_false() {
    let fixture = copy_fixture();
    let reforms = fixture
        .path()
        .join("common/government_reforms/test_governments.txt");
    let original = fs::read_to_string(&reforms).unwrap();
    append_fixture(
        fixture.path(),
        "history/countries/AAA - Alpha.txt",
        "add_government_reform = conditional_charter",
    );
    fs::write(
        fixture.path().join("common/ideas/00_conditions.txt"),
        "charter = { free = yes trigger = { has_government_attribute = test_charter } one = {} }",
    )
    .unwrap();
    for body in [
        "custom_attributes = { test_charter = yes test_charter = no }",
        "custom_attributes = { test_charter = yes } conditional = { allow = { unsupported = yes } custom_attributes = { test_charter = no } }",
    ] {
        fs::write(
            &reforms,
            format!("{original}\nconditional_charter = {{ {body} }}"),
        )
        .unwrap();
        let loaded = VanillaWorld::load(fixture.path(), "1444.11.11").unwrap();
        assert_eq!(selected_idea(&loaded, "AAA"), None);
        assert_eq!(
            selected_idea(&loaded, "BBB").as_deref(),
            Some("test_common")
        );
    }
    fs::write(&reforms, format!("{original}\nconditional_charter = {{ conditional = {{ allow = {{ always = yes }} custom_attributes = {{ test_charter = yes }} }} }}")).unwrap();
    let known = VanillaWorld::load(fixture.path(), "1444.11.11").unwrap();
    assert_eq!(selected_idea(&known, "AAA").as_deref(), Some("charter"));
    append_fixture(
        fixture.path(),
        "history/countries/AAA - Alpha.txt",
        "BBB = { revolution_target = yes }",
    );
    fs::write(
        fixture.path().join("common/ideas/00_conditions.txt"),
        "target = { free = yes trigger = { is_revolution_target = no } one = {} }",
    )
    .unwrap();
    let unknown = VanillaWorld::load(fixture.path(), "1444.11.11").unwrap();
    assert_eq!(selected_idea(&unknown, "BBB"), None);
}

#[test]
fn capital_geography_uses_shared_groups_and_the_bound_capital_with_scope_checks() {
    let fixture = copy_fixture();
    append_fixture(
        fixture.path(),
        "map/default.map",
        "region = \"region.txt\"\nsuperregion = \"superregion.txt\"",
    );
    fs::write(fixture.path().join("map/region.txt"),
        "home = { areas = { east_area } monsoon = { start = 6 end = 9 } monsoon = { start = 7 end = 8 } } away = { areas = { north_area sea_area } } empty = { }").unwrap();
    fs::write(
        fixture.path().join("map/superregion.txt"),
        "continent = { restrict_charter home } other = { away } vacant = {}",
    )
    .unwrap();
    let conditions = fixture.path().join("common/ideas/00_conditions.txt");
    fs::write(&conditions, "geographic = { free = yes trigger = { capital_scope = { region = home superregion = continent } } one = {} }").unwrap();
    let loaded = VanillaWorld::load(fixture.path(), "1444.11.11").unwrap();
    assert_eq!(selected_idea(&loaded, "AAA").as_deref(), Some("geographic"));
    assert_eq!(
        selected_idea(&loaded, "BBB").as_deref(),
        Some("test_common")
    );
    assert_eq!(loaded.report.administration.regions, 3);
    assert_eq!(loaded.report.administration.superregions, 3);
    assert_eq!(
        loaded
            .report
            .administration
            .unapplied_geography_fields
            .len(),
        3
    );
    let later = VanillaWorld::load(fixture.path(), "1450.1.1").unwrap();
    assert_eq!(selected_idea(&later, "AAA").as_deref(), Some("test_alpha"));
    for trigger in [
        "region = home",
        "capital_scope = { tag = AAA }",
        "capital_scope = { superregion = missing }",
    ] {
        fs::write(
            &conditions,
            format!("geographic = {{ free = yes trigger = {{ {trigger} }} one = {{}} }}"),
        )
        .unwrap();
        let unknown = VanillaWorld::load(fixture.path(), "1444.11.11").unwrap();
        assert_eq!(selected_idea(&unknown, "AAA"), None);
    }
    fs::write(
        &conditions,
        "geographic = { free = yes trigger = { capital_scope = { region = home } } one = {} }",
    )
    .unwrap();
    append_fixture(
        fixture.path(),
        "history/countries/AAA - Alpha.txt",
        "1444.11.11 = { capital = 1 }",
    );
    let invalid_capital = VanillaWorld::load(fixture.path(), "1444.11.11").unwrap();
    assert_eq!(selected_idea(&invalid_capital, "AAA"), None);
    assert!(
        invalid_capital.report.ideas.unresolved_countries["AAA"]
            .contains("candidate:geographic:unknown_capital")
    );
}

#[test]
fn ambiguous_or_dangling_geographic_memberships_fail_with_source() {
    let fixture = copy_fixture();
    append_fixture(fixture.path(), "map/default.map", "region = \"region.txt\"");
    let path = fixture.path().join("map/region.txt");
    for groups in [
        "home = { areas = { east_area } areas = { north_area } }",
        "home = { areas = { east_area } } home = { areas = { north_area } }",
        "home = { areas = { missing } }",
        "home = { areas = { east_area } } away = { areas = { east_area } }",
    ] {
        fs::write(&path, groups).unwrap();
        assert!(VanillaWorld::load(fixture.path(), "1444.11.11").is_err());
    }
    fs::write(
        &path,
        "home = { areas = { east_area } areas = { north_area } }",
    )
    .unwrap();
    let error = VanillaWorld::load(fixture.path(), "1444.11.11")
        .err()
        .unwrap()
        .to_string();
    assert!(error.contains("region.txt") && error.contains("one area list"));
}

#[test]
fn national_traditions_select_from_flags_and_ruler_and_progress_stays_explicit() {
    let loaded = VanillaWorld::load(fixture(), "1444.11.11").unwrap();
    let alpha = loaded.country("AAA").unwrap();
    assert_eq!(
        alpha.ideas.unwrap().groups.as_ref().unwrap()["test_alpha"],
        Some(0)
    );
    assert_eq!(
        alpha.declared_idea_effects.known_totals_micros["test_tradition"],
        125_000
    );
    assert!(
        !alpha
            .declared_idea_effects
            .known_totals_micros
            .contains_key("global_tax_modifier")
    );
    let later = VanillaWorld::load(fixture(), "1450.1.1").unwrap();
    assert_eq!(
        later
            .country("AAA")
            .unwrap()
            .ideas
            .unwrap()
            .groups
            .as_ref()
            .unwrap()["test_alpha"],
        None
    );
    assert!(later.report.ideas.unresolved_countries["AAA"].contains("unknown_historical_progress"));
}

#[test]
fn saved_idea_progress_overrides_unknown_historical_progress() {
    let content =
        ContentSet::from_installation(fixture(), &[], None, None, TextEncoding::Utf8).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("idea-progress.eu4");
    fs::write(
        &path,
        r#"EU4txt
date=1450.1.1
player="AAA"
countries={
    AAA={
        human=yes
        active_idea_groups={
            test_alpha=2
        }
    }
}
provinces={}
"#,
    )
    .unwrap();

    let loaded = VanillaWorld::load_from_content_with_save(&content, &path).unwrap();
    let entity = loaded.initial.country_entity("AAA").unwrap();
    assert_eq!(
        loaded.initial.world().ideas().countries[&entity].groups,
        Some(BTreeMap::from([("test_alpha".into(), Some(2))]))
    );
    assert!(!loaded.report.ideas.unresolved_countries.contains_key("AAA"));
    assert!(
        !loaded
            .country("AAA")
            .unwrap()
            .declared_idea_effects
            .unresolved
            .contains("ideas:test_alpha:unknown_progress")
    );
}

#[test]
fn earlier_unknown_candidate_blocks_fallback_but_false_and_true_short_circuit() {
    let fixture = copy_fixture();
    let file = fixture.path().join("common/ideas/00_conditions.txt");
    fs::write(&file, "first = { free = yes trigger = { OR = { unsupported = yes tag = AAA } } start = { global_tax_modifier = 0.125 } one = {} }\nsecond = { free = yes trigger = { AND = { unsupported = yes tag = AAA } } one = {} }").unwrap();
    let loaded = VanillaWorld::load(fixture.path(), "1444.11.11").unwrap();
    assert!(
        loaded
            .country("AAA")
            .unwrap()
            .ideas
            .unwrap()
            .groups
            .as_ref()
            .unwrap()
            .contains_key("first")
    );
    assert!(
        loaded
            .country("BBB")
            .unwrap()
            .ideas
            .unwrap()
            .groups
            .is_none()
    );
    assert!(
        loaded.report.ideas.unresolved_countries["BBB"]
            .contains("candidate:first:unsupported_trigger:unsupported")
    );
    fs::write(
        &file,
        "first = { free = yes trigger = { AND = { unsupported = yes tag = AAA } } one = {} }",
    )
    .unwrap();
    let loaded = VanillaWorld::load(fixture.path(), "1444.11.11").unwrap();
    assert!(
        loaded
            .country("AAA")
            .unwrap()
            .ideas
            .unwrap()
            .groups
            .is_none()
    );
    assert!(
        loaded
            .country("BBB")
            .unwrap()
            .ideas
            .unwrap()
            .groups
            .as_ref()
            .unwrap()
            .contains_key("test_common")
    );
}

#[test]
fn flag_history_is_ordered_and_unknown_scoped_mutations_are_not_assumed_false() {
    let fixture = copy_fixture();
    let path = fixture.path().join("history/countries/AAA - Alpha.txt");
    let history = fs::read_to_string(&path).unwrap();
    fs::write(&path, format!("{history}\n1443.1.1 = {{ clr_country_flag = not_implemented }}\n1445.1.1 = {{ set_country_flag = not_implemented }}")).unwrap();
    let loaded = VanillaWorld::load(fixture.path(), "1444.11.11").unwrap();
    assert!(
        !loaded
            .report
            .unapplied_country_fields
            .contains_key("clr_country_flag")
    );
    assert!(
        !loaded
            .report
            .unapplied_country_fields
            .contains_key("set_country_flag")
    );
    assert!(
        loaded
            .country("AAA")
            .unwrap()
            .ideas
            .unwrap()
            .groups
            .as_ref()
            .unwrap()
            .contains_key("test_common")
    );
    fs::write(
        &path,
        format!(
            "{history}\nif = {{ limit = {{ always = yes }} clr_country_flag = not_implemented }}"
        ),
    )
    .unwrap();
    let loaded = VanillaWorld::load(fixture.path(), "1444.11.11").unwrap();
    assert!(loaded.report.unapplied_country_fields.contains_key("if"));
    assert!(
        loaded
            .country("AAA")
            .unwrap()
            .ideas
            .unwrap()
            .groups
            .is_none()
    );
    fs::write(&path, format!("{history}\nadd_idea_group = test_common")).unwrap();
    let loaded = VanillaWorld::load(fixture.path(), "1444.11.11").unwrap();
    assert!(
        loaded.report.ideas.unresolved_countries["AAA"].contains("unapplied_history_selection")
    );
}

#[test]
fn duplicate_properties_preserve_missing_effects_and_duplicate_groups_fail_with_source() {
    let fixture = copy_fixture();
    let path = fixture.path().join("common/ideas/00_conditions.txt");
    fs::write(&path, "first = { free = yes trigger = { NOT = { tag = BBB tag = CCC } } start = { global_tax_modifier = 0.25 global_tax_modifier = 0.5 } one = {} }").unwrap();
    let loaded = VanillaWorld::load(fixture.path(), "1444.11.11").unwrap();
    let effects = loaded.country("AAA").unwrap().declared_idea_effects;
    assert!(
        effects
            .unresolved
            .contains("ideas:traditions:first:unresolved:global_tax_modifier")
    );
    assert!(
        !effects
            .known_totals_micros
            .contains_key("global_tax_modifier")
    );
    assert!(
        loaded
            .country("BBB")
            .unwrap()
            .ideas
            .unwrap()
            .groups
            .as_ref()
            .unwrap()
            .contains_key("test_common")
    );
    fs::write(&path, "test_alpha = { free = yes one = {} }").unwrap();
    let error = VanillaWorld::load(fixture.path(), "1444.11.11")
        .err()
        .unwrap()
        .to_string();
    assert!(error.contains("ambiguous idea group definition") && error.contains("test_ideas.txt"));
}

#[test]
fn translated_and_native_progress_have_the_same_live_attribute_effects() {
    let mut loaded = VanillaWorld::load(fixture(), "1444.11.11").unwrap();
    let province = loaded.initial.province_entity(2).unwrap();
    let alpha = loaded.initial.country_entity("AAA").unwrap();
    let before = loaded
        .initial
        .world()
        .attributes()
        .evaluate(loaded.initial.world(), province, "tax_basis")
        .unwrap();
    let mut ideas = loaded.initial.world().ideas().clone();
    ideas
        .countries
        .get_mut(&alpha)
        .unwrap()
        .groups
        .as_mut()
        .unwrap()
        .insert("test_alpha".into(), Some(2));
    loaded.initial.set_ideas(ideas).unwrap();
    let after = loaded
        .initial
        .world()
        .attributes()
        .evaluate(loaded.initial.world(), province, "tax_basis")
        .unwrap();
    assert!(after.known_subtotal_micros > before.known_subtotal_micros);
    assert!(
        after.steps[0]
            .contributions
            .iter()
            .any(|c| c.source == "ideas:completion:test_alpha" && c.micros == 500_000)
    );
    let saved = loaded.save().unwrap();
    loaded.restore(&saved).unwrap();
    assert_eq!(
        after,
        loaded
            .initial
            .world()
            .attributes()
            .evaluate(loaded.initial.world(), province, "tax_basis")
            .unwrap()
    );
}

#[test]
fn uppercase_tag_paid_groups_and_unlock_scripts_keep_their_actual_scope() {
    let fixture = copy_fixture();
    fs::write(fixture.path().join("common/ideas/00_conditions.txt"), "paid = { category = ADM important = yes trigger = { always = yes } one = { global_tax_modifier = 5 } }\nfirst = { free = yes trigger = { TAG = AAA } start = { global_tax_modifier = 0.125 } one = { effect = { add_country_modifier = { name = deferred } } } bonus = {} }").unwrap();
    let mut loaded = VanillaWorld::load(fixture.path(), "1444.11.11").unwrap();
    assert_eq!(loaded.report.ideas.skipped_paid_groups, 1);
    assert!(!loaded.initial.world().ideas().groups.contains_key("paid"));
    assert_eq!(loaded.initial.world().ideas().known_ideas["one"]["paid"], 0);
    assert_eq!(
        loaded
            .country("AAA")
            .unwrap()
            .declared_idea_effects
            .known_totals_micros["global_tax_modifier"],
        125_000
    );
    let entity = loaded.initial.country_entity("AAA").unwrap();
    assert!(
        loaded
            .initial
            .world()
            .ideas()
            .property_effects(entity, "global_tax_modifier")
            .unwrap()
            .unresolved
            .is_empty()
    );
    let mut ideas = loaded.initial.world().ideas().clone();
    ideas
        .countries
        .get_mut(&entity)
        .unwrap()
        .groups
        .as_mut()
        .unwrap()
        .insert("first".into(), Some(1));
    loaded.initial.set_ideas(ideas).unwrap();
    assert!(
        loaded
            .country("AAA")
            .unwrap()
            .declared_idea_effects
            .unresolved
            .contains("ideas:idea:first:one:unexecuted:unlock_script")
    );
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
    assert!(
        !loaded
            .report
            .unapplied_country_fields
            .contains_key("set_country_flag")
    );
    assert!(
        !loaded
            .report
            .unapplied_province_fields
            .contains_key("discovered_by")
    );
    assert!(
        !loaded
            .report
            .unapplied_examples
            .contains_key("province:discovered_by")
    );
    assert_eq!(loaded.report.province_discovery_states, 1);
    let north = loaded.initial.province_entity(1).unwrap();
    let alpha = loaded.initial.country_entity("AAA").unwrap();
    assert_eq!(
        loaded
            .initial
            .world()
            .province_discovered_by(north, alpha)
            .unwrap(),
        Some(true)
    );
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
    let mut native = World::eu4_seeded(schema, vec![], 0);
    native.set_eu4_event_seed(None);
    let alpha = native.add_entity("AAA", EntityKind::Country);
    let beta = native.add_entity("BBB", EntityKind::Country);
    let north = native.add_entity("1", EntityKind::Province);
    let east = native.add_entity("2", EntityKind::Province);
    native.set_province_has_port(north, false).unwrap();
    native.set_province_has_port(east, true).unwrap();
    native
        .set_province_center_of_trade_level(north, Some(0))
        .unwrap();
    native
        .set_province_center_of_trade_level(east, Some(2))
        .unwrap();
    for id in 3..=6 {
        let province = native.add_entity(id.to_string(), EntityKind::Province);
        native.set_province_has_port(province, false).unwrap();
        if id == 4 {
            native
                .set_province_center_of_trade_level(province, Some(0))
                .unwrap();
        }
    }
    native
        .set_map_knowledge(MapKnowledge {
            provinces: BTreeMap::from([(
                north,
                ProvinceDiscovery {
                    discovered_by: BTreeSet::from([alpha]),
                    unresolved_countries: BTreeSet::new(),
                },
            )]),
        })
        .unwrap();
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
            "0": {"statuses":{"at_war":null,"client_nation":false,"colonial_nation":false,"federation_nation":false,"revolution_target":false},"national_focus":{"skill":null},"ruler":{"name":"Iris","dynasty":"Sun","birth_date":{"year":1410,"month":1,"day":1},"administration":2,"diplomacy":3,"military":4},
                "heir":{"name":"Rowan","regnal_name":"Rowan I","dynasty":"Sun","birth_date":{"year":1440,"month":1,"day":1},"historical_death_date":{"year":1444,"month":11,"day":12},"administration":1,"diplomacy":2,"military":3,"claim_milli":87125,"block_disinherit":true},
                "consort":{"name":"Vale","country_of_origin":1,"female":true,"birth_date":{"year":1415,"month":1,"day":1},"historical_death_date":{"year":1450,"month":1,"day":1}}},
            "1": {"statuses":{"at_war":null,"client_nation":false,"colonial_nation":false,"federation_nation":false,"revolution_target":false},"national_focus":{"skill":null},"ruler":{"name":"Council","regent":true,"administration":0,"diplomacy":0,"military":0}}
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
    native
        .set_governments(
            serde_json::from_str(include_str!(
                "../../../examples/world/native_governments.json"
            ))
            .unwrap(),
        )
        .unwrap();
    native
        .set_ideas(
            serde_json::from_str(include_str!("../../../examples/world/native_ideas.json"))
                .unwrap(),
        )
        .unwrap();
    native
        .set_affiliations(
            serde_json::from_str(include_str!(
                "../../../examples/world/native_affiliations.json"
            ))
            .unwrap(),
        )
        .unwrap();
    native
        .set_economy(
            serde_json::from_str(
                r#"{
        "buildings": {
            "test_market":{"cost_milli":87125,"construction_months":9},
            "test_fort":{"cost_milli":200000,"construction_months":18}
        },
        "trade_goods": {
            "grain":{"runtime_index":1,"base_price_milli":2125},
            "cloth":{"runtime_index":2,"base_price_milli":3000},
            "gold":{"runtime_index":3,"base_price_milli":0,"gold_type":true}
        },
        "trade_nodes": {
            "river":{"location":2,"members":[2],"outgoing":["harbor"],"inland":true},
            "harbor":{"location":3,"members":[3,4],"outgoing":[],"end":true}
        },
        "provinces": {
            "2":{"buildings":["test_fort"],"trade_good":"grain"},
            "3":{"buildings":["test_market"],"trade_good":"cloth"},
            "5":{"buildings":[]}
        }
    }"#,
            )
            .unwrap(),
        )
        .unwrap();
    native
        .set_trade_node_route_orders(&BTreeMap::from([
            ("river".into(), vec!["harbor".into()]),
            ("harbor".into(), Vec::new()),
        ]))
        .unwrap();
    let mut technology = augustus_core::Technology::default();
    for branch in ["administrative", "diplomatic", "military"] {
        let mut levels: Vec<_> = [1300, 1340, 1390, 1430]
            .into_iter()
            .map(|year| augustus_core::TechnologyLevel {
                year,
                values_micros: Default::default(),
                flags: Default::default(),
                enabled_units: Vec::new(),
                unresolved_properties: Default::default(),
            })
            .collect();
        match branch {
            "administrative" => {
                levels[0].values_micros.insert("test_yield".into(), 12500);
                levels[3].flags.insert("test_capability".into(), true);
            }
            "diplomatic" => {
                levels[3].values_micros.insert("test_range".into(), 7125000);
                levels[3].enabled_units.push("sample_light_ship".into());
            }
            _ => {
                levels[0]
                    .values_micros
                    .insert("test_strength".into(), 1250000);
                levels[0].enabled_units = vec![
                    "test_infantry".into(),
                    "test_cavalry".into(),
                    "test_artillery".into(),
                ];
            }
        }
        technology.tables.insert(branch.into(), levels);
    }
    technology.countries.insert(
        alpha,
        ["administrative", "diplomatic", "military"]
            .into_iter()
            .map(|b| (b.into(), 3))
            .collect(),
    );
    native.set_technology(technology).unwrap();
    native
        .set_unit_templates(augustus_core::UnitTemplates {
            definitions: [
                (
                    "sample_light_ship".into(),
                    augustus_core::UnitTemplate {
                        category: "light_ship".into(),
                        properties: vec![
                            ("unit_type", "sample_group"),
                            ("hull_size", "11"),
                            ("base_cannons", "7"),
                            ("sail_speed", "6"),
                            ("trade_power", "1.25"),
                            ("sailors", "25"),
                        ]
                        .into_iter()
                        .map(|(name, value)| augustus_core::UnitTemplateProperty {
                            name: name.into(),
                            value: value.into(),
                        })
                        .collect(),
                    },
                ),
                (
                    "test_infantry".into(),
                    augustus_core::UnitTemplate {
                        category: "infantry".into(),
                        properties: vec![("unit_type", "test_group")]
                            .into_iter()
                            .map(|(name, value)| augustus_core::UnitTemplateProperty {
                                name: name.into(),
                                value: value.into(),
                            })
                            .collect(),
                    },
                ),
                (
                    "test_cavalry".into(),
                    augustus_core::UnitTemplate {
                        category: "cavalry".into(),
                        properties: vec![augustus_core::UnitTemplateProperty {
                            name: "unit_type".into(),
                            value: "test_group".into(),
                        }],
                    },
                ),
                (
                    "test_artillery".into(),
                    augustus_core::UnitTemplate {
                        category: "artillery".into(),
                        properties: vec![augustus_core::UnitTemplateProperty {
                            name: "unit_type".into(),
                            value: "test_group".into(),
                        }],
                    },
                ),
            ]
            .into_iter()
            .collect(),
        })
        .unwrap();
    native
        .set_country_resources(
            [
                (
                    alpha,
                    augustus_core::CountryResources {
                        mercantilism_milli: Some(18500),
                        ..Default::default()
                    },
                ),
                (beta, augustus_core::CountryResources::default()),
            ]
            .into_iter()
            .collect(),
        )
        .unwrap();
    native.set_modifiers(serde_json::from_str(r#"{
        "definitions": {
            "test_river":{"values_micros":{"test_trade_value":7125000,"test_growth":-2500},"flags":{}},
            "test_harbor":{"values_micros":{"test_trade_value":1125000},"flags":{"test_capability":true}},
            "test_state":{"values_micros":{"test_tax_modifier":12500},"flags":{}},
            "test_empty":{"values_micros":{},"flags":{}}
        },
        "active":{"0":{"test_state":{"hidden":true}},"2":{"test_harbor":{},"test_river":{}}}
    }"#).unwrap()).unwrap();
    native
        .set_metrics(
            serde_json::from_str(include_str!("../../../examples/world/native_metrics.json"))
                .unwrap(),
        )
        .unwrap();
    native
        .set_attributes(
            serde_json::from_str(include_str!(
                "../../../examples/world/native_attributes.json"
            ))
            .unwrap(),
        )
        .unwrap();
    native
        .set_administration(
            serde_json::from_str(include_str!(
                "../../../examples/world/native_administration.json"
            ))
            .unwrap(),
        )
        .unwrap();
    native
        .set_calendar(augustus_core::Calendar {
            start: augustus_core::WorldDate {
                year: 1444,
                month: 11,
                day: 11,
            },
            month_lengths: [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31],
        })
        .unwrap();
    native
        .set_taxation(augustus_core::Taxation::new(
            "tax_after_autonomy",
            [alpha, beta].into_iter().collect(),
        ))
        .unwrap();
    native
        .set_monthly_accruals(
            serde_json::from_str(include_str!("../../../examples/world/native_accruals.json"))
                .unwrap(),
        )
        .unwrap();
    native
        .set_monthly_convergence(Some(
            serde_json::from_str(include_str!(
                "../../../examples/world/native_convergence.json"
            ))
            .unwrap(),
        ))
        .unwrap();
    native
        .set_named_values(
            serde_json::from_str(
                r#"{"entities":{"0":{},"1":{},"2":{},"3":{},"4":{},"5":{},"6":{},"7":{}}}"#,
            )
            .unwrap(),
        )
        .unwrap();
    native
        .set_markers(augustus_core::Markers {
            entities: [
                (alpha, [("not_implemented".into(), true)].into()),
                (beta, Default::default()),
            ]
            .into(),
            ..Default::default()
        })
        .unwrap();
    native
        .set_global_markers(augustus_core::GlobalMarkers {
            known: true,
            ..Default::default()
        })
        .unwrap();
    native
        .set_subjects(augustus_core::Subjects {
            types: ["default", "test_vassal"].map(String::from).into(),
            known: true,
            counts: [
                (augustus_core::EntityId(0), 1),
                (augustus_core::EntityId(1), 0),
            ]
            .into(),
        })
        .unwrap();
    for index in 4..8 {
        native
            .mark_target_absent(augustus_core::EntityId(index), augustus_core::Target::Owner)
            .unwrap();
    }
    let mut native_ideas = native.ideas().clone();
    native_ideas.known_ideas = loaded.initial.world().ideas().known_ideas.clone();
    native.set_ideas(native_ideas).unwrap();
    assert_eq!(
        loaded.initial.world().governments(),
        native.governments(),
        "government state differs"
    );
    assert_eq!(loaded.initial.world().politics(), native.politics());
    assert_eq!(loaded.initial.world().attributes(), native.attributes());
    assert_eq!(
        loaded.initial.world().affiliations(),
        native.affiliations(),
        "affiliation state differs"
    );
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
fn external_starting_units_are_bound_before_startup_and_saved() {
    let baseline = VanillaWorld::load(fixture(), "1444.11.11").unwrap();
    let port = baseline
        .initial
        .provinces()
        .iter()
        .find(|province| {
            baseline
                .initial
                .province_entity(province.id)
                .and_then(|entity| baseline.initial.world().province_has_port(entity).ok())
                == Some(true)
        })
        .map(|province| province.id)
        .expect("fixture must contain a port province");

    let mut external = VanillaExternalState::default();
    external.armies.push(VanillaExternalArmy {
        owner: "AAA".into(),
        location: 1,
        name: "Northern Army".into(),
        regiments: vec!["infantry".into(), "cavalry".into()],
    });
    external.naval_units.push(VanillaExternalNavalUnit {
        owner: "AAA".into(),
        location: port,
        unit_type: "light_ship".into(),
    });

    let mut loaded =
        VanillaWorld::load_with_external_state(fixture(), "1444.11.11", &external).unwrap();
    assert_eq!(loaded.initial.armies().len(), 1);
    assert_eq!(loaded.initial.armies()[0].regiments.len(), 2);
    assert_eq!(loaded.initial.naval_units().len(), 1);
    assert_eq!(loaded.initial.fleets().len(), 1);
    assert_eq!(loaded.initial.fleets()[0].ships.len(), 1);
    assert_eq!(
        loaded.initial.naval_units()[0].location.0,
        loaded.initial.province_entity(port).unwrap().0
    );

    let snapshot = loaded.save().unwrap();
    loaded.restore(&snapshot).unwrap();
    assert_eq!(loaded.initial.armies().len(), 1);
    assert_eq!(loaded.initial.naval_units().len(), 1);
    assert_eq!(loaded.initial.fleets().len(), 1);

    let grouped = VanillaExternalState {
        naval_fleets: vec![VanillaExternalFleet {
            owner: "AAA".into(),
            location: port,
            name: Some("Test Navy".into()),
            ship_types: vec!["galley".into(), "light_ship".into()],
        }],
        ..Default::default()
    };
    let grouped =
        VanillaWorld::load_with_external_state(fixture(), "1444.11.11", &grouped).unwrap();
    assert_eq!(grouped.initial.naval_units().len(), 2);
    assert_eq!(grouped.initial.fleets().len(), 1);
    assert_eq!(
        grouped.initial.fleets()[0].name.as_deref(),
        Some("Test Navy")
    );
    assert_eq!(grouped.initial.fleets()[0].ships.len(), 2);
}

#[test]
fn external_national_focus_replaces_the_content_value_before_startup() {
    let mut external = VanillaExternalState::default();
    external.country_national_focus.insert(
        "AAA".into(),
        NationalFocus {
            skill: Some(RulerSkill::Military),
        },
    );
    let loaded =
        VanillaWorld::load_with_external_state(fixture(), "1444.11.11", &external).unwrap();
    let country = loaded.initial.country_entity("AAA").unwrap();
    assert_eq!(
        loaded.initial.world().national_focus(country).unwrap(),
        NationalFocus {
            skill: Some(RulerSkill::Military)
        }
    );
}

#[test]
fn external_advisors_are_translated_by_content_type_before_startup() {
    let root = copy_fixture();
    fs::create_dir_all(root.path().join("common/advisortypes")).unwrap();
    fs::write(
        root.path().join("common/advisortypes/test.txt"),
        "treasurer = { monarch_power = ADM }",
    )
    .unwrap();
    let mut external = VanillaExternalState::default();
    external.advisors.insert(
        "AAA".into(),
        vec![VanillaExternalAdvisor {
            type_name: "treasurer".into(),
            skill: 2,
            id: Some(17),
        }],
    );
    let loaded =
        VanillaWorld::load_with_external_state(root.path(), "1444.11.11", &external).unwrap();
    let country = loaded.initial.country_entity("AAA").unwrap();
    let advisors = loaded
        .initial
        .world()
        .advisors()
        .countries
        .get(&country)
        .and_then(Option::as_ref)
        .unwrap();
    assert_eq!(advisors[0].type_name, "treasurer");
    assert_eq!(advisors[0].category, RulerSkill::Administration);
    assert_eq!(advisors[0].skill, 2);
    let view = loaded.country("AAA").unwrap();
    assert!(view.advisors_known);
    assert_eq!(view.advisors.unwrap()[0].type_name, "treasurer");
    assert_eq!(view.advisors.unwrap()[0].skill, 2);
    let unknown = loaded.country("BBB").unwrap();
    assert!(!unknown.advisors_known);
    assert!(unknown.advisors.is_none());
}

#[test]
fn country_view_reports_known_advisor_slots_without_claiming_a_full_roster() {
    let root = copy_fixture();
    fs::create_dir_all(root.path().join("common/advisortypes")).unwrap();
    fs::write(
        root.path().join("common/advisortypes/test.txt"),
        "advisor = { monarch_power = ADM } army_reformer = { monarch_power = MIL }",
    )
    .unwrap();
    fs::create_dir_all(root.path().join("common/on_actions")).unwrap();
    fs::write(
        root.path().join("common/on_actions/advisor_slots.txt"),
        "on_startup = { if = { limit = { tag = AAA } country_event = { id = probe.1 days = 1 } } }",
    )
    .unwrap();
    fs::create_dir_all(root.path().join("events")).unwrap();
    fs::write(
        root.path().join("events/advisor_slots.txt"),
        "namespace = probe country_event = { id = probe.1 is_triggered_only = yes immediate = { remove_advisor_by_category = DIP hire_advisor = { type = army_reformer skill = 1 } } }",
    )
    .unwrap();

    let mut loaded = VanillaWorld::load(root.path(), "1444.11.11").unwrap();
    let initial = loaded.country("AAA").unwrap();
    assert!(!initial.advisors_known);
    assert!(initial.advisors.is_none());
    assert!(initial.advisor_slots.is_none());

    loaded.advance(1).unwrap();

    let view = loaded.country("AAA").unwrap();
    assert!(!view.advisors_known);
    assert!(view.advisors.is_none());
    let slots = view.advisor_slots.unwrap();
    assert_eq!(slots.len(), 2);
    assert!(!slots.contains_key(&RulerSkill::Administration));
    assert!(slots[&RulerSkill::Diplomacy].is_none());
    let military = slots[&RulerSkill::Military].as_ref().unwrap();
    assert_eq!(military.type_name, "army_reformer");
    assert_eq!(military.skill, 1);

    let serialized = serde_json::to_value(loaded.country("AAA").unwrap()).unwrap();
    let serialized_slots = serialized["advisor_slots"].as_object().unwrap();
    assert!(!serialized_slots.contains_key("Administration"));
    assert_eq!(serialized_slots["Diplomacy"], serde_json::Value::Null);
    assert_eq!(serialized_slots["Military"]["type_name"], "army_reformer");
}

#[test]
fn runtime_advisor_binding_refreshes_live_power_income_sources() {
    let root = copy_fixture();
    fs::create_dir_all(root.path().join("common/advisortypes")).unwrap();
    fs::write(
        root.path().join("common/advisortypes/test.txt"),
        "treasurer = { monarch_power = ADM }",
    )
    .unwrap();
    let mut loaded = VanillaWorld::load(root.path(), "1444.11.11").unwrap();
    let country = loaded.initial.country_entity("AAA").unwrap();
    let before = loaded
        .initial
        .world()
        .attributes()
        .evaluate(loaded.initial.world(), country, "monthly_adm")
        .unwrap();
    assert!(
        before
            .unresolved
            .contains("power_income:advisors_not_initialized")
    );

    loaded
        .set_advisors(BTreeMap::from([(
            "AAA".into(),
            vec![VanillaExternalAdvisor {
                type_name: "treasurer".into(),
                skill: 2,
                id: Some(17),
            }],
        )]))
        .unwrap();
    let after = loaded
        .initial
        .world()
        .attributes()
        .evaluate(loaded.initial.world(), country, "monthly_adm")
        .unwrap();
    assert!(
        !after
            .unresolved
            .contains("power_income:advisors_not_initialized")
    );
    assert!(
        after
            .steps
            .iter()
            .flat_map(|step| &step.contributions)
            .any(|contribution| {
                contribution.source == "advisor:treasurer"
                    && contribution.property == "skill"
                    && contribution.micros == 2_000_000
            })
    );
    assert_eq!(loaded.country("AAA").unwrap().advisors.unwrap()[0].skill, 2);
}

#[test]
fn external_power_projection_adds_the_minimum_power_bonus() {
    for (projection, expected_bonus) in [(50_000, true), (49_999, false)] {
        let root = copy_fixture();
        let external = VanillaExternalState {
            country_metric_values: BTreeMap::from([(
                "AAA".into(),
                BTreeMap::from([("power_projection".into(), Some(projection))]),
            )]),
            ..Default::default()
        };
        let loaded =
            VanillaWorld::load_with_external_state(root.path(), "1444.11.11", &external).unwrap();
        let country = loaded.initial.country_entity("AAA").unwrap();
        assert_eq!(
            loaded.country("AAA").unwrap().metrics.unwrap()["power_projection"],
            Some(projection)
        );
        let evaluation = loaded
            .initial
            .world()
            .attributes()
            .evaluate(loaded.initial.world(), country, "monthly_adm")
            .unwrap();
        assert_eq!(
            evaluation
                .steps
                .iter()
                .flat_map(|step| step.contributions.iter())
                .any(|contribution| contribution.source == "metric:power_projection"),
            expected_bonus
        );
    }
    let root = copy_fixture();
    let loaded = VanillaWorld::load_with_external_state(
        root.path(),
        "1444.11.11",
        &VanillaExternalState::default(),
    )
    .unwrap();
    let country = loaded.initial.country_entity("AAA").unwrap();
    let evaluation = loaded
        .initial
        .world()
        .attributes()
        .evaluate(loaded.initial.world(), country, "monthly_adm")
        .unwrap();
    assert!(
        evaluation
            .unresolved
            .contains("metric:power_projection:unknown")
    );
}

#[test]
fn loaded_country_power_modifier_reaches_monthly_income_sources() {
    let root = copy_fixture();
    let modifiers = root
        .path()
        .join("common/event_modifiers/test_modifiers.txt");
    let text = fs::read_to_string(&modifiers).unwrap();
    fs::write(
        modifiers,
        text.replace(
            "test_state = { test_tax_modifier = 0.0125 }",
            "test_state = { test_tax_modifier = 0.0125 country_admin_power = 1 }",
        ),
    )
    .unwrap();

    let loaded = VanillaWorld::load(root.path(), "1444.11.11").unwrap();
    let country = loaded.initial.country_entity("AAA").unwrap();
    let evaluation = loaded
        .initial
        .world()
        .attributes()
        .evaluate(loaded.initial.world(), country, "monthly_adm")
        .unwrap();

    assert!(
        evaluation
            .steps
            .iter()
            .flat_map(|step| &step.contributions)
            .any(|contribution| {
                contribution.source == "modifier:AAA:test_state"
                    && contribution.property == "country_admin_power"
                    && contribution.micros == 1_000_000
            })
    );
    assert!(
        evaluation
            .unresolved
            .contains("power_income:remaining_sources_not_bound")
    );
}

#[test]
fn external_monthly_power_income_overrides_unresolved_producers() {
    let root = copy_fixture();
    let defines = root.path().join("common/defines.lua");
    let text = fs::read_to_string(&defines).unwrap();
    fs::write(
        &defines,
        text.replace(
            "HRE_RANK = 3,",
            "HRE_RANK = 3,\n        DEBT_LIMIT_DIP = -999,\n        POWER_MAX = 999,",
        ),
    )
    .unwrap();

    let external = VanillaExternalState {
        country_metric_values: BTreeMap::from([(
            "AAA".into(),
            BTreeMap::from([
                ("adm_power".into(), Some(10)),
                ("dip_power".into(), Some(10)),
                ("mil_power".into(), Some(10)),
            ]),
        )]),
        country_metric_ceilings: BTreeMap::from([(
            "AAA".into(),
            BTreeMap::from([
                ("adm_power".into(), Some(20)),
                ("dip_power".into(), Some(20)),
                ("mil_power".into(), Some(20)),
            ]),
        )]),
        country_monthly_power_income: BTreeMap::from([(
            "AAA".into(),
            BTreeMap::from([
                ("adm_power".into(), Some(7)),
                ("dip_power".into(), Some(6)),
                ("mil_power".into(), Some(5)),
            ]),
        )]),
        ..Default::default()
    };

    let mut loaded =
        VanillaWorld::load_with_external_state(root.path(), "1444.11.11", &external).unwrap();
    let entity = loaded.initial.country_entity("AAA").unwrap();
    let evaluation = loaded
        .initial
        .world()
        .attributes()
        .evaluate(loaded.initial.world(), entity, "monthly_adm")
        .unwrap();
    assert_eq!(evaluation.value_micros, Some(7_000_000));
    assert!(evaluation.unresolved.is_empty());

    loaded.advance(20).unwrap();
    assert_eq!(
        loaded.country("AAA").unwrap().metrics.unwrap()["adm_power"],
        Some(17)
    );
    assert_eq!(
        loaded.country("AAA").unwrap().metrics.unwrap()["dip_power"],
        Some(16)
    );
    assert_eq!(
        loaded.country("AAA").unwrap().metrics.unwrap()["mil_power"],
        Some(15)
    );
}

#[test]
fn runtime_monthly_power_income_refresh_replaces_only_selected_formulas() {
    let root = copy_fixture();
    let defines = root.path().join("common/defines.lua");
    let text = fs::read_to_string(&defines).unwrap();
    fs::write(
        &defines,
        text.replace(
            "HRE_RANK = 3,",
            "HRE_RANK = 3,\n        DEBT_LIMIT_DIP = -999,\n        POWER_MAX = 999,",
        ),
    )
    .unwrap();
    let external = VanillaExternalState {
        country_metric_values: BTreeMap::from([(
            "AAA".into(),
            BTreeMap::from([
                ("adm_power".into(), Some(10)),
                ("dip_power".into(), Some(10)),
                ("mil_power".into(), Some(10)),
            ]),
        )]),
        country_metric_ceilings: BTreeMap::from([(
            "AAA".into(),
            BTreeMap::from([
                ("adm_power".into(), Some(20)),
                ("dip_power".into(), Some(20)),
                ("mil_power".into(), Some(20)),
            ]),
        )]),
        ..Default::default()
    };
    let mut loaded =
        VanillaWorld::load_with_external_state(root.path(), "1444.11.11", &external).unwrap();
    loaded
        .set_country_monthly_power_income(BTreeMap::from([(
            "AAA".into(),
            BTreeMap::from([
                ("adm_power".into(), Some(8)),
                ("dip_power".into(), Some(9)),
                ("mil_power".into(), Some(10)),
            ]),
        )]))
        .unwrap();
    let entity = loaded.initial.country_entity("AAA").unwrap();
    let evaluation = loaded
        .initial
        .world()
        .attributes()
        .evaluate(loaded.initial.world(), entity, "monthly_adm")
        .unwrap();
    assert_eq!(evaluation.value_micros, Some(8_000_000));
    loaded.advance(20).unwrap();
    assert_eq!(
        loaded.country("AAA").unwrap().metrics.unwrap()["adm_power"],
        Some(18)
    );
    assert_eq!(
        loaded.country("AAA").unwrap().metrics.unwrap()["dip_power"],
        Some(19)
    );
    assert_eq!(
        loaded.country("AAA").unwrap().metrics.unwrap()["mil_power"],
        Some(20)
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

#[test]
fn loaded_vanilla_world_can_select_and_resume_a_monthly_pipeline() {
    let mut loaded = VanillaWorld::load(fixture(), "1444.11.11").unwrap();
    let country = loaded.initial.country_entity("AAA").unwrap();
    let world_phase = PipelinePhase {
        id: "fixture-world:v1",
        country,
        message: "world",
    };
    let parallel_phase = PipelinePhase {
        id: "fixture-parallel:v1",
        country,
        message: "parallel",
    };
    let serial_phase = PipelinePhase {
        id: "fixture-serial:v1",
        country,
        message: "serial",
    };
    let pipeline = MonthlyPipeline::new(
        "fixture-monthly-pipeline:v1",
        Some(&world_phase),
        Some(&parallel_phase),
        Some(&serial_phase),
    )
    .unwrap();

    loaded.select_monthly_system(&pipeline).unwrap();
    loaded
        .advance_with_systems(21, None, Some(&pipeline))
        .unwrap();
    assert_eq!(
        loaded
            .initial
            .world()
            .events()
            .iter()
            .map(|event| event.2.as_str())
            .collect::<Vec<_>>(),
        ["world", "parallel", "serial"]
    );

    let save = loaded.save().unwrap();
    let mut resumed = VanillaWorld::load(fixture(), "1444.11.11").unwrap();
    resumed.select_monthly_system(&pipeline).unwrap();
    resumed.restore(&save).unwrap();
    assert_eq!(resumed.initial.world().day(), 21);
    assert_eq!(
        resumed.initial.world().monthly_system_id(),
        Some("fixture-monthly-pipeline:v1")
    );
}

#[test]
fn loaded_vanilla_world_can_run_standard_economy_inside_a_monthly_pipeline() {
    let state = VanillaExternalState {
        country_resources: BTreeMap::from([(
            "AAA".into(),
            CountryResources {
                treasury_milli: Some(10_000),
                ..Default::default()
            },
        )]),
        province_annual_tax_milli: BTreeMap::from([(1, Some(12_000)), (2, Some(8_000))]),
        production_inputs: BTreeMap::from([(
            2,
            VanillaProductionInputState {
                input: Some(VanillaProductionInput {
                    base_value_milli: 1_000,
                    country_modifier_milli: 20,
                    legacy_province_modifier_milli: 0,
                    queried_province_modifier_milli: -10,
                }),
                unresolved: BTreeSet::new(),
            },
        )]),
        country_monthly_cashflows_milli: BTreeMap::from([("AAA".into(), None)]),
        country_monthly_cashflow_breakdowns: BTreeMap::from([(
            "AAA".into(),
            CashflowBreakdown {
                trade_milli: None,
                trade_income_pass: Some(VanillaTradeIncomePassInput {
                    eligible_records: vec![VanillaTradeIncomeInput {
                        node_value_milli: 3_817,
                        country_record_value_milli: 349,
                        base_context_modifier_milli: 20,
                        selected_context_modifier_milli: 50,
                        virtual_context_modifier_milli: None,
                        minimum_context_modifier_milli: 10,
                        maximum_context_modifier_milli: 2_000,
                        post_context_modifier_milli: 100,
                    }],
                }),
                gold_milli: Some(100),
                expenses_milli: Some(50),
                expense_categories_milli: BTreeMap::new(),
            },
        )]),
        ..Default::default()
    };
    let mut loaded =
        VanillaWorld::load_with_external_state(fixture(), "1444.11.11", &state).unwrap();
    let country = loaded.initial.country_entity("AAA").unwrap();
    let before = loaded.initial.world().country_resources()[&country]
        .treasury_milli
        .unwrap();
    let taxation = loaded.initial.world().taxation().unwrap().clone();
    let production = loaded.initial.world().monthly_production().unwrap().clone();
    let cashflows = loaded.initial.world().monthly_cashflows().unwrap().clone();
    let accruals = loaded.initial.world().monthly_accruals().unwrap().clone();
    let convergence = loaded
        .initial
        .world()
        .monthly_convergence()
        .unwrap()
        .clone();

    let economy = MonthlyEconomy;
    let parallel = PipelinePhase {
        id: "vanilla-parallel-after-economy:v1",
        country,
        message: "parallel",
    };
    let serial = MonthlyRelationExpensePhase {
        country,
        amount_milli: 25,
        message: "serial",
    };
    let pipeline = MonthlyPipeline::new(
        "vanilla-standard-economy-pipeline:v1",
        Some(&economy),
        Some(&parallel),
        Some(&serial),
    )
    .unwrap();

    loaded.select_monthly_system(&pipeline).unwrap();
    loaded.initial.set_taxation(taxation).unwrap();
    loaded.initial.set_monthly_production(production).unwrap();
    loaded
        .initial
        .set_monthly_cashflows(Some(cashflows))
        .unwrap();
    loaded.initial.set_monthly_accruals(accruals).unwrap();
    loaded
        .initial
        .set_monthly_convergence(Some(convergence))
        .unwrap();
    loaded
        .advance_with_systems(21, None, Some(&pipeline))
        .unwrap();

    {
        let world = loaded.initial.world();
        let tax = &world
            .taxation()
            .unwrap()
            .last_settlement
            .as_ref()
            .unwrap()
            .accounts[&country];
        let production = &world
            .monthly_production()
            .unwrap()
            .last_settlement
            .as_ref()
            .unwrap()
            .accounts[&country];
        let cashflow = &world
            .monthly_cashflows()
            .unwrap()
            .last_settlement
            .as_ref()
            .unwrap()
            .accounts[&country];
        let tax_credit = tax.credited_milli.expect("known tax credit");
        let production_credit = production.credited_milli.expect("known production credit");
        let cashflow_credit = cashflow.credited_milli.expect("known cashflow credit");
        assert_eq!(tax_credit, 666);
        assert_eq!(production_credit, 84);
        assert_eq!(cashflow_credit, 1_608);
        assert_eq!(before, 10_000);
        assert_eq!(
            world.country_resources()[&country].treasury_milli,
            Some(12_333)
        );
        assert_eq!(
            world.country_resources()[&country].treasury_milli,
            Some(before + tax_credit + production_credit + cashflow_credit - 25)
        );
        let ledger = &world.country_ledgers()[&country];
        assert_eq!(
            ledger.income_sources_milli[COUNTRY_LEDGER_INCOME_TAX_CATEGORY as usize],
            666
        );
        assert_eq!(
            ledger.income_sources_milli[COUNTRY_LEDGER_INCOME_PRODUCTION_CATEGORY as usize],
            84
        );
        assert_eq!(
            ledger.income_sources_milli[COUNTRY_LEDGER_INCOME_TRADE_CATEGORY as usize],
            1_558
        );
        assert_eq!(
            ledger.income_sources_milli[COUNTRY_LEDGER_INCOME_GOLD_CATEGORY as usize],
            100
        );
        assert_eq!(
            ledger.expense_sources_milli[COUNTRY_LEDGER_SEND_OFFICERS_CATEGORY as usize],
            25
        );
        assert_eq!(
            world
                .events()
                .iter()
                .map(|event| event.2.as_str())
                .collect::<Vec<_>>(),
            ["parallel", "serial"]
        );
    }
    loaded
        .consolidate_country_ledgers_monthly(BTreeMap::new())
        .unwrap();
    let ledger = &loaded.initial.world().country_ledgers()[&country];
    assert_eq!(
        ledger.income_snapshot_milli[COUNTRY_LEDGER_INCOME_TAX_CATEGORY as usize],
        666
    );
    assert_eq!(
        ledger.income_snapshot_milli[COUNTRY_LEDGER_INCOME_PRODUCTION_CATEGORY as usize],
        84
    );
    assert_eq!(
        ledger.income_snapshot_milli[COUNTRY_LEDGER_INCOME_TRADE_CATEGORY as usize],
        1_558
    );
    assert_eq!(
        ledger.income_snapshot_milli[COUNTRY_LEDGER_INCOME_GOLD_CATEGORY as usize],
        100
    );
    assert_eq!(
        ledger.expense_snapshot_milli[COUNTRY_LEDGER_SEND_OFFICERS_CATEGORY as usize],
        25
    );
    assert_eq!(ledger.expense_total_milli, 25);
    assert_eq!(ledger.expense_filtered_total_milli, 25);

    let save = loaded.save().unwrap();
    let mut resumed =
        VanillaWorld::load_with_external_state(fixture(), "1444.11.11", &state).unwrap();
    let resumed_taxation = resumed.initial.world().taxation().unwrap().clone();
    let resumed_production = resumed
        .initial
        .world()
        .monthly_production()
        .unwrap()
        .clone();
    let resumed_cashflows = resumed.initial.world().monthly_cashflows().unwrap().clone();
    let resumed_accruals = resumed.initial.world().monthly_accruals().unwrap().clone();
    let resumed_convergence = resumed
        .initial
        .world()
        .monthly_convergence()
        .unwrap()
        .clone();
    resumed.select_monthly_system(&pipeline).unwrap();
    resumed.initial.set_taxation(resumed_taxation).unwrap();
    resumed
        .initial
        .set_monthly_production(resumed_production)
        .unwrap();
    resumed
        .initial
        .set_monthly_cashflows(Some(resumed_cashflows))
        .unwrap();
    resumed
        .initial
        .set_monthly_accruals(resumed_accruals)
        .unwrap();
    resumed
        .initial
        .set_monthly_convergence(Some(resumed_convergence))
        .unwrap();
    resumed.restore(&save).unwrap();
    let mut continuous = loaded;
    continuous
        .advance_with_systems(52, None, Some(&pipeline))
        .unwrap();
    resumed
        .advance_with_systems(52, None, Some(&pipeline))
        .unwrap();
    assert_eq!(resumed.save().unwrap(), continuous.save().unwrap());
}

#[test]
fn loaded_vanilla_world_can_select_and_resume_a_yearly_pipeline() {
    let mut loaded = VanillaWorld::load(fixture(), "1444.11.11").unwrap();
    let country = loaded.initial.country_entity("AAA").unwrap();
    let world_phase = PipelinePhase {
        id: "fixture-yearly-world:v1",
        country,
        message: "yearly-world",
    };
    let country_phase = PipelinePhase {
        id: "fixture-yearly-country:v1",
        country,
        message: "yearly-country",
    };
    let pipeline = YearlyPipeline::new(
        "fixture-yearly-pipeline:v1",
        Some(&world_phase),
        Some(&country_phase),
    )
    .unwrap();

    loaded.select_yearly_system(&pipeline).unwrap();
    loaded.advance_with_yearly_system(52, &pipeline).unwrap();
    assert_eq!(
        loaded
            .initial
            .world()
            .events()
            .iter()
            .map(|event| event.2.as_str())
            .collect::<Vec<_>>(),
        ["yearly-world", "yearly-country"]
    );

    let save = loaded.save().unwrap();
    let mut resumed = VanillaWorld::load(fixture(), "1444.11.11").unwrap();
    resumed.select_yearly_system(&pipeline).unwrap();
    resumed.restore(&save).unwrap();
    assert_eq!(resumed.initial.world().day(), 52);
    assert_eq!(
        resumed.initial.world().yearly_system_id(),
        Some("fixture-yearly-pipeline:v1")
    );
}
