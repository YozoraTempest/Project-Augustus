// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{EntityKind, Schema, WorldDate};
use augustus_eu4::{ScriptCompiler, ScriptLibrary, TextDocument, TextEncoding, VanillaWorld};
use std::{fs, path::Path};

fn fixture() -> tempfile::TempDir {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world");
    let temp = tempfile::tempdir().unwrap();
    for entry in walkdir::WalkDir::new(root.clone()) {
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
    fs::write(
        path,
        old.replace(
            "NMilitary = {",
            "NDiplomacy = {\n TRUCE_YEARS = 1,\n },\n NMilitary = {",
        ),
    )
    .unwrap();
    temp
}
fn startup(root: &Path, text: &str) {
    let path = root.join("common/on_actions/control.txt");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

#[test]
fn startup_translates_to_one_graph_and_restore_retains_renewed_deadlines() {
    let temp = fixture();
    startup(
        temp.path(),
        "on_startup={if={limit={tag=AAA} add_truce_with=BBB add_truce_with=ROOT}} ",
    );
    let mut loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(
        loaded.report.estates.startup_unresolved.is_empty(),
        "{:?}",
        loaded.report.estates.startup_unresolved
    );
    assert_eq!(loaded.report.setup.truce_months, Some(12));
    let aaa = loaded.initial.country_entity("AAA").unwrap();
    let bbb = loaded.initial.country_entity("BBB").unwrap();
    assert!(loaded.initial.world().truce_with(aaa, bbb).unwrap());
    assert!(loaded.initial.world().truce_with(bbb, aaa).unwrap());
    assert!(loaded.initial.world().truce_with(aaa, aaa).unwrap());
    assert!(loaded.initial.world().truces().known_countries.is_empty());
    loaded.initial.advance(30).unwrap();
    let library = ScriptLibrary::new();
    let plan = ScriptCompiler::new(loaded.initial.world().schema(), &library, true)
        .with_truce_months(Some(12))
        .compile(
            &TextDocument::from_bytes(
                "renew.txt",
                b"BBB={add_truce_with=ROOT if={limit={truce_with=ROOT has_truce=yes} log=renewed}}"
                    .to_vec(),
                TextEncoding::Utf8,
            ),
            EntityKind::Country,
        )
        .unwrap();
    loaded.initial.execute(aaa, &plan.plan).unwrap();
    let saved = loaded.save().unwrap();
    let mut resumed = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    resumed.restore(&saved).unwrap();
    loaded.initial.advance(355).unwrap();
    resumed.initial.advance(355).unwrap();
    assert!(loaded.initial.world().truce_with(aaa, bbb).unwrap());
    assert!(!loaded.initial.world().truce_with(aaa, aaa).unwrap());
    loaded.initial.advance(31).unwrap();
    resumed.initial.advance(10).unwrap();
    resumed.initial.advance(21).unwrap();
    assert!(!loaded.initial.world().truce_with(aaa, bbb).unwrap());
    assert_eq!(loaded.save().unwrap(), resumed.save().unwrap());
    startup(
        temp.path(),
        "on_startup={if={limit={tag=AAA} add_truce_with=AAA}} ",
    );
    let mut changed = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(changed.restore(&saved).is_err());
}

#[test]
fn country_history_truce_deadline_uses_its_historical_date() {
    let temp = fixture();
    let history = temp.path().join("history/countries/AAA - Alpha.txt");
    let contents = fs::read_to_string(&history).unwrap();
    fs::write(
        history,
        format!("{contents}\n1444.10.29 = {{ add_truce_with = BBB }}\n"),
    )
    .unwrap();

    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let aaa = loaded.initial.country_entity("AAA").unwrap();
    let bbb = loaded.initial.country_entity("BBB").unwrap();
    assert!(loaded.initial.world().truce_with(aaa, bbb).unwrap());
    assert!(loaded.initial.world().truce_with(bbb, aaa).unwrap());
    assert!(
        !loaded
            .report
            .unapplied_country_fields
            .contains_key("add_truce_with")
    );

    let (first, second) = if aaa <= bbb { (aaa, bbb) } else { (bbb, aaa) };
    let record = loaded.initial.world().truces().records[&first][&second]
        .as_ref()
        .unwrap();
    assert!(record.monthly);
    assert_eq!(
        loaded
            .initial
            .world()
            .calendar()
            .unwrap()
            .date(record.expires_on.unwrap())
            .unwrap(),
        WorldDate {
            year: 1445,
            month: 10,
            day: 29,
        }
    );
}

#[test]
fn undated_country_history_truce_starts_at_the_campaign_date() {
    let temp = fixture();
    let history = temp.path().join("history/countries/AAA - Alpha.txt");
    let contents = fs::read_to_string(&history).unwrap();
    fs::write(history, format!("{contents}\nadd_truce_with = BBB\n")).unwrap();

    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let aaa = loaded.initial.country_entity("AAA").unwrap();
    let bbb = loaded.initial.country_entity("BBB").unwrap();
    assert!(loaded.initial.world().truce_with(aaa, bbb).unwrap());
    let (first, second) = if aaa <= bbb { (aaa, bbb) } else { (bbb, aaa) };
    let record = loaded.initial.world().truces().records[&first][&second]
        .as_ref()
        .unwrap();
    assert_eq!(
        loaded
            .initial
            .world()
            .calendar()
            .unwrap()
            .date(record.expires_on.unwrap())
            .unwrap(),
        WorldDate {
            year: 1445,
            month: 11,
            day: 11,
        }
    );
}

#[test]
fn expired_country_history_truce_is_known_absent() {
    let temp = fixture();
    let history = temp.path().join("history/countries/AAA - Alpha.txt");
    let contents = fs::read_to_string(&history).unwrap();
    fs::write(
        history,
        format!("{contents}\n1430.1.1 = {{ add_truce_with = BBB }}\n"),
    )
    .unwrap();

    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let aaa = loaded.initial.country_entity("AAA").unwrap();
    let bbb = loaded.initial.country_entity("BBB").unwrap();
    assert!(!loaded.initial.world().truce_with(aaa, bbb).unwrap());
    assert!(!loaded.initial.world().truce_with(bbb, aaa).unwrap());
}

#[test]
fn country_history_truce_without_duration_stays_unresolved() {
    let temp = fixture();
    let defines = temp.path().join("common/defines.lua");
    fs::write(
        &defines,
        fs::read_to_string(&defines)
            .unwrap()
            .replace(" TRUCE_YEARS = 1,\n", ""),
    )
    .unwrap();
    let history = temp.path().join("history/countries/AAA - Alpha.txt");
    let contents = fs::read_to_string(&history).unwrap();
    fs::write(history, format!("{contents}\nadd_truce_with = BBB\n")).unwrap();

    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert_eq!(
        loaded.report.unapplied_country_fields.get("add_truce_with"),
        Some(&1)
    );
    let aaa = loaded.initial.country_entity("AAA").unwrap();
    let bbb = loaded.initial.country_entity("BBB").unwrap();
    assert!(loaded.initial.world().truce_with(aaa, bbb).is_err());
}

#[test]
fn unsupported_operands_and_missing_duration_report_the_original_location() {
    let schema = Schema::new(["unused"]).unwrap();
    let library = ScriptLibrary::new();
    for optimise in [false, true] {
        for text in [
            "add_truce_with={target=BBB years=1}",
            "capital_scope={add_truce_with=BBB}",
            "if={limit={has_truce=maybe} log=bad}",
            "capital_scope={if={limit={truce_with=BBB} log=bad}}",
        ] {
            let error = ScriptCompiler::new(&schema, &library, optimise)
                .with_truce_months(Some(12))
                .compile(
                    &TextDocument::from_bytes(
                        "bad-truce.txt",
                        text.as_bytes().to_vec(),
                        TextEncoding::Utf8,
                    ),
                    EntityKind::Country,
                )
                .unwrap_err();
            assert!(error.to_string().contains("bad-truce.txt:1:"), "{error}");
        }
        let error = ScriptCompiler::new(&schema, &library, optimise)
            .compile(
                &TextDocument::from_bytes(
                    "unknown.txt",
                    b"add_truce_with=BBB".to_vec(),
                    TextEncoding::Utf8,
                ),
                EntityKind::Country,
            )
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("default truce duration is unknown")
        );
    }
}

#[test]
fn unknown_graph_aborts_startup_without_leaking_prior_grants() {
    let temp = fixture();
    startup(
        temp.path(),
        "on_startup={if={limit={tag=AAA} add_truce_with=BBB if={limit={BBB={truce_with=BBB}} log=unknown}}} ",
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(!loaded.report.estates.startup_unresolved.is_empty());
    assert!(loaded.initial.world().truces().is_empty());
    assert!(loaded.initial.world().save("post-rollback").is_ok());
}

#[test]
fn positive_integral_truce_defines_are_required_and_lua_overrides_apply() {
    for value in ["0", "-1", "1.5"] {
        let temp = fixture();
        let path = temp.path().join("common/defines.lua");
        fs::write(
            &path,
            fs::read_to_string(&path)
                .unwrap()
                .replace("TRUCE_YEARS = 1", &format!("TRUCE_YEARS = {value}")),
        )
        .unwrap();
        assert!(
            VanillaWorld::load(temp.path(), "1444.11.11").is_err(),
            "{value}"
        );
    }
    let temp = fixture();
    let path = temp.path().join("common/defines/override.lua");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, "NDefines.NDiplomacy.TRUCE_YEARS = 2").unwrap();
    startup(temp.path(), "on_startup={add_truce_with=BBB}");
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert_eq!(loaded.report.setup.truce_months, Some(24));
    let temp = fixture();
    let path = temp.path().join("common/defines.lua");
    fs::write(
        &path,
        fs::read_to_string(&path)
            .unwrap()
            .replace("TRUCE_YEARS = 1,", ""),
    )
    .unwrap();
    startup(temp.path(), "on_startup={add_truce_with=BBB}");
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert_eq!(loaded.report.setup.truce_months, None);
    assert!(!loaded.report.estates.startup_unresolved.is_empty());
}
