// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{EntityId, EntityKind, Markers, Schema, Target, World};
use augustus_eu4::{ScriptCompiler, ScriptLibrary, TextDocument, TextEncoding, VanillaWorld};
use std::{fs, path::Path};

fn document(text: &str) -> TextDocument {
    TextDocument::from_bytes(
        "self-authored-owned-iteration.txt",
        text.as_bytes().to_vec(),
        TextEncoding::Utf8,
    )
}

fn world() -> World {
    let mut world = World::new(Schema::new(["unused"]).unwrap());
    for name in ["AAA", "BBB", "Empty"] {
        world.add_entity(name, EntityKind::Country);
    }
    for name in ["3", "1", "2", "Sea"] {
        world.add_entity(name, EntityKind::Province);
    }
    for province in [3, 4, 5] {
        world.set_owner(EntityId(province), EntityId(0)).unwrap();
    }
    world
        .mark_target_absent(EntityId(6), Target::Owner)
        .unwrap();
    world
        .set_markers(Markers {
            entities: [
                (EntityId(0), Default::default()),
                (EntityId(3), Default::default()),
                (EntityId(4), Default::default()),
                (EntityId(5), Default::default()),
            ]
            .into(),
            ..Default::default()
        })
        .unwrap();
    world
}

#[test]
fn translation_reads_current_order_live_filters_and_restored_randomness() {
    let text = r#"
        set_country_flag=gate
        every_owned_province={
            log=first
            owner={clr_country_flag=gate}
            limit={owner={has_country_flag=gate}}
        }
        every_owned_province={log=member}
        every_owned_province={
            limit={province_id=3}
            owner={every_owned_province={limit={OR={province_id=1 province_id=2}} log=inner}}
            log=outer
        }
        random_owned_province={limit={OR={province_id=1 province_id=2}} log=chosen}
        random_owned_province={limit={always=no} log=wrong}
        log=returned
    "#;
    let mut outputs = Vec::new();
    for optimise in [false, true] {
        let mut world = world();
        // Reacquiring province 3 appends it after provinces 1 and 2.
        world.set_owner(EntityId(3), EntityId(1)).unwrap();
        world.set_owner(EntityId(3), EntityId(0)).unwrap();
        let library = ScriptLibrary::new();
        let plan = ScriptCompiler::new(world.schema(), &library, optimise)
            .compile(&document(text), EntityKind::Country)
            .unwrap()
            .plan;
        world.execute(EntityId(0), &plan).unwrap();
        let rows: Vec<_> = world
            .events()
            .iter()
            .map(|row| (row.1, row.2.as_str()))
            .collect();
        assert_eq!(
            &rows[..7],
            &[
                (EntityId(4), "first"),
                (EntityId(4), "member"),
                (EntityId(5), "member"),
                (EntityId(3), "member"),
                (EntityId(4), "inner"),
                (EntityId(5), "inner"),
                (EntityId(3), "outer"),
            ]
        );
        assert!([EntityId(4), EntityId(5)].contains(&rows[7].0));
        assert_eq!(rows[7].1, "chosen");
        assert_eq!(rows[8], (EntityId(0), "returned"));
        assert_eq!(rows.len(), 9);
        let mut restored = World::restore(
            &world.save("owned-compat-iteration").unwrap(),
            "owned-compat-iteration",
            world.schema(),
        )
        .unwrap();
        for state in [&mut world, &mut restored] {
            state.execute(EntityId(0), &plan).unwrap();
            let before = state.save("owned-compat-iteration").unwrap();
            state.execute(EntityId(2), &plan[1..4]).unwrap();
            assert_eq!(state.save("owned-compat-iteration").unwrap(), before);
        }
        assert_eq!(world, restored);
        outputs.push(world.save("owned-compat-iteration").unwrap());
    }
    assert_eq!(outputs[0], outputs[1]);
}

