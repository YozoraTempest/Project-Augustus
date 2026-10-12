// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{EntityKind, Schema, World};
use augustus_eu4::{
    ContentRoot, ContentSet, ContentSource, DefinitionKind, ScriptCompiler, ScriptLibrary,
    TextDocument, TextEncoding,
};

fn document(text: &str) -> TextDocument {
    TextDocument::from_bytes(
        "self-authored.txt",
        text.as_bytes().to_vec(),
        TextEncoding::Utf8,
    )
}

#[test]
fn script_view_uses_original_filename_order_and_direct_children() {
    let root = tempfile::tempdir().unwrap();
    let effects = root.path().join("common/scripted_effects");
    std::fs::create_dir_all(effects.join("nested")).unwrap();
    std::fs::write(effects.join("Z_first.txt"), "winner={log=first}").unwrap();
    std::fs::write(effects.join("a_second.txt"), "winner={log=second}").unwrap();
    std::fs::write(effects.join("nested/hidden.txt"), "hidden={log=hidden}").unwrap();
    let content = ContentSet::resolve(vec![ContentSource::base(
        ContentRoot::open(root.path()).unwrap(),
    )])
    .unwrap();
    let files = ScriptLibrary::files(&content, DefinitionKind::ScriptedEffects).unwrap();
    assert_eq!(
        files
            .iter()
            .map(|(_, origin)| origin.relative_path.file_name().unwrap().to_str().unwrap())
            .collect::<Vec<_>>(),
        ["Z_first.txt", "a_second.txt"]
    );
    assert!(ScriptLibrary::files(&content, DefinitionKind::Buildings).is_err());
    let library = ScriptLibrary::from_content(&content, TextEncoding::Utf8).unwrap();
    assert_eq!(library.ignored_definitions().len(), 1);
    let schema = Schema::new(["unused"]).unwrap();
    let script = ScriptCompiler::new(&schema, &library, true)
        .compile(&document("winner=yes"), EntityKind::Country)
        .unwrap();
    let mut world = World::new(schema.clone());
    let country = world.add_entity("Test", EntityKind::Country);
    world.execute(country, &script.plan).unwrap();
    assert_eq!(world.events()[0].2, "first");
    assert!(
        ScriptCompiler::new(&schema, &library, true)
            .compile(&document("hidden=yes"), EntityKind::Country)
            .is_err()
    );
}

#[test]
fn compiled_calls_bind_first_declarations_and_preserve_live_state() {
    let schema = Schema::new(["treasury", "variable/counter"]).unwrap();
    let mut library = ScriptLibrary::new();
    library
        .add_effects(&document(
            "fund={add_treasury=1.125} fund={add_treasury=99}",
        ))
        .unwrap();
    library
        .add_triggers(&document("ready={always=yes} ready={always=no}"))
        .unwrap();
    let input = document(
        "fund=yes capital_scope={set_variable={which=counter value=2.125} change_variable={which=counter value=-0.125} if={limit={check_variable={which=counter value=2}} owner={add_treasury=2}} else={log=incorrect}} if={limit={ready=yes} log=ready} else={log=incorrect}",
    );
    let mut worlds = Vec::new();
    for optimise in [false, true] {
        let script = ScriptCompiler::new(&schema, &library, optimise)
            .compile(&input, EntityKind::Country)
            .unwrap();
        let mut world = World::new(schema.clone());
        let country = world.add_entity("Test", EntityKind::Country);
        let province = world.add_entity("Test capital", EntityKind::Province);
        world.set_owner(province, country).unwrap();
        world.set_capital(country, province).unwrap();
        world.execute(country, &script.plan).unwrap();
        assert_eq!(
            world
                .value(country, schema.field("treasury").unwrap())
                .unwrap(),
            3125
        );
        assert_eq!(
            world
                .value(province, schema.field("variable/counter").unwrap())
                .unwrap(),
            2000
        );
        assert_eq!(world.events()[0].2, "ready");
        assert_eq!(script.folded_branches, usize::from(optimise));
        worlds.push(world);
    }
    assert_eq!(worlds[0], worlds[1]);
    assert_eq!(library.ignored_definitions().len(), 2);
}

