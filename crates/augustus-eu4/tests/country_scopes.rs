// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{EntityKind, Markers, Schema, Target, World};
use augustus_eu4::{ScriptCompiler, ScriptLibrary, TextDocument, TextEncoding, VanillaWorld};
use std::{fs, path::Path};

fn document(text: &str) -> TextDocument {
    TextDocument::from_bytes(
        "self-authored-country-scopes.txt",
        text.as_bytes().to_vec(),
        TextEncoding::Utf8,
    )
}

#[test]
fn named_country_identity_and_effects_survive_last_province_changes_and_restore() {
    let schema = Schema::new(["unused"]).unwrap();
    let library = ScriptLibrary::new();
    for optimise in [false, true] {
        let mut world = World::new(schema.clone());
        let a = world.add_entity("AAA", EntityKind::Country);
        let b = world.add_entity("B02", EntityKind::Country);
        let province = world.add_entity("1", EntityKind::Province);
        world.set_owner(province, a).unwrap();
        world.set_capital(a, province).unwrap();
        world
            .set_markers(Markers {
                entities: [(a, Default::default()), (b, Default::default())].into(),
                ..Default::default()
            })
            .unwrap();
        let compile = |text, scope| {
            ScriptCompiler::new(&schema, &library, optimise)
                .compile(&document(text), scope)
                .unwrap()
                .plan
        };
        let write = compile(
            "B02={set_country_flag=written log=written} if={limit={B02={TAG=B02 exists=no has_country_flag=written} AAA={}} log=checked}",
            EntityKind::Province,
        );
        world.execute(province, &write).unwrap();
        assert_eq!(world.events()[0].1, b);
        assert_eq!(world.events()[0].2, "written");
        assert_eq!(world.events()[1].2, "checked");
        let query = compile(
            "if={limit={B02={exists=yes} AAA={exists=no}} log=changed} else={log=before}",
            EntityKind::Country,
        );
        world.execute(a, &query).unwrap();
        assert_eq!(world.events().last().unwrap().2, "before");
        world.set_owner(province, b).unwrap();
        world.execute(a, &query).unwrap();
        assert_eq!(world.events().last().unwrap().2, "changed");
        world.set_owner(province, a).unwrap();
        let erase = compile(
            "B02={clr_country_flag=written log=erased}",
            EntityKind::Country,
        );
        world.execute(a, &erase).unwrap();
        assert!(!world.has_marker(b, "written").unwrap());
        assert_eq!(world.events().last().unwrap().1, b);
        let saved = world.save("country-scopes").unwrap();
        let mut restored = World::restore(&saved, "country-scopes", &schema).unwrap();
        restored.execute(a, &query).unwrap();
        assert_eq!(restored.events().last().unwrap().2, "before");
        let nested = compile(
            "B02={AAA={capital_scope={owner={B02={log=nested}}}}}",
            EntityKind::Country,
        );
        restored.execute(a, &nested).unwrap();
        assert_eq!(restored.events().last().unwrap().1, b);
        assert_eq!(restored.events().last().unwrap().2, "nested");
        let logic = compile(
            "if={limit={AND={B02={TAG=B02} NOT={B02={has_country_flag=written}}}} log=logic}",
            EntityKind::Country,
        );
        restored.execute(a, &logic).unwrap();
        assert_eq!(restored.events().last().unwrap().2, "logic");
    }
}

#[test]
fn unregistered_country_names_remain_errors_and_other_entity_types_do_not_match() {
    let schema = Schema::new(["unused"]).unwrap();
    let library = ScriptLibrary::new();
    for optimise in [false, true] {
        let mut world = World::new(schema.clone());
        let a = world.add_entity("AAA", EntityKind::Country);
        let province = world.add_entity("BBB", EntityKind::Province);
        world.mark_target_absent(province, Target::Owner).unwrap();
        for text in [
            "BBB={log=invalid}",
            "if={limit={BBB={}} log=invalid}",
            "if={limit={NOT={ZZZ={always=yes}}} log=invalid}",
        ] {
            let mut candidate = world.clone();
            let plan = ScriptCompiler::new(&schema, &library, optimise)
                .compile(&document(text), EntityKind::Country)
                .unwrap()
                .plan;
            assert!(candidate.execute(a, &plan).is_err(), "{text}");
            assert!(candidate.events().is_empty());
        }
        let b = world.add_entity("BBB", EntityKind::Country);
        let plan = ScriptCompiler::new(&schema, &library, optimise)
            .compile(&document("BBB={log=country}"), EntityKind::Province)
            .unwrap()
            .plan;
        world.execute(province, &plan).unwrap();
        assert_eq!(world.events().last().unwrap().1, b);
    }
}

