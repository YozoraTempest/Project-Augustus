// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::{fs, path::Path};

use augustus_core::{EntityId, NavalConstructionInput};
use augustus_eu4::{
    ContentSet, TextEncoding, VanillaExternalState, VanillaMilitaryReport, VanillaWorld,
};
use tempfile::TempDir;
use walkdir::WalkDir;

fn example_world() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world")
}

fn external_state() -> VanillaExternalState {
    serde_json::from_str(include_str!(
        "../../../examples/military-roster/external_state.json"
    ))
    .unwrap()
}

fn loaded_example() -> VanillaWorld {
    VanillaWorld::load_with_external_state(example_world(), "1444.11.11", &external_state())
        .unwrap()
}

fn province_id_for_entity(world: &VanillaWorld, entity: EntityId) -> u32 {
    world
        .initial
        .provinces()
        .iter()
        .find(|province| world.initial.province_entity(province.id) == Some(entity))
        .expect("core province maps to an EU4 province")
        .id
}

fn assert_report_matches_core(world: &VanillaWorld, report: &VanillaMilitaryReport) {
    for row in &report.armies {
        let army = world
            .initial
            .armies()
            .iter()
            .find(|army| army.id == row.id)
            .expect("reported army exists in core");
        assert_eq!(
            row.country,
            world.initial.world().entity(army.owner).unwrap().name
        );
        assert_eq!(
            row.province_id,
            province_id_for_entity(world, army.location)
        );
        assert_eq!(
            row.province_name,
            world.initial.province(row.province_id).unwrap().name
        );
        assert_eq!(row.name, army.name);
        assert_eq!(row.regiments.len(), army.regiments.len());
        for (regiment_row, regiment) in row.regiments.iter().zip(&army.regiments) {
            assert_eq!(regiment_row.id, regiment.id);
            assert_eq!(regiment_row.unit_type, regiment.unit_type);
            assert_eq!(regiment_row.name, regiment.name);
            assert_eq!(regiment_row.strength_milli, regiment.strength_milli);
            assert_eq!(regiment_row.morale_milli, regiment.morale_milli);
            assert_eq!(
                regiment_row.home_province,
                regiment
                    .home_province
                    .map(|id| province_id_for_entity(world, id))
            );
        }
    }

    for row in &report.fleets {
        let fleet = world
            .initial
            .fleets()
            .iter()
            .find(|fleet| fleet.id == row.id)
            .expect("reported fleet exists in core");
        assert_eq!(
            row.country,
            world.initial.world().entity(fleet.owner).unwrap().name
        );
        assert_eq!(
            row.province_id,
            province_id_for_entity(world, fleet.location)
        );
        assert_eq!(
            row.province_name,
            world.initial.province(row.province_id).unwrap().name
        );
        assert_eq!(row.name, fleet.name);
        assert_eq!(row.ships.len(), fleet.ships.len());
        for (ship_row, ship_id) in row.ships.iter().zip(&fleet.ships) {
            let ship = world
                .initial
                .naval_units()
                .iter()
                .find(|ship| ship.id == *ship_id)
                .expect("fleet member exists in core");
            assert_eq!(ship_row.id, ship.id);
            assert_eq!(ship_row.unit_type, ship.unit_type);
            assert_eq!(ship_row.name, ship.name);
            assert_eq!(ship_row.morale_milli, ship.morale_milli);
            assert_eq!(
                ship_row.home_province,
                ship.home_province
                    .map(|id| province_id_for_entity(world, id))
            );
        }
    }
}

fn assert_report_traits<T: Clone + std::fmt::Debug + PartialEq + Eq + serde::Serialize>() {}

