// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    DiplomaticKind, DiplomaticRelation, EntityId, EntityKind, Politics, Schema, Subjects, World,
};
use augustus_eu4::{ScriptCompiler, ScriptLibrary, TextDocument, TextEncoding, VanillaWorld};
use std::{fs, path::Path};

fn document(text: &str) -> TextDocument {
    TextDocument::from_bytes(
        "self-authored-subjects.txt",
        text.as_bytes().to_vec(),
        TextEncoding::Utf8,
    )
}

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

fn write(root: &Path, name: &str, text: &str) {
    let path = root.join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

#[test]
fn typed_member_and_capital_owner_queries_translate_in_both_compilation_modes() {
    let schema = Schema::new(["unused"]).unwrap();
    let library = ScriptLibrary::new();
    for optimise in [false, true] {
        let mut world = World::new(schema.clone());
        for name in ["AAA", "BBB", "CCC"] {
            world.add_entity(name, EntityKind::Country);
        }
        world.add_entity("Port", EntityKind::Province);
        world.set_owner(EntityId(3), EntityId(1)).unwrap();
        world.set_capital(EntityId(1), EntityId(3)).unwrap();
        world
            .set_politics(Politics {
                relations: vec![DiplomaticRelation {
                    kind: DiplomaticKind::Subject,
                    first: EntityId(0),
                    second: EntityId(1),
                    subject_type: Some("test_vassal".into()),
                }],
                ..Default::default()
            })
            .unwrap();
        let types = ["test_vassal", "test_union"].map(String::from).into();
        world.set_subjects(Subjects { types, known: true }).unwrap();
        let plan = ScriptCompiler::new(&schema, &library, optimise)
            .with_subject_type_ids(world.subjects().types.clone())
            .compile(&document(r#"
                if={limit={any_subject_country={tag=BBB is_subject_of_type=test_vassal capital_scope={owner={is_subject_of_type=test_vassal}}}} log=direct}
                if={limit={any_subject_country={is_subject_of_type=test_union}} log=wrong} else={log=no_union}
            "#), EntityKind::Country).unwrap().plan;
        world.execute(EntityId(0), &plan).unwrap();
        assert_eq!(
            world
                .events()
                .iter()
                .map(|row| (row.1, row.2.as_str()))
                .collect::<Vec<_>>(),
            [(EntityId(0), "direct"), (EntityId(0), "no_union")]
        );
    }
}

#[test]
fn invalid_types_and_scopes_fail_before_unreachable_branches_are_optimised() {
    let schema = Schema::new(["unused"]).unwrap();
    let library = ScriptLibrary::new();
    for optimise in [false, true] {
        for text in [
            "if={limit={always=no is_subject_of_type=missing} log=wrong}",
            "if={limit={any_subject_country={always=no is_subject_of_type=missing}} log=wrong}",
            "if={limit={is_subject_of_type=$type$} log=wrong}",
            "if={limit={is_subject_of_type={type=test_vassal}} log=wrong}",
            "if={limit={any_subject_country=yes} log=wrong}",
            "if={limit={capital_scope={is_subject_of_type=test_vassal}} log=wrong}",
            "if={limit={capital_scope={any_subject_country={always=yes}}} log=wrong}",
        ] {
            let error = ScriptCompiler::new(&schema, &library, optimise)
                .with_subject_type_ids(["test_vassal".into()].into())
                .compile(&document(text), EntityKind::Country)
                .unwrap_err();
            assert!(error.location().is_some(), "{error}");
        }
        assert!(
            ScriptCompiler::new(&schema, &library, optimise)
                .compile(
                    &document("if={limit={is_subject_of_type=test_vassal} log=wrong}"),
                    EntityKind::Country
                )
                .is_err()
        );
    }
}

#[test]
fn initial_conditions_and_later_events_share_loaded_subject_declarations_and_relations() {
    let temp = fixture();
    write(
        temp.path(),
        "common/ideas/test_ideas.txt",
        "subject_ideas={free=yes trigger={is_subject_of_type=test_vassal} start={global_tax_modifier=0.1} one={} bonus={}}\n independent_ideas={free=yes trigger={NOT={is_subject_of_type=test_vassal}} one={}}",
    );
    write(
        temp.path(),
        "common/estates/subjects.txt",
        "estate_subject={trigger={is_subject_of_type=test_vassal}}",
    );
    write(
        temp.path(),
        "common/on_actions/subjects.txt",
        "on_startup={if={limit={tag=AAA any_subject_country={is_subject_of_type=test_vassal}} country_event={id=subject.later days=1}}}",
    );
    write(
        temp.path(),
        "events/subjects.txt",
        r#"
        country_event={id=subject.later is_triggered_only=yes trigger={any_subject_country={is_subject_of_type=test_vassal}}
            immediate={if={limit={any_subject_country={tag=BBB is_subject_of_type=test_vassal}} log=subject_found}}
            option={name=continue}}
    "#,
    );
    let mut loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(
        loaded.report.estates.startup_unresolved.is_empty(),
        "{:?}",
        loaded.report.estates.startup_unresolved
    );
    assert!(loaded.initial.world().subjects().known);
    assert_eq!(
        loaded.country("BBB").unwrap().estates.unwrap()["estate_subject"].active,
        Some(true)
    );
    assert_eq!(
        loaded.country("AAA").unwrap().estates.unwrap()["estate_subject"].active,
        Some(false)
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
            .contains_key("subject_ideas")
    );
    let saved = loaded.save().unwrap();
    let mut resumed = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    resumed.restore(&saved).unwrap();
    loaded.advance(1).unwrap();
    resumed.advance(1).unwrap();
    assert_eq!(loaded.save().unwrap(), resumed.save().unwrap());
    assert_eq!(
        loaded.initial.world().events().last().unwrap().2,
        "subject_found"
    );
}

#[test]
fn future_history_is_skipped_and_active_unapplied_changes_and_failed_startup_remain_unknown() {
    let temp = fixture();
    let history = "history/countries/BBB - Beta.txt";
    let original = fs::read_to_string(temp.path().join(history)).unwrap();
    write(
        temp.path(),
        history,
        &(original.clone() + "\n1445.1.1={grant_independence=yes}"),
    );
    assert!(
        VanillaWorld::load(temp.path(), "1444.11.11")
            .unwrap()
            .initial
            .world()
            .subjects()
            .known
    );
    let later = VanillaWorld::load(temp.path(), "1445.1.1").unwrap();
    assert!(!later.initial.world().subjects().known);
    let member = later.initial.country_entity("BBB").unwrap();
    assert!(
        later
            .initial
            .world()
            .is_subject_of_type(member, "test_vassal")
            .is_err()
    );
    write(temp.path(), history, &original);
    let province = "history/provinces/1 - North.txt";
    // A scoped, unexecuted relation change invalidates the graph regardless of its stale edges.
    let province_original = fs::read_to_string(temp.path().join(province)).unwrap();
    write(
        temp.path(),
        province,
        &(province_original.clone()
            + "\nif={limit={always=yes} owner={change_subject_type={subject_type=test_vassal}}}"),
    );
    assert!(
        !VanillaWorld::load(temp.path(), "1444.11.11")
            .unwrap()
            .initial
            .world()
            .subjects()
            .known
    );
    write(temp.path(), province, &province_original);
    write(
        temp.path(),
        "common/scripted_effects/subjects.txt",
        "custom_relation_history={grant_independence=yes}",
    );
    write(
        temp.path(),
        history,
        &(original.clone() + "\n1445.1.1={custom_relation_history=yes}"),
    );
    assert!(
        VanillaWorld::load(temp.path(), "1444.11.11")
            .unwrap()
            .initial
            .world()
            .subjects()
            .known
    );
    write(
        temp.path(),
        history,
        &(original.clone() + "\nif={limit={always=yes} custom_relation_history=yes}"),
    );
    assert!(
        !VanillaWorld::load(temp.path(), "1444.11.11")
            .unwrap()
            .initial
            .world()
            .subjects()
            .known
    );
    write(temp.path(), history, &original);
    write(
        temp.path(),
        "common/on_actions/subjects.txt",
        "on_startup={log=rolled_back unsupported_effect=yes}",
    );
    let failed = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(!failed.initial.world().subjects().known);
    assert!(failed.initial.world().events().is_empty());
    assert!(!failed.report.estates.startup_unresolved.is_empty());
}
