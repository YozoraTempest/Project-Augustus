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

#[test]
fn world_cli_reports_crlf_source_lines_and_does_not_save_on_definition_error() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world");
    let temp = tempfile::tempdir().unwrap();
    let game_root = temp.path().join("game");
    copy_directory(&fixture, &game_root);
    let definition_path = game_root.join("map/definition.csv");
    fs::write(
        &definition_path,
        "province;red;green;blue;x;x\r\n\r\n\
         1;240;10;10;North;x\r\n\
         2;10;240;10;East;x\r\n\
         3;10;10;240;Sea;x\r\n\
         4;120;90;10;Highland;x\r\n\
         5;240;10;240;Unused One;x\r\n\
         6;240;10;240;Unused Two;x\r\n",
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_augustus-world"))
        .arg("--game-root")
        .arg(&game_root)
        .args(["--province", "1"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["province_sources"][0]["line"], 3);

    fs::write(
        &definition_path,
        "province;red;green;blue;x;x\r\n\r\n\
         1;invalid;10;10;North;x\r\n",
    )
    .unwrap();
    let save_path = temp.path().join("should-not-exist.json");
    let failed = Command::new(env!("CARGO_BIN_EXE_augustus-world"))
        .arg("--game-root")
        .arg(&game_root)
        .arg("--save")
        .arg(&save_path)
        .output()
        .unwrap();
    assert!(!failed.status.success());
    let stderr = String::from_utf8_lossy(&failed.stderr);
    assert!(
        stderr.contains("definition.csv:3:1: invalid definition integer"),
        "{stderr}"
    );
    assert!(!save_path.exists());
}