#[test]
fn report_is_a_read_only_projection_of_current_core_rosters() {
    assert_report_traits::<VanillaMilitaryReport>();
    let world = loaded_example();
    let snapshot = world.save().unwrap();
    let report = world.military_roster(None, None).unwrap();

    assert_eq!(report.armies.len(), 3);
    assert_eq!(report.fleets.len(), 3);
    assert_eq!(
        report
            .armies
            .iter()
            .filter(|army| army.name == "Twin Stack")
            .count(),
        3
    );
    assert_eq!(
        report
            .fleets
            .iter()
            .filter(|fleet| fleet.name.as_deref() == Some("Twin Fleet"))
            .count(),
        2
    );
    assert_report_matches_core(&world, &report);
    assert_eq!(report, report.clone());
    assert!(!serde_json::to_vec(&report).unwrap().is_empty());
    assert!(!format!("{report:?}").is_empty());
    assert_eq!(world.save().unwrap(), snapshot);
    assert_eq!(world.military_roster(None, None).unwrap(), report);

    let north_guard = report
        .armies
        .iter()
        .find(|army| army.country == "AAA" && army.province_id == 1)
        .unwrap();
    assert_eq!(north_guard.regiments[0].strength_milli, Some(0));
    assert_eq!(north_guard.regiments[0].morale_milli, Some(625));
    assert_eq!(north_guard.regiments[1].strength_milli, Some(569));
    assert_eq!(north_guard.regiments[1].morale_milli, Some(0));
    assert_eq!(north_guard.regiments[2].strength_milli, None);
    assert_eq!(north_guard.regiments[2].morale_milli, None);

    let north_guard_core = world
        .initial
        .armies()
        .iter()
        .find(|army| army.id == north_guard.id)
        .unwrap();
    let core_home = north_guard_core.regiments[0].home_province.unwrap();
    assert_ne!(
        usize::try_from(north_guard.regiments[0].home_province.unwrap()).unwrap(),
        core_home.0
    );
    assert_eq!(north_guard.regiments[0].home_province, Some(1));

    let north_fleet = report
        .fleets
        .iter()
        .find(|fleet| fleet.country == "AAA" && fleet.province_id == 1)
        .unwrap();
    assert_eq!(north_fleet.ships[0].name.as_deref(), Some("North Star"));
    assert_eq!(north_fleet.ships[0].morale_milli, Some(0));
    assert_eq!(north_fleet.ships[1].name, None);
    assert_eq!(north_fleet.ships[1].morale_milli, Some(875));
    let unnamed_fleet = report
        .fleets
        .iter()
        .find(|fleet| fleet.country == "AAA" && fleet.province_id == 2)
        .unwrap();
    assert_eq!(unnamed_fleet.name, None);
    assert_eq!(unnamed_fleet.ships[0].morale_milli, None);
}

#[test]
fn country_and_province_filters_are_combined_and_validate_inputs() {
    let world = loaded_example();
    let no_units = VanillaWorld::load(example_world(), "1444.11.11")
        .unwrap()
        .military_roster(None, None)
        .unwrap();
    assert!(no_units.armies.is_empty());
    assert!(no_units.fleets.is_empty());

    let country = world.military_roster(Some("AAA"), None).unwrap();
    assert_eq!(country.armies.len(), 2);
    assert_eq!(country.fleets.len(), 2);
    assert!(country.armies.iter().all(|army| army.country == "AAA"));
    assert!(country.fleets.iter().all(|fleet| fleet.country == "AAA"));

    let province = world.military_roster(None, Some(1)).unwrap();
    assert_eq!(province.armies.len(), 2);
    assert_eq!(province.fleets.len(), 2);
    assert!(province.armies.iter().all(|army| army.province_id == 1));
    assert!(province.fleets.iter().all(|fleet| fleet.province_id == 1));

    let combined = world.military_roster(Some("AAA"), Some(2)).unwrap();
    assert_eq!(combined.armies.len(), 1);
    assert_eq!(combined.fleets.len(), 1);
    assert_eq!(combined.armies[0].country, "AAA");
    assert_eq!(combined.armies[0].province_id, 2);
    assert_eq!(combined.fleets[0].country, "AAA");
    assert_eq!(combined.fleets[0].province_id, 2);

    let empty = world.military_roster(Some("BBB"), Some(2)).unwrap();
    assert!(empty.armies.is_empty());
    assert!(empty.fleets.is_empty());
    assert!(world.military_roster(Some("ZZZ"), None).is_err());
    assert!(world.military_roster(None, Some(999)).is_err());
}

#[test]
fn updated_member_state_is_live_and_snapshot_restore_preserves_roster_ids() {
    let mut world = loaded_example();
    let initial_report = world.military_roster(None, None).unwrap();
    let initial_snapshot = world.save().unwrap();
    let owner = world.initial.country_entity("AAA").unwrap();
    let location = world.initial.province_entity(1).unwrap();
    let army = world
        .initial
        .armies()
        .iter()
        .find(|army| army.name == "Twin Stack" && army.owner == owner && army.location == location)
        .unwrap();
    let army_id = army.id;
    let morale_regiment_id = army.regiments[0].id;
    let regiment_id = army.regiments[1].id;
    world
        .initial
        .set_regiment_morale_milli(army_id, morale_regiment_id, 875)
        .unwrap();
    world
        .initial
        .apply_regiment_strength_loss_milli(army_id, regiment_id, 100, 1_000)
        .unwrap();

    let updated_report = world.military_roster(None, None).unwrap();
    assert_eq!(
        updated_report
            .armies
            .iter()
            .map(|army| army.id)
            .collect::<Vec<_>>(),
        initial_report
            .armies
            .iter()
            .map(|army| army.id)
            .collect::<Vec<_>>()
    );
    let updated_army = updated_report
        .armies
        .iter()
        .find(|army| army.id == army_id)
        .unwrap();
    assert_eq!(updated_army.regiments[0].morale_milli, Some(875));
    assert_eq!(updated_army.regiments[1].strength_milli, Some(469));

    let updated_snapshot = world.save().unwrap();
    let mut restored = loaded_example();
    restored.restore(&updated_snapshot).unwrap();
    assert_eq!(
        restored.military_roster(None, None).unwrap(),
        updated_report
    );
    assert_eq!(restored.save().unwrap(), updated_snapshot);

    world.restore(&initial_snapshot).unwrap();
    assert_eq!(world.military_roster(None, None).unwrap(), initial_report);
}

