// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    DiplomaticKind, DiplomaticRelation, EntityKind, EventCatalog, EventChoice, PendingEventId,
    Politics, Schema, Subjects, World,
};
use augustus_eu4::{ScriptCompiler, ScriptLibrary, TextDocument, TextEncoding, VanillaWorld};
use std::{fs, path::Path};

fn document(text: &str) -> TextDocument {
    TextDocument::from_bytes(
        "self-authored-root-scopes.txt",
        text.as_bytes().to_vec(),
        TextEncoding::Utf8,
    )
}

fn fixture() -> World {
    let mut world = World::new(Schema::new(["unused"]).unwrap());
    let a = world.add_entity("AAA", EntityKind::Country);
    let b = world.add_entity("BBB", EntityKind::Country);
    let province = world.add_entity("1", EntityKind::Province);
    world.set_owner(province, a).unwrap();
    world.set_capital(a, province).unwrap();
    world
        .set_politics(Politics {
            relations: vec![DiplomaticRelation {
                kind: DiplomaticKind::Subject,
                first: a,
                second: b,
                subject_type: Some("test_vassal".into()),
            }],
            offices: [("imperial_emperor".into(), a)].into(),
            ..Default::default()
        })
        .unwrap();
    world
        .set_subjects(Subjects {
            types: ["test_vassal".into()].into(),
            known: true,
            ..Default::default()
        })
        .unwrap();
    world.bind_global_scope("holder", a).unwrap();
    world.bind_global_scope("place", province).unwrap();
    world
}

#[test]
fn compiled_root_queries_share_live_membership_and_keep_root_through_logic_and_iteration() {
    let base = fixture();
    let library = ScriptLibrary::new();
    for optimise in [false, true] {
        let mut world = base.clone();
        let plan = ScriptCompiler::new(base.schema(), &library, optimise)
            .with_global_scope_types([("holder".into(), EntityKind::Country), ("place".into(), EntityKind::Province), ("missing".into(), EntityKind::Country)].into())
            .compile(&document("BBB={if={limit={AND={is_subject_of=ROOT OR={ROOT={tag=AAA} always=no} NOT={ROOT={tag=BBB}} is_subject_of=emperor is_subject_of=event_target:holder NOT={is_subject_of=event_target:place} NOT={is_subject_of=event_target:missing}}} ROOT={log=checked}}} if={limit={any_subject_country={is_subject_of=ROOT}} log=member} every_owned_province={limit={ROOT={tag=AAA}} ROOT={log=owned}}"), EntityKind::Country).unwrap().plan;
        world.execute(augustus_core::EntityId(0), &plan).unwrap();
        assert_eq!(
            world
                .events()
                .iter()
                .map(|e| (e.1.0, e.2.as_str()))
                .collect::<Vec<_>>(),
            [(0, "checked"), (0, "member"), (0, "owned")]
        );
        let mut politics = world.politics().clone();
        politics.relations.clear();
        world.set_politics(politics).unwrap();
        world.execute(augustus_core::EntityId(0), &plan).unwrap();
        assert_eq!(
            world
                .events()
                .iter()
                .skip(3)
                .map(|e| e.2.as_str())
                .collect::<Vec<_>>(),
            ["owned"]
        );
    }
}

