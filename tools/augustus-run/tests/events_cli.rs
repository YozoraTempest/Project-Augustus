// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::process::{Command, Output};

fn run(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_augustus-events"))
        .args(arguments)
        .output()
        .unwrap()
}

#[test]
fn event_cli_dispatches_advances_selects_and_reports_pending_choices() {
    let waiting = run(&["--trigger", "augustus_council.1"]);
    assert!(
        waiting.status.success(),
        "{}",
        String::from_utf8_lossy(&waiting.stderr)
    );
    let waiting: serde_json::Value = serde_json::from_slice(&waiting.stdout).unwrap();
    assert_eq!(waiting["pending_events"][0]["id"], 0);
    assert_eq!(
        waiting["pending_events"][0]["options"],
        serde_json::json!(["0", "1"])
    );
    assert_eq!(waiting["log"].as_array().unwrap().len(), 1);
    let mut views = Vec::new();
    for input in ["eu4", "native"] {
        let output = run(&[
            "--input",
            input,
            "--trigger",
            "augustus_council.1",
            "--days",
            "5",
            "--choose",
            "0:0",
        ]);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let view: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(view["entities"][0]["values"]["treasury"], 7500);
        assert_eq!(view["entities"][1]["values"]["variable/phase"], 3000);
        assert!(view["pending_events"].as_array().unwrap().is_empty());
        assert_eq!(view["log"][1][0], 5);
        views.push(view);
    }
    assert_eq!(views[0], views[1]);
}

#[test]
fn event_cli_rejects_invalid_choices_and_wrong_scope() {
    for arguments in [
        vec!["--trigger", "augustus_council.1", "--choose", "0:absent"],
        vec![
            "--trigger",
            "augustus_council.1",
            "--choose",
            "0:0",
            "--choose",
            "0:0",
        ],
        vec!["--trigger", "augustus_council.1", "--entity", "1"],
        vec!["--choose", "invalid"],
    ] {
        let output = run(&arguments);
        assert!(!output.status.success());
        assert!(!output.stderr.is_empty());
    }
}
