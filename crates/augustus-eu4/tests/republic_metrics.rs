// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{EntityId, EntityKind, Governments, Schema, World};
use augustus_eu4::{ScriptCompiler, ScriptLibrary, TextDocument, TextEncoding};

fn document(text: &str) -> TextDocument {
    TextDocument::from_bytes(
        "self-authored-institutions.txt",
        text.as_bytes().to_vec(),
        TextEncoding::Utf8,
    )
}

fn world(schema: Schema, metric: &str) -> World {
    let mut world = World::new(schema);
    world.add_entity("Union", EntityKind::Country);
    world.add_entity("Port", EntityKind::Province);
    world.set_owner(EntityId(1), EntityId(0)).unwrap();
    world.set_capital(EntityId(0), EntityId(1)).unwrap();
    let scene: serde_json::Value = serde_json::from_str(include_str!(
        "../../../examples/government-transitions/scene.json"
    ))
    .unwrap();
    let mut governments: Governments =
        serde_json::from_value(scene["governments"].clone()).unwrap();
    let state = governments.countries.get_mut(&EntityId(0)).unwrap();
    state.metrics = Some([(metric.into(), Some(100_000))].into());
    state.inactive_metrics = if metric == "republican_tradition" {
        [("devotion".into(), Some(76_000))].into()
    } else {
        [("republican_tradition".into(), Some(76_000))].into()
    };
    world.set_governments(governments).unwrap();
    world
}

fn compile(schema: &Schema, text: &str, optimise: bool) -> Vec<augustus_core::Action> {
    ScriptCompiler::new(schema, &ScriptLibrary::new(), optimise)
        .with_government_metric_ids(
            ["legitimacy", "republican_tradition", "devotion"]
                .map(str::to_owned)
                .into(),
        )
        .compile(&document(text), EntityKind::Country)
        .unwrap()
        .plan
}

#[test]
fn active_institution_metrics_use_fractional_bounds_and_owner_scope() {
    let schema = Schema::new(["unused"]).unwrap();
    for metric in ["republican_tradition", "devotion"] {
        let mut saved = Vec::new();
        for optimise in [false, true] {
            let mut world = world(schema.clone(), metric);
            let script = format!(
                "add_{metric}=-24.875 if={{limit={{{metric}=75.125 NOT={{{metric}=75.126}}}} log=fraction}} add_{metric}=1000000 add_{metric}=-1000000 add_{metric}=10 add_{metric}=0.125 add_{metric}=0.125 add_{metric}=0.125 capital_scope={{owner={{add_{metric}=0.875}}}} if={{limit={{{metric}=11.25 NOT={{{metric}=11.251}}}} log=owner}}"
            );
            world
                .execute(EntityId(0), &compile(&schema, &script, optimise))
                .unwrap();
            assert_eq!(
                world.government_metric_value(EntityId(0), metric).unwrap(),
                11_250
            );
            assert_eq!(
                world
                    .events()
                    .iter()
                    .map(|row| row.2.as_str())
                    .collect::<Vec<_>>(),
                ["fraction", "owner"]
            );
            saved.push(world.save("institutions").unwrap());
        }
        assert_eq!(saved[0], saved[1]);
    }
}

#[test]
fn inactive_values_are_readable_and_are_not_changed_by_active_metric_effects() {
    let schema = Schema::new(["unused"]).unwrap();
    for metric in ["republican_tradition", "devotion"] {
        let inactive = if metric == "republican_tradition" {
            "devotion"
        } else {
            "republican_tradition"
        };
        for optimise in [false, true] {
            let mut world = world(schema.clone(), metric);
            world.execute(EntityId(0), &compile(&schema, &format!("add_{inactive}=-1000000 add_{inactive}=7.125 if={{limit={{{inactive}=76 NOT={{{inactive}=76.001}}}} log=retained}}"), optimise)).unwrap();
            assert!(
                !world
                    .government_metric_active(EntityId(0), inactive)
                    .unwrap()
            );
            assert_eq!(
                world
                    .government_metric_stored_value(EntityId(0), inactive)
                    .unwrap(),
                76_000
            );
            assert_eq!(world.events().last().unwrap().2, "retained");
        }
    }
}

#[test]
fn unknown_values_and_unsupported_metric_parameters_keep_their_boundaries() {
    let schema = Schema::new(["unused"]).unwrap();
    for metric in ["republican_tradition", "devotion"] {
        for invalid in [
            format!("add_{metric}={{value=10}}"),
            format!("add_{metric}=ROOT"),
            format!("add_{metric}=0.0001"),
            format!("capital_scope={{add_{metric}=10}}"),
        ] {
            let error = ScriptCompiler::new(&schema, &ScriptLibrary::new(), true)
                .with_government_metric_ids([metric.into()].into())
                .compile(&document(&invalid), EntityKind::Country)
                .unwrap_err();
            assert!(error.location().is_some());
            assert!(error.to_string().contains("self-authored-institutions.txt"));
        }
        let mut world = world(schema.clone(), metric);
        let mut state = world.governments().clone();
        state
            .countries
            .get_mut(&EntityId(0))
            .unwrap()
            .metrics
            .as_mut()
            .unwrap()
            .insert(metric.into(), None);
        world.set_governments(state).unwrap();
        assert!(
            world
                .execute(
                    EntityId(0),
                    &compile(&schema, &format!("add_{metric}=10"), true)
                )
                .is_err()
        );
        assert_eq!(
            world.governments().countries[&EntityId(0)]
                .metrics
                .as_ref()
                .unwrap()[metric],
            None
        );
    }
}
