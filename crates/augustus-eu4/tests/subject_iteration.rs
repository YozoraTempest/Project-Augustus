// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{Action, Condition, EntityKind, Schema, Target};
use augustus_eu4::{ScriptCompiler, ScriptLibrary, TextDocument, TextEncoding};

fn document(text: &str) -> TextDocument {
    TextDocument::from_bytes("subjects.txt", text.as_bytes().to_vec(), TextEncoding::Utf8)
}

#[test]
fn subject_effects_compile_to_the_shared_operation_and_validate_before_optimization() {
    let schema = Schema::new(["unused"]).unwrap();
    let library = ScriptLibrary::new();
    for optimize in [false, true] {
        let compiler = || {
            ScriptCompiler::new(&schema, &library, optimize)
                .with_subject_type_ids(["vassal".into()].into())
        };
        let compiled = compiler()
            .compile(
                &document("every_subject_country={log=visited limit={is_subject_of_type=vassal}}"),
                EntityKind::Country,
            )
            .unwrap();
        assert_eq!(
            compiled.plan,
            vec![Action::ForEachSubjectCountry {
                country: Target::Current,
                condition: Condition::All(vec![Condition::SubjectType {
                    target: Target::Current,
                    subject_type: "vassal".into()
                }]),
                plan: vec![Action::Emit("visited".into())]
            }]
        );
        for text in [
            "every_subject_country=yes",
            "every_subject_country={limit={always=yes} limit={always=no}}",
            "every_subject_country={limit={always=no is_subject_of_type=missing}}",
            "every_subject_country={limit={always=no} unsupported_effect=yes}",
        ] {
            let error = compiler()
                .compile(&document(text), EntityKind::Country)
                .unwrap_err();
            assert!(error.to_string().contains("subjects.txt:1:"));
        }
        assert!(
            compiler()
                .compile(&document("every_subject_country={}"), EntityKind::Province)
                .is_err()
        );
    }
}
