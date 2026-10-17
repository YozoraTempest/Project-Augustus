// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::{fs, path::Path, process::Command};

// Synthetic input only; no installed game or third-party mod files are required.

fn write(root: &Path, file: &str, contents: &str) {
    let path = root.join(file);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

#[test]
fn define_inspection_evaluates_overlay_and_selected_difficulty_without_editing_input() {
    let fixture = tempfile::tempdir().unwrap();
    let base = fixture.path().join("base");
    let pack = fixture.path().join("pack");
    write(&base, "common/defines.lua", "NDefines={NTest={VALUE=2}}");
    write(
        &base,
        "common/defines/difficulty_hard.lua",
        "NDefines.NTest.VALUE=NDefines.NTest.VALUE*3",
    );
    write(&pack, "descriptor.mod", "name=Patch");
    write(&pack, "common/defines/patch.lua", "NDefines.NTest.VALUE=5");
    let output = Command::new(env!("CARGO_BIN_EXE_eu4-inspect"))
        .arg("defines")
        .arg(&base)
        .arg("--mod")
        .arg(&pack)
        .args(["--difficulty", "hard"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["difficulty"], "hard");
    assert_eq!(
        report["values"]["NDefines.NTest.VALUE"]["value"]["value"],
        15.0
    );
    assert_eq!(report["executed_files"].as_array().unwrap().len(), 3);
    assert_eq!(
        fs::read_to_string(base.join("common/defines.lua")).unwrap(),
        "NDefines={NTest={VALUE=2}}"
    );
    write(
        &pack,
        "common/defines/patch.lua",
        "NDefines.NTest.VALUE = error('test failure')",
    );
    let output = Command::new(env!("CARGO_BIN_EXE_eu4-inspect"))
        .arg("defines")
        .arg(&base)
        .arg("--mod")
        .arg(&pack)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("patch.lua"));
}

#[test]
fn resolved_parse_reports_provenance_and_parses_the_winner() {
    let fixture = tempfile::tempdir().unwrap();
    let base = fixture.path().join("base");
    let pack = fixture.path().join("pack");
    write(&base, "common/buildings/example.txt", "old={ cost=1 }");
    write(&pack, "descriptor.mod", "name=Pack");
    write(&pack, "common/buildings/example.txt", "selected={ cost=2 }");
    let output = Command::new(env!("CARGO_BIN_EXE_eu4-inspect"))
        .arg("resolve")
        .arg(&base)
        .arg("--mod")
        .arg(&pack)
        .args(["--file", "common/buildings/example.txt", "--parse"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report = String::from_utf8(output.stdout).unwrap();
    assert!(report.contains("Effective source: Pack"));
    assert!(report.contains("overridden by file from Pack"));
    assert!(report.contains("selected = {...}"));
    assert!(!report.contains("old = {...}"));
    assert_eq!(
        fs::read_to_string(base.join("common/buildings/example.txt")).unwrap(),
        "old={ cost=1 }"
    );
}

#[test]
fn user_directory_overlays_mod_and_base_files() {
    let fixture = tempfile::tempdir().unwrap();
    let base = fixture.path().join("base");
    let pack = fixture.path().join("pack");
    let user = fixture.path().join("user");
    write(&base, "common/defines.lua", "from_base = true");
    write(&pack, "descriptor.mod", "name=Pack");
    write(&pack, "common/defines.lua", "from_mod = true");
    write(&user, "common/defines.lua", "from_user = true");

    let output = Command::new(env!("CARGO_BIN_EXE_eu4-inspect"))
        .arg("resolve")
        .arg(&base)
        .arg("--mod")
        .arg(&pack)
        .arg("--user-directory")
        .arg(&user)
        .args(["--file", "common/defines.lua", "--parse"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report = String::from_utf8(output.stdout).unwrap();
    assert!(report.contains("Effective source: EU4 user directory"));
    assert!(report.contains("overridden by file from EU4 user directory"));
    assert!(report.contains("from_user = true"));
    assert!(!report.contains("from_mod = true"));
    assert!(!report.contains("from_base = true"));
}

#[test]
fn launcher_config_selects_only_supplied_enabled_mod_directories() {
    let fixture = tempfile::tempdir().unwrap();
    let base = fixture.path().join("base");
    let active = fixture.path().join("active");
    let inactive = fixture.path().join("inactive");
    let launcher_config = fixture.path().join("dlc_load.json");
    write(&base, "common/defines.lua", "from_base = true");
    write(&active, "descriptor.mod", "name=Active");
    write(&active, "common/defines.lua", "from_active = true");
    write(&inactive, "descriptor.mod", "name=Inactive");
    write(&inactive, "common/defines.lua", "from_inactive = true");
    fs::write(&launcher_config, r#"{"enabled_mods":["Active"]}"#).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_eu4-inspect"))
        .arg("resolve")
        .arg(&base)
        .arg("--mod")
        .arg(&inactive)
        .arg("--mod")
        .arg(&active)
        .arg("--launcher-config")
        .arg(&launcher_config)
        .args(["--file", "common/defines.lua", "--parse"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report = String::from_utf8(output.stdout).unwrap();
    assert!(report.contains("Effective source: Active"));
    assert!(report.contains("from_active = true"));
    assert!(!report.contains("Inactive"));
    assert!(!report.contains("from_inactive"));
}

#[test]
fn definition_query_lists_duplicates_without_claiming_a_winner() {
    let fixture = tempfile::tempdir().unwrap();
    write(
        fixture.path(),
        "common/scripted_effects/one.txt",
        "action={}\naction={}\nunique={}",
    );
    let output = Command::new(env!("CARGO_BIN_EXE_eu4-inspect"))
        .arg("definitions")
        .arg(fixture.path())
        .args(["scripted-effects", "--name", "action"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report = String::from_utf8(output.stdout).unwrap();
    assert!(report.contains("Duplicate names: 1"));
    assert!(report.contains("Same-name definition precedence is not resolved."));
    assert!(report.contains("one.txt:1:1"));
    assert!(report.contains("one.txt:2:1"));
    let output = Command::new(env!("CARGO_BIN_EXE_eu4-inspect"))
        .arg("definitions")
        .arg(fixture.path())
        .args(["scripted-effects", "--duplicates"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let report = String::from_utf8(output.stdout).unwrap();
    assert!(report.contains("Definition: action"));
    assert!(!report.contains("Definition: unique"));
}

#[test]
fn missing_dependencies_are_ignored_cycles_fail_and_parse_requires_a_file() {
    let fixture = tempfile::tempdir().unwrap();
    let base = fixture.path().join("base");
    fs::create_dir(&base).unwrap();
    let pack = fixture.path().join("pack");
    write(
        &pack,
        "descriptor.mod",
        "name=Pack dependencies={ Missing }",
    );
    let output = Command::new(env!("CARGO_BIN_EXE_eu4-inspect"))
        .arg("resolve")
        .arg(&base)
        .arg("--mod")
        .arg(&pack)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    write(&pack, "descriptor.mod", "name=Pack dependencies={ Pack }");
    let output = Command::new(env!("CARGO_BIN_EXE_eu4-inspect"))
        .arg("resolve")
        .arg(&base)
        .arg("--mod")
        .arg(&pack)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("cyclic mod dependencies")
    );
    let output = Command::new(env!("CARGO_BIN_EXE_eu4-inspect"))
        .arg("resolve")
        .arg(&base)
        .arg("--parse")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
}