fn content_with_coastal_province() -> (TempDir, ContentSet) {
    let source = example_world();
    let temp = tempfile::tempdir().unwrap();
    for entry in WalkDir::new(&source) {
        let entry = entry.unwrap();
        let target = temp
            .path()
            .join(entry.path().strip_prefix(&source).unwrap());
        if entry.file_type().is_dir() {
            fs::create_dir_all(target).unwrap();
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
    let map_path = temp.path().join("map/default.map");
    let map = fs::read_to_string(&map_path).unwrap();
    fs::write(map_path, format!("{map}\nforce_coastal = {{ 1 }}\n")).unwrap();
    let content =
        ContentSet::from_installation(temp.path(), &[], None, None, TextEncoding::Utf8).unwrap();
    (temp, content)
}

#[test]
fn completed_naval_construction_appears_and_old_snapshots_remove_it() {
    let (_temp, content) = content_with_coastal_province();
    let mut world = VanillaWorld::load_from_content(&content, "1444.11.11").unwrap();
    let baseline_snapshot = world.save().unwrap();
    let owner = world.initial.country_entity("AAA").unwrap();
    let location = world.initial.province_entity(1).unwrap();
    world
        .initial
        .enqueue_naval_constructions(&[NavalConstructionInput {
            owner,
            location,
            unit_type: "sample_light_ship".into(),
            completion_day: 1,
        }])
        .unwrap();
    let pending_snapshot = world.save().unwrap();
    assert!(world.military_roster(None, None).unwrap().fleets.is_empty());

    world.advance(1).unwrap();
    let completed_report = world.military_roster(None, None).unwrap();
    assert_eq!(completed_report.fleets.len(), 1);
    assert_eq!(completed_report.fleets[0].country, "AAA");
    assert_eq!(completed_report.fleets[0].province_id, 1);
    assert_eq!(completed_report.fleets[0].ships.len(), 1);
    assert_eq!(
        completed_report.fleets[0].ships[0].unit_type,
        "sample_light_ship"
    );
    assert_eq!(completed_report.fleets[0].ships[0].home_province, Some(1));
    let completed_snapshot = world.save().unwrap();

    let mut resumed = VanillaWorld::load_from_content(&content, "1444.11.11").unwrap();
    resumed.restore(&pending_snapshot).unwrap();
    resumed.advance(1).unwrap();
    assert_eq!(
        resumed.military_roster(None, None).unwrap(),
        completed_report
    );
    assert_eq!(resumed.save().unwrap(), completed_snapshot);

    resumed.restore(&baseline_snapshot).unwrap();
    assert!(
        resumed
            .military_roster(None, None)
            .unwrap()
            .fleets
            .is_empty()
    );
}

#[test]
fn self_authored_eu4_save_imports_land_and_naval_rosters() {
    let (_temp, content) = content_with_coastal_province();
    let directory = tempfile::tempdir().unwrap();
    let save = directory.path().join("military-roster.eu4");
    fs::write(
        &save,
        r#"EU4txt
date=1444.11.11
player=AAA
countries={
  AAA={
    army={
      name="Save Army"
      location=1
      regiment={type=test_infantry name="Saved First" strength=0.569 morale=0 home=2}
      regiment={type=test_cavalry}
    }
    navy={
      name="Save Navy"
      location=1
      ship={type=sample_light_ship name="Saved Ship" morale=0.750 home=2}
    }
  }
  BBB={}
}
provinces={}
"#,
    )
    .unwrap();

    let loaded = VanillaWorld::load_from_content_with_save(&content, &save).unwrap();
    let report = loaded.military_roster(None, None).unwrap();
    assert_eq!(report.armies.len(), 1);
    assert_eq!(report.armies[0].name, "Save Army");
    assert_eq!(report.armies[0].country, "AAA");
    assert_eq!(report.armies[0].regiments[0].strength_milli, Some(569));
    assert_eq!(report.armies[0].regiments[0].morale_milli, Some(0));
    assert_eq!(report.armies[0].regiments[0].home_province, Some(2));
    assert_eq!(report.armies[0].regiments[1].strength_milli, None);
    assert_eq!(report.fleets.len(), 1);
    assert_eq!(report.fleets[0].name.as_deref(), Some("Save Navy"));
    assert_eq!(
        report.fleets[0].ships[0].name.as_deref(),
        Some("Saved Ship")
    );
    assert_eq!(report.fleets[0].ships[0].morale_milli, Some(750));
    assert_eq!(report.fleets[0].ships[0].home_province, Some(2));
    assert_report_matches_core(&loaded, &report);
}
