// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{EntityId, EntityKind, Schema, World};
use augustus_eu4::{ScriptCompiler, ScriptLibrary, TextDocument, TextEncoding, VanillaWorld};
use std::{fs, path::Path};

fn document(text: &str) -> TextDocument {
    TextDocument::from_bytes(
        "self-authored-government.txt",
        text.as_bytes().to_vec(),
        TextEncoding::Utf8,
    )
}

#[test]
fn compiled_queries_read_live_government_reforms_and_features_in_both_modes() {
    let schema = Schema::new(["unused"]).unwrap();
    let mut library = ScriptLibrary::new();
    library
        .add_triggers(&document(
            "civic_check={government=civic has_reform=council has_government_attribute=assembly}",
        ))
        .unwrap();
    let text = document(
        "if={limit={civic_check=yes NOT={government=royal}} log=country} capital_scope={owner={if={limit={civic_check=yes} log=owner}}}",
    );
    let mut saved = Vec::new();
    for optimise in [false, true] {
        let compiled = ScriptCompiler::new(&schema, &library, optimise)
            .compile(&text, EntityKind::Country)
            .unwrap();
        assert_eq!(compiled.folded_branches, 0);
        let mut world = World::new(schema.clone());
        world.add_entity("AAA", EntityKind::Country);
        world.add_entity("Port", EntityKind::Province);
        world.set_owner(EntityId(1), EntityId(0)).unwrap();
        world.set_capital(EntityId(0), EntityId(1)).unwrap();
        world.set_governments(serde_json::from_str(r#"{
            "reforms":{"council":{"features":{"assembly":true},"modifier":{"values_micros":{},"flags":{}},"unresolved_properties":[]}},
            "metrics":{},"countries":{"0":{"kind":"civic","reforms":["council"],"metrics":null}}
        }"#).unwrap()).unwrap();
        world.execute(EntityId(0), &compiled.plan).unwrap();
        assert_eq!(
            world
                .events()
                .iter()
                .map(|e| e.2.as_str())
                .collect::<Vec<_>>(),
            ["country", "owner"]
        );
        let mut next = world.governments().clone();
        let state = next.countries.get_mut(&EntityId(0)).unwrap();
        state.kind = Some("royal".into());
        state.reforms = Some(Default::default());
        world.set_governments(next).unwrap();
        world.execute(EntityId(0), &compiled.plan).unwrap();
        assert_eq!(world.events().len(), 2);
        saved.push(world.save("identity").unwrap());
    }
    assert_eq!(saved[0], saved[1]);
}

#[test]
fn unsupported_operands_and_wrong_scope_are_rejected_with_source_in_dead_branches() {
    let schema = Schema::new(["unused"]).unwrap();
    let library = ScriptLibrary::new();
    for optimise in [false, true] {
        for trigger in [
            "government=ROOT",
            "government=AAA",
            "government=emperor",
            "government=event_target:sample",
            "government=dictatorship",
            "government=has_harem",
            "government={kind=monarchy}",
            "has_reform=\"\"",
            "has_government_attribute=0",
        ] {
            let text = document(&format!(
                "if={{limit={{always=no}}\n if={{limit={{{trigger}}} log=bad}}}}"
            ));
            let error = ScriptCompiler::new(&schema, &library, optimise)
                .compile(&text, EntityKind::Country)
                .unwrap_err()
                .to_string();
            assert!(
                error.contains("self-authored-government.txt:2:"),
                "{trigger}: {error}"
            );
        }
        for trigger in [
            "government=monarchy",
            "has_reform=council",
            "has_government_attribute=assembly",
        ] {
            let error = ScriptCompiler::new(&schema, &library, optimise)
                .compile(
                    &document(&format!("if={{limit={{{trigger}}} log=bad}}")),
                    EntityKind::Province,
                )
                .unwrap_err()
                .to_string();
            assert!(error.contains("country scope"), "{error}");
        }
    }
}

#[test]
fn loaded_base_reforms_and_parameterized_initial_conditions_use_shared_queries() {
    let temp = tempfile::tempdir().unwrap();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world");
    for entry in walkdir::WalkDir::new(&fixture) {
        let entry = entry.unwrap();
        let target = temp
            .path()
            .join(entry.path().strip_prefix(&fixture).unwrap());
        if entry.file_type().is_dir() {
            fs::create_dir_all(target).unwrap();
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
    fs::write(temp.path().join("common/government_reforms/test_governments.txt"), "test_court={monarchy=yes custom_attributes={assembly=yes}} test_council={republic=yes} test_charter={}").unwrap();
    fs::create_dir_all(temp.path().join("common/scripted_triggers")).unwrap();
    fs::write(
        temp.path().join("common/scripted_triggers/identity.txt"),
        "is_revolutionary_republic_trigger={government=$KIND$ has_reform=$REFORM$ has_government_attribute=$FEATURE$}",
    )
    .unwrap();
    fs::create_dir_all(temp.path().join("common/estates")).unwrap();
    fs::write(
        temp.path().join("common/estates/identity.txt"),
        "assembly={trigger={is_revolutionary_republic_trigger={KIND=monarchy REFORM=test_court FEATURE=assembly}}}",
    )
    .unwrap();
    fs::write(temp.path().join("common/ideas/test_ideas.txt"), "test_alpha={free=yes trigger={government=monarchy has_reform=test_court has_government_attribute=assembly} start={test_identity=0.25} first={test_a=0.1} bonus={test_b=0.1}} test_common={free=yes start={test_common=0.1} first={test_a=0.1} bonus={test_b=0.1}}").unwrap();
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let entity = loaded.initial.country_entity("AAA").unwrap();
    let world = loaded.initial.world();
    assert_eq!(world.government_kind(entity).unwrap(), "monarchy");
    assert!(
        world
            .government_reform_selected(entity, "test_court")
            .unwrap()
    );
    assert!(world.government_feature(entity, "assembly").unwrap());
    assert_eq!(
        world.estates().countries[&entity]["assembly"].active,
        Some(true),
        "{}",
        serde_json::to_string(&loaded.report.estates).unwrap()
    );
    assert!(
        loaded
            .country("AAA")
            .unwrap()
            .ideas
            .unwrap()
            .groups
            .is_some(),
        "{}",
        serde_json::to_string(&loaded.report.ideas).unwrap()
    );
    assert!(
        loaded
            .country("AAA")
            .unwrap()
            .ideas
            .unwrap()
            .groups
            .as_ref()
            .unwrap()
            .contains_key("test_alpha")
    );
}
