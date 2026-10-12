// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::*;
use augustus_eu4::*;

fn document(text: &str) -> TextDocument {
    TextDocument::from_bytes(
        "self-authored-grounds.txt",
        text.as_bytes().to_vec(),
        TextEncoding::Utf8,
    )
}

fn types(text: &str) -> CasusBelliTypes {
    let directory = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(directory.path().join("common/cb_types")).unwrap();
    std::fs::write(directory.path().join("common/cb_types/control.txt"), text).unwrap();
    let content = ContentSet::resolve(vec![ContentSource::base(
        ContentRoot::open(directory.path()).unwrap(),
    )])
    .unwrap();
    CasusBelliTypes::from_content(
        &content,
        TextEncoding::Utf8,
        &Schema::new(["unused"]).unwrap(),
        &ScriptLibrary::new(),
    )
    .unwrap()
}

fn world(types: &CasusBelliTypes) -> World {
    let mut world = World::new(Schema::new(["unused"]).unwrap());
    for name in ["AAA", "BBB", "CCC"] {
        world.add_entity(name, EntityKind::Country);
    }
    for index in 0..2 {
        let province = world.add_entity(format!("province-{index}"), EntityKind::Province);
        world.set_owner(province, EntityId(index)).unwrap();
    }
    world
        .set_calendar(Calendar {
            start: WorldDate {
                year: 1444,
                month: 11,
                day: 11,
            },
            month_lengths: [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31],
        })
        .unwrap();
    world
        .set_justifications(Justifications {
            definitions: types.definitions(),
            ..Default::default()
        })
        .unwrap();
    world
}

#[test]
fn explicit_default_zero_and_negative_months_execute_the_observed_normalization() {
    let types = types(
        "restoration={is_triggered_only=yes months=2} missing={is_triggered_only=yes} zero={is_triggered_only=yes months=0}",
    );
    assert_eq!(types.types["missing"].default_months, 12);
    assert_eq!(types.report.definitions, 3);
    let schema = Schema::new(["unused"]).unwrap();
    let library = ScriptLibrary::new();
    for (name, value, expected) in [
        ("restoration", "months=1", 30),
        ("restoration", "", 61),
        ("restoration", "months=0", 61),
        ("restoration", "months=-1", 61),
        ("missing", "", 365),
        ("zero", "", 0),
    ] {
        let mut world = world(&types);
        let compiled = ScriptCompiler::new(&schema, &library, true)
            .with_casus_belli_types(types.types.clone())
            .compile(
                &document(&format!(
                    "add_casus_belli={{type={name} target=BBB {value}}}"
                )),
                EntityKind::Country,
            )
            .unwrap();
        world.execute(EntityId(0), &compiled.plan).unwrap();
        assert_eq!(
            world.justifications().active[&EntityId(0)][&EntityId(1)][name].expires_on,
            Some(expected)
        );
    }
}

#[test]
fn forward_and_reverse_effects_share_live_typed_endpoint_resolution() {
    let types = types("restoration={is_triggered_only=yes months=2}");
    let schema = Schema::new(["unused"]).unwrap();
    let library = ScriptLibrary::new();
    let text = "add_casus_belli={type=restoration target=event_target:ally months=1} reverse_add_casus_belli={type=restoration target=BBB months=2} if={limit={has_casus_belli={type=restoration target=BBB}} log=direct} if={limit={has_casus_belli_against=BBB} log=any} remove_casus_belli={type=restoration target=BBB} reverse_remove_casus_belli={type=restoration target=BBB}";
    let mut results = Vec::new();
    for optimise in [false, true] {
        let compiled = ScriptCompiler::new(&schema, &library, optimise)
            .with_casus_belli_types(types.types.clone())
            .with_global_scope_types([("ally".into(), EntityKind::Country)].into_iter().collect())
            .compile(&document(text), EntityKind::Country)
            .unwrap();
        let mut world = world(&types);
        world.bind_global_scope("ally", EntityId(1)).unwrap();
        world.execute(EntityId(0), &compiled.plan).unwrap();
        assert_eq!(
            world
                .events()
                .iter()
                .map(|e| e.2.as_str())
                .collect::<Vec<_>>(),
            ["direct", "any"]
        );
        assert!(
            !world
                .has_justification(EntityId(0), EntityId(1), None)
                .unwrap()
        );
        assert!(
            !world
                .has_justification(EntityId(1), EntityId(0), None)
                .unwrap()
        );
        results.push(world.save("directions").unwrap());
    }
    assert_eq!(results[0], results[1]);
}

