// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::*;
use augustus_eu4::{ScriptCompiler, ScriptLibrary, TextDocument, TextEncoding};

#[test]
fn effects_and_queries_use_the_existing_stock_and_quantize_each_literal() {
    let schema = Schema::new(["unused"]).unwrap();
    let library = ScriptLibrary::new();
    for optimise in [false, true] {
        let mut world = World::new(schema.clone());
        let aaa = world.add_entity("AAA", EntityKind::Country);
        world.add_entity("BBB", EntityKind::Country);
        world
            .set_country_resources(
                [(
                    aaa,
                    CountryResources {
                        treasury_milli: Some(1234),
                        manpower_milli: Some(0),
                        mercantilism_milli: None,
                    },
                )]
                .into(),
            )
            .unwrap();
        let compiled=ScriptCompiler::new(&schema,&library,optimise).compile(&TextDocument::from_bytes("manpower.txt",b"add_manpower=1 add_manpower=0.5 add_manpower=0.0005 add_manpower=0.0005 if={limit={manpower=1.500999 NOT={manpower=1.501}} log=quantized} add_manpower=100 if={limit={manpower=100} log=excess} add_manpower=-999 if={limit={manpower=0 NOT={manpower=0.001}} log=zero}".to_vec(),TextEncoding::Utf8),EntityKind::Country).unwrap();
        world.execute(aaa, &compiled.plan).unwrap();
        assert_eq!(world.country_resources()[&aaa].manpower_milli, Some(0));
        assert_eq!(world.country_resources()[&aaa].treasury_milli, Some(1234));
        assert_eq!(
            world
                .events()
                .iter()
                .map(|event| event.2.as_str())
                .collect::<Vec<_>>(),
            ["quantized", "excess", "zero"]
        );
    }
}

#[test]
fn unverified_operands_and_wrong_scopes_report_their_source() {
    let schema = Schema::new(["unused"]).unwrap();
    let library = ScriptLibrary::new();
    for text in [
        "add_manpower=ROOT",
        "add_manpower={value=1}",
        "capital_scope={add_manpower=1}",
        "capital_scope={if={limit={manpower=1} log=bad}}",
    ] {
        let error = ScriptCompiler::new(&schema, &library, true)
            .compile(
                &TextDocument::from_bytes(
                    "bad-manpower.txt",
                    text.as_bytes().to_vec(),
                    TextEncoding::Utf8,
                ),
                EntityKind::Country,
            )
            .unwrap_err();
        assert!(error.to_string().contains("bad-manpower.txt:1:"));
    }
}

#[test]
fn a_supported_effect_does_not_initialize_unknown_original_resources() {
    let schema = Schema::new(["unused"]).unwrap();
    let library = ScriptLibrary::new();
    let mut world = World::new(schema.clone());
    let aaa = world.add_entity("AAA", EntityKind::Country);
    let compiled = ScriptCompiler::new(&schema, &library, true)
        .compile(
            &TextDocument::from_bytes(
                "unknown.txt",
                b"add_manpower=1".to_vec(),
                TextEncoding::Utf8,
            ),
            EntityKind::Country,
        )
        .unwrap();
    assert!(world.execute(aaa, &compiled.plan).is_err());
    assert!(world.country_resources().is_empty());
    assert!(world.save("unknown:1").is_err());
}

#[test]
fn failed_startup_content_identity_tracks_parameters_and_excludes_source_paths() {
    use augustus_eu4::VanillaWorld;
    use std::{fs, path::Path};
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world");
    let roots = [tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap()];
    for root in &roots {
        for entry in walkdir::WalkDir::new(&source) {
            let entry = entry.unwrap();
            let target = root
                .path()
                .join(entry.path().strip_prefix(&source).unwrap());
            if entry.file_type().is_dir() {
                fs::create_dir_all(target).unwrap();
            } else {
                fs::copy(entry.path(), target).unwrap();
            }
        }
        fs::create_dir_all(root.path().join("common/on_actions")).unwrap();
        fs::write(
            root.path().join("common/on_actions/control.txt"),
            "on_startup={if={limit={tag=AAA} add_manpower=1}}",
        )
        .unwrap();
    }
    let loaded = VanillaWorld::load(roots[0].path(), "1444.11.11").unwrap();
    assert!(!loaded.report.estates.startup_unresolved.is_empty());
    let saved = loaded.save().unwrap();
    let mut moved = VanillaWorld::load(roots[1].path(), "1444.11.11").unwrap();
    moved.restore(&saved).unwrap();
    assert_eq!(moved.save().unwrap(), saved);
    fs::write(
        roots[1].path().join("common/on_actions/control.txt"),
        "on_startup={if={limit={tag=AAA} add_manpower=2}}",
    )
    .unwrap();
    let mut changed = VanillaWorld::load(roots[1].path(), "1444.11.11").unwrap();
    assert!(changed.restore(&saved).is_err());
    assert!(
        changed
            .initial
            .world()
            .country_resources()
            .values()
            .all(|resources| resources.manpower_milli.is_none())
    );
}
