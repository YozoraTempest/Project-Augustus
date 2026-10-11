// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::{
    fs,
    path::{Path, PathBuf},
};

use augustus_eu4::VanillaWorld;

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world")
}

fn copy_fixture() -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    for entry in walkdir::WalkDir::new(fixture()) {
        let entry = entry.unwrap();
        let target = temp
            .path()
            .join(entry.path().strip_prefix(fixture()).unwrap());
        if entry.file_type().is_dir() {
            fs::create_dir_all(target).unwrap();
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
    temp
}

#[test]
fn translated_attributes_explain_sources_and_keep_incomplete_state_explicit() {
    let loaded = VanillaWorld::load(fixture(), "1444.11.11").unwrap();
    let north = loaded.province(1).unwrap();
    let tax = &north.attributes["tax_basis"];
    assert_eq!(tax.base_micros, Some(2_125_000));
    assert_eq!(tax.known_subtotal_micros, Some(2_656_250));
    assert_eq!(tax.value_micros, None);
    assert_eq!(tax.steps[0].contributions[0].source, "static:city");
    assert_eq!(tax.steps[0].contributions[0].micros, 250_000);
    assert_eq!(
        north.attributes["manpower_contribution"].known_subtotal_micros,
        Some(962_500_000)
    );
    assert_eq!(
        loaded.country("AAA").unwrap().province_attribute_totals["tax_basis"].known_subtotal_micros,
        5_100_000
    );
    assert!(loaded.province(3).unwrap().attributes.is_empty());
    assert_eq!(
        loaded.report.attributes.manpower_per_base_micros,
        250_000_000
    );
    assert_eq!(
        loaded.report.attributes.definition_sources["define:manpower_per_base"].line,
        6
    );
    let future = VanillaWorld::load(fixture(), "1445.1.1").unwrap();
    assert!(
        future.province(1).unwrap().attributes["tax_basis"]
            .unresolved
            .contains("pending_modifier:province:1:test_harbor")
    );
    let temp = copy_fixture();
    let history = temp.path().join("history/provinces/1 - North.txt");
    let text = fs::read_to_string(&history).unwrap();
    fs::write(history, text + "\n1444.1.1 = { controller = AAA }\n").unwrap();
    let occupied = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert_eq!(
        occupied.province(1).unwrap().attributes["tax_basis"].known_subtotal_micros,
        Some(1_593_750)
    );
}

#[test]
fn duplicate_static_definitions_and_properties_preserve_ambiguity_without_guessing() {
    let temp = copy_fixture();
    let path = temp.path().join("common/static_modifiers/test_static.txt");
    let text = fs::read_to_string(&path).unwrap();
    fs::write(
        &path,
        text + "\ncity = { local_tax_modifier = 0.5 }\ncity = { local_tax_modifier = 0.75 }\n",
    )
    .unwrap();
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert_eq!(
        loaded.report.attributes.ambiguous_definitions["static_modifiers:city"].len(),
        3
    );
    assert!(
        !loaded
            .initial
            .world()
            .attributes()
            .static_modifiers
            .contains_key("city")
    );
    let tax = &loaded.province(1).unwrap().attributes["tax_basis"];
    assert_eq!(tax.known_subtotal_micros, Some(2_125_000));
    assert!(tax.unresolved.contains("ambiguous_static:city"));
    let temp = copy_fixture();
    let path = temp.path().join("common/buildings/test_buildings.txt");
    let text = fs::read_to_string(&path).unwrap();
    fs::write(
        path,
        text.replace(
            "local_tax_modifier = 0.125",
            "local_tax_modifier = 0.125 local_tax_modifier = 0.5",
        ),
    )
    .unwrap();
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let tax = &loaded.province(2).unwrap().attributes["tax_basis"];
    assert_eq!(tax.known_subtotal_micros, Some(4_600_000));
    assert!(
        tax.unresolved
            .contains("ambiguous:buildings:test_market:local_tax_modifier")
    );
}

#[test]
fn expressions_overrides_and_invalid_attribute_shapes_are_rejected() {
    for (path, from, to, message) in [
        (
            "common/defines.lua",
            "0.25,",
            "0.25 * 2,",
            "nonnegative literal",
        ),
        (
            "common/defines.lua",
            "0.25,",
            "-0.25,",
            "nonnegative literal",
        ),
        (
            "common/defines.lua",
            "BASE_MP_TO_MANPOWER = 0.25,",
            "BASE_MP_TO_MANPOWER = 0.25,\nBASE_MP_TO_MANPOWER = 0.5,",
            "unique literal",
        ),
        (
            "common/buildings/test_buildings.txt",
            "modifier = { test_power",
            "modifier = { 1 } modifier = { test_power",
            "expected setup block",
        ),
        (
            "common/buildings/test_buildings.txt",
            "local_tax_modifier = 0.125",
            "local_tax_modifier = yes",
            "numeric property",
        ),
    ] {
        let temp = copy_fixture();
        let path = temp.path().join(path);
        let text = fs::read_to_string(&path).unwrap();
        fs::write(path, text.replace(from, to)).unwrap();
        let error = VanillaWorld::load(temp.path(), "1444.11.11").unwrap_err();
        assert!(error.to_string().contains(message), "{error}");
    }
    let temp = copy_fixture();
    fs::create_dir_all(temp.path().join("common/defines")).unwrap();
    fs::write(
        temp.path().join("common/defines/override.lua"),
        "NDefines.NMilitary.BASE_MP_TO_MANPOWER = 0.5\n",
    )
    .unwrap();
    let error = VanillaWorld::load(temp.path(), "1444.11.11").unwrap_err();
    assert!(
        error.to_string().contains("unsupported Lua evaluation"),
        "{error}"
    );
    assert!(error.location().is_some());
}
