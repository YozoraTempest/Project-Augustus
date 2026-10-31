// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::{fs, io::Write, path::Path};

use augustus_eu4::{ContentRoot, ContentSet, ContentSource, VanillaWorld, read_vanilla_save_state};
use zip::{ZipWriter, write::SimpleFileOptions};

fn content() -> ContentSet {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world");
    ContentSet::resolve(vec![ContentSource::base(ContentRoot::open(base).unwrap())]).unwrap()
}

fn write_text_save(directory: &Path, name: &str, body: &str) -> std::path::PathBuf {
    let path = directory.join(name);
    fs::write(
        &path,
        format!("EU4txt\ndate=1444.11.11\nplayer=AAA\ncountries={{ AAA={{ human=yes {body} }} }}\nprovinces={{}}\n"),
    )
    .unwrap();
    path
}

fn write_zip_save(directory: &Path, name: &str, body: &str) -> std::path::PathBuf {
    let path = directory.join(name);
    let mut archive = ZipWriter::new(fs::File::create(&path).unwrap());
    archive
        .start_file("gamestate", SimpleFileOptions::default())
        .unwrap();
    write!(
        archive,
        "EU4txt\ndate=1444.11.11\nplayer=AAA\ncountries={{ AAA={{ human=yes {body} }} }}\nprovinces={{}}\n"
    )
    .unwrap();
    archive.finish().unwrap();
    path
}

fn duplicate_cases() -> Vec<(&'static str, &'static str, &'static str)> {
    vec![
        (
            "army",
            "army={ name=Guard location=1 regiment={ type=infantry } name=Guard }",
            "duplicate saved army field name",
        ),
        (
            "army",
            "army={ regiment={ type=infantry } name=Guard location=1 name=Reserve }",
            "duplicate saved army field name",
        ),
        (
            "army",
            "army={ location=1 name=Guard regiment={ type=infantry } location=1 }",
            "duplicate saved army field location",
        ),
        (
            "army",
            "army={ regiment={ type=infantry } name=Guard location=1 location=2 }",
            "duplicate saved army field location",
        ),
        (
            "navy",
            "navy={ name=Patrol location=1 ship={ type=galley } name=Patrol }",
            "duplicate saved navy field name",
        ),
        (
            "navy",
            "navy={ ship={ type=galley } name=Patrol location=1 name=Reserve }",
            "duplicate saved navy field name",
        ),
        (
            "navy",
            "navy={ location=1 name=Patrol ship={ type=galley } location=1 }",
            "duplicate saved navy field location",
        ),
        (
            "navy",
            "navy={ ship={ type=galley } name=Patrol location=1 location=2 }",
            "duplicate saved navy field location",
        ),
    ]
}

#[test]
fn repeated_army_and_navy_name_or_location_fields_are_rejected() {
    let directory = tempfile::tempdir().unwrap();

    for (index, (kind, body, expected_error)) in duplicate_cases().into_iter().enumerate() {
        let path = write_text_save(directory.path(), &format!("duplicate-{index}.eu4"), body);
        let error = read_vanilla_save_state(path).unwrap_err();
        assert!(
            error.to_string().contains(expected_error),
            "{kind} duplicate should report {expected_error:?}: {error}"
        );
    }
}

#[test]
fn zipped_saves_reject_repeated_military_name_and_location_fields() {
    let directory = tempfile::tempdir().unwrap();

    for (index, (kind, body, expected_error)) in duplicate_cases().into_iter().enumerate() {
        let path = write_zip_save(directory.path(), &format!("duplicate-{index}.eu4"), body);
        let error = read_vanilla_save_state(path).unwrap_err();
        assert!(
            error.to_string().contains(expected_error),
            "ZIP {kind} duplicate should report {expected_error:?}: {error}"
        );
    }
}

#[test]
fn valid_multiple_formations_load_and_survive_snapshot_restore() {
    let directory = tempfile::tempdir().unwrap();
    let path = write_text_save(
        directory.path(),
        "formations.eu4",
        r#"
            army={
                name="Northern Guard"
                location=1
                unknown_army_state={ ignored=yes }
                regiment={ type=infantry name="First Regiment" strength=0.750 morale=0.500 home=1 unknown_member_field=9 }
                regiment={ type=cavalry strength=1.000 morale=0.875 }
            }
            army={ location=2 regiment={ type=artillery } }
            navy={ name="Coastal Patrol" location=1 unknown_fleet_state=yes ship={ type=galley name="North Star" morale=0.950 home=1 unknown_member_field=9 } ship={ type=light_ship morale=0.875 } }
            navy={ location=2 ship={ type=transport } }
        "#,
    );

    let save = read_vanilla_save_state(path).unwrap();
    assert_eq!(save.external_state.armies.len(), 2);
    assert_eq!(save.external_state.naval_fleets.len(), 2);
    assert_eq!(save.external_state.armies[0].name, "Northern Guard");
    assert_eq!(save.external_state.armies[1].name, "");
    assert_eq!(
        save.external_state.naval_fleets[0].name.as_deref(),
        Some("Coastal Patrol")
    );
    assert_eq!(save.external_state.naval_fleets[1].name, None);

    let mut world = VanillaWorld::load_from_content_with_save_state(&content(), &save).unwrap();
    let report = world.military_roster(None, None).unwrap();
    assert_eq!(report.armies.len(), 2);
    assert_eq!(report.armies[0].name, "Northern Guard");
    assert_eq!(report.armies[0].regiments.len(), 2);
    assert_eq!(
        report.armies[0].regiments[0].name.as_deref(),
        Some("First Regiment")
    );
    assert_eq!(report.armies[0].regiments[0].strength_milli, Some(750));
    assert_eq!(report.armies[0].regiments[0].home_province, Some(1));
    assert_eq!(report.armies[1].name, "");
    assert_eq!(report.armies[1].regiments.len(), 1);
    assert_eq!(report.fleets.len(), 2);
    assert_eq!(report.fleets[0].name.as_deref(), Some("Coastal Patrol"));
    assert_eq!(report.fleets[0].ships.len(), 2);
    assert_eq!(
        report.fleets[0].ships[0].name.as_deref(),
        Some("North Star")
    );
    assert_eq!(report.fleets[0].ships[0].morale_milli, Some(950));
    assert_eq!(report.fleets[1].name, None);
    assert_eq!(report.fleets[1].ships.len(), 1);

    let snapshot = world.save().unwrap();
    world.restore(&snapshot).unwrap();
    assert_eq!(world.military_roster(None, None).unwrap(), report);
}
