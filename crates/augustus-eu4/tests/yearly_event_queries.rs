// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_eu4::{VanillaWorld, VanillaYearlyEventQuery};
use std::{fs, path::Path};

fn fixture() -> tempfile::TempDir {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world");
    let temp = tempfile::tempdir().unwrap();
    for entry in walkdir::WalkDir::new(&root) {
        let entry = entry.unwrap();
        let target = temp.path().join(entry.path().strip_prefix(&root).unwrap());
        if entry.file_type().is_dir() {
            fs::create_dir_all(target).unwrap();
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
    temp
}

fn write_yearly_pulse_content(root: &Path) {
    let on_actions = root.join("common/on_actions/yearly.txt");
    fs::create_dir_all(on_actions.parent().unwrap()).unwrap();
    fs::write(on_actions, "on_yearly_pulse={events={yearly.pulse}}").unwrap();
    let events = root.join("events/yearly.txt");
    fs::create_dir_all(events.parent().unwrap()).unwrap();
    fs::write(
        events,
        r#"
country_event = {
    id = yearly.pulse
    is_triggered_only = yes
    trigger = { tag = AAA }
    immediate = { log = yearly_pulse }
    option = { name = yearly_pulse_a }
}
"#,
    )
    .unwrap();
}

#[test]
fn yearly_event_query_reports_country_configuration_without_mutation() {
    let temp = fixture();
    write_yearly_pulse_content(temp.path());
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let snapshot = loaded.save().unwrap();
    let report = loaded
        .query_vanilla_yearly_event(&VanillaYearlyEventQuery {
            country: "AAA".into(),
        })
        .unwrap();
    assert_eq!(report.events, vec!["yearly.pulse"]);
    assert!(
        report
            .system_id
            .as_deref()
            .is_some_and(|id| id.starts_with("eu4-on-yearly-pulse:1:"))
    );
    assert!(report.active);
    assert_eq!(loaded.save().unwrap(), snapshot);
}

#[test]
fn yearly_event_query_reports_empty_configuration_and_rejects_invalid_inputs() {
    let temp = fixture();
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let report = loaded
        .query_vanilla_yearly_event(&VanillaYearlyEventQuery {
            country: "AAA".into(),
        })
        .unwrap();
    assert!(report.events.is_empty());
    assert!(report.system_id.is_none());
    assert!(!report.active);
    assert!(
        loaded
            .query_vanilla_yearly_event(&VanillaYearlyEventQuery {
                country: "ZZZ".into(),
            })
            .is_err()
    );
    let value = serde_json::json!({"country": "AAA", "unexpected": true});
    assert!(serde_json::from_value::<VanillaYearlyEventQuery>(value).is_err());
}
