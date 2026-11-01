// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

const TITLE: &str = "替换议案\n“北方”";
const DESCRIPTION: &str = "替换正文第一行\n替换正文第二行";
const OPTION_SUPPORT: &str = "替换赞成\n立即实施";

fn example(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples")
        .join(path)
}

fn command() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_augustus-world"));
    command
        .arg("--game-root")
        .arg(example("world"))
        .arg("--mod")
        .arg(example("localisation-replace"))
        .arg("--external-state")
        .arg(example("localisation-replace/external_state.json"))
        .args([
            "--player",
            "AAA",
            "--country",
            "AAA",
            "--days",
            "0",
            "--omit-load-report",
        ]);
    command
}

fn successful_json(output: Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn assert_event(output: &Value) {
    let event = &output["vanilla_pending_event_queries"][0]["pending_events"][0];
    assert_eq!(event["event"], "localisation_replace.1");
    assert_eq!(event["title"], TITLE);
    assert_eq!(event["description"], DESCRIPTION);
    assert_eq!(event["options"], serde_json::json!(["0", "1"]));
    assert_eq!(event["option_labels"]["0"], OPTION_SUPPORT);
    assert_eq!(
        event["option_labels"]["1"],
        "localisation_replace.option_reject"
    );
    assert_eq!(
        event["description"].to_string(),
        serde_json::to_string(DESCRIPTION).unwrap()
    );
    assert_eq!(
        event["option_labels"]["0"].to_string(),
        serde_json::to_string(OPTION_SUPPORT).unwrap()
    );
}

#[test]
fn cli_queries_replacement_text_and_preserves_snapshot_across_load() {
    let temp = tempfile::tempdir().unwrap();
    let queries = example("localisation-replace/pending-event-queries.json");
    let baseline = temp.path().join("baseline.json");
    let queried = temp.path().join("queried.json");
    let restored = temp.path().join("restored.json");

    let baseline_output = command().arg("--save").arg(&baseline).output().unwrap();
    assert!(
        baseline_output.status.success(),
        "{}",
        String::from_utf8_lossy(&baseline_output.stderr)
    );

    let queried_json = successful_json(
        command()
            .arg("--vanilla-pending-event-queries")
            .arg(&queries)
            .arg("--save")
            .arg(&queried)
            .output()
            .unwrap(),
    );
    assert_event(&queried_json);
    assert_eq!(fs::read(&queried).unwrap(), fs::read(&baseline).unwrap());

    let restored_json = successful_json(
        command()
            .arg("--load")
            .arg(&queried)
            .arg("--vanilla-pending-event-queries")
            .arg(&queries)
            .arg("--save")
            .arg(&restored)
            .output()
            .unwrap(),
    );
    assert_event(&restored_json);
    assert_eq!(fs::read(&restored).unwrap(), fs::read(&queried).unwrap());
}
