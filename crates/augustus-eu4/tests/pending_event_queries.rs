// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_eu4::{VanillaPendingEventQuery, VanillaWorld};
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

fn write_pending_event_content(root: &Path) {
    let on_actions = root.join("common/on_actions/pending.txt");
    fs::create_dir_all(on_actions.parent().unwrap()).unwrap();
    fs::write(on_actions, "on_startup={events={pending.choice}}").unwrap();
    let events = root.join("events/pending.txt");
    fs::create_dir_all(events.parent().unwrap()).unwrap();
    fs::write(
        events,
        r#"
country_event = {
    id = pending.choice
    is_triggered_only = yes
    trigger = { tag = AAA }
    title = pending_choice_title
    desc = pending_choice_description
    goto = 1
    option = { name = pending_choice_accept goto = 2 }
    option = { name = pending_choice_stay }
}
"#,
    )
    .unwrap();
    let localisation = root.join("localisation/pending_l_english.yml");
    fs::create_dir_all(localisation.parent().unwrap()).unwrap();
    fs::write(
        localisation,
        "l_english:\n pending_choice_title:0 \"Pending Title\"\n pending_choice_description:0 \"Pending Description\"\n pending_choice_accept:0 \"Accept Choice\"\n pending_choice_stay:0 \"Stay Choice\"\n",
    )
    .unwrap();
}

#[test]
fn pending_event_query_reports_localized_choice_without_mutation() {
    let temp = fixture();
    write_pending_event_content(temp.path());
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let snapshot = loaded.save().unwrap();
    let report = loaded
        .query_vanilla_pending_events(&VanillaPendingEventQuery {
            pending: None,
            country: Some("AAA".into()),
        })
        .unwrap();
    assert_eq!(report.pending_events.len(), 1);
    let event = &report.pending_events[0];
    assert_eq!(event.event, "pending.choice");
    assert_eq!(event.title, "Pending Title");
    assert_eq!(event.description.as_deref(), Some("Pending Description"));
    assert_eq!(event.options, vec!["0", "1"]);
    assert_eq!(event.option_labels["0"], "Accept Choice");
    assert_eq!(event.location.as_deref(), Some("1"));
    assert_eq!(event.option_locations["0"].as_deref(), Some("2"));
    assert_eq!(loaded.save().unwrap(), snapshot);
}

#[test]
fn pending_event_query_supports_all_and_id_filters_and_rejects_invalid_inputs() {
    let temp = fixture();
    write_pending_event_content(temp.path());
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let all = loaded
        .query_vanilla_pending_events(&VanillaPendingEventQuery {
            pending: None,
            country: None,
        })
        .unwrap();
    let id = all.pending_events[0].id;
    assert_eq!(
        loaded
            .query_vanilla_pending_events(&VanillaPendingEventQuery {
                pending: Some(id),
                country: None,
            })
            .unwrap()
            .pending_events
            .len(),
        1
    );
    assert!(
        loaded
            .query_vanilla_pending_events(&VanillaPendingEventQuery {
                pending: Some(id + 100),
                country: None,
            })
            .is_err()
    );
    assert!(
        loaded
            .query_vanilla_pending_events(&VanillaPendingEventQuery {
                pending: None,
                country: Some("ZZZ".into()),
            })
            .is_err()
    );
    let value = serde_json::json!({"unexpected": true});
    assert!(serde_json::from_value::<VanillaPendingEventQuery>(value).is_err());
}
