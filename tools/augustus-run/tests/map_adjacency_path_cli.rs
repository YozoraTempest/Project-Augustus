// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::{fs, path::Path, process::Command};

fn copy_directory(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let target = destination.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_directory(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn example_world() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world")
}

fn configured_fixture(destination: &Path, adjacency_path: &str, fixed_csv: &str) {
    copy_directory(&example_world(), destination);

    let default_map_path = destination.join("map/default.map");
    let mut default_map = fs::read_to_string(&default_map_path).unwrap();
    if !default_map.ends_with('\n') {
        default_map.push('\n');
    }
    default_map.push_str(&format!("adjacencies = \"{adjacency_path}\"\n"));
    fs::write(default_map_path, default_map).unwrap();

    fs::write(destination.join("map/adjacencies.csv"), fixed_csv).unwrap();
}

fn write_adjacency(destination: &Path, relative_path: &str, contents: &str) {
    let path = destination.join("map").join(relative_path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

fn world_command(game_root: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_augustus-world"));
    command
        .arg("--game-root")
        .arg(game_root)
        .arg("--omit-load-report");
    command
}

#[test]
fn configured_subdirectory_adjacencies_override_the_fixed_path_and_resume() {
    let temp = tempfile::tempdir().unwrap();
    let game_root = temp.path().join("game");
    configured_fixture(
        &game_root,
        "topology/special.csv",
        "From;To;Type\n1;99;sea\n",
    );
    write_adjacency(
        &game_root,
        "topology/special.csv",
        "From;To;Type\n1;2;sea\n",
    );

    let initial_save = temp.path().join("initial.json");
    let initial = world_command(&game_root)
        .args(["--days", "0", "--save"])
        .arg(&initial_save)
        .output()
        .unwrap();
    assert!(
        initial.status.success(),
        "{}",
        String::from_utf8_lossy(&initial.stderr)
    );
    assert!(initial_save.is_file());

    let resumed_save = temp.path().join("resumed.json");
    let resumed = world_command(&game_root)
        .arg("--load")
        .arg(&initial_save)
        .args(["--days", "1", "--save"])
        .arg(&resumed_save)
        .output()
        .unwrap();
    assert!(
        resumed.status.success(),
        "{}",
        String::from_utf8_lossy(&resumed.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&resumed.stdout).unwrap();
    assert_eq!(report["simulation"]["day"], 1);
    assert!(resumed_save.is_file());
}

#[test]
fn missing_or_invalid_configured_adjacencies_fail_before_saving() {
    let temp = tempfile::tempdir().unwrap();
    let valid_fixed_csv = "From;To;Type\n1;2;sea\n";

    let missing_root = temp.path().join("missing-game");
    configured_fixture(&missing_root, "topology/missing.csv", valid_fixed_csv);
    let missing_save = temp.path().join("missing-save.json");
    let missing = world_command(&missing_root)
        .arg("--save")
        .arg(&missing_save)
        .output()
        .unwrap();
    assert!(!missing.status.success());
    let missing_stderr = String::from_utf8_lossy(&missing.stderr).replace('\\', "/");
    assert!(
        missing_stderr.contains("map/topology/missing.csv"),
        "{missing_stderr}"
    );
    assert!(!missing_save.exists());

    let invalid_root = temp.path().join("invalid-game");
    configured_fixture(&invalid_root, "topology/invalid.csv", valid_fixed_csv);
    write_adjacency(
        &invalid_root,
        "topology/invalid.csv",
        "From;To;Type\n1;99;sea\n",
    );
    let invalid_save = temp.path().join("invalid-save.json");
    let invalid = world_command(&invalid_root)
        .arg("--save")
        .arg(&invalid_save)
        .output()
        .unwrap();
    assert!(!invalid.status.success());
    let invalid_stderr = String::from_utf8_lossy(&invalid.stderr).replace('\\', "/");
    assert!(
        invalid_stderr.contains("map/topology/invalid.csv")
            && invalid_stderr.contains("adjacency references an undefined province"),
        "{invalid_stderr}"
    );
    assert!(!invalid_save.exists());
}
