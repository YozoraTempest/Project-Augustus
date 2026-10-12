// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{Action, EntityId, EntityKind, FieldId, Schema, World};
use augustus_eu4::{ScriptCompiler, ScriptLibrary, TextDocument, TextEncoding};

fn document(text: &str) -> TextDocument {
    TextDocument::from_bytes(
        "self-authored-descriptions.txt",
        text.as_bytes().to_vec(),
        TextEncoding::Utf8,
    )
}

fn descriptions(plan: &[Action], result: &mut Vec<(String, bool)>) {
    for action in plan {
        match action {
            Action::Description { key, hidden } => result.push((key.clone(), *hidden)),
            Action::If {
                then, otherwise, ..
            }
            | Action::RandomBranch { then, otherwise } => {
                descriptions(then, result);
                descriptions(otherwise, result);
            }
            Action::Preview { plan }
            | Action::Schedule { plan, .. }
            | Action::With { plan, .. }
            | Action::InScope { plan, .. } => descriptions(plan, result),
            _ => (),
        }
    }
}

#[test]
fn literal_descriptions_keep_order_duplicates_hidden_context_and_existing_execution() {
    let schema = Schema::new(["variable/phase"]).unwrap();
    let mut library = ScriptLibrary::new();
    library
        .add_effects(&document("word={custom_tooltip=macro_text}"))
        .unwrap();
    let text = document(
        r#"
        set_variable={which=phase value=1}
        custom_tooltip=first
        custom_tooltip=second
        custom_tooltip=first
        hidden_effect={word=yes if={limit={check_variable={which=phase value=1}} custom_tooltip=conditional_hidden} capital_scope={custom_tooltip=province_hidden} change_variable={which=phase value=1}}
        custom_tooltip=visible_again
        tooltip={custom_tooltip=preview hidden_effect={custom_tooltip=preview_hidden} set_variable={which=phase value=99}}
        log=actual
    "#,
    );
    for optimise in [false, true] {
        let compiled = ScriptCompiler::new(&schema, &library, optimise)
            .compile(&text, EntityKind::Country)
            .unwrap();
        let mut labels = Vec::new();
        descriptions(&compiled.plan, &mut labels);
        assert_eq!(
            labels,
            [
                "first",
                "second",
                "first",
                "macro_text",
                "conditional_hidden",
                "province_hidden",
                "visible_again",
                "preview",
                "preview_hidden"
            ]
            .into_iter()
            .zip([false, false, false, true, true, true, false, false, true])
            .map(|(k, h)| (k.into(), h))
            .collect::<Vec<_>>()
        );
        assert!(compiled.sources.iter().any(|(_, at)| at.line == 4));
        let mut state = World::new(schema.clone());
        state.add_entity("AAA", EntityKind::Country);
        let province = state.add_entity("Port", EntityKind::Province);
        state.set_owner(province, EntityId(0)).unwrap();
        state.set_capital(EntityId(0), province).unwrap();
        state.execute(EntityId(0), &compiled.plan).unwrap();
        assert_eq!(state.value(EntityId(0), FieldId(0)).unwrap(), 2000);
        assert_eq!(state.events(), &[(0, EntityId(0), "actual".into())]);
        let province_plan = ScriptCompiler::new(&schema, &library, optimise)
            .compile(
                &document("custom_tooltip=province_text"),
                EntityKind::Province,
            )
            .unwrap();
        state.execute(province, &province_plan.plan).unwrap();
    }
}

#[test]
fn unsupported_description_forms_report_sources_even_in_hidden_or_folded_branches() {
    let schema = Schema::new(["unused"]).unwrap();
    let library = ScriptLibrary::new();
    for optimise in [false, true] {
        for value in [
            "\"\"",
            "\" \"",
            "{}",
            "\"$parameter$\"",
            "\"[Root.GetName]\"",
            "key@ROOT",
        ] {
            for text in [
                format!("custom_tooltip={value}"),
                format!("hidden_effect={{\n custom_tooltip={value}\n}}"),
                format!("if={{limit={{always=no}} custom_tooltip={value}}}"),
            ] {
                let error = ScriptCompiler::new(&schema, &library, optimise)
                    .compile(&document(&text), EntityKind::Country)
                    .unwrap_err()
                    .to_string();
                assert!(error.contains("self-authored-descriptions.txt"), "{error}");
                if text.contains('\n') {
                    assert!(error.contains(":2:"), "{error}");
                }
            }
        }
    }
}
