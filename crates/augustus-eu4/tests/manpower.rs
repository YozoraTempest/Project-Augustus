// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::*;
use augustus_eu4::{
    ScriptCompiler, ScriptLibrary, TextDocument, TextEncoding, VanillaExternalState, VanillaWorld,
};

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

#[test]
fn external_country_resources_are_bound_before_startup_effects() {
    use std::{collections::BTreeMap, fs, path::Path};

    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world");
    let root = tempfile::tempdir().unwrap();
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
        root.path().join("common/on_actions/resource_provider.txt"),
        "on_startup={if={limit={tag=AAA} add_manpower=1}}",
    )
    .unwrap();

    let resources = BTreeMap::from([(
        "AAA".into(),
        CountryResources {
            manpower_milli: Some(5_000),
            ..Default::default()
        },
    )]);
    let loaded =
        VanillaWorld::load_with_country_resources(root.path(), "1444.11.11", &resources).unwrap();
    let entity = loaded.initial.country_entity("AAA").unwrap();
    assert_eq!(
        loaded.initial.world().country_resources()[&entity].manpower_milli,
        Some(1_005_000)
    );
    assert!(
        !loaded
            .report
            .estates
            .startup_unresolved
            .iter()
            .any(|reason| reason.contains("manpower") && reason.contains("unknown"))
    );
}

#[test]
fn external_state_bundle_binds_resources_metrics_and_production_together() {
    use std::{
        collections::{BTreeMap, BTreeSet},
        fs,
        path::Path,
    };

    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world");
    let root = tempfile::tempdir().unwrap();
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
    let defines = fs::read_to_string(root.path().join("common/defines.lua")).unwrap();
    fs::write(
        root.path().join("common/defines.lua"),
        defines.replace(
            "NCountry = {",
            "NCountry = {\n ESTATE_START_CROWNLAND_INFLUENCE = 12,\n ESTATE_MIN_DISTRIBUTED_CROWNLAND = 40,",
        ),
    )
    .unwrap();
    fs::create_dir_all(root.path().join("common/estates")).unwrap();
    fs::write(
        root.path().join("common/estates/external.txt"),
        "estate_guild = { trigger = { always = yes } base_influence = 12 }",
    )
    .unwrap();

    let state = VanillaExternalState {
        country_metric_values: BTreeMap::from([(
            "AAA".into(),
            BTreeMap::from([("prestige".into(), Some(26_942))]),
        )]),
        country_national_focus: Default::default(),
        country_metric_ceilings: Default::default(),
        country_resources: BTreeMap::from([(
            "AAA".into(),
            CountryResources {
                treasury_milli: Some(1_234),
                manpower_milli: Some(5_000),
                mercantilism_milli: Some(18_500),
            },
        )]),
        production_inputs: BTreeMap::from([(
            2,
            VanillaProductionInputState {
                input: Some(VanillaProductionInput {
                    base_value_milli: 1_000,
                    country_modifier_milli: 0,
                    legacy_province_modifier_milli: 0,
                    queried_province_modifier_milli: 0,
                }),
                unresolved: Default::default(),
            },
        )]),
        production_value_inputs: Default::default(),
        trade_good_price_impacts_milli: Default::default(),
        country_annual_tax_milli: Default::default(),
        province_annual_tax_milli: Default::default(),
        province_raw_autonomy_milli: Default::default(),
        estate_privileges: BTreeMap::from([(
            "AAA".into(),
            BTreeMap::from([("estate_guild".into(), Some(BTreeSet::new()))]),
        )]),
        country_markers: Default::default(),
        country_statuses: Default::default(),
        estate_loyalty_targets: BTreeMap::from([(
            "AAA".into(),
            BTreeMap::from([("estate_guild".into(), Some(44_600))]),
        )]),
        country_monthly_cashflows_milli: Default::default(),
        country_monthly_power_income: Default::default(),
        armies: Default::default(),
        naval_units: Default::default(),
    };
    let encoded = serde_json::to_vec(&state).unwrap();
    let decoded: VanillaExternalState = serde_json::from_slice(&encoded).unwrap();
    let partial: VanillaExternalState =
        serde_json::from_str(r#"{"country_resources":{"AAA":{"manpower_milli":5000}}}"#).unwrap();
    assert_eq!(partial.country_resources["AAA"].treasury_milli, None);
    let mut loaded =
        VanillaWorld::load_with_external_state(root.path(), "1444.11.11", &decoded).unwrap();

    let country = loaded.country("AAA").unwrap();
    assert_eq!(country.resources.unwrap().treasury_milli, Some(1_234));
    assert_eq!(country.metrics.unwrap()["prestige"], Some(26_942));
    let province = loaded.initial.province_entity(2).unwrap();
    assert_eq!(
        loaded.initial.world().monthly_production().unwrap().inputs[&province]
            .input
            .unwrap()
            .base_value_milli,
        1_000
    );
    let country_entity = loaded.initial.country_entity("AAA").unwrap();
    assert_eq!(
        loaded.initial.world().estates().countries[&country_entity]["estate_guild"].privileges,
        Some(BTreeSet::new())
    );
    assert_eq!(
        loaded.initial.world().estates().countries[&country_entity]["estate_guild"]
            .loyalty_target_milli,
        Some(44_600)
    );
    loaded
        .set_estate_loyalty_targets(BTreeMap::from([(
            "AAA".into(),
            BTreeMap::from([("estate_guild".into(), Some(45_000))]),
        )]))
        .unwrap();
    assert_eq!(
        loaded.initial.world().estates().countries[&country_entity]["estate_guild"]
            .loyalty_target_milli,
        Some(45_000)
    );
    loaded.advance(30).unwrap();
    let production = loaded
        .initial
        .world()
        .monthly_production()
        .and_then(|state| state.last_settlement.as_ref())
        .unwrap();
    let production_account = &production.accounts[&country_entity];
    assert_eq!(production_account.annual_value_milli, Some(1_000));
    assert_eq!(production_account.monthly_value_milli, Some(83));
    let saved = loaded.save().unwrap();
    let mut resumed =
        VanillaWorld::load_with_external_state(root.path(), "1444.11.11", &decoded).unwrap();
    resumed.restore(&saved).unwrap();
    assert_eq!(resumed.save().unwrap(), saved);
}