#[test]
fn prerequisites_use_issuer_context_and_top_level_from_switches_to_target() {
    let types = types(
        "issuer={is_triggered_only=yes prerequisites={tag=AAA}} opponent={is_triggered_only=yes prerequisites={FROM={tag=BBB}}} missing_rule={is_triggered_only=yes prerequisites_self={unimplemented_rule=yes}}",
    );
    assert!(
        types
            .report
            .uncompiled_maintenance
            .contains_key("missing_rule")
    );
    let mut world = world(&types);
    for name in ["issuer", "opponent", "missing_rule"] {
        world
            .grant_justification(EntityId(0), EntityId(1), name, GrantTerm::Permanent)
            .unwrap();
    }
    world.advance_to(1).unwrap();
    assert!(
        world
            .has_justification(EntityId(0), EntityId(1), Some("issuer"))
            .unwrap()
    );
    assert!(
        world
            .has_justification(EntityId(0), EntityId(1), Some("opponent"))
            .unwrap()
    );
    assert!(
        world
            .has_justification(EntityId(0), EntityId(1), Some("missing_rule"))
            .is_err()
    );
    world
        .grant_justification(EntityId(1), EntityId(0), "issuer", GrantTerm::Permanent)
        .unwrap();
    world
        .grant_justification(EntityId(1), EntityId(0), "opponent", GrantTerm::Permanent)
        .unwrap();
    world.advance_to(2).unwrap();
    assert!(
        !world
            .has_justification(EntityId(1), EntityId(0), Some("issuer"))
            .unwrap()
    );
    assert!(
        !world
            .has_justification(EntityId(1), EntityId(0), Some("opponent"))
            .unwrap()
    );
}

#[test]
fn malformed_unbound_and_unimplemented_automatic_forms_keep_their_source_locations() {
    let types = types("restoration={is_triggered_only=yes months=2} automatic={}");
    let schema = Schema::new(["unused"]).unwrap();
    let library = ScriptLibrary::new();
    for text in [
        "add_casus_belli={type=restoration}",
        "add_casus_belli={target=BBB}",
        "add_casus_belli={type=unknown target=BBB}",
        "add_casus_belli={type=restoration target=ROOT}",
        "add_casus_belli={type=restoration target=event_target:unknown}",
        "add_casus_belli={type=restoration target=BBB months=1 months=2}",
        "add_casus_belli={type=restoration target=BBB months=1.5}",
        "remove_casus_belli={type=restoration target=BBB months=1}",
        "if={limit={has_casus_belli={type=automatic target=BBB}} log=unverified}",
        "if={limit={has_casus_belli_against=BBB} log=unverified}",
    ] {
        let error = ScriptCompiler::new(&schema, &library, true)
            .with_casus_belli_types(types.types.clone())
            .compile(&document(&format!("\n\n{text}")), EntityKind::Country)
            .unwrap_err();
        assert_eq!(error.location().unwrap().line, 3);
    }
    let error = ScriptCompiler::new(&schema, &library, true)
        .with_casus_belli_types(types.types.clone())
        .compile(
            &document("add_casus_belli={type=restoration target=BBB}"),
            EntityKind::Province,
        )
        .unwrap_err();
    assert_eq!(error.location().unwrap().line, 1);
}

#[test]
fn content_overlay_selects_type_defaults_without_copying_the_base_definition() {
    let base = tempfile::tempdir().unwrap();
    let replacement = tempfile::tempdir().unwrap();
    for (root, months) in [(base.path(), 2), (replacement.path(), 5)] {
        std::fs::create_dir_all(root.join("common/cb_types")).unwrap();
        std::fs::write(
            root.join("common/cb_types/control.txt"),
            format!("restoration={{is_triggered_only=yes months={months}}}"),
        )
        .unwrap();
    }
    std::fs::write(replacement.path().join("descriptor.mod"), "name=Community").unwrap();
    let mod_source = ContentSource::mod_directory(replacement.path(), TextEncoding::Utf8).unwrap();
    let content = ContentSet::resolve(vec![
        ContentSource::base(ContentRoot::open(base.path()).unwrap()),
        mod_source,
    ])
    .unwrap();
    let types = CasusBelliTypes::from_content(
        &content,
        TextEncoding::Utf8,
        &Schema::new(["unused"]).unwrap(),
        &ScriptLibrary::new(),
    )
    .unwrap();
    assert_eq!(types.types["restoration"].default_months, 5);
}

#[test]
fn ambiguous_and_out_of_range_type_metadata_are_rejected_at_the_definition() {
    for text in [
        "ground={months=-1}",
        "ground={months=2147483648}",
        "ground={months=1.5}",
        "ground={months=1 months=2}",
        "ground={} ground={}",
    ] {
        let directory = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(directory.path().join("common/cb_types")).unwrap();
        std::fs::write(directory.path().join("common/cb_types/control.txt"), text).unwrap();
        let content = ContentSet::resolve(vec![ContentSource::base(
            ContentRoot::open(directory.path()).unwrap(),
        )])
        .unwrap();
        let error = CasusBelliTypes::from_content(
            &content,
            TextEncoding::Utf8,
            &Schema::new(["unused"]).unwrap(),
            &ScriptLibrary::new(),
        )
        .unwrap_err();
        assert_eq!(error.location().unwrap().line, 1);
    }
}