#[test]
fn province_flags_follow_owned_province_scope() {
    let text = r#"
        every_owned_province={
            if={limit={NOT={has_province_flag=ready}} set_province_flag=ready}
        }
        every_owned_province={
            limit={has_province_flag=ready}
            log=flagged
            clr_province_flag=ready
        }
    "#;
    let mut world = world();
    let plan = ScriptCompiler::new(world.schema(), &ScriptLibrary::new(), true)
        .compile(&document(text), EntityKind::Country)
        .unwrap()
        .plan;

    world.execute(EntityId(0), &plan).unwrap();

    for province in [EntityId(3), EntityId(4), EntityId(5)] {
        assert!(!world.has_marker(province, "ready").unwrap());
    }
    assert_eq!(
        world
            .events()
            .iter()
            .filter(|event| event.2 == "flagged")
            .count(),
        3
    );
}

#[test]
fn owned_effects_reject_invalid_bodies_and_scope_before_optimisation() {
    let schema = Schema::new(["unused"]).unwrap();
    let library = ScriptLibrary::new();
    for keyword in ["every_owned_province", "random_owned_province"] {
        for (scope, body) in [
            (EntityKind::Province, "{log=wrong}"),
            (EntityKind::Country, "yes"),
            (EntityKind::Country, "{limit=yes log=wrong}"),
            (
                EntityKind::Country,
                "{limit={always=no} unknown_effect=yes}",
            ),
            (
                EntityKind::Country,
                "{limit={always=yes} limit={always=no}}",
            ),
            (
                EntityKind::Country,
                "{limit={has_country_flag=gate} log=wrong}",
            ),
            (EntityKind::Country, "{add_treasury=1}"),
            (EntityKind::Country, "{cede_province=BBB}"),
            (EntityKind::Country, "{every_owned_province={log=wrong}}"),
        ] {
            let text = format!("{keyword}={body}");
            for optimise in [false, true] {
                let error = ScriptCompiler::new(&schema, &library, optimise)
                    .compile(&document(&text), scope)
                    .unwrap_err();
                assert!(error.location().is_some(), "{text}");
                assert!(error.path().ends_with("self-authored-owned-iteration.txt"));
            }
        }
    }
}

fn fixture() -> tempfile::TempDir {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world");
    let temp = tempfile::tempdir().unwrap();
    for entry in walkdir::WalkDir::new(&root) {
        let entry = entry.unwrap();
        let path = temp.path().join(entry.path().strip_prefix(&root).unwrap());
        if entry.file_type().is_dir() {
            fs::create_dir_all(path).unwrap();
        } else {
            fs::copy(entry.path(), path).unwrap();
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
fn startup_discovers_and_resumes_events_inside_both_owned_effects() {
    let temp = fixture();
    write(
        temp.path(),
        "common/on_actions/test.txt",
        "on_startup={events={owned.start}}",
    );
    write(
        temp.path(),
        "events/test.txt",
        r#"
        country_event={id=owned.start trigger={tag=AAA} is_triggered_only=yes immediate={
            every_owned_province={province_event={id=owned.member days=1}}
            random_owned_province={province_event={id=owned.pick days=1}}
            log=returned
        } option={name=ok}}
        province_event={id=owned.member is_triggered_only=yes immediate={log=member} option={name=ok}}
        province_event={id=owned.pick is_triggered_only=yes immediate={log=chosen} option={name=ok}}
    "#,
    );
    let mut loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(
        loaded.report.estates.startup_unresolved.is_empty(),
        "{:?}",
        loaded.report.estates.startup_unresolved
    );
    let members: Vec<_> = loaded
        .initial
        .world()
        .owned_provinces(loaded.initial.country_entity("AAA").unwrap())
        .unwrap()
        .to_vec();
    assert!(!members.is_empty());
    assert_eq!(loaded.initial.world().pending_tasks(), members.len() + 1);
    let saved = loaded.save().unwrap();
    let mut restored = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    restored.restore(&saved).unwrap();
    loaded.advance(1).unwrap();
    restored.advance(1).unwrap();
    assert_eq!(loaded.save().unwrap(), restored.save().unwrap());
    let rows = loaded.initial.world().events();
    assert_eq!(
        rows.iter()
            .filter(|row| row.2 == "member")
            .map(|row| row.1)
            .collect::<Vec<_>>(),
        members
    );
    let chosen: Vec<_> = rows.iter().filter(|row| row.2 == "chosen").collect();
    assert_eq!(chosen.len(), 1);
    assert!(members.contains(&chosen[0].1));
}
