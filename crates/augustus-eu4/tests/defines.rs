// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_eu4::{
    ContentSet, DefineDifficulty, DefineValue, Defines, TextEncoding, VanillaWorld,
};
use std::{fs, path::Path};

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn content(root: &Path, mods: &[std::path::PathBuf]) -> ContentSet {
    ContentSet::from_installation(root, mods, None, None, TextEncoding::Utf8).unwrap()
}

#[test]
fn expressions_functions_and_shared_globals_execute_in_effective_file_order() {
    let temp = tempfile::tempdir().unwrap();
    write(
        temp.path(),
        "common/defines.lua",
        "NDefines={NTest={VALUE=1, NOTE='two -- words', DUP=1, DUP=2}}\n factor=3",
    );
    write(
        temp.path(),
        "common/defines/20_later.lua",
        "NDefines.NTest.VALUE = scale(NDefines.NTest.VALUE) + factor\nNDefines.NTest.DUP=nil",
    );
    write(
        temp.path(),
        "common/defines/10_first.lua",
        "function scale(x) return x*2 end\n local sum=0 for i=1,4 do sum=sum+i end\nNDefines.NTest.VALUE=sum",
    );
    write(
        temp.path(),
        "common/defines/nested/ignored.lua",
        "error('must not execute nested files')",
    );
    let definitions =
        Defines::from_content(&content(temp.path(), &[]), DefineDifficulty::Normal).unwrap();
    assert_eq!(
        definitions.values["NDefines.NTest.VALUE"].value,
        DefineValue::Number(23.0)
    );
    assert_eq!(
        definitions.values["NDefines.NTest.NOTE"].value,
        DefineValue::String("two -- words".into())
    );
    assert!(!definitions.values.contains_key("NDefines.NTest.DUP"));
    assert_eq!(
        definitions
            .executed_files
            .iter()
            .map(|source| source.path.file_name().unwrap().to_str().unwrap())
            .collect::<Vec<_>>(),
        ["defines.lua", "10_first.lua", "20_later.lua"]
    );
    assert!(
        definitions.values["NDefines.NTest.VALUE"]
            .source
            .path
            .ends_with("20_later.lua")
    );
    assert_eq!(definitions.values["NDefines.NTest.NOTE"].source.line, 1);
}

#[test]
fn only_selected_difficulty_executes_after_ordinary_overrides() {
    let temp = tempfile::tempdir().unwrap();
    write(
        temp.path(),
        "common/defines.lua",
        "NDefines={NTest={VALUE=1}}",
    );
    write(
        temp.path(),
        "common/defines/zz_override.lua",
        "NDefines.NTest.VALUE=2",
    );
    write(
        temp.path(),
        "common/defines/difficulty_hard.lua",
        "NDefines.NTest.VALUE=NDefines.NTest.VALUE*3",
    );
    write(
        temp.path(),
        "common/defines/difficulty_normal.lua",
        "NDefines.NTest.VALUE=NDefines.NTest.VALUE+3",
    );
    write(
        temp.path(),
        "common/defines/difficulty_very_hard.lua",
        "this is invalid Lua",
    );
    let content = content(temp.path(), &[]);
    for (difficulty, expected, last) in [
        (DefineDifficulty::Normal, 5.0, "difficulty_normal.lua"),
        (DefineDifficulty::Hard, 6.0, "difficulty_hard.lua"),
        (DefineDifficulty::Easy, 2.0, "zz_override.lua"),
    ] {
        let definitions = Defines::from_content(&content, difficulty).unwrap();
        assert_eq!(
            definitions.values["NDefines.NTest.VALUE"].value,
            DefineValue::Number(expected)
        );
        assert!(
            definitions
                .executed_files
                .last()
                .unwrap()
                .path
                .ends_with(last)
        );
    }
    let error = Defines::from_content(&content, DefineDifficulty::VeryHard).unwrap_err();
    assert!(error.to_string().contains("difficulty_very_hard.lua"));
    assert!(error.location().is_some());
}

#[test]
fn resolved_mod_overrides_use_the_shared_virtual_content_sources() {
    let temp = tempfile::tempdir().unwrap();
    let game = temp.path().join("game");
    let pack = temp.path().join("pack");
    write(&game, "common/defines.lua", "NDefines={NTest={VALUE=1}}");
    write(
        &game,
        "common/defines/10_patch.lua",
        "NDefines.NTest.VALUE=99",
    );
    write(&pack, "descriptor.mod", "name=Patch");
    write(
        &pack,
        "common/defines/10_patch.lua",
        "NDefines.NTest.VALUE=5",
    );
    write(
        &pack,
        "common/defines/20_append.lua",
        "NDefines.NTest.VALUE=NDefines.NTest.VALUE+2",
    );
    let definitions = Defines::from_content(
        &content(&game, std::slice::from_ref(&pack)),
        DefineDifficulty::Normal,
    )
    .unwrap();
    assert_eq!(
        definitions.values["NDefines.NTest.VALUE"].value,
        DefineValue::Number(7.0)
    );
    assert_eq!(definitions.executed_files.len(), 3);
    assert!(
        definitions.executed_files[1]
            .path
            .starts_with(fs::canonicalize(pack).unwrap())
    );
}

