// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    Administration, EntityKind, FieldId, Politics, ProvinceAdministration, Schema, World,
};
use augustus_eu4::{ScriptCompiler, ScriptLibrary, TextDocument, TextEncoding, VanillaWorld};
use std::{collections::BTreeSet, fs, path::Path};

fn document(text: &str) -> TextDocument {
    TextDocument::from_bytes(
        "self-authored-scoped-conditions.txt",
        text.as_bytes().to_vec(),
        TextEncoding::Utf8,
    )
}

#[test]
fn scoped_conditions_compile_to_live_core_queries_with_or_without_optimisation() {
    let schema = Schema::new(["variable/seen"]).unwrap();
    let library = ScriptLibrary::new();
    let text = "if={limit={capital_scope={area=home region=union superregion=outer continent=land owner={tag=AAA}} emperor={tag=AAA} event_target:place={continent=land}} set_variable={which=seen value=1}} else={set_variable={which=seen value=2}}";
    for optimise in [false, true] {
        let compiled = ScriptCompiler::new(&schema, &library, optimise)
            .with_global_scope_types([("place".into(), EntityKind::Province)].into())
            .compile(&document(text), EntityKind::Country)
            .unwrap();
        let mut world = World::new(schema.clone());
        let country = world.add_entity("AAA", EntityKind::Country);
        let province = world.add_entity("Home", EntityKind::Province);
        world.set_owner(province, country).unwrap();
        world.set_capital(country, province).unwrap();
        world
            .set_administration(Administration {
                areas: ["home".into()].into(),
                regions: [("union".into(), ["home".into()].into())].into(),
                superregions: [("outer".into(), ["union".into()].into())].into(),
                continents: [("land".into(), [province].into())].into(),
                provinces: [(
                    province,
                    ProvinceAdministration {
                        area: Some("home".into()),
                        ..Default::default()
                    },
                )]
                .into(),
                ..Default::default()
            })
            .unwrap();
        world
            .set_politics(Politics {
                offices: [("imperial_emperor".into(), country)].into(),
                ..Default::default()
            })
            .unwrap();
        world.bind_global_scope("place", province).unwrap();
        world.execute(country, &compiled.plan).unwrap();
        assert_eq!(world.value(country, FieldId(0)).unwrap(), 1000);
        world.clear_global_scope("place").unwrap();
        world.execute(country, &compiled.plan).unwrap();
        assert_eq!(world.value(country, FieldId(0)).unwrap(), 2000);
        assert!(!compiled.sources.is_empty());
    }
}

#[test]
fn invalid_scopes_operands_and_false_branch_bodies_are_rejected_at_the_source() {
    let schema = Schema::new(["unused"]).unwrap();
    let library = ScriptLibrary::new();
    for (kind, condition) in [
        (EntityKind::Country, "owner={always=yes}"),
        (EntityKind::Province, "capital_scope={}"),
        (EntityKind::Country, "continent=land"),
        (
            EntityKind::Country,
            "capital_scope={owner={continent=land}}",
        ),
        (EntityKind::Country, "capital_scope={area=ROOT}"),
        (
            EntityKind::Country,
            "capital_scope={region=event_target:place}",
        ),
        (
            EntityKind::Country,
            "capital_scope={superregion=\"$GROUP$\"}",
        ),
        (EntityKind::Country, "capital_scope={continent=\"\"}"),
        (EntityKind::Country, "capital_scope={area={home}}"),
        (EntityKind::Country, "event_target:missing={}"),
        (EntityKind::Country, "event_target:={}"),
        (
            EntityKind::Country,
            "always=no event_target:place={unsupported=yes}",
        ),
        (
            EntityKind::Country,
            "always=no capital_scope={unsupported=yes}",
        ),
    ] {
        for optimise in [false, true] {
            let error = ScriptCompiler::new(&schema, &library, optimise)
                .with_global_scope_types([("place".into(), EntityKind::Province)].into())
                .compile(
                    &document(&format!("if={{limit={{{condition}}} log=unreachable}}")),
                    kind,
                )
                .unwrap_err();
            assert!(error.location().is_some(), "{condition}: {error}");
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
    let map = temp.path().join("map/default.map");
    let text = fs::read_to_string(&map).unwrap();
    fs::write(map,text+"\nregion=\"region.txt\" superregion=\"superregion.txt\" continent=\"continent.txt\"\n").unwrap();
    fs::write(
        temp.path().join("map/region.txt"),
        "north={areas={north_area}} east={areas={east_area}}",
    )
    .unwrap();
    fs::write(
        temp.path().join("map/superregion.txt"),
        "outer={north east}",
    )
    .unwrap();
    fs::write(
        temp.path().join("map/continent.txt"),
        "near={1 1} far={2} empty={} island_check_provinces={1 2}",
    )
    .unwrap();
    temp
}

#[test]
fn imported_geography_drives_startup_and_changes_content_identity() {
    let temp = fixture();
    let actions = temp.path().join("common/on_actions");
    fs::create_dir_all(&actions).unwrap();
    fs::write(actions.join("test.txt"),"on_startup={if={limit={tag=AAA capital_scope={area=east_area region=east superregion=outer continent=far owner={tag=AAA}}} log=geography_passed}}").unwrap();
    let players = BTreeSet::from(["AAA".into()]);
    let loaded = VanillaWorld::load_with_players(temp.path(), "1444.11.11", &players).unwrap();
    assert!(
        loaded.report.estates.pending_startup_effects.is_empty(),
        "{:?}",
        loaded.report.estates.startup_unresolved
    );
    assert_eq!(loaded.report.administration.continents, 3);
    assert!(
        loaded
            .report
            .administration
            .unapplied_geography_fields
            .contains_key("continent:island_check_provinces")
    );
    assert_eq!(
        loaded
            .initial
            .world()
            .events()
            .iter()
            .filter(|e| e.2 == "geography_passed")
            .count(),
        1
    );
    let saved = loaded.save().unwrap();
    let mut restored =
        VanillaWorld::load_with_players(temp.path(), "1444.11.11", &players).unwrap();
    restored.restore(&saved).unwrap();
    assert_eq!(restored.save().unwrap(), saved);
    fs::write(
        temp.path().join("map/continent.txt"),
        "near={1 2} far={} empty={}",
    )
    .unwrap();
    let mut changed = VanillaWorld::load_with_players(temp.path(), "1444.11.11", &players).unwrap();
    assert!(changed.restore(&saved).is_err());
    assert!(changed.initial.world().events().is_empty());
}

#[test]
fn malformed_continents_and_invalid_province_memberships_are_rejected() {
    for body in [
        "near={1} near={2}",
        "near={1} far={1}",
        "near={99}",
        "near={0}",
        "near={no}",
        "near={color={1 2 3} 1}",
        "near=yes",
    ] {
        let temp = fixture();
        fs::write(temp.path().join("map/continent.txt"), body).unwrap();
        assert!(
            VanillaWorld::load(temp.path(), "1444.11.11").is_err(),
            "{body}"
        );
    }
}
