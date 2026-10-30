// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::{fs, path::Path};

use augustus_core::VanillaNavalConstructionCostResult;
use augustus_eu4::{ContentSet, TextEncoding, VanillaNavalConstructionCostQuery, VanillaWorld};
use tempfile::TempDir;
use walkdir::WalkDir;

fn source_fixture() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world")
}

fn fixture() -> TempDir {
    let temp = tempfile::tempdir().unwrap();
    let game = temp.path().join("game");
    for entry in WalkDir::new(source_fixture()) {
        let entry = entry.unwrap();
        let target = game.join(entry.path().strip_prefix(source_fixture()).unwrap());
        if entry.file_type().is_dir() {
            fs::create_dir_all(target).unwrap();
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }

    let light_ship = fs::read_to_string(game.join("common/units/sample_light_ship.txt")).unwrap();
    assert!(light_ship.contains("type = light_ship"));
    fs::write(
        game.join("common/units/sample_galley.txt"),
        light_ship.replace("type = light_ship", "type = galley"),
    )
    .unwrap();
    write(
        &game,
        "common/defines/20_naval_cost.lua",
        "NDefines.NMilitary.GALLEY_COST=12\n",
    );
    temp
}

fn write(root: &Path, path: &str, contents: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

fn naval_cost_mod(temp: &TempDir, cost: &str) -> std::path::PathBuf {
    let path = temp.path().join("naval-cost-mod");
    write(&path, "descriptor.mod", "name = \"Naval Cost Test\"\n");
    write(
        &path,
        "common/defines/20_naval_cost.lua",
        &format!("NDefines.NMilitary.GALLEY_COST={cost}\n"),
    );
    path
}

fn load_world(
    temp: &TempDir,
    mods: &[std::path::PathBuf],
    controller: Option<&str>,
) -> (ContentSet, VanillaWorld) {
    let game = temp.path().join("game");
    let content =
        ContentSet::from_installation(&game, mods, None, None, TextEncoding::Utf8).unwrap();
    let controller = controller
        .map(|tag| format!("controller={tag}"))
        .unwrap_or_default();
    let save = temp.path().join("world.eu4");
    fs::write(
        &save,
        format!(
            "EU4txt\ndate=1444.11.11\nplayer=AAA\ncountries={{ AAA={{ treasury=20.000 }} BBB={{}} }}\nprovinces={{ -1={{ owner=AAA {controller} }} -2={{ owner=AAA controller=AAA }} }}\n"
        ),
    )
    .unwrap();
    let world = VanillaWorld::load_from_content_with_save(&content, &save).unwrap();
    (content, world)
}

fn query(country: &str) -> VanillaNavalConstructionCostQuery {
    VanillaNavalConstructionCostQuery {
        country: country.into(),
        province_id: 1,
        unit_type: "sample_galley".into(),
        country_modifier_milli: Some(0),
        additional_country_modifier_milli: Some(0),
        province_modifier_milli: Some(0),
        naval_quantity_milli: Some(10_000),
        naval_limit_milli: Some(17_000),
    }
}

#[test]
fn galley_cost_query_uses_the_effective_mod_define_and_does_not_mutate_state() {
    let temp = fixture();
    let patch = naval_cost_mod(&temp, "10");
    let (content, world) = load_world(&temp, std::slice::from_ref(&patch), Some("AAA"));

    assert_eq!(
        world.report.setup.naval_construction_cost_defines_milli["GALLEY_COST"],
        10_000
    );
    let source = &world.report.setup.definition_sources["naval_construction_cost:GALLEY_COST"];
    assert!(source.path.starts_with(fs::canonicalize(&patch).unwrap()));

    let before = world.save().unwrap();
    let report = world.query_naval_construction_cost(&query("AAA")).unwrap();
    assert_eq!(
        report.evaluation,
        Some(VanillaNavalConstructionCostResult {
            modifier_factor_milli: 1_000,
            cost_milli: 10_000,
        })
    );
    assert!(report.unresolved.is_empty());
    assert_eq!(world.save().unwrap(), before);

    let mut restored =
        VanillaWorld::load_from_content_with_save(&content, temp.path().join("world.eu4")).unwrap();
    restored.restore(&before).unwrap();
    assert_eq!(
        restored
            .query_naval_construction_cost(&query("AAA"))
            .unwrap(),
        report
    );
}

#[test]
fn galley_cost_define_accepts_whole_amount_boundaries_and_preserves_source_location() {
    for (cost, base_milli, expected_cost) in
        [("0", 0, 1_000), ("2147483", 2_147_483_000, 2_147_483_000)]
    {
        let temp = fixture();
        let patch = naval_cost_mod(&temp, cost);
        let (_, world) = load_world(&temp, std::slice::from_ref(&patch), Some("AAA"));

        assert_eq!(
            world.report.setup.naval_construction_cost_defines_milli["GALLEY_COST"],
            base_milli
        );
        let source = &world.report.setup.definition_sources["naval_construction_cost:GALLEY_COST"];
        assert!(source.path.starts_with(fs::canonicalize(&patch).unwrap()));
        assert_eq!(source.line, 1);

        let report = world.query_naval_construction_cost(&query("AAA")).unwrap();
        assert_eq!(report.evaluation.unwrap().cost_milli, expected_cost);
    }
}

#[test]
fn galley_cost_define_rejects_fractional_negative_and_out_of_range_values_with_source_location() {
    for cost in [
        "1.001",
        "10.125",
        "0.0000001",
        "-0.0000001",
        "-1",
        "2147484",
    ] {
        let temp = fixture();
        let patch = naval_cost_mod(&temp, cost);
        let (_, world) = load_world(&temp, std::slice::from_ref(&patch), Some("AAA"));

        assert!(
            world
                .report
                .setup
                .naval_construction_cost_define_errors
                .contains_key("GALLEY_COST"),
            "{cost}"
        );
        let error = world
            .query_naval_construction_cost(&query("AAA"))
            .unwrap_err();
        assert!(error.to_string().contains("GALLEY_COST"), "{cost}");
        assert!(error.location().is_some(), "{cost}");
    }
}

#[test]
fn missing_define_and_caller_inputs_remain_unresolved() {
    let temp = fixture();
    fs::remove_file(temp.path().join("game/common/defines/20_naval_cost.lua")).unwrap();
    let (_, world) = load_world(&temp, &[], Some("AAA"));

    let mut incomplete = query("AAA");
    incomplete.country_modifier_milli = None;
    incomplete.additional_country_modifier_milli = None;
    incomplete.province_modifier_milli = None;
    incomplete.naval_quantity_milli = None;
    incomplete.naval_limit_milli = None;
    let report = world.query_naval_construction_cost(&incomplete).unwrap();

    assert_eq!(report.evaluation, None);
    for unresolved in [
        "naval_construction_cost:missing_define:GALLEY_COST",
        "naval_construction_cost:missing_input:country_modifier_milli",
        "naval_construction_cost:missing_input:additional_country_modifier_milli",
        "naval_construction_cost:missing_input:province_modifier_milli",
        "naval_construction_cost:missing_input:naval_quantity_milli",
        "naval_construction_cost:missing_input:naval_limit_milli",
    ] {
        assert!(report.unresolved.iter().any(|entry| entry == unresolved));
    }
}

#[test]
fn over_limit_zero_limit_and_negative_quantities_never_produce_a_cost() {
    let temp = fixture();
    let patch = naval_cost_mod(&temp, "10");
    let (_, world) = load_world(&temp, &[patch], Some("AAA"));
    let before = world.save().unwrap();

    let mut over_limit = query("AAA");
    over_limit.naval_quantity_milli = Some(17_001);
    let report = world.query_naval_construction_cost(&over_limit).unwrap();
    assert_eq!(report.evaluation, None);
    assert!(
        report
            .unresolved
            .contains(&"naval_construction_cost:over_limit".into())
    );

    let mut zero_limit = query("AAA");
    zero_limit.naval_limit_milli = Some(0);
    let report = world.query_naval_construction_cost(&zero_limit).unwrap();
    assert_eq!(report.evaluation, None);
    assert!(
        report
            .unresolved
            .contains(&"naval_construction_cost:zero_naval_limit".into())
    );

    let mut negative_quantity = query("AAA");
    negative_quantity.naval_quantity_milli = Some(-1);
    assert!(
        world
            .query_naval_construction_cost(&negative_quantity)
            .unwrap_err()
            .to_string()
            .contains("naval quantity must be nonnegative")
    );

    let mut negative_limit = query("AAA");
    negative_limit.naval_limit_milli = Some(-1);
    assert!(
        world
            .query_naval_construction_cost(&negative_limit)
            .unwrap_err()
            .to_string()
            .contains("naval limit must be nonnegative")
    );
    assert_eq!(world.save().unwrap(), before);
}

#[test]
fn invalid_defines_are_deferred_until_query_identity_is_valid() {
    let temp = fixture();
    let patch = naval_cost_mod(&temp, "10.0005");
    let (_, mut world) = load_world(&temp, &[patch], Some("AAA"));
    assert!(
        world
            .report
            .setup
            .naval_construction_cost_define_errors
            .contains_key("GALLEY_COST")
    );

    let error = world
        .query_naval_construction_cost(&query("MISSING"))
        .unwrap_err();
    assert!(error.to_string().contains("unknown country MISSING"));
    assert!(!error.to_string().contains("GALLEY_COST"));

    let mut unknown_province = query("AAA");
    unknown_province.province_id = u32::MAX;
    let error = world
        .query_naval_construction_cost(&unknown_province)
        .unwrap_err();
    assert!(error.to_string().contains("unknown province"));
    assert!(!error.to_string().contains("GALLEY_COST"));

    let mut unknown_template = query("AAA");
    unknown_template.unit_type = "missing_galley".into();
    let error = world
        .query_naval_construction_cost(&unknown_template)
        .unwrap_err();
    assert!(error.to_string().contains("unknown unit template"));
    assert!(!error.to_string().contains("GALLEY_COST"));

    let mut non_naval = query("AAA");
    non_naval.unit_type = "test_infantry".into();
    let error = world.query_naval_construction_cost(&non_naval).unwrap_err();
    assert!(error.to_string().contains("is not naval"));
    assert!(!error.to_string().contains("GALLEY_COST"));

    world
        .initial
        .set_province_controller(1, Some("BBB"))
        .unwrap();
    let mismatched = world.query_naval_construction_cost(&query("AAA")).unwrap();
    assert_eq!(
        mismatched.unresolved,
        ["naval_construction_cost:controller_mismatch"]
    );

    world
        .initial
        .set_province_controller(1, Some("AAA"))
        .unwrap();
    let error = world
        .query_naval_construction_cost(&query("AAA"))
        .unwrap_err();
    assert!(error.to_string().contains("GALLEY_COST"));
    assert!(error.to_string().contains("20_naval_cost.lua"));
    assert!(error.location().is_some());
}

#[test]
fn unsupported_categories_and_owner_controller_mismatches_remain_unresolved() {
    let temp = fixture();
    let patch = naval_cost_mod(&temp, "10");
    let (_, mut world) = load_world(&temp, &[patch], Some("AAA"));

    let mut light_ship = query("AAA");
    light_ship.unit_type = "sample_light_ship".into();
    let unsupported = world.query_naval_construction_cost(&light_ship).unwrap();
    assert_eq!(unsupported.evaluation, None);
    assert_eq!(
        unsupported.unresolved,
        ["naval_construction_cost:unsupported_category"]
    );

    world
        .initial
        .set_province_controller(1, Some("BBB"))
        .unwrap();
    let before_query = world.save().unwrap();
    let mut negative_quantity = query("AAA");
    negative_quantity.naval_quantity_milli = Some(-1);
    assert!(
        world
            .query_naval_construction_cost(&negative_quantity)
            .unwrap_err()
            .to_string()
            .contains("naval quantity must be nonnegative")
    );
    let mut negative_limit = query("AAA");
    negative_limit.naval_limit_milli = Some(-1);
    assert!(
        world
            .query_naval_construction_cost(&negative_limit)
            .unwrap_err()
            .to_string()
            .contains("naval limit must be nonnegative")
    );
    let owner_mismatch = world.query_naval_construction_cost(&query("BBB")).unwrap();
    assert_eq!(
        owner_mismatch.unresolved,
        ["naval_construction_cost:owner_mismatch"]
    );
    let controller_mismatch = world.query_naval_construction_cost(&query("AAA")).unwrap();
    assert_eq!(
        controller_mismatch.unresolved,
        ["naval_construction_cost:controller_mismatch"]
    );
    assert_eq!(world.save().unwrap(), before_query);
}

#[test]
fn unknown_owner_and_controller_remain_unresolved() {
    let temp = fixture();
    let patch = naval_cost_mod(&temp, "10");
    let content = ContentSet::from_installation(
        temp.path().join("game"),
        &[patch],
        None,
        None,
        TextEncoding::Utf8,
    )
    .unwrap();
    let world = VanillaWorld::load_from_content(&content, "1444.11.11").unwrap();
    let unknown_controller = world.query_naval_construction_cost(&query("AAA")).unwrap();
    assert_eq!(unknown_controller.evaluation, None);
    assert!(
        unknown_controller
            .unresolved
            .contains(&"naval_construction_cost:controller_unknown".into())
    );

    let temp = fixture();
    let patch = naval_cost_mod(&temp, "10");
    let (_, mut world) = load_world(&temp, &[patch], Some("AAA"));
    world.initial.set_province_owner(1, None).unwrap();
    let unknown_owner = world.query_naval_construction_cost(&query("AAA")).unwrap();
    assert_eq!(unknown_owner.evaluation, None);
    assert!(
        unknown_owner
            .unresolved
            .contains(&"naval_construction_cost:owner_unknown_or_absent".into())
    );
}
