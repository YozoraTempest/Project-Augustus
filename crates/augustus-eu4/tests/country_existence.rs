// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{EntityKind, Schema, Target, World};
use augustus_eu4::{ScriptCompiler, ScriptLibrary, TextDocument, TextEncoding, VanillaWorld};
use std::{fs, path::Path};

fn document(text: &str) -> TextDocument {
    TextDocument::from_bytes(
        "self-authored-existence.txt",
        text.as_bytes().to_vec(),
        TextEncoding::Utf8,
    )
}

#[test]
fn literal_and_current_queries_use_live_ownership_in_both_compilation_modes() {
    let schema = Schema::new(["unused"]).unwrap();
    let library = ScriptLibrary::new();
    for optimise in [false, true] {
        let mut world = World::new(schema.clone());
        let active = world.add_entity("AAA", EntityKind::Country);
        let dormant = world.add_entity("BBB", EntityKind::Country);
        let province = world.add_entity("Home", EntityKind::Province);
        let sea = world.add_entity("Sea", EntityKind::Province);
        world.set_owner(province, active).unwrap();
        world.mark_target_absent(sea, Target::Owner).unwrap();
        let compile = |kind, text| {
            ScriptCompiler::new(&schema, &library, optimise)
                .compile(&document(text), kind)
                .unwrap()
                .plan
        };
        let query = compile(
            EntityKind::Country,
            "if={limit={exists=AAA NOT={exists=BBB}} log=active} else={log=changed}",
        );
        world.execute(dormant, &query).unwrap();
        assert_eq!(world.events().last().unwrap().2, "active");
        world.set_owner(province, dormant).unwrap();
        world.execute(active, &query).unwrap();
        assert_eq!(world.events().last().unwrap().2, "changed");
        let current = compile(
            EntityKind::Country,
            "if={limit={exists=no} log=inactive} else={log=present}",
        );
        world.execute(active, &current).unwrap();
        assert_eq!(world.events().last().unwrap().2, "inactive");
        let province_query = compile(
            EntityKind::Province,
            "if={limit={exists=no NOT={exists=yes} exists=BBB} log=province}",
        );
        world.execute(province, &province_query).unwrap();
        assert_eq!(world.events().last().unwrap().2, "province");
        let invalid = compile(
            EntityKind::Country,
            "if={limit={NOT={exists=ZZZ}} log=unregistered}",
        );
        assert!(world.execute(active, &invalid).is_err());
    }
}

#[test]
fn unsupported_operands_are_located_even_inside_unreachable_branches() {
    let schema = Schema::new(["unused"]).unwrap();
    let library = ScriptLibrary::new();
    for optimise in [false, true] {
        for operand in [
            "maybe",
            "aaa",
            "AAAA",
            "ROOT",
            "PREV",
            "emperor",
            "event_target:holder",
            "\"\"",
            "{AAA}",
        ] {
            let error = ScriptCompiler::new(&schema, &library, optimise)
                .compile(
                    &document(&format!(
                        "if={{limit={{always=no exists={operand}}} log=unreachable}}"
                    )),
                    EntityKind::Country,
                )
                .unwrap_err();
            assert!(error.location().is_some(), "{operand}: {error}");
        }
    }
}

fn fixture() -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world");
    for entry in walkdir::WalkDir::new(&source) {
        let entry = entry.unwrap();
        let target = temp
            .path()
            .join(entry.path().strip_prefix(&source).unwrap());
        if entry.file_type().is_dir() {
            fs::create_dir_all(target).unwrap();
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
    temp
}

#[test]
fn complete_unowned_history_drives_startup_and_idea_selection_but_missing_history_stays_unknown() {
    let temp = fixture();
    // Every land province has a complete history. Cores do not grant territory.
    fs::write(
        temp.path().join("history/provinces/1 - North.txt"),
        "owner=AAA add_core=BBB 1500.1.1={owner=BBB}",
    )
    .unwrap();
    fs::write(
        temp.path().join("history/provinces/2 - East.txt"),
        "culture=alpha add_core=BBB 1500.1.1={owner=BBB}",
    )
    .unwrap();
    let actions = temp.path().join("common/on_actions");
    fs::create_dir_all(&actions).unwrap();
    fs::write(
        actions.join("test.txt"),
        "on_startup={if={limit={tag=AAA exists=AAA NOT={exists=BBB}} log=ownership_passed}}",
    )
    .unwrap();
    fs::write(
        temp.path().join("common/ideas/00_conditions.txt"),
        "dormant={free=yes trigger={tag=BBB exists=no} one={}} ",
    )
    .unwrap();
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let b = loaded.initial.country_entity("BBB").unwrap();
    assert!(!loaded.initial.world().has_owned_province(b).unwrap());
    assert!(
        loaded
            .initial
            .world()
            .events()
            .iter()
            .any(|e| e.2 == "ownership_passed")
    );
    assert!(
        loaded
            .country("BBB")
            .unwrap()
            .ideas
            .unwrap()
            .groups
            .as_ref()
            .unwrap()
            .contains_key("dormant")
    );
    let saved = loaded.save().unwrap();
    let mut restored = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    restored.restore(&saved).unwrap();
    assert_eq!(saved, restored.save().unwrap());
    fs::remove_file(temp.path().join("history/provinces/2 - East.txt")).unwrap();
    let mut missing = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(missing.initial.world().has_owned_province(b).is_err());
    assert!(!missing.report.estates.pending_startup_effects.is_empty());
    assert!(missing.restore(&saved).is_err());
}

#[test]
fn unhandled_conditional_owner_assignment_does_not_prove_absence() {
    let temp = fixture();
    fs::write(
        temp.path().join("history/provinces/1 - North.txt"),
        "owner=AAA",
    )
    .unwrap();
    fs::write(
        temp.path().join("history/provinces/2 - East.txt"),
        "if={limit={always=yes} owner=BBB}",
    )
    .unwrap();
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(
        loaded
            .initial
            .world()
            .has_owned_province(loaded.initial.country_entity("BBB").unwrap())
            .is_err()
    );
}
