// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::EntityKind;
use augustus_eu4::{
    ScriptCompiler, ScriptLibrary, TextDocument, TextEncoding, VanillaExternalState, VanillaWorld,
};
use std::{collections::BTreeMap, fs, path::Path};

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
    let defines = temp.path().join("common/defines.lua");
    let text = fs::read_to_string(&defines).unwrap();
    fs::write(
        defines,
        text.replace(
            "NMilitary = {",
            "NDiplomacy = {\n TRUCE_YEARS = 1,\n },\n NMilitary = {",
        ),
    )
    .unwrap();
    temp
}

fn compile(text: &str, world: &VanillaWorld) -> augustus_core::Plan {
    ScriptCompiler::new(world.initial.world().schema(), &ScriptLibrary::new(), true)
        .with_truce_months(Some(12))
        .compile(
            &TextDocument::from_bytes(
                "rival-condition.txt",
                text.as_bytes().to_vec(),
                TextEncoding::Utf8,
            ),
            EntityKind::Country,
        )
        .unwrap()
        .plan
}

#[test]
fn is_rival_uses_a_complete_directional_list_and_restores_it() {
    let temp = fixture();
    let external = VanillaExternalState {
        country_statuses: BTreeMap::from([
            (
                "AAA".into(),
                BTreeMap::from([
                    ("rivals_initialized".into(), Some(true)),
                    ("is_rival:BBB".into(), Some(true)),
                ]),
            ),
            (
                "BBB".into(),
                BTreeMap::from([("rivals_initialized".into(), Some(true))]),
            ),
        ]),
        ..Default::default()
    };
    let mut loaded =
        VanillaWorld::load_with_external_state(temp.path(), "1444.11.11", &external).unwrap();
    let aaa = loaded.initial.country_entity("AAA").unwrap();
    let bbb = loaded.initial.country_entity("BBB").unwrap();

    let positive = compile("if={limit={is_rival=BBB} add_truce_with=ROOT}", &loaded);
    loaded.initial.execute(aaa, &positive).unwrap();
    assert!(loaded.initial.world().truce_with(aaa, aaa).unwrap());

    let reverse = compile("if={limit={is_rival=AAA} add_truce_with=ROOT}", &loaded);
    loaded.initial.execute(bbb, &reverse).unwrap();
    assert!(!loaded.initial.world().truces().records.contains_key(&bbb));

    let snapshot = loaded.save().unwrap();
    let mut resumed =
        VanillaWorld::load_with_external_state(temp.path(), "1444.11.11", &external).unwrap();
    resumed.restore(&snapshot).unwrap();
    assert_eq!(
        resumed
            .initial
            .world()
            .politics()
            .status(aaa, "is_rival:BBB"),
        Some(true)
    );
    assert_eq!(
        resumed
            .initial
            .world()
            .politics()
            .status(bbb, "is_rival:AAA"),
        Some(false)
    );
    assert!(resumed.initial.world().truce_with(aaa, aaa).unwrap());
}

#[test]
fn is_rival_does_not_turn_an_uninitialized_list_into_no() {
    let temp = fixture();
    let external = VanillaExternalState {
        country_statuses: BTreeMap::from([(
            "AAA".into(),
            BTreeMap::from([
                ("rivals_initialized".into(), Some(true)),
                ("is_rival:BBB".into(), Some(true)),
            ]),
        )]),
        ..Default::default()
    };
    let mut loaded =
        VanillaWorld::load_with_external_state(temp.path(), "1444.11.11", &external).unwrap();
    let bbb = loaded.initial.country_entity("BBB").unwrap();
    let plan = compile("if={limit={is_rival=AAA} add_truce_with=ROOT}", &loaded);
    assert!(loaded.initial.execute(bbb, &plan).is_err());
    assert!(!loaded.initial.world().truces().records.contains_key(&bbb));
}
