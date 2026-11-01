// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_eu4::{
    ContentRoot, ContentSet, ContentSource, TextEncoding, VanillaExternalState,
    VanillaPendingEventQuery, VanillaPendingEventQueryReport, VanillaWorld,
};
use std::{collections::BTreeSet, fs, path::Path};

const DATE: &str = "1444.11.11";
const EVENT_TITLE: &str = "替换议案\n“北方”";
const EVENT_DESCRIPTION: &str = "替换正文第一行\n替换正文第二行";
const OPTION_SUPPORT: &str = "替换赞成\n立即实施";
const OPTION_REJECT_KEY: &str = "localisation_replace.option_reject";

fn examples() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples")
}

fn event_content(extra_mod: Option<&Path>) -> ContentSet {
    let examples = examples();
    let mut sources = vec![
        ContentSource::base(ContentRoot::open(examples.join("world")).unwrap()),
        ContentSource::mod_directory(examples.join("localisation-replace"), TextEncoding::Utf8)
            .unwrap(),
    ];
    if let Some(path) = extra_mod {
        sources.push(ContentSource::mod_directory(path, TextEncoding::Utf8).unwrap());
    }
    ContentSet::resolve(sources).unwrap()
}

fn external_state(relative_path: &str) -> VanillaExternalState {
    serde_json::from_slice(&fs::read(examples().join(relative_path)).unwrap()).unwrap()
}

fn load_event_world(content: &ContentSet) -> VanillaWorld {
    VanillaWorld::load_from_content_with_players_and_external_state(
        content,
        DATE,
        &BTreeSet::from(["AAA".to_owned()]),
        &external_state("localisation-replace/external_state.json"),
    )
    .unwrap()
}

fn pending_event(world: &VanillaWorld) -> VanillaPendingEventQueryReport {
    world
        .query_vanilla_pending_events(&VanillaPendingEventQuery {
            pending: None,
            country: Some("AAA".into()),
        })
        .unwrap()
}

fn assert_event_text(
    report: &VanillaPendingEventQueryReport,
    title: &str,
    description: &str,
    support: &str,
    reject: &str,
) {
    assert_eq!(report.pending_events.len(), 1);
    let event = &report.pending_events[0];
    assert_eq!(event.event, "localisation_replace.1");
    assert_eq!(event.title, title);
    assert_eq!(event.description.as_deref(), Some(description));
    assert_eq!(event.options, vec!["0".to_owned(), "1".to_owned()]);
    assert_eq!(event.option_labels["0"], support);
    assert_eq!(event.option_labels["1"], reject);
}

fn write_overlay(root: &Path, descriptor: &str, files: &[(&str, &str)]) {
    fs::write(root.join("descriptor.mod"), descriptor).unwrap();
    for (relative_path, text) in files {
        let path = root.join(relative_path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
}

#[test]
fn event_text_replacement_layer_wins_conflicts_and_restores_from_snapshot() {
    let content = event_content(None);
    let loaded = load_event_world(&content);
    let snapshot = loaded.save().unwrap();

    assert_event_text(
        &pending_event(&loaded),
        EVENT_TITLE,
        EVENT_DESCRIPTION,
        OPTION_SUPPORT,
        OPTION_REJECT_KEY,
    );
    assert_eq!(loaded.save().unwrap().as_bytes(), snapshot.as_bytes());

    let mut restored = load_event_world(&content);
    restored.restore(&snapshot).unwrap();
    assert_eq!(restored.save().unwrap().as_bytes(), snapshot.as_bytes());
    assert_event_text(
        &pending_event(&restored),
        EVENT_TITLE,
        EVENT_DESCRIPTION,
        OPTION_SUPPORT,
        OPTION_REJECT_KEY,
    );
    assert_eq!(restored.save().unwrap().as_bytes(), snapshot.as_bytes());
}

#[test]
fn event_text_loader_uses_only_vfs_effective_localisation_files() {
    let shadow = tempfile::tempdir().unwrap();
    write_overlay(
        shadow.path(),
        "name = \"Same Path Shadow\"\ndependencies = { \"Event Localisation Replacement\" }",
        &[(
            "localisation/replace/a_event_l_english.yml",
            "l_english:\n localisation_replace.title:0 \"遮蔽标题\"\n",
        )],
    );
    let shadow_content = event_content(Some(shadow.path()));
    let shadow_world = load_event_world(&shadow_content);
    assert_event_text(
        &pending_event(&shadow_world),
        "遮蔽标题",
        "普通说明",
        "普通赞成",
        "替换拒绝乙",
    );

    let replacement = tempfile::tempdir().unwrap();
    write_overlay(
        replacement.path(),
        "name = \"Localisation Replace Path\"\ndependencies = { \"Event Localisation Replacement\" }\nreplace_path = \"localisation/replace\"",
        &[(
            "localisation/replace/overlay_l_english.yml",
            "l_english:\n localisation_replace.title:0 \"目录替换标题\"\n",
        )],
    );
    let replacement_content = event_content(Some(replacement.path()));
    let replacement_world = load_event_world(&replacement_content);
    assert_event_text(
        &pending_event(&replacement_world),
        "目录替换标题",
        "普通说明",
        "普通赞成",
        "普通拒绝",
    );
}

fn mission_content(extra_mod: Option<&Path>) -> ContentSet {
    let examples = examples();
    let mut sources = vec![
        ContentSource::base(ContentRoot::open(examples.join("world")).unwrap()),
        ContentSource::mod_directory(examples.join("mission-claims"), TextEncoding::Utf8).unwrap(),
    ];
    if let Some(path) = extra_mod {
        sources.push(ContentSource::mod_directory(path, TextEncoding::Utf8).unwrap());
    }
    ContentSet::resolve(sources).unwrap()
}

fn load_mission_world(content: &ContentSet) -> VanillaWorld {
    VanillaWorld::load_from_content_with_players_and_external_state(
        content,
        DATE,
        &BTreeSet::from(["AAA".to_owned(), "BBB".to_owned()]),
        &external_state("mission-claims/external_state.json"),
    )
    .unwrap()
}

#[test]
fn mission_titles_and_descriptions_are_resolved_from_replacement_localisation_after_restore() {
    let original_content = mission_content(None);
    let original = load_mission_world(&original_content);
    let snapshot = original.save().unwrap();

    let overlay = tempfile::tempdir().unwrap();
    write_overlay(
        overlay.path(),
        "name = \"Mission Text Replacement\"",
        &[
            (
                "localisation/z_mission_normal_l_english.yml",
                "l_english:\n aaa_claim_first_title:0 \"普通层任务标题\"\n aaa_claim_first_desc:0 \"普通层任务说明\"\n",
            ),
            (
                "localisation/replace/a_mission_replace_l_english.yml",
                "l_english:\n aaa_claim_first_title:0 \"替换任务标题\\n第一行\"\n aaa_claim_first_desc:0 \"替换任务说明\\n第一行\"\n",
            ),
        ],
    );
    let replacement_content = mission_content(Some(overlay.path()));
    let mut restored = load_mission_world(&replacement_content);
    restored.restore(&snapshot).unwrap();

    let report = restored.country_missions("AAA").unwrap();
    let first = report
        .missions
        .iter()
        .find(|mission| mission.id == "aaa_claim_first")
        .unwrap();
    assert_eq!(first.title, "替换任务标题\n第一行");
    assert_eq!(first.description.as_deref(), Some("替换任务说明\n第一行"));
    assert_eq!(restored.save().unwrap().as_bytes(), snapshot.as_bytes());
}
