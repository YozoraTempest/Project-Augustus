// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    Administration, CoreType, EntityId, EntityKind, ProvinceAdministration, Schema, World,
};
use augustus_eu4::{ScriptCompiler, ScriptLibrary, TextDocument, TextEncoding};

fn document(text: &str) -> TextDocument {
    TextDocument::from_bytes(
        "self-authored-province-cores.txt",
        text.as_bytes().to_vec(),
        TextEncoding::Utf8,
    )
}

fn fixture() -> World {
    let mut world = World::new(Schema::new(["counter"]).unwrap());
    let a = world.add_entity("AAA", EntityKind::Country);
    let b = world.add_entity("BBB", EntityKind::Country);
    world.add_entity("CCC", EntityKind::Country);
    let p = world.add_entity("1", EntityKind::Province);
    let q = world.add_entity("2", EntityKind::Province);
    world.set_owner(p, a).unwrap();
    world.set_owner(q, b).unwrap();
    world
        .set_administration(Administration {
            provinces: [
                (
                    p,
                    ProvinceAdministration {
                        cores_known: true,
                        cores: [(a, CoreType::Full)].into(),
                        ..Default::default()
                    },
                ),
                (
                    q,
                    ProvinceAdministration {
                        cores_known: true,
                        cores: [(b, CoreType::Full)].into(),
                        ..Default::default()
                    },
                ),
            ]
            .into(),
            ..Default::default()
        })
        .unwrap();
    world.bind_global_scope("holder", a).unwrap();
    world.bind_global_scope("place", q).unwrap();
    world
}

fn compile(world: &World, text: &str, scope: EntityKind, optimise: bool) -> augustus_core::Plan {
    ScriptCompiler::new(world.schema(), &ScriptLibrary::new(), optimise)
        .with_global_scope_types(
            [
                ("holder".into(), EntityKind::Country),
                ("place".into(), EntityKind::Province),
                ("missing".into(), EntityKind::Country),
            ]
            .into(),
        )
        .compile(&document(text), scope)
        .unwrap()
        .plan
}

#[test]
fn both_scope_directions_share_live_cores_and_keep_the_declared_root() {
    for optimise in [false, true] {
        let mut world = fixture();
        let plan = compile(
            &world,
            "2={add_core=ROOT if={limit={is_core=AAA NOT={is_territorial_core=AAA}} log=foreign}} if={limit={is_core=0002} log=country} remove_core=2 2={if={limit={NOT={is_core=AAA}} log=removed} add_territorial_core=AAA} if={limit={is_core=2 is_territorial_core=2} log=territorial}",
            EntityKind::Country,
            optimise,
        );
        world.execute(EntityId(0), &plan).unwrap();
        assert_eq!(
            world
                .events()
                .iter()
                .map(|(_, _, text)| text.as_str())
                .collect::<Vec<_>>(),
            ["foreign", "country", "removed", "territorial"]
        );
        assert_eq!(
            world.province_core(EntityId(4), EntityId(0)).unwrap(),
            Some(CoreType::Territorial)
        );
        assert_eq!(world.entity(EntityId(4)).unwrap().owner, Some(EntityId(1)));
        let plan = compile(
            &world,
            "AAA={remove_core=ROOT add_core=ROOT if={limit={is_core=ROOT NOT={is_territorial_core=ROOT}} log=province_root}}",
            EntityKind::Province,
            optimise,
        );
        world.execute(EntityId(4), &plan).unwrap();
        assert_eq!(world.events().last().unwrap().2, "province_root");
    }
}

#[test]
fn global_operands_resolve_live_rebindings_and_known_absence_without_owner_fallbacks() {
    let mut world = fixture();
    let plan = compile(
        &world,
        "event_target:place={add_core=event_target:holder if={limit={is_core=event_target:holder} log=matched} add_core=event_target:missing if={limit={is_core=event_target:missing} log=unexpected}}",
        EntityKind::Country,
        true,
    );
    world.execute(EntityId(0), &plan).unwrap();
    world.bind_global_scope("holder", EntityId(2)).unwrap();
    world.bind_global_scope("place", EntityId(3)).unwrap();
    world.execute(EntityId(0), &plan).unwrap();
    assert_eq!(
        world
            .events()
            .iter()
            .map(|(_, _, text)| text.as_str())
            .collect::<Vec<_>>(),
        ["matched", "matched"]
    );
    assert_eq!(
        world.province_core(EntityId(3), EntityId(2)).unwrap(),
        Some(CoreType::Full)
    );
    assert_eq!(world.province_core(EntityId(4), EntityId(2)).unwrap(), None);
}

#[test]
fn unsupported_dynamic_or_mistyped_operands_keep_source_locations() {
    let world = fixture();
    for text in [
        "add_core=AAA",
        "1={add_core=2}",
        "1={add_core=yes}",
        "remove_core=0",
        "add_core=ROOT",
        "1={add_core=FROM}",
        "1={add_core=event_target:place}",
        "add_core=event_target:holder",
        "1={add_core=event_target:undeclared}",
        "if={limit={is_core=AAA} log=bad}",
        "1={if={limit={is_territorial_core=2} log=bad}}",
        "add_core={province=1}",
    ] {
        let result = ScriptCompiler::new(world.schema(), &ScriptLibrary::new(), true)
            .with_global_scope_types(
                [
                    ("holder".into(), EntityKind::Country),
                    ("place".into(), EntityKind::Province),
                ]
                .into(),
            )
            .compile(&document(&format!("\n{text}")), EntityKind::Country);
        let error = result.expect_err(text);
        assert_eq!(error.location().unwrap().line, 2, "{text}: {error}");
        assert!(
            error
                .to_string()
                .contains("self-authored-province-cores.txt")
        );
    }
}