#[test]
fn nested_country_bodies_are_checked_before_optimisation_and_numeric_scopes_keep_their_type() {
    let schema = Schema::new(["unused"]).unwrap();
    let library = ScriptLibrary::new();
    for optimise in [false, true] {
        for text in [
            "if={limit={always=no BBB={unsupported=yes}} log=invalid}",
            "if={limit={always=no} BBB={unsupported=yes}}",
            "BBB={owner={log=invalid}}",
            "BBB={province_event={id=invalid}}",
            "if={limit={BBB={province_id=1}} log=invalid}",
            "bbb={log=invalid}",
        ] {
            let error = ScriptCompiler::new(&schema, &library, optimise)
                .compile(&document(text), EntityKind::Country)
                .unwrap_err();
            assert!(error.location().is_some(), "{text}: {error}");
        }
        let mut world = World::new(schema.clone());
        let a = world.add_entity("AAA", EntityKind::Country);
        let country = world.add_entity("123", EntityKind::Country);
        let province = world.add_entity("123", EntityKind::Province);
        world.mark_target_absent(province, Target::Owner).unwrap();
        let plan = ScriptCompiler::new(&schema, &library, optimise)
            .compile(
                &document("123={if={limit={province_id=123 AAA={TAG=AAA}} log=province}}"),
                EntityKind::Country,
            )
            .unwrap()
            .plan;
        world.execute(country, &plan).unwrap();
        assert_eq!(world.events().last().unwrap().1, province);
        assert_ne!(a, country);
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
    fs::write(
        temp.path().join("history/provinces/1 - North.txt"),
        "owner=AAA",
    )
    .unwrap();
    fs::write(
        temp.path().join("history/provinces/2 - East.txt"),
        "owner=AAA",
    )
    .unwrap();
    fs::create_dir_all(temp.path().join("common/on_actions")).unwrap();
    temp
}

#[test]
fn startup_effects_and_idea_conditions_share_registered_landless_country_identity() {
    let temp = fixture();
    fs::write(temp.path().join("common/on_actions/test.txt"),
        "on_startup={if={limit={tag=AAA BBB={tag=BBB exists=no}} BBB={set_country_flag=written log=dormant} if={limit={BBB={has_country_flag=written exists=no}} log=passed}}}").unwrap();
    fs::write(
        temp.path().join("common/ideas/00_scopes.txt"),
        "dormant_neighbor={free=yes trigger={tag=AAA BBB={tag=BBB exists=no}} one={}}",
    )
    .unwrap();
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(
        loaded.report.estates.pending_startup_effects.is_empty(),
        "{:?}",
        loaded.report.estates.startup_unresolved
    );
    let b = loaded.initial.country_entity("BBB").unwrap();
    assert!(!loaded.initial.world().has_owned_province(b).unwrap());
    assert!(loaded.initial.world().has_marker(b, "written").unwrap());
    assert_eq!(
        loaded
            .initial
            .world()
            .events()
            .iter()
            .map(|e| e.2.as_str())
            .collect::<Vec<_>>(),
        ["dormant", "passed"]
    );
    assert!(
        loaded
            .country("AAA")
            .unwrap()
            .ideas
            .unwrap()
            .groups
            .as_ref()
            .unwrap()
            .contains_key("dormant_neighbor")
    );
    let saved = loaded.save().unwrap();
    let mut restored = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    restored.restore(&saved).unwrap();
    assert_eq!(saved, restored.save().unwrap());
}

#[test]
fn an_unknown_country_in_startup_abandons_the_candidate_without_committing_its_prefix() {
    let temp = fixture();
    fs::write(temp.path().join("common/on_actions/test.txt"),
        "on_startup={if={limit={tag=AAA} BBB={set_country_flag=written log=prefix} if={limit={ZZZ={always=yes}} log=invalid}}}").unwrap();
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(!loaded.report.estates.pending_startup_effects.is_empty());
    assert!(
        loaded
            .report
            .estates
            .startup_unresolved
            .iter()
            .any(|reason| reason.contains("unregistered named entity ZZZ"))
    );
    assert!(loaded.initial.world().events().is_empty());
    assert!(
        !loaded
            .initial
            .world()
            .has_marker(loaded.initial.country_entity("BBB").unwrap(), "written")
            .unwrap()
    );
}