#[test]
fn malformed_and_unbounded_sources_fail_with_the_effective_source() {
    for (source, detail) in [
        (
            "NDefines={NTest={VALUE=1}}\nerror('deliberate failure')",
            "deliberate failure",
        ),
        (
            "NDefines={NTest={VALUE=1}}\nwhile true do end",
            "instruction limit",
        ),
        ("NDefines={NTest={VALUE=1/0}}", "non-finite"),
        (
            "NDefines={NTest={VALUE=string.rep('x',100000000)}}",
            "memory",
        ),
        ("NDefines={NTest={VALUE=os.time()}}", "os"),
        ("NDefines={NTest={VALUE=io.open('file')}}", "io"),
        ("NDefines={NTest={VALUE=math.random()}}", "random"),
        (
            "NDefines={NTest={VALUE=loadstring('return 3')()}}",
            "loadstring",
        ),
    ] {
        let temp = tempfile::tempdir().unwrap();
        write(temp.path(), "common/defines.lua", source);
        let error = Defines::from_content(&content(temp.path(), &[]), DefineDifficulty::Normal)
            .unwrap_err();
        assert!(error.to_string().contains(detail), "{error}");
        assert!(error.to_string().contains("defines.lua"), "{error}");
        assert!(error.location().is_some());
    }
}

#[test]
fn source_tracking_preserves_field_locations_and_marks_dynamic_chunks() {
    let temp = tempfile::tempdir().unwrap();
    write(
        temp.path(),
        "common/defines.lua",
        "-- comment\nNDefines={\n NTest={\n  VALUE=1,\n  NOTE='VALUE=4',\n },\n}\n",
    );
    let first =
        Defines::from_content(&content(temp.path(), &[]), DefineDifficulty::Normal).unwrap();
    assert_eq!(first.values["NDefines.NTest.VALUE"].source.line, 4);
    write(
        temp.path(),
        "common/defines/patch.lua",
        "-- same numeric value, new source\nNDefines.NTest.VALUE=1\n",
    );
    let second =
        Defines::from_content(&content(temp.path(), &[]), DefineDifficulty::Normal).unwrap();
    assert_eq!(second.values["NDefines.NTest.VALUE"].source.line, 2);
    assert!(
        second.values["NDefines.NTest.VALUE"]
            .source
            .path
            .ends_with("patch.lua")
    );
    write(
        temp.path(),
        "common/defines/patch.lua",
        "-- dynamic write\nfor i=1,2 do NDefines.NTest.VALUE=i end\n",
    );
    let third =
        Defines::from_content(&content(temp.path(), &[]), DefineDifficulty::Normal).unwrap();
    assert_eq!(third.values["NDefines.NTest.VALUE"].source.line, 1);
    assert_eq!(
        third.values["NDefines.NTest.VALUE"].value,
        DefineValue::Number(2.0)
    );
    assert_eq!(third.values["NDefines.NTest.NOTE"].source.line, 5);
    write(
        temp.path(),
        "common/defines.lua",
        "NDefines={NTest={VALUE=0}}",
    );
    write(
        temp.path(),
        "common/defines/patch.lua",
        "if true then NDefines.NTest.VALUE=-0 end",
    );
    let negative =
        Defines::from_content(&content(temp.path(), &[]), DefineDifficulty::Normal).unwrap();
    let DefineValue::Number(value) = negative.values["NDefines.NTest.VALUE"].value else {
        panic!("expected number")
    };
    assert_eq!(value.to_bits(), (-0.0_f64).to_bits());
}

fn world_fixture() -> tempfile::TempDir {
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

#[test]
fn evaluated_world_rules_share_results_restore_and_reject_changed_effective_content() {
    let temp = world_fixture();
    let base = temp.path().join("common/defines.lua");
    let source = fs::read_to_string(&base).unwrap();
    fs::write(&base, source.replace("0.25,", "(0.1 + 0.2) - 0.05,")).unwrap();
    write(
        temp.path(),
        "common/defines/first.lua",
        "NDefines.NMilitary.BASE_MP_TO_MANPOWER=NDefines.NMilitary.BASE_MP_TO_MANPOWER*2\nNDefines.NCountry.BASE_POWER_INCREASE=math.floor(8/2)\nNDefines.NCountry.DEBT_LIMIT_DIP=-77",
    );
    let mut whole = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert_eq!(
        whole.report.attributes.manpower_per_base_micros,
        500_000_000
    );
    assert_eq!(
        whole.initial.world().metrics().definitions["dip_power"].minimum,
        -77
    );
    whole.advance(45).unwrap();
    let saved = whole.save().unwrap();
    let mut restored = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    restored.restore(&saved).unwrap();
    whole.advance(320).unwrap();
    restored.advance(320).unwrap();
    assert_eq!(whole.save().unwrap(), restored.save().unwrap());
    write(
        temp.path(),
        "common/defines/first.lua",
        "NDefines.NMilitary.BASE_MP_TO_MANPOWER=0.75",
    );
    let mut changed = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(changed.restore(&saved).is_err());
}

#[test]
fn invalid_world_consumed_numeric_types_and_ranges_remain_rejected() {
    for (value, detail) in [
        ("'0.25'", "numeric define"),
        ("-0.25", "nonnegative define"),
        ("1e100", "fixed-point range"),
        ("{}", "numeric define"),
    ] {
        let temp = world_fixture();
        write(
            temp.path(),
            "common/defines/patch.lua",
            &format!("NDefines.NMilitary.BASE_MP_TO_MANPOWER={value}"),
        );
        let error = VanillaWorld::load(temp.path(), "1444.11.11").unwrap_err();
        assert!(error.to_string().contains(detail), "{error}");
    }
}
