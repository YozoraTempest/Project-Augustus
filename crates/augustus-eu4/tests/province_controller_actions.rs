// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::{fs, path::Path};

use augustus_core::{Action, EntityKind, EntityRef, ScopeSelector, Target};
use augustus_eu4::{ContentSet, TextEncoding, VanillaNavalConstructionRequest, VanillaWorld};
use tempfile::TempDir;
use walkdir::WalkDir;

fn source_fixture() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world")
}

fn fixture() -> (TempDir, ContentSet) {
    let temp = tempfile::tempdir().unwrap();
    for entry in WalkDir::new(source_fixture()) {
        let entry = entry.unwrap();
        let target = temp
            .path()
            .join(entry.path().strip_prefix(source_fixture()).unwrap());
        if entry.file_type().is_dir() {
            fs::create_dir_all(target).unwrap();
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }

    let map_path = temp.path().join("map/default.map");
    let mut map = fs::read_to_string(&map_path).unwrap();
    map.push_str("\nforce_coastal = { 1 2 }\n");
    fs::write(map_path, map).unwrap();
    let content =
        ContentSet::from_installation(temp.path(), &[], None, None, TextEncoding::Utf8).unwrap();
    (temp, content)
}

fn write_save(directory: &Path, name: &str, controller: &str) -> std::path::PathBuf {
    let path = directory.join(name);
    fs::write(
        &path,
        format!(
            "EU4txt\ndate = 1444.11.11\nplayer = AAA\ncountries = {{ AAA = {{ treasury = 20.000 }} BBB = {{ }} }}\nprovinces = {{ -1 = {{ owner = AAA controller = {controller} }} -2 = {{ owner = AAA controller = AAA }} }}\n"
        ),
    )
    .unwrap();
    path
}

fn controller_action(country: &str) -> Action {
    Action::SetProvinceController {
        target: Target::Current,
        controller: Some(EntityRef::Selected(ScopeSelector::Named {
            kind: EntityKind::Country,
            name: country.into(),
        })),
    }
}

fn request() -> VanillaNavalConstructionRequest {
    VanillaNavalConstructionRequest {
        country: "AAA".into(),
        province_id: 1,
        unit_type: "sample_light_ship".into(),
        completion_day: 3,
        cost_milli: 5_000,
    }
}

#[test]
fn executed_controller_change_updates_projection_and_blocks_naval_submission_atomically() {
    let (directory, content) = fixture();
    let save_path = write_save(directory.path(), "controller-execute.eu4", "AAA");
    let mut world = VanillaWorld::load_from_content_with_save(&content, &save_path).unwrap();
    let province = world.initial.province_entity(1).unwrap();
    let owner = world.initial.country_entity("AAA").unwrap();
    let controller = world.initial.country_entity("BBB").unwrap();

    assert_eq!(
        world.initial.province(1).unwrap().controller.as_deref(),
        Some("AAA")
    );
    world
        .initial
        .execute(province, &[controller_action("BBB")])
        .unwrap();

    assert_eq!(
        world
            .initial
            .world()
            .province_controller_state(province)
            .unwrap(),
        Some(Some(controller))
    );
    assert_eq!(
        world.initial.province(1).unwrap().controller.as_deref(),
        Some("BBB")
    );
    assert_eq!(
        world.initial.world().entity(province).unwrap().owner,
        Some(owner)
    );

    let before_rejected_request = world.save().unwrap();
    assert!(world.submit_naval_constructions(&[request()]).is_err());
    assert_eq!(world.save().unwrap(), before_rejected_request);
    assert!(world.initial.world().naval_constructions().is_empty());
}

#[test]
fn scheduled_controller_change_restores_projection_and_allows_naval_submission() {
    let (directory, content) = fixture();
    let save_path = write_save(directory.path(), "controller-schedule.eu4", "BBB");
    let mut world = VanillaWorld::load_from_content_with_save(&content, &save_path).unwrap();
    let province = world.initial.province_entity(1).unwrap();
    let owner = world.initial.country_entity("AAA").unwrap();
    let controller = world.initial.country_entity("AAA").unwrap();

    world
        .initial
        .execute(
            province,
            &[Action::Schedule {
                delay: 1,
                plan: vec![controller_action("AAA")],
            }],
        )
        .unwrap();
    let scheduled_snapshot = world.save().unwrap();

    let mut resumed = VanillaWorld::load_from_content_with_save(&content, &save_path).unwrap();
    resumed.restore(&scheduled_snapshot).unwrap();
    assert_eq!(
        resumed.initial.province(1).unwrap().controller.as_deref(),
        Some("BBB")
    );
    resumed.advance(1).unwrap();
    let advanced_snapshot = resumed.save().unwrap();

    let mut restored = VanillaWorld::load_from_content_with_save(&content, &save_path).unwrap();
    restored.restore(&advanced_snapshot).unwrap();
    assert_eq!(
        restored.initial.province(1).unwrap().controller.as_deref(),
        Some("AAA")
    );
    assert_eq!(
        restored
            .initial
            .world()
            .province_controller_state(province)
            .unwrap(),
        Some(Some(controller))
    );
    assert_eq!(
        restored.initial.world().entity(province).unwrap().owner,
        Some(owner)
    );

    let ids = restored.submit_naval_constructions(&[request()]).unwrap();
    assert_eq!(ids.len(), 1);
    assert_eq!(restored.initial.world().naval_constructions().len(), 1);
}
