// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{EntityKind, Schema};
use augustus_eu4::{ScriptCompiler, ScriptLibrary, TextDocument, TextEncoding};

fn document(timing: &str) -> TextDocument {
    TextDocument::from_bytes("timing.txt",format!("country_event={{id=probe.1 {timing} is_triggered_only=yes immediate={{change_variable={{which=count value=1}}}} option={{name=done}}}}").into_bytes(),TextEncoding::Utf8)
}

#[test]
fn inactive_mean_time_declarations_compile_to_the_same_shared_event() {
    let schema = Schema::new(["variable/count"]).unwrap();
    let library = ScriptLibrary::new();
    for optimize in [false, true] {
        let baseline = ScriptCompiler::new(&schema, &library, optimize)
            .compile_events(&document(""))
            .unwrap();
        for timing in [
            "mean_time_to_happen={days=1}",
            "mean_time_to_happen={years=10000}",
            "mean_time_to_happen={months=10 modifier={factor=0 treasury=100}}",
        ] {
            let compiler =
                ScriptCompiler::new(&schema, &library, optimize).with_treasury_slot(Some(
                    augustus_core::NumericSlot::Resource(augustus_core::CountryResource::Treasury),
                ));
            let compiled = compiler.compile_events(&document(timing)).unwrap();
            assert_eq!(compiled.definitions, baseline.definitions);
            assert_eq!(compiled.conditions, baseline.conditions);
            let mut world = augustus_core::World::new(schema.clone());
            let country = world.add_entity("AAA", EntityKind::Country);
            world
                .set_event_catalog(augustus_core::EventCatalog::new(compiled.definitions).unwrap())
                .unwrap();
            world.trigger_event("probe.1", country).unwrap();
            assert_eq!(
                world.value(country, augustus_core::FieldId(0)).unwrap(),
                1000
            );
            world.advance_to(60).unwrap();
            assert_eq!(
                world.value(country, augustus_core::FieldId(0)).unwrap(),
                1000
            );
        }
    }
}

#[test]
fn ignored_declarations_still_validate_all_fields_and_references() {
    let schema = Schema::new(["variable/count"]).unwrap();
    let library = ScriptLibrary::new();
    for body in [
        "days=one",
        "days=1.5",
        "days=-1",
        "days=1 days=2",
        "days=1 years=1",
        "unknown=1",
        "days=1 modifier={always=yes}",
        "days=1 modifier={factor=1 factor=2}",
        "days=1 modifier={factor=1 unsupported_trigger=yes}",
    ] {
        let timing = format!("mean_time_to_happen={{{body}}}");
        let error = ScriptCompiler::new(&schema, &library, true)
            .compile_events(&document(&timing))
            .unwrap_err();
        assert!(error.to_string().contains("timing.txt:1:"));
    }
    let automatic=TextDocument::from_bytes("automatic.txt",b"country_event={id=probe.1 mean_time_to_happen={days=1} is_triggered_only=no option={name=done}}".to_vec(),TextEncoding::Utf8);
    assert!(
        ScriptCompiler::new(&schema, &library, true)
            .compile_events(&automatic)
            .unwrap_err()
            .to_string()
            .contains("automatic event dispatch is not supported")
    );
}
