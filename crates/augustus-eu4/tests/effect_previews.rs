// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{Action, EntityId, EntityKind, Schema, World};
use augustus_eu4::{ScriptCompiler, ScriptLibrary, TextDocument, TextEncoding};

fn document(text: &str) -> TextDocument {
    TextDocument::from_bytes(
        "self-authored-previews.txt",
        text.as_bytes().to_vec(),
        TextEncoding::Utf8,
    )
}

#[test]
fn preview_retains_nested_plans_and_sources_without_executing_them() {
    let schema = Schema::new(["variable/phase"]).unwrap();
    let mut library = ScriptLibrary::new();
    library
        .add_effects(&document(
            "preview_step={tooltip={change_variable={which=phase value=9}}}",
        ))
        .unwrap();
    let text = document(
        r#"
        set_variable={which=phase value=1}
        tooltip={
            preview_step=yes
            if={limit={is_variable_equal={which=phase value=1}}
                log=unexpected set_variable={which=phase value=8}}
            capital_scope={owner={change_variable={which=phase value=7}}}
            hidden_effect={change_variable={which=phase value=6}}
        }
        hidden_effect={change_variable={which=phase value=1}}
        tooltip={}
        log=actual
    "#,
    );
    for optimise in [false, true] {
        let compiled = ScriptCompiler::new(&schema, &library, optimise)
            .compile(&text, EntityKind::Country)
            .unwrap();
        assert!(
            matches!(&compiled.plan[1], Action::Preview { plan } if plan.len()==4 && matches!(&plan[0], Action::Preview { .. }))
        );
        assert!(matches!(&compiled.plan[3], Action::Preview { plan } if plan.is_empty()));
        assert!(compiled.sources.iter().any(|(_, source)| source.line == 5));
        let mut state = World::new(schema.clone());
        state.add_entity("AAA", EntityKind::Country);
        state.execute(EntityId(0), &compiled.plan).unwrap();
        assert_eq!(
            state.value(EntityId(0), augustus_core::FieldId(0)).unwrap(),
            2000
        );
        assert_eq!(state.events(), &[(0, EntityId(0), "actual".into())]);
    }
}

#[test]
fn preview_bodies_still_reject_unsupported_grammar_scope_and_recursive_macros() {
    let schema = Schema::new(["unused"]).unwrap();
    let mut library = ScriptLibrary::new();
    library
        .add_effects(&document("recursive={tooltip={recursive=yes}}"))
        .unwrap();
    for optimise in [false, true] {
        for text in [
            "tooltip=yes",
            "tooltip={\n unsupported_effect=yes\n}",
            "tooltip={if={limit={always=no} unsupported_effect=yes}}",
            "tooltip={capital_scope={set_country_flag=wrong_scope}}",
            "tooltip={else={}}",
            "tooltip={recursive=yes}",
        ] {
            let error = ScriptCompiler::new(&schema, &library, optimise)
                .compile(&document(text), EntityKind::Country)
                .unwrap_err()
                .to_string();
            assert!(error.contains("self-authored-previews.txt"), "{error}");
            if text.contains('\n') {
                assert!(error.contains(":2:"), "{error}");
            }
        }
    }
}
