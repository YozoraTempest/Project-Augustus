// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::{fs, path::Path};

use augustus_core::{EntityKind, Schema};
use augustus_eu4::{
    ContentRoot, ContentSet, ContentSource, ScriptCompiler, ScriptLibrary, TextDocument,
    TextEncoding, VanillaExternalState, VanillaWorld,
};
use std::collections::{BTreeMap, BTreeSet};

fn fixture() -> tempfile::TempDir {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world");
    let temp = tempfile::tempdir().unwrap();
    for entry in walkdir::WalkDir::new(&source) {
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
    append(
        temp.path(),
        "history/countries/BBB - Beta.txt",
        "technology_group = test_group",
    );
    append(
        temp.path(),
        "history/provinces/1 - North.txt",
        "discovered_by = test_group\n1444.11.11 = { discovered_by = BBB }",
    );
    append(
        temp.path(),
        "history/provinces/2 - East.txt",
        "discovered_by = AAA\n1444.11.12 = { discovered_by = unknown_group }",
    );
    temp
}

fn append(root: &Path, relative_path: &str, text: &str) {
    let path = root.join(relative_path);
    let previous = fs::read_to_string(&path).unwrap();
    fs::write(path, format!("{previous}\n{text}\n")).unwrap();
}

#[test]
fn history_expands_technology_groups_and_applies_dated_discoveries_at_the_start_date() {
    let fixture = fixture();
    let loaded = VanillaWorld::load(fixture.path(), "1444.11.11").unwrap();
    let world = loaded.initial.world();
    let alpha = loaded.initial.country_entity("AAA").unwrap();
    let beta = loaded.initial.country_entity("BBB").unwrap();
    let north = loaded.initial.province_entity(1).unwrap();
    let east = loaded.initial.province_entity(2).unwrap();

    assert_eq!(
        world.province_discovered_by(north, alpha).unwrap(),
        Some(true)
    );
    assert_eq!(
        world.province_discovered_by(north, beta).unwrap(),
        Some(true)
    );
    assert_eq!(
        world.province_discovered_by(east, alpha).unwrap(),
        Some(true)
    );
    assert_eq!(
        world.province_discovered_by(east, beta).unwrap(),
        Some(false)
    );
    assert_eq!(loaded.report.province_discovery_states, 2);
    assert!(
        !loaded
            .report
            .unapplied_province_fields
            .contains_key("discovered_by")
    );

    let schema = Schema::new([] as [&str; 0]).unwrap();
    let script = ScriptCompiler::new(&schema, &ScriptLibrary::new(), true)
        .compile(
            &TextDocument::from_bytes(
                "self-authored-discovery.txt",
                b"if={limit={has_discovered=BBB} log=discovered} else={log=undiscovered}".to_vec(),
                TextEncoding::Utf8,
            ),
            EntityKind::Province,
        )
        .unwrap();
    let mut runtime = world.clone();
    runtime.execute(north, &script.plan).unwrap();
    runtime.execute(east, &script.plan).unwrap();
    assert_eq!(
        runtime
            .events()
            .iter()
            .map(|event| event.2.as_str())
            .collect::<Vec<_>>(),
        ["discovered", "undiscovered"]
    );
}

#[test]
fn unknown_history_identifier_stays_unknown_and_is_reported() {
    let fixture = fixture();
    let loaded = VanillaWorld::load(fixture.path(), "1444.11.12").unwrap();
    let alpha = loaded.initial.country_entity("AAA").unwrap();
    let beta = loaded.initial.country_entity("BBB").unwrap();
    let east = loaded.initial.province_entity(2).unwrap();

    assert_eq!(
        loaded
            .initial
            .world()
            .province_discovered_by(east, alpha)
            .unwrap(),
        Some(true)
    );
    assert_eq!(
        loaded
            .initial
            .world()
            .province_discovered_by(east, beta)
            .unwrap(),
        None
    );
    assert_eq!(
        loaded.report.unresolved_discovery_identifiers["unknown_group"],
        1
    );
}

#[test]
fn saved_discovery_set_replaces_history_including_an_explicit_empty_set() {
    let fixture = fixture();
    let state = VanillaExternalState {
        province_discovered_by: BTreeMap::from([(1, BTreeSet::new())]),
        ..Default::default()
    };
    let loaded =
        VanillaWorld::load_with_external_state(fixture.path(), "1444.11.11", &state).unwrap();
    let world = loaded.initial.world();
    let alpha = loaded.initial.country_entity("AAA").unwrap();
    let beta = loaded.initial.country_entity("BBB").unwrap();
    let north = loaded.initial.province_entity(1).unwrap();

    assert_eq!(
        world.province_discovered_by(north, alpha).unwrap(),
        Some(false)
    );
    assert_eq!(
        world.province_discovered_by(north, beta).unwrap(),
        Some(false)
    );
    assert_eq!(
        world.map_knowledge().provinces[&north].discovered_by,
        BTreeSet::new()
    );
    assert!(
        world.map_knowledge().provinces[&north]
            .unresolved_countries
            .is_empty()
    );
}

#[test]
fn save_discovery_array_replaces_history_before_world_startup() {
    let fixture = fixture();
    let save_path = fixture.path().join("map-knowledge.eu4");
    fs::write(
        &save_path,
        r#"EU4txt
date=1444.11.11
player="AAA"
countries={
    AAA={ human=yes }
    BBB={}
}
provinces={
    -1={ discovered_by={BBB} }
    -2={ discovered_by={} }
}
"#,
    )
    .unwrap();
    let content = ContentSet::resolve(vec![ContentSource::base(
        ContentRoot::open(fixture.path()).unwrap(),
    )])
    .unwrap();
    let loaded = VanillaWorld::load_from_content_with_save(&content, save_path).unwrap();
    let world = loaded.initial.world();
    let alpha = loaded.initial.country_entity("AAA").unwrap();
    let beta = loaded.initial.country_entity("BBB").unwrap();
    let north = loaded.initial.province_entity(1).unwrap();
    let east = loaded.initial.province_entity(2).unwrap();

    assert_eq!(
        world.province_discovered_by(north, alpha).unwrap(),
        Some(false)
    );
    assert_eq!(
        world.province_discovered_by(north, beta).unwrap(),
        Some(true)
    );
    assert_eq!(
        world.province_discovered_by(east, alpha).unwrap(),
        Some(false)
    );
    assert_eq!(
        world.province_discovered_by(east, beta).unwrap(),
        Some(false)
    );
}