#[test]
fn mixed_event_definitions_reset_root_type_and_options_recheck_current_relations() {
    let base = fixture();
    let library = ScriptLibrary::new();
    let source = document(
        "country_event={id=parent is_triggered_only=yes immediate={BBB={country_event={id=child} ROOT={log=returned} country_event={id=child days=2}} capital_scope={province_event={id=port}}} option={name=accept ROOT={log=parent_option}}} province_event={id=port is_triggered_only=yes immediate={owner={ROOT={if={limit={province_id=1} log=province}} if={limit={BBB={is_subject_of=ROOT}} log=wrong}}} option={name=accept ROOT={log=province_option}}} country_event={id=child is_triggered_only=yes immediate={ROOT={if={limit={tag=BBB} log=child}}} option={name=accept trigger={ROOT={tag=BBB}} ROOT={log=child_option}}}",
    );
    for optimise in [false, true] {
        let compiled = ScriptCompiler::new(base.schema(), &library, optimise)
            .compile_events(&source)
            .unwrap();
        let mut world = base.clone();
        world
            .set_event_catalog(
                EventCatalog::new(compiled.definitions)
                    .unwrap()
                    .with_option_conditions(compiled.option_conditions)
                    .unwrap(),
            )
            .unwrap();
        let parent = world
            .trigger_event("parent", augustus_core::EntityId(0))
            .unwrap();
        assert_eq!(parent, PendingEventId(2));
        assert_eq!(
            world
                .events()
                .iter()
                .map(|e| (e.1.0, e.2.as_str()))
                .collect::<Vec<_>>(),
            [(1, "child"), (0, "returned"), (2, "province")]
        );
        let saved = world.save("compiled-root").unwrap();
        let mut restored = World::restore_with_events(
            &saved,
            "compiled-root",
            base.schema(),
            vec![],
            world.event_catalog().clone(),
        )
        .unwrap();
        for w in [&mut world, &mut restored] {
            for pending in [0, 1, 2] {
                w.choose_event(&EventChoice {
                    pending: PendingEventId(pending),
                    option: "0".into(),
                })
                .unwrap();
            }
            w.advance_to(2).unwrap();
        }
        assert_eq!(world, restored);
        assert_eq!(
            world
                .events()
                .iter()
                .skip(3)
                .map(|e| (e.1.0, e.2.as_str()))
                .collect::<Vec<_>>(),
            [
                (1, "child_option"),
                (2, "province_option"),
                (0, "parent_option"),
                (1, "child")
            ]
        );
    }
}

#[test]
fn startup_branches_and_idea_selection_use_their_country_root_inside_other_scopes() {
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
    fs::create_dir_all(temp.path().join("common/on_actions")).unwrap();
    fs::write(temp.path().join("common/on_actions/root.txt"), "on_startup={if={limit={tag=AAA} BBB={if={limit={ROOT={tag=AAA} is_subject_of=ROOT} ROOT={set_country_flag=root_written log=checked}}} every_country={limit={ROOT={tag=AAA}} ROOT={log=iteration}} if={limit={capital_scope={ROOT={tag=AAA} owner={BBB={is_subject_of=ROOT}}}} log=capital}}}").unwrap();
    fs::write(
        temp.path().join("common/ideas/00_root.txt"),
        "root_group={free=yes trigger={ROOT={tag=BBB} is_subject_of=AAA} one={}}",
    )
    .unwrap();
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(
        loaded.report.estates.pending_startup_effects.is_empty(),
        "{:?}",
        loaded.report.estates.startup_unresolved
    );
    let a = loaded.initial.country_entity("AAA").unwrap();
    assert!(
        loaded
            .initial
            .world()
            .has_marker(a, "root_written")
            .unwrap()
    );
    assert_eq!(
        loaded
            .initial
            .world()
            .events()
            .iter()
            .map(|e| (e.1, e.2.as_str()))
            .collect::<Vec<_>>(),
        [
            (a, "checked"),
            (a, "iteration"),
            (a, "iteration"),
            (a, "capital")
        ]
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
            .contains_key("root_group")
    );
}

#[test]
fn invalid_root_types_and_unconfirmed_contexts_are_errors_even_in_unselected_branches() {
    let schema = Schema::new(["unused"]).unwrap();
    let library = ScriptLibrary::new();
    for optimise in [false, true] {
        for text in [
            "if={limit={always=no ROOT={province_id=1}} log=invalid}",
            "if={limit={always=no} ROOT={owner={log=invalid}}}",
            "if={limit={is_subject_of=FROM} log=invalid}",
            "if={limit={is_subject_of=event_target:undeclared} log=invalid}",
            "FROM={log=invalid}",
        ] {
            let error = ScriptCompiler::new(&schema, &library, optimise)
                .compile(&document(text), EntityKind::Country)
                .unwrap_err();
            assert!(error.location().is_some(), "{error}");
        }
        let error = ScriptCompiler::new(&schema, &library, optimise)
            .compile(&document("is_subject_of=AAA"), EntityKind::Province)
            .unwrap_err();
        assert!(error.location().is_some());
    }
}
