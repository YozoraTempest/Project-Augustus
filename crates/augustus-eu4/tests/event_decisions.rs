// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    CountryController, EmptyWeightChoice, EntityKind, EventCatalog, EventWeight, Schema,
    WeightedEventDecisions, World,
};
use augustus_eu4::{ScriptCompiler, ScriptLibrary, TextDocument, TextEncoding};
use std::sync::Arc;

fn document(text: &str) -> TextDocument {
    TextDocument::from_bytes(
        "self-authored-weights.txt",
        text.as_bytes().to_vec(),
        TextEncoding::Utf8,
    )
}

#[test]
fn integer_weight_metadata_is_separate_from_effects() {
    let schema = Schema::new(["unused"]).unwrap();
    let library = ScriptLibrary::new();
    for optimise in [false, true] {
        let compiled = ScriptCompiler::new(&schema, &library, optimise).compile_events(&document(
            "country_event={id=choice is_triggered_only=yes option={name=A ai_chance={factor=2 modifier={factor=3 always=yes} modifier={factor=2 always=no}} log=A} option={name=B ai_chance={}} option={name=C}}"
        )).unwrap();
        assert_eq!(compiled.definitions[0].options[0].effects.len(), 1);
        let weights = &compiled.option_weights["choice"];
        assert_eq!(weights["0"].factor, 2000);
        assert_eq!(weights["0"].modifiers[0].factor, 3000);
        assert_eq!(weights["1"], EventWeight::default());
        assert!(!weights.contains_key("2"));
        let mut world = World::new(schema.clone());
        let country = world.add_entity("Union", EntityKind::Country);
        assert_eq!(weights["0"].evaluate(&world, country).unwrap(), 6000);
    }
}

#[test]
fn translated_weights_filter_options_and_execute_the_shared_command_pipeline() {
    let schema = Schema::new(["unused"]).unwrap();
    let library = ScriptLibrary::new();
    let compiled = ScriptCompiler::new(&schema, &library, true).compile_events(&document(
        "country_event={id=choice is_triggered_only=yes option={name=A trigger={always=no} ai_chance={factor=100} log=A} option={name=B ai_chance={factor=0} log=B} option={name=C ai_chance={factor=1 modifier={factor=2 always=yes}} log=C} after={log=after}}"
    )).unwrap();
    let mut world = World::new(schema);
    let country = world.add_entity("Union", EntityKind::Country);
    world
        .set_country_controller(country, CountryController::Ai)
        .unwrap();
    let catalog = EventCatalog::new(compiled.definitions)
        .unwrap()
        .with_option_weights(compiled.option_weights)
        .unwrap()
        .with_option_conditions(compiled.option_conditions)
        .unwrap()
        .with_decision_system(Arc::new(WeightedEventDecisions {
            empty: EmptyWeightChoice::FirstDeclared,
        }))
        .unwrap();
    world.set_event_catalog(catalog).unwrap();
    world.trigger_event("choice", country).unwrap();
    assert_eq!(world.queued_event_choices()[0].option, "2");
    world.process_event_commands().unwrap();
    assert_eq!(
        world
            .events()
            .iter()
            .map(|row| row.2.as_str())
            .collect::<Vec<_>>(),
        ["C", "after"]
    );
}

#[test]
fn malformed_or_unknown_weight_fields_are_rejected_even_inside_false_conditions() {
    let schema = Schema::new(["unused"]).unwrap();
    let library = ScriptLibrary::new();
    for invalid in [
        "ai_chance=1",
        "ai_chance={factor=1 factor=2}",
        "ai_chance={} ai_chance={}",
        "ai_chance={factor=10001}",
        "ai_chance={factor=0.0001}",
        "ai_chance={factor=0.001}",
        "ai_chance={factor=0.010}",
        "ai_chance={modifier={factor=0.5 always=no}}",
        "ai_chance={factor=10000}",
        "ai_chance={modifier={factor=1000 always=yes}}",
        "ai_chance={factor=variable}",
        "ai_chance={modifier={always=no}}",
        "ai_chance={modifier={factor=1 factor=2 always=no}}",
        "ai_chance={modifier={factor=0 always=no mystery=yes}}",
        "ai_chance={weight=1}",
    ] {
        let input = format!(
            "country_event={{id=choice is_triggered_only=yes option={{name=A {invalid}}}}}"
        );
        let error = ScriptCompiler::new(&schema, &library, true)
            .compile_events(&document(&input))
            .unwrap_err();
        assert!(error.location().is_some(), "{invalid}");
        assert!(error.to_string().contains("self-authored-weights.txt"));
    }
}
