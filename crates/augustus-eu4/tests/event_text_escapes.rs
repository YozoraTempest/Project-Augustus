// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_eu4::{
    ContentRoot, ContentSet, ContentSource, TextEncoding, VanillaExternalState,
    VanillaPendingEventQuery, VanillaPendingEventQueryReport, VanillaWorld,
};
use std::{collections::BTreeSet, fs, path::Path};

const DATE: &str = "1444.11.11";
const TITLE: &str = "议案：\"北方议事会\"";
const DESCRIPTION: &str = "第一段\n第二段\t缩进；字面序列 \\n 保留";
const FIRST_OPTION: &str = "Café says \"oui\";\tpath C:\\archive\nfollow-up";

fn content() -> ContentSet {
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    ContentSet::resolve(vec![
        ContentSource::base(ContentRoot::open(examples.join("world")).unwrap()),
        ContentSource::mod_directory(
            examples.join("event-text-escapes"),
            TextEncoding::Windows1252,
        )
        .unwrap(),
    ])
    .unwrap()
}

fn external_state() -> VanillaExternalState {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/event-text-escapes/external_state.json");
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

fn load() -> VanillaWorld {
    let content = content();
    VanillaWorld::load_from_content_with_players_and_external_state(
        &content,
        DATE,
        &BTreeSet::from(["AAA".to_owned()]),
        &external_state(),
    )
    .unwrap()
}

fn assert_localized_event(report: &VanillaPendingEventQueryReport) {
    assert_eq!(report.pending_events.len(), 1);
    let event = &report.pending_events[0];
    assert_eq!(event.event, "event_text_escapes.1");
    assert_eq!(event.title, TITLE);
    assert_eq!(event.description.as_deref(), Some(DESCRIPTION));
    assert_eq!(event.options, vec!["0".to_owned(), "1".to_owned()]);
    assert_eq!(event.option_labels["0"], FIRST_OPTION);
    assert_eq!(
        event.option_labels["1"],
        "event_text_escapes.option_missing"
    );
}

#[test]
fn localized_event_text_survives_read_only_query_and_snapshot_restore() {
    let loaded = load();
    let snapshot = loaded.save().unwrap();
    let query = VanillaPendingEventQuery {
        pending: None,
        country: Some("AAA".into()),
    };

    let report = loaded.query_vanilla_pending_events(&query).unwrap();
    assert_localized_event(&report);
    assert_eq!(loaded.save().unwrap().as_bytes(), snapshot.as_bytes());

    let mut restored = load();
    restored.restore(&snapshot).unwrap();
    assert_eq!(restored.save().unwrap().as_bytes(), snapshot.as_bytes());
    let restored_report = restored.query_vanilla_pending_events(&query).unwrap();
    assert_localized_event(&restored_report);
    assert_eq!(restored.save().unwrap().as_bytes(), snapshot.as_bytes());
}
