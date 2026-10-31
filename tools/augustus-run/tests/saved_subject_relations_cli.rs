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
fn world_cli_binds_saved_subject_relations_before_startup_effects() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world");
    let temp = tempfile::tempdir().unwrap();
    let game_root = temp.path().join("game");
    copy_directory(&fixture, &game_root);
    fs::write(
        game_root.join("common/country_tags/saved_subjects.txt"),
        "CCC = \"countries/Gamma.txt\"\n",
    )
    .unwrap();
    fs::write(
        game_root.join("common/countries/Gamma.txt"),
        "color = { 40 120 70 }\n",
    )
    .unwrap();
    fs::write(
        game_root.join("history/countries/CCC - Gamma.txt"),
        "capital = 1\nprimary_culture = alpha\nreligion = test_faith\ngovernment = monarchy\n",
    )
    .unwrap();
    fs::write(
        game_root.join("common/subject_types/saved_subjects.txt"),
        "test_union = { copy_from = default }\n",
    )
    .unwrap();
    let on_actions = game_root.join("common/on_actions");
    fs::create_dir_all(&on_actions).unwrap();
    fs::write(
        on_actions.join("saved_subjects.txt"),
        r#"
on_startup = {
    if = {
        limit = {
            tag = AAA
            is_subject_of = BBB
            is_subject_of_type = test_union
        }
        add_treasury = 3
        log = saved_subject_relations_imported
    }
}
"#,
    )
    .unwrap();

    let save = temp.path().join("saved-subjects.eu4");
    fs::write(
        &save,
        r#"EU4txt
date=1445.1.1
player=AAA
diplomacy={
    dependency={ first=BBB second=AAA subject_type=test_union }
    dependency={ first=CCC second=BBB subject_type=test_vassal }
}
countries={ AAA={human=yes treasury=5.000 completed_missions={}} BBB={human=no treasury=0} CCC={human=no treasury=0} }
provinces={}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_augustus-world"))
        .arg("--game-root")
        .arg(&game_root)
        .arg("--eu4-save")
        .arg(&save)
        .args(["--days", "0", "--country", "AAA"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["country"]["resources"]["treasury_milli"], 8_000);
}
