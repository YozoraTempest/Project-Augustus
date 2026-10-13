// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{EntityId, EventChoice, PendingEventId};
use augustus_eu4::VanillaWorld;
use std::{fs, path::Path};

fn write(root: &Path, name: &str, text: &str) {
    let path = root.join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn copy(source: &Path, target: &Path) {
    for entry in walkdir::WalkDir::new(source) {
        let entry = entry.unwrap();
        let path = target.join(entry.path().strip_prefix(source).unwrap());
        if entry.file_type().is_dir() {
            fs::create_dir_all(path).unwrap();
        } else {
            fs::copy(entry.path(), path).unwrap();
        }
    }
}

fn fixture() -> tempfile::TempDir {
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let temp = tempfile::tempdir().unwrap();
    copy(&examples.join("world"), temp.path());
    copy(&examples.join("scripted-events/eu4"), temp.path());
    fs::rename(
        temp.path().join("events/demo.txt"),
        temp.path().join("events/A_first.txt"),
    )
    .unwrap();
    write(
        temp.path(),
        "common/on_actions/test.txt",
        "on_startup={events={scripts.start} if={limit={tag=AAA} log=startup}}",
    );
    temp
}

#[test]
fn mission_selection_is_initialized_before_startup_and_invalidated_on_failure() {
    let temp = fixture();
    write(
        temp.path(),
        "missions/selection.txt",
        "first={slot=1 potential={tag=AAA NOT={has_country_flag=alternate}} sample_first={}} second={slot=1 potential={tag=AAA has_country_flag=alternate} sample_second={}}",
    );
    write(
        temp.path(),
        "common/on_actions/test.txt",
        "on_startup={if={limit={tag=AAA has_mission=sample_first} complete_mission=sample_first set_country_flag=alternate swap_non_generic_missions=no}}",
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let world = loaded.initial.world();
    let first = loaded.initial.country_entity("AAA").unwrap();
    let other = loaded.initial.country_entity("BBB").unwrap();
    assert!(world.objective_available(first, "sample_second").unwrap());
    assert!(!world.objective_available(first, "sample_first").unwrap());
    assert!(world.completion_recorded(first, "sample_first").unwrap());
    assert!(!world.objective_available(other, "sample_first").unwrap());
    assert!(loaded.report.estates.startup_unresolved.is_empty());
    assert!(loaded.report.missions.known_countries > 0);

    write(
        temp.path(),
        "common/on_actions/test.txt",
        "on_startup={if={limit={tag=AAA} set_country_flag=alternate swap_non_generic_missions=yes unsupported_effect=yes}}",
    );
    let failed = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(!failed.report.estates.startup_unresolved.is_empty());
    assert_eq!(failed.report.missions.known_countries, 0);
    assert!(
        failed
            .initial
            .world()
            .objective_available(first, "sample_first")
            .is_err()
    );
    assert!(
        failed
            .initial
            .world()
            .completion_recorded(first, "sample_first")
            .is_err()
    );
    assert_eq!(failed.initial.world().objectives().groups.len(), 2);
    assert!(
        failed
            .initial
            .world()
            .numeric_value(
                first,
                &augustus_core::NumericSlot::Named("unresolved".into())
            )
            .is_err()
    );

    let later = VanillaWorld::load(temp.path(), "1450.1.1").unwrap();
    assert!(
        later
            .initial
            .world()
            .completion_recorded(first, "sample_first")
            .is_err()
    );
}

#[test]
fn unresolved_selection_has_the_same_save_in_different_installation_paths() {
    let temp = fixture();
    write(
        temp.path(),
        "missions/partial.txt",
        "partial={slot=1 potential={tag=AAA unknown_criterion=yes} sample={}}",
    );
    let other = tempfile::tempdir().unwrap();
    copy(temp.path(), other.path());
    let first = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let second = VanillaWorld::load(other.path(), "1444.11.11").unwrap();
    assert_eq!(first.save().unwrap(), second.save().unwrap());
    assert!(first.report.missions.unresolved["AAA"].contains("missions"));
    assert!(!first.report.missions.unresolved["AAA"].contains(&temp.path().display().to_string()));
}

#[test]
fn startup_event_pictures_are_attached_to_the_shared_catalog_and_saved_after_immediate() {
    let temp = fixture();
    write(
        temp.path(),
        "common/on_actions/test.txt",
        "on_startup={events={picture.start}}",
    );
    write(
        temp.path(),
        "events/pictures.txt",
        "country_event={id=picture.start is_triggered_only=yes picture={trigger={has_country_flag=ready} picture=ready} picture=waiting immediate={set_country_flag=ready country_event={id=picture.clear days=1}} option={name=ok}} country_event={id=picture.clear is_triggered_only=yes immediate={clr_country_flag=ready} option={name=ok}}",
    );
    let mut loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(
        loaded.report.estates.pending_startup_effects.is_empty(),
        "{:?}",
        loaded.report.estates.startup_unresolved
    );
    let id = loaded
        .initial
        .world()
        .pending_events()
        .iter()
        .find(|pending| pending.current == EntityId(0))
        .unwrap()
        .id;
    assert_eq!(
        loaded.initial.world().pending_event_picture(id).unwrap(),
        Some("ready")
    );
    loaded.advance(1).unwrap();
    assert!(
        !loaded
            .initial
            .world()
            .has_marker(EntityId(0), "ready")
            .unwrap()
    );
    assert_eq!(
        loaded.initial.world().pending_event_picture(id).unwrap(),
        Some("ready")
    );
    let save = loaded.save().unwrap();
    let mut restored = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    restored.restore(&save).unwrap();
    assert_eq!(restored.save().unwrap(), save);
    assert_eq!(
        restored.initial.world().pending_event_picture(id).unwrap(),
        Some("ready")
    );
}

#[test]
fn a_changed_unselected_picture_is_still_different_effective_startup_content() {
    let temp = fixture();
    write(
        temp.path(),
        "common/on_actions/test.txt",
        "on_startup={events={picture.start}}",
    );
    let text = "country_event={id=picture.start is_triggered_only=yes picture=first picture={trigger={always=no} picture=unselected} option={name=ok}}";
    write(temp.path(), "events/pictures.txt", text);
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let save = loaded.save().unwrap();
    write(
        temp.path(),
        "events/pictures.txt",
        &text.replace("picture=unselected", "picture=changed"),
    );
    let mut changed = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert_eq!(
        loaded.initial.world().pending_events(),
        changed.initial.world().pending_events()
    );
    assert_eq!(
        loaded
            .initial
            .world()
            .pending_event_picture(PendingEventId(0))
            .unwrap(),
        changed
            .initial
            .world()
            .pending_event_picture(PendingEventId(0))
            .unwrap()
    );
    assert!(changed.restore(&save).is_err());
}

#[test]
fn startup_macros_bind_first_declarations_and_discover_delayed_event_dependencies() {
    let temp = fixture();
    // Nested definitions are outside the loader's direct-file view.
    write(
        temp.path(),
        "common/scripted_effects/nested/earlier.txt",
        "begin={log=wrong_nested}",
    );
    let mut loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(
        loaded.report.estates.pending_startup_effects.is_empty(),
        "{:?}",
        loaded.report.estates.startup_unresolved
    );
    assert_eq!(
        loaded.initial.world().events(),
        &[
            (0, EntityId(0), "first".into()),
            (0, EntityId(0), "upper".into()),
            (0, EntityId(0), "lower".into()),
            (0, EntityId(0), "startup".into())
        ]
    );
    let ignored = &loaded.report.estates.startup_script_ignored_definitions;
    assert_eq!(ignored["begin"].len(), 2);
    assert_eq!(ignored["ready"].len(), 2);
    assert!(
        ignored["begin"][0]
            .path
            .ends_with("common/scripted_effects/Z_first.txt")
    );
    assert!(
        ignored["begin"][1]
            .path
            .ends_with("common/scripted_effects/a_second.txt")
    );
    assert!(
        ignored["ready"][0]
            .path
            .ends_with("common/scripted_triggers/Z_first.txt")
    );
    assert_eq!(loaded.initial.world().pending_events().len(), 1);
    let saved = loaded.save().unwrap();
    let mut restored = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    restored.restore(&saved).unwrap();
    for state in [&mut loaded, &mut restored] {
        state
            .initial
            .choose_event(&EventChoice {
                pending: PendingEventId(0),
                option: "0".into(),
            })
            .unwrap();
        assert_eq!(state.initial.world().events().last().unwrap().2, "after");
        state.advance(1).unwrap();
        assert_eq!(state.initial.world().events().last().unwrap().2, "later");
        state
            .initial
            .choose_event(&EventChoice {
                pending: PendingEventId(1),
                option: "0".into(),
            })
            .unwrap();
        assert_eq!(state.initial.world().events().last().unwrap().2, "end");
        assert!(state.initial.world().pending_events().is_empty());
    }
    assert_eq!(loaded.save().unwrap(), restored.save().unwrap());
}

#[test]
fn expanded_macro_changes_participate_in_the_content_guard() {
    let temp = fixture();
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let saved = loaded.save().unwrap();
    let path = temp.path().join("common/scripted_effects/Z_first.txt");
    let text = fs::read_to_string(&path).unwrap();
    fs::write(&path, text.replace("log = first", "log = changed")).unwrap();
    let mut changed = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(changed.restore(&saved).is_err());
    let before = changed.save().unwrap();
    // Editing an unbound template changes no effective core plan.
    fs::write(
        &path,
        text.replace("log = first", "log = changed")
            .replace("log = { first second }", "log = { third fourth }"),
    )
    .unwrap();
    let same = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert_eq!(same.save().unwrap(), before);
}

#[test]
fn startup_locations_reach_the_catalog_and_participate_in_the_content_guard() {
    let temp = fixture();
    write(
        temp.path(),
        "common/on_actions/test.txt",
        "on_startup={if={limit={tag=AAA} country_event={id=locations.start}}}",
    );
    let event = "country_event={id=locations.start is_triggered_only=yes goto=2 option={name=visit goto=1}}";
    write(temp.path(), "events/locations.txt", event);
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(loaded.report.estates.startup_unresolved.is_empty());
    let world = loaded.initial.world();
    let id = world.pending_events()[0].id;
    let province = |name: &str| {
        world
            .resolve_scope(&augustus_core::ScopeSelector::Named {
                kind: augustus_core::EntityKind::Province,
                name: name.into(),
            })
            .unwrap()
    };
    assert_eq!(world.pending_event_location(id).unwrap(), province("2"));
    assert_eq!(
        world.pending_event_option_location(id, "0").unwrap(),
        province("1")
    );
    let saved = loaded.save().unwrap();
    let mut resumed = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    resumed.restore(&saved).unwrap();
    assert_eq!(resumed.save().unwrap(), saved);
    write(
        temp.path(),
        "events/locations.txt",
        &event.replace("goto=2", "goto=1"),
    );
    let mut changed = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let before = changed.save().unwrap();
    assert!(changed.restore(&saved).is_err());
    assert_eq!(changed.save().unwrap(), before);
}

#[test]
fn reached_macro_failures_keep_the_definition_source_and_roll_back_all_startup_writes() {
    for (definition, call, reason) in [
        (
            "bad={\nlog={first second}\n}",
            "bad=yes",
            "unsupported script value",
        ),
        (
            "bad={\nlog>wrong\n}",
            "bad=yes",
            "unsupported script operator",
        ),
        (
            "bad={\nbad=yes\n}",
            "bad=yes",
            "recursive or excessive script call",
        ),
        (
            "bad={\nprovince_event={id=wrong}\n}",
            "bad=yes",
            "event call requires a matching entity scope",
        ),
    ] {
        let temp = fixture();
        write(temp.path(), "common/scripted_effects/bad.txt", definition);
        write(
            temp.path(),
            "events/A_first.txt",
            &format!(
                "country_event={{id=scripts.start is_triggered_only=yes immediate={{{call}}} option={{name=Continue}}}}"
            ),
        );
        let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
        assert!(loaded.initial.world().events().is_empty());
        assert!(loaded.initial.world().pending_events().is_empty());
        let report = &loaded.report.estates;
        assert!(
            report
                .startup_unresolved
                .iter()
                .any(|item| item.contains(reason)),
            "{:?}",
            report.startup_unresolved
        );
        let location = &report.pending_startup_effects[0];
        assert!(location.path.ends_with("common/scripted_effects/bad.txt"));
        assert_eq!(location.line, 2);
    }
}

#[test]
fn unbound_templates_are_allowed_but_invalid_calls_and_missing_references_still_block() {
    for (effect, reason, expected_path) in [
        (
            "unused_effect=yes",
            "unsupported script value",
            "common/scripted_effects/Z_first.txt",
        ),
        (
            "opening={parameter=value}",
            "expected scalar",
            "events/A_first.txt",
        ),
        (
            "not_registered=yes",
            "unsupported effect not_registered",
            "events/A_first.txt",
        ),
        (
            "country_event={id=missing}",
            "missing startup event missing",
            "events",
        ),
    ] {
        let temp = fixture();
        write(
            temp.path(),
            "events/A_first.txt",
            &format!(
                "country_event={{id=scripts.start is_triggered_only=yes immediate={{{effect}}} option={{name=Continue}}}}"
            ),
        );
        let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
        assert!(loaded.initial.world().events().is_empty());
        assert!(
            loaded
                .report
                .estates
                .startup_unresolved
                .iter()
                .any(|item| item.contains(reason)),
            "{:?}",
            loaded.report.estates.startup_unresolved
        );
        assert!(
            loaded.report.estates.pending_startup_effects[0]
                .path
                .ends_with(expected_path)
        );
    }
}
