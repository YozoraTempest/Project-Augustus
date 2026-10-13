// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{EntityKind, Schema};
use augustus_eu4::{ScriptCompiler, ScriptLibrary, TextDocument, TextEncoding, VanillaWorld};
use std::{fs, path::Path};

fn fixture() -> tempfile::TempDir {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world");
    let temp = tempfile::tempdir().unwrap();
    for entry in walkdir::WalkDir::new(&root) {
        let entry = entry.unwrap();
        let target = temp.path().join(entry.path().strip_prefix(&root).unwrap());
        if entry.file_type().is_dir() {
            fs::create_dir_all(target).unwrap();
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
    let path = temp.path().join("common/defines.lua");
    let old = fs::read_to_string(&path).unwrap();
    fs::write(path,old.replace("NCountry = {","NCountry = {\n MIN_LIBERTY_DESIRE = 0,\n MAX_LIBERTY_DESIRE = 100,\n MONTHLY_LIBERTY_DECREASE = 0.1,")).unwrap();
    temp
}

fn write(root: &Path, name: &str, text: &str) {
    let path = root.join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn quantity(loaded: &VanillaWorld, country: &str) -> Option<i64> {
    let entity = loaded.initial.country_entity(country).unwrap();
    loaded.initial.world().metrics().entities[&entity]["temporary_liberty_desire"]
}

#[test]
fn startup_writes_and_monthly_rules_restore_without_inventing_a_complete_total() {
    let temp = fixture();
    write(
        temp.path(),
        "common/on_actions/control.txt",
        "on_startup={if={limit={tag=BBB} add_liberty_desire=25} if={limit={tag=AAA} add_liberty_desire=50}} ",
    );
    let mut whole = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert_eq!(
        (quantity(&whole, "AAA"), quantity(&whole, "BBB")),
        (Some(50000), Some(25000))
    );
    assert!(whole.report.estates.startup_unresolved.is_empty());
    let bbb = whole.initial.country_entity("BBB").unwrap();
    let result = whole
        .initial
        .world()
        .attributes()
        .evaluate(whole.initial.world(), bbb, "liberty_desire")
        .unwrap();
    assert_eq!(result.value_micros, None);
    assert!(
        result
            .unresolved
            .iter()
            .any(|s| s.contains("subject_type_and_dynamic_contributions"))
    );
    whole.initial.advance(20).unwrap();
    assert_eq!(
        (quantity(&whole, "AAA"), quantity(&whole, "BBB")),
        (Some(0), Some(24900))
    );
    let saved = whole.save().unwrap();
    let mut resumed = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    resumed.restore(&saved).unwrap();
    whole.initial.advance(62).unwrap();
    resumed.initial.advance(31).unwrap();
    resumed.initial.advance(31).unwrap();
    assert_eq!(whole.save().unwrap(), resumed.save().unwrap());
    assert_eq!(quantity(&whole, "BBB"), Some(24700));
    let library = ScriptLibrary::new();
    let plan = ScriptCompiler::new(whole.initial.world().schema(), &library, true)
        .compile(
            &TextDocument::from_bytes(
                "partial.txt",
                b"if={limit={liberty_desire=0} log=known}".to_vec(),
                TextEncoding::Utf8,
            ),
            EntityKind::Country,
        )
        .unwrap();
    let mut candidate = whole.initial.clone();
    assert!(candidate.execute(bbb, &plan.plan).is_err());
}

#[test]
fn unverified_history_does_not_erase_stability_knowledge_or_invent_temporary_values() {
    for scoped in [false, true] {
        let temp = fixture();
        let path = fs::read_dir(temp.path().join("history/countries"))
            .unwrap()
            .map(|p| p.unwrap().path())
            .find(|p| p.file_name().unwrap().to_string_lossy().starts_with("AAA"))
            .unwrap();
        let old = fs::read_to_string(&path).unwrap();
        fs::write(
            path,
            format!(
                "{old}\n{}",
                if scoped {
                    "BBB={add_liberty_desire=1}"
                } else {
                    "liberty_desire=1"
                }
            ),
        )
        .unwrap();
        let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
        assert_eq!(quantity(&loaded, "AAA"), None);
        assert_eq!(
            quantity(&loaded, "BBB"),
            if scoped { None } else { Some(0) }
        );
        let aaa = loaded.initial.country_entity("AAA").unwrap();
        assert_eq!(
            loaded
                .initial
                .world()
                .metrics()
                .value(loaded.initial.world(), aaa, "stability")
                .unwrap(),
            0
        );
    }
}

#[test]
fn unknown_rates_preserve_zero_but_do_not_predict_a_nonzero_next_month() {
    let temp = fixture();
    let path = temp.path().join("common/defines.lua");
    fs::write(
        &path,
        fs::read_to_string(&path)
            .unwrap()
            .replace("MONTHLY_LIBERTY_DECREASE = 0.1,", ""),
    )
    .unwrap();
    let mut loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let bbb = loaded.initial.country_entity("BBB").unwrap();
    loaded.initial.advance(20).unwrap();
    assert_eq!(quantity(&loaded, "BBB"), Some(0));
    loaded
        .initial
        .execute(
            bbb,
            &[augustus_core::Action::UpdateNumeric {
                target: augustus_core::Target::Current,
                slot: augustus_core::NumericSlot::Metric("temporary_liberty_desire".into()),
                operation: augustus_core::NumericOperation::Set,
                value: 1,
            }],
        )
        .unwrap();
    loaded.initial.advance(31).unwrap();
    assert_eq!(quantity(&loaded, "BBB"), None);
    let saved = loaded.save().unwrap();
    loaded.restore(&saved).unwrap();
    assert_eq!(loaded.save().unwrap(), saved);
}

#[test]
fn unsupported_operands_and_scope_report_source_and_lua_rate_changes_are_not_ignored() {
    let schema = Schema::new(["unused"]).unwrap();
    let library = ScriptLibrary::new();
    for optimise in [false, true] {
        for text in [
            "add_liberty_desire=ROOT",
            "add_liberty_desire=1001",
            "add_liberty_desire=0.0001",
            "add_liberty_desire={value=1}",
            "capital_scope={add_liberty_desire=1}",
            "capital_scope={if={limit={liberty_desire=0} log=bad}}",
        ] {
            let error = ScriptCompiler::new(&schema, &library, optimise)
                .with_metric_ids(["temporary_liberty_desire".into()].into())
                .compile(
                    &TextDocument::from_bytes(
                        "unsupported-liberty.txt",
                        text.as_bytes().to_vec(),
                        TextEncoding::Utf8,
                    ),
                    EntityKind::Country,
                )
                .unwrap_err();
            assert!(error.location().is_some(), "{error}");
        }
    }
    let temp = fixture();
    write(
        temp.path(),
        "common/defines/control.lua",
        "NDefines.NCountry.MONTHLY_LIBERTY_DECREASE = 0.2\n",
    );
    let error = VanillaWorld::load(temp.path(), "1444.11.11").unwrap_err();
    assert!(error.to_string().contains("MONTHLY_LIBERTY_DECREASE"));
    assert!(error.location().is_some());
}

#[test]
fn failed_startup_marks_values_unknown_and_changed_monthly_content_rejects_a_save() {
    let temp = fixture();
    write(
        temp.path(),
        "common/on_actions/control.txt",
        "on_startup={if={limit={tag=BBB} add_liberty_desire=25 unsupported_effect=yes}}",
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert_eq!(quantity(&loaded, "BBB"), None);
    assert!(!loaded.report.estates.startup_unresolved.is_empty());
    assert!(loaded.initial.world().events().is_empty());
    let saved = loaded.save().unwrap();
    let path = temp.path().join("common/defines.lua");
    fs::write(
        &path,
        fs::read_to_string(&path).unwrap().replace(
            "MONTHLY_LIBERTY_DECREASE = 0.1,",
            "MONTHLY_LIBERTY_DECREASE = 0.2,",
        ),
    )
    .unwrap();
    let mut changed = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(changed.restore(&saved).is_err());
    assert_eq!(quantity(&changed, "BBB"), None);
}