#[test]
fn unsupported_semantics_recursion_and_scope_errors_report_source_positions() {
    let schema = Schema::new(["treasury", "variable/x"]).unwrap();
    let mut library = ScriptLibrary::new();
    library.add_effects(&document("loop={loop=yes}")).unwrap();
    for input in [
        "loop=yes",
        "\nadd_prestige=1",
        "owner={add_treasury=1}",
        "add_treasury=1.0001",
        "set_variable={which=missing@ROOT value=1}",
        "if={limit={always=yes} add_prestige=1}",
        "set_variable={which=x value=1 value=2}",
        "else={log=orphan}",
        "if={limit={unsupported=yes} log=x}",
        "if={limit={NOT={}} log=x}",
    ] {
        let error = ScriptCompiler::new(&schema, &library, true)
            .compile(&document(input), EntityKind::Country)
            .unwrap_err();
        assert_eq!(error.path().to_string_lossy(), "self-authored.txt");
        assert!(error.location().is_some(), "{input}");
    }
}

#[test]
fn identifier_case_and_fractional_literals_are_preserved() {
    let schema = Schema::new(["treasury"]).unwrap();
    let mut library = ScriptLibrary::new();
    library
        .add_effects(&document(
            "Bonus={add_treasury=0.001} bonus={add_treasury=-1.2}",
        ))
        .unwrap();
    let script = ScriptCompiler::new(&schema, &library, false)
        .compile(&document("Bonus=yes bonus=yes"), EntityKind::Country)
        .unwrap();
    let mut world = World::new(schema);
    let country = world.add_entity("Test", EntityKind::Country);
    world.execute(country, &script.plan).unwrap();
    assert_eq!(
        world
            .value(country, world.schema().field("treasury").unwrap())
            .unwrap(),
        -1199
    );
}

#[test]
fn library_registration_preserves_unsupported_templates_for_call_site_validation() {
    let mut library = ScriptLibrary::new();
    library
        .add_effects(&TextDocument::from_bytes(
            "effect-library.txt",
            b"list={\nlog={first second}\n} operator={\nlog>text\n} good={log=good}".to_vec(),
            TextEncoding::Utf8,
        ))
        .unwrap();
    library
        .add_triggers(&TextDocument::from_bytes(
            "trigger-library.txt",
            b"list={\nalways={yes no}\n} operator={\nalways>yes\n}".to_vec(),
            TextEncoding::Utf8,
        ))
        .unwrap();
    let schema = Schema::new(["unused"]).unwrap();
    for optimise in [false, true] {
        let compiled = ScriptCompiler::new(&schema, &library, optimise)
            .compile(&document("good=yes"), EntityKind::Country)
            .unwrap();
        assert_eq!(compiled.plan, [augustus_core::Action::Emit("good".into())]);
        assert!(
            compiled
                .sources
                .iter()
                .any(|(path, _)| path == std::path::Path::new("effect-library.txt"))
        );
        for (input, path, message) in [
            (
                "if={limit={always=no} list=yes}",
                "effect-library.txt",
                "unsupported script value",
            ),
            (
                "operator=yes",
                "effect-library.txt",
                "unsupported script operator",
            ),
            (
                "if={limit={always=no list=yes} log=unused}",
                "trigger-library.txt",
                "unsupported script value",
            ),
            (
                "if={limit={operator=no} log=unused}",
                "trigger-library.txt",
                "unsupported script operator",
            ),
        ] {
            let error = ScriptCompiler::new(&schema, &library, optimise)
                .compile(&document(input), EntityKind::Country)
                .unwrap_err();
            assert_eq!(error.path(), std::path::Path::new(path));
            assert!(error.to_string().contains(message), "{error}");
            assert!(error.location().is_some());
        }
    }
    // Explicit input still rejects unsupported grammar during its initial parse.
    assert!(
        ScriptCompiler::new(&schema, &library, true)
            .compile(
                &document("if={limit={always=no} log={first second}}"),
                EntityKind::Country
            )
            .is_err()
    );
    assert!(library.add_effects(&document("invalid=scalar")).is_err());
}
