// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::EntityKind;
use augustus_eu4::{
    ContentSet, ScriptCompiler, ScriptLibrary, TextDocument, TextEncoding, VanillaWorld,
};
use std::{fs, path::Path};

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
    temp
}

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn document(text: &str) -> TextDocument {
    TextDocument::from_bytes(
        "self-authored-mechanic.txt",
        text.as_bytes().to_vec(),
        TextEncoding::Utf8,
    )
}

fn query(trigger: &str, message: &str) -> String {
    format!("if={{limit={{{trigger}}} log={message}}}")
}

fn evaluate(loaded: &mut VanillaWorld, script: &str, optimise: bool) -> Result<(), String> {
    let library = ScriptLibrary::new();
    let plan = ScriptCompiler::new(loaded.initial.world().schema(), &library, optimise)
        .compile(&document(script), EntityKind::Country)
        .map_err(|e| e.to_string())?;
    let alpha = loaded.initial.country_entity("AAA").unwrap();
    loaded
        .initial
        .execute(alpha, &plan.plan)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[test]
fn attached_mechanics_follow_reform_union_availability_and_save_restore() {
    let temp = fixture();
    write(
        temp.path(),
        "common/government_mechanics/mechanics.txt",
        "assembly={available={always=yes}} dormant={available={always=no}} council={}",
    );
    write(
        temp.path(),
        "common/government_reforms/mechanics.txt",
        "first={government_abilities={assembly dormant}} second={government_abilities={assembly council}} gated={conditional={allow={always=yes} government_abilities={council}} conditional={allow={always=no} government_abilities={dormant}}}",
    );
    let script = r#"
        if={limit={NOT={has_government_mechanic=assembly}} log=before}
        add_government_reform=first
        if={limit={has_government_mechanic=assembly NOT={has_government_mechanic=dormant} NOT={has_government_mechanic=council}} log=first}
        capital_scope={owner={if={limit={has_government_mechanic=assembly} log=owner}}}
        add_government_reform=second
        if={limit={has_government_mechanic=assembly has_government_mechanic=council} log=both}
        remove_government_reform=first
        if={limit={has_government_mechanic=assembly has_government_mechanic=council} log=second}
        remove_government_reform=second
        if={limit={NOT={has_government_mechanic=assembly} NOT={has_government_mechanic=council}} log=removed}
        add_government_reform=gated
        if={limit={has_government_mechanic=council NOT={has_government_mechanic=dormant}} log=gated}
    "#;
    let mut saves = Vec::new();
    for optimise in [false, true] {
        let mut loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
        assert_eq!(loaded.report.governments.mechanic_definitions, 3);
        evaluate(&mut loaded, script, optimise).unwrap();
        assert_eq!(
            loaded
                .initial
                .world()
                .events()
                .iter()
                .map(|(_, _, text)| text.as_str())
                .collect::<Vec<_>>(),
            [
                "before", "first", "owner", "both", "second", "removed", "gated"
            ]
        );
        let snapshot = loaded.save().unwrap();
        let mut restored = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
        restored.restore(&snapshot).unwrap();
        assert_eq!(restored.initial.world(), loaded.initial.world());
        evaluate(
            &mut restored,
            "remove_government_reform=gated if={limit={NOT={has_government_mechanic=council}} log=restored_removal}",
            optimise,
        )
        .unwrap();
        assert_eq!(
            restored.initial.world().events().last().unwrap().2,
            "restored_removal"
        );
        saves.push(snapshot);
    }
    assert_eq!(saves[0], saves[1]);
}

#[test]
fn initial_estates_and_parameterized_ideas_use_the_same_dlc_filtered_queries() {
    let temp = fixture();
    write(
        temp.path(),
        "common/government_mechanics/mechanics.txt",
        "assembly={available={has_dlc=\"Domination\"}} dormant={available={NOT={has_dlc=\"Domination\"}}}",
    );
    write(
        temp.path(),
        "common/government_reforms/test_governments.txt",
        "test_court={monarchy=yes government_abilities={assembly dormant} custom_attributes={assembly=no}} test_council={republic=yes} test_charter={}",
    );
    write(
        temp.path(),
        "common/scripted_triggers/mechanic.txt",
        "is_revolutionary_republic_trigger={has_government_mechanic=$MECHANIC$}",
    );
    write(
        temp.path(),
        "common/estates/mechanic.txt",
        "assembly_estate={trigger={is_revolutionary_republic_trigger={MECHANIC=assembly} NOT={has_government_mechanic=dormant}}}",
    );
    write(
        temp.path(),
        "common/ideas/test_ideas.txt",
        "test_alpha={free=yes trigger={has_government_mechanic=assembly} start={test_identity=0.25} first={test_a=0.1} bonus={test_b=0.1}} test_common={free=yes start={test_common=0.1} first={test_a=0.1} bonus={test_b=0.1}}",
    );
    let mut loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let alpha = loaded.initial.country_entity("AAA").unwrap();
    assert_eq!(
        loaded.initial.world().estates().countries[&alpha]["assembly_estate"].active,
        Some(true)
    );
    assert!(
        loaded.initial.world().ideas().countries[&alpha]
            .groups
            .as_ref()
            .unwrap()
            .contains_key("test_alpha")
    );
    evaluate(
        &mut loaded,
        "if={limit={has_government_mechanic=assembly NOT={has_government_attribute=assembly}} log=distinct}",
        true,
    )
    .unwrap();
    assert_eq!(
        loaded.initial.world().events().last().unwrap().2,
        "distinct"
    );
}

#[test]
fn uncertain_definitions_cannot_turn_negation_into_a_successful_query() {
    for (definition, extra, referenced) in [
        (
            "assembly={available={has_country_flag=gate}}",
            "",
            "assembly",
        ),
        (
            "assembly={available={always=yes}}",
            "assembly={available={always=no}}",
            "assembly",
        ),
        (
            "assembly={available={always=yes} available={always=no}}",
            "",
            "assembly",
        ),
        ("assembly={}", "", "missing"),
    ] {
        let temp = fixture();
        write(
            temp.path(),
            "common/government_mechanics/first.txt",
            definition,
        );
        if !extra.is_empty() {
            write(temp.path(), "common/government_mechanics/second.txt", extra);
        }
        write(
            temp.path(),
            "common/government_reforms/mechanics.txt",
            &format!("uncertain={{government_abilities={{{referenced}}}}}"),
        );
        let mut loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
        let trigger = format!("NOT={{has_government_mechanic={referenced}}}");
        evaluate(
            &mut loaded,
            &(query(&trigger, "unselected") + " add_government_reform=uncertain"),
            true,
        )
        .unwrap();
        let saved = loaded.save().unwrap();
        let mut restored = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
        restored.restore(&saved).unwrap();
        assert_eq!(loaded.initial.world(), restored.initial.world());
        let failure = evaluate(&mut loaded, &query(&trigger, "wrong"), true).unwrap_err();
        assert!(failure.contains("unknown government feature"), "{failure}");
        assert_eq!(loaded.initial.world().events().len(), 1);
        assert!(loaded.save().is_err());
        evaluate(&mut restored, "remove_government_reform=uncertain", true).unwrap();
        evaluate(&mut restored, &query(&trigger, "removed"), true).unwrap();
    }
    let temp = fixture();
    write(
        temp.path(),
        "common/government_mechanics/mechanics.txt",
        "assembly={} dormant={available={always=no}}",
    );
    write(
        temp.path(),
        "common/government_reforms/mechanics.txt",
        "uncertain={conditional={allow={has_country_flag=gate} government_abilities={assembly dormant}}}",
    );
    let mut loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    evaluate(&mut loaded, "add_government_reform=uncertain if={limit={NOT={has_government_mechanic=dormant}} log=unavailable}", true).unwrap();
    assert!(
        evaluate(
            &mut loaded,
            "if={limit={has_government_mechanic=assembly} log=unknown}",
            true
        )
        .unwrap_err()
        .contains("unknown government feature")
    );
}

#[test]
fn effective_mod_mechanic_files_override_base_availability() {
    let temp = fixture();
    write(
        temp.path(),
        "common/government_mechanics/availability.txt",
        "assembly={available={always=no}}",
    );
    write(
        temp.path(),
        "common/government_reforms/mechanics.txt",
        "assembly_reform={government_abilities={assembly}}",
    );
    let pack = tempfile::tempdir().unwrap();
    write(pack.path(), "descriptor.mod", "name=MechanicPatch");
    write(
        pack.path(),
        "common/government_mechanics/availability.txt",
        "assembly={available={always=yes}}",
    );
    let content = ContentSet::from_installation(
        temp.path(),
        &[pack.path().to_path_buf()],
        None,
        None,
        TextEncoding::Utf8,
    )
    .unwrap();
    let mut loaded = VanillaWorld::load_from_content(&content, "1444.11.11").unwrap();
    assert_eq!(loaded.report.governments.mechanic_definitions, 1);
    assert_eq!(
        loaded.report.governments.definition_sources["mechanic:assembly"].path,
        fs::canonicalize(
            pack.path()
                .join("common/government_mechanics/availability.txt")
        )
        .unwrap()
    );
    evaluate(&mut loaded, "add_government_reform=assembly_reform if={limit={has_government_mechanic=assembly} log=overridden}", true).unwrap();
    assert_eq!(
        loaded.initial.world().events().last().unwrap().2,
        "overridden"
    );
}

#[test]
fn malformed_operands_and_wrong_scope_fail_before_execution() {
    let loaded = VanillaWorld::load(fixture().path(), "1444.11.11").unwrap();
    let library = ScriptLibrary::new();
    for optimise in [false, true] {
        for operand in ["{}", "0", "\"\"", "event_target:mechanic", "$MECHANIC$"] {
            let inner = query(&format!("has_government_mechanic={operand}"), "bad");
            let script = format!("if={{limit={{always=no}}\n{inner}}}");
            let failure = ScriptCompiler::new(loaded.initial.world().schema(), &library, optimise)
                .compile(&document(&script), EntityKind::Country)
                .unwrap_err()
                .to_string();
            assert!(
                failure.contains("self-authored-mechanic.txt:2:"),
                "{failure}"
            );
        }
        let failure = ScriptCompiler::new(loaded.initial.world().schema(), &library, optimise)
            .compile(
                &document("if={limit={has_government_mechanic=assembly} log=bad}"),
                EntityKind::Province,
            )
            .unwrap_err()
            .to_string();
        assert!(failure.contains("country scope"), "{failure}");
    }
    let temp = fixture();
    write(
        temp.path(),
        "common/government_reforms/malformed.txt",
        "bad={\ngovernment_abilities=assembly\n}",
    );
    let failure = VanillaWorld::load(temp.path(), "1444.11.11")
        .err()
        .unwrap()
        .to_string();
    assert!(failure.contains("malformed.txt:2:"), "{failure}");
    assert!(
        failure.contains("expected government mechanic list"),
        "{failure}"
    );
}
