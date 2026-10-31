// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_eu4::{VanillaExternalState, VanillaProvinceDiscoveryQuery, VanillaWorld};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

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
fn province_discovery_query_reports_sets_tristate_and_preserves_snapshot() {
    let fixture = fixture();
    let loaded = VanillaWorld::load(fixture.path(), "1444.11.11").unwrap();
    let before = loaded.save().unwrap();

    let all = loaded
        .query_vanilla_province_discovery(&VanillaProvinceDiscoveryQuery {
            province: 1,
            country: None,
        })
        .unwrap();
    assert!(all.state_known);
    assert_eq!(all.discovered, None);
    assert!(all.discovered_by.contains("AAA"));
    assert!(all.discovered_by.contains("BBB"));
    assert!(all.unresolved_by.is_empty());

    let beta = loaded
        .query_vanilla_province_discovery(&VanillaProvinceDiscoveryQuery {
            province: 1,
            country: Some("BBB".into()),
        })
        .unwrap();
    assert_eq!(beta.discovered, Some(true));

    let unknown_province = loaded
        .query_vanilla_province_discovery(&VanillaProvinceDiscoveryQuery {
            province: 3,
            country: Some("BBB".into()),
        })
        .unwrap();
    assert!(!unknown_province.state_known);
    assert_eq!(unknown_province.discovered, None);

    let unresolved = VanillaWorld::load(fixture.path(), "1444.11.12")
        .unwrap()
        .query_vanilla_province_discovery(&VanillaProvinceDiscoveryQuery {
            province: 2,
            country: Some("BBB".into()),
        })
        .unwrap();
    assert!(unresolved.state_known);
    assert_eq!(unresolved.discovered, None);
    assert!(unresolved.unresolved_by.contains("BBB"));

    assert_eq!(loaded.save().unwrap(), before);
}

#[test]
fn province_discovery_query_preserves_saved_empty_override_and_strict_inputs() {
    let fixture = fixture();
    let state = VanillaExternalState {
        province_discovered_by: BTreeMap::from([(1, BTreeSet::new())]),
        ..Default::default()
    };
    let loaded =
        VanillaWorld::load_with_external_state(fixture.path(), "1444.11.11", &state).unwrap();
    let report = loaded
        .query_vanilla_province_discovery(&VanillaProvinceDiscoveryQuery {
            province: 1,
            country: Some("AAA".into()),
        })
        .unwrap();
    assert!(report.state_known);
    assert_eq!(report.discovered, Some(false));
    assert!(report.discovered_by.is_empty());
    assert!(report.unresolved_by.is_empty());

    let error = loaded
        .query_vanilla_province_discovery(&VanillaProvinceDiscoveryQuery {
            province: 1,
            country: Some("ZZZ".into()),
        })
        .unwrap_err()
        .to_string();
    assert!(error.contains("unknown EU4 discovery country ZZZ"));

    let error = serde_json::from_str::<VanillaProvinceDiscoveryQuery>(
        r#"{"province":1,"country":null,"unexpected":true}"#,
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("unknown field"));
}
