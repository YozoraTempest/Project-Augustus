// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::*;
use augustus_eu4::*;
use std::{fs, path::Path};

fn document(text: &str) -> TextDocument {
    TextDocument::from_bytes(
        "events/self_authored.txt",
        text.as_bytes().to_vec(),
        TextEncoding::Utf8,
    )
}

fn world() -> World {
    let mut world = World::new(Schema::new(["unused"]).unwrap());
    world.add_entity("AAA", EntityKind::Country);
    world.add_entity("Port", EntityKind::Province);
    world.set_metrics(serde_json::from_str(r#"{
        "definitions":{
            "adm_power":{"kind":"Country","minimum":-100,"maximum":9223372036854775807,"baseline":0,"divisor":1,"modifier":null,"unresolved_properties":[],"growth_ceiling":true},
            "dip_power":{"kind":"Country","minimum":-999,"maximum":9223372036854775807,"baseline":0,"divisor":1,"modifier":null,"unresolved_properties":[],"growth_ceiling":true},
            "mil_power":{"kind":"Country","minimum":-100,"maximum":9223372036854775807,"baseline":0,"divisor":1,"modifier":null,"unresolved_properties":[],"growth_ceiling":true}},
        "entities":{"0":{"adm_power":42,"dip_power":84,"mil_power":126}},
        "ceilings":{"0":{"adm_power":999,"dip_power":999,"mil_power":999}}
    }"#).unwrap()).unwrap();
    world
}

#[test]
fn translated_integer_operations_and_conditions_use_authoritative_stocks() {
    let text = r#"country_event={id=power.start is_triggered_only=yes immediate={
        add_adm_power=-100000 add_dip_power=-100000 add_mil_power=-100000
        add_adm_power=0.999 add_dip_power=-0.999 add_mil_power=1.999
        if={limit={adm_power=-100 NOT={adm_power=-99} dip_power=-999 NOT={dip_power=-998}
            mil_power=-99 NOT={mil_power=-98} adm_power=-100.999} log=minimum}
        add_adm_power=100000 add_dip_power=100000 add_mil_power=100000
        if={limit={adm_power=999 NOT={adm_power=1000} dip_power=999.999 mil_power=999} log=maximum}
    } option={name=ok}}"#;
    let mut saved = Vec::new();
    for optimise in [false, true] {
        let mut world = world();
        let library = ScriptLibrary::new();
        let compiled = ScriptCompiler::new(world.schema(), &library, optimise)
            .with_metric_ids(world.metrics().definitions.keys().cloned().collect())
            .compile_events(&document(text))
            .unwrap();
        let catalog = EventCatalog::new(compiled.definitions).unwrap();
        world.set_event_catalog(catalog).unwrap();
        world.trigger_event("power.start", EntityId(0)).unwrap();
        assert_eq!(
            world
                .events()
                .iter()
                .map(|(_, _, text)| text.as_str())
                .collect::<Vec<_>>(),
            ["minimum", "maximum"]
        );
        saved.push(world.save("fixture").unwrap());
    }
    assert_eq!(saved[0], saved[1]);
}

#[test]
fn undeclared_stocks_wrong_scopes_and_dynamic_operands_keep_source_errors() {
    let world = world();
    let library = ScriptLibrary::new();
    for (text, register) in [
        (
            "country_event={id=bad is_triggered_only=yes immediate={add_adm_power=1} option={name=ok}}",
            false,
        ),
        (
            "country_event={id=bad is_triggered_only=yes immediate={capital_scope={add_dip_power=1}} option={name=ok}}",
            true,
        ),
        (
            "country_event={id=bad is_triggered_only=yes immediate={add_mil_power=2147483648} option={name=ok}}",
            true,
        ),
        (
            "country_event={id=bad is_triggered_only=yes immediate={if={limit={adm_power=BBB} log=bad}} option={name=ok}}",
            true,
        ),
    ] {
        let compiler = ScriptCompiler::new(world.schema(), &library, true);
        let compiler = if register {
            compiler.with_metric_ids(world.metrics().definitions.keys().cloned().collect())
        } else {
            compiler
        };
        let error = compiler.compile_events(&document(text)).unwrap_err();
        assert!(error.location().is_some(), "{error}");
        assert!(error.to_string().contains("self_authored.txt"));
    }
}

#[test]
fn loading_reads_signed_limits_and_keeps_uninitialized_stocks_unknown() {
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
    let defines = temp.path().join("common/defines.lua");
    let text = fs::read_to_string(&defines).unwrap();
    fs::write(
        &defines,
        text.replace(
            "NMilitary = {",
            "NMilitary = {\n        GREAT_POWER_MAX = 8,",
        )
        .replace(
            "HRE_RANK = 3,",
            "HRE_RANK = 3,\n        DEBT_LIMIT_DIP = -99,\n        POWER_MAX = 500,",
        ),
    )
    .unwrap();
    fs::create_dir_all(temp.path().join("common/estates")).unwrap();
    fs::write(
        temp.path().join("common/estates/power.txt"),
        "estate_power={trigger={NOT={adm_power=1} capital_scope={owner={dip_power=1}}}}",
    )
    .unwrap();
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let world = loaded.initial.world();
    assert_eq!(world.metrics().definitions["dip_power"].minimum, -99);
    assert_eq!(loaded.report.setup.monarch_power_base_maximum, Some(500));
    assert!(
        loaded
            .report
            .setup
            .definition_sources
            .contains_key("monarch_power:dip_minimum")
    );
    assert!(
        loaded.country("AAA").unwrap().estates.unwrap()["estate_power"]
            .active
            .is_none()
    );
    assert!(
        !loaded
            .report
            .estates
            .unsupported_fields
            .contains_key("trigger:owner")
    );
    for country in loaded.initial.countries() {
        let entity = loaded.initial.country_entity(&country.id).unwrap();
        for name in ["adm_power", "dip_power", "mil_power"] {
            assert!(world.metrics().value(world, entity, name).is_err());
            assert!(world.metrics().ceiling(world, entity, name).is_err());
        }
    }
    fs::create_dir_all(temp.path().join("common/defines")).unwrap();
    fs::write(
        temp.path().join("common/defines/power.lua"),
        "NDefines.NCountry.DEBT_LIMIT_DIP = -77",
    )
    .unwrap();
    let error = VanillaWorld::load(temp.path(), "1444.11.11").err().unwrap();
    assert!(error.to_string().contains("Lua evaluation"));
    assert!(error.to_string().contains("power.lua"));
}
