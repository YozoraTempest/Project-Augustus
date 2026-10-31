// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::{fs, path::Path, process::Command};

fn example_world() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world")
}

#[test]
fn duplicate_saved_military_fields_fail_without_changing_snapshot_output() {
    let temp = tempfile::tempdir().unwrap();
    let cases = [
        (
            "army-name",
            "army={ location=1 name=First regiment={ type=infantry } name=Second }",
            "duplicate saved army field name",
        ),
        (
            "army-location",
            "army={ name=First location=1 regiment={ type=infantry } location=2 }",
            "duplicate saved army field location",
        ),
        (
            "navy-name",
            "navy={ location=1 name=First ship={ type=galley } name=Second }",
            "duplicate saved navy field name",
        ),
        (
            "navy-location",
            "navy={ name=First location=1 ship={ type=galley } location=2 }",
            "duplicate saved navy field location",
        ),
    ];

    for (name, formation, expected_error) in cases {
        let save = temp.path().join(format!("{name}.eu4"));
        fs::write(
            &save,
            format!(
                "EU4txt\ndate=1444.11.11\nplayer=AAA\ncountries={{ AAA={{ human=yes {formation} }} }}\nprovinces={{}}\n"
            ),
        )
        .unwrap();

        let snapshot = temp.path().join(format!("{name}.snapshot"));
        let sentinel = format!("existing snapshot for {name}\n");
        fs::write(&snapshot, &sentinel).unwrap();

        let output = Command::new(env!("CARGO_BIN_EXE_augustus-world"))
            .arg("--game-root")
            .arg(example_world())
            .arg("--eu4-save")
            .arg(&save)
            .args(["--days", "0", "--omit-load-report", "--save"])
            .arg(&snapshot)
            .output()
            .unwrap();

        assert!(!output.status.success(), "{name} unexpectedly loaded");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains(expected_error),
            "{name} failed for an unexpected reason: {stderr}"
        );
        assert_eq!(fs::read_to_string(&snapshot).unwrap(), sentinel);
    }
}
