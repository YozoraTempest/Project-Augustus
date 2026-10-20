// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::*;
use augustus_eu4::{ScriptCompiler, ScriptLibrary, TextDocument, TextEncoding};

fn document(text: &str) -> TextDocument {
    TextDocument::from_bytes("prestige.txt", text.as_bytes().to_vec(), TextEncoding::Utf8)
}

fn compiler<'a>(
    schema: &'a Schema,
    library: &'a ScriptLibrary,
    optimize: bool,
) -> ScriptCompiler<'a> {
    ScriptCompiler::new(schema, library, optimize).with_metric_ids(["prestige".into()].into())
}

fn world() -> (World, EntityId, EntityId) {
    let mut world = World::new(Schema::new(["unused"]).unwrap());
    let aaa = world.add_entity("AAA", EntityKind::Country);
    let bbb = world.add_entity("BBB", EntityKind::Country);
    world
        .set_metrics(Metrics {
            definitions: [(
                "prestige".into(),
                MetricDefinition {
                    kind: EntityKind::Country,
                    minimum: -100_000,
                    maximum: 100_000,
                    baseline: 0,
                    divisor: 100_000,
                    modifier: None,
                    unresolved_properties: Default::default(),
                    below_baseline: None,
                    growth_ceiling: false,
                },
            )]
            .into(),
            entities: [
                (aaa, [("prestige".into(), Some(26_942))].into()),
                (bbb, [("prestige".into(), Some(5_000))].into()),
            ]
            .into(),
            ceilings: Default::default(),
        })
        .unwrap();
    (world, aaa, bbb)
}

#[test]
fn prestige_effects_keep_thousandths_bounds_and_live_country_comparisons() {
    for optimize in [false, true] {
        let schema = Schema::new(["unused"]).unwrap();
        let library = ScriptLibrary::new();
        let (mut world, aaa, bbb) = world();
        let plan = compiler(&schema, &library, optimize)
            .compile(
                &document("add_prestige=-1000 add_prestige=100 add_prestige=0.999 add_prestige=0.0005 add_prestige=0.0005 if={limit={prestige=0.999 NOT={prestige=1}} log=quantized} add_prestige=1000 if={limit={prestige=100} log=ceiling} BBB={if={limit={NOT={prestige=ROOT}} log=country}}"),
                EntityKind::Country,
            )
            .unwrap();
        world.execute(aaa, &plan.plan).unwrap();
        assert_eq!(
            world
                .numeric_value(aaa, &NumericSlot::Metric("prestige".into()))
                .unwrap(),
            100_000
        );
        assert_eq!(
            world
                .events()
                .iter()
                .map(|event| event.2.as_str())
                .collect::<Vec<_>>(),
            ["quantized", "ceiling", "country"]
        );
        assert_eq!(
            world
                .numeric_value(bbb, &NumericSlot::Metric("prestige".into()))
                .unwrap(),
            5_000
        );
    }
}

#[test]
fn unknown_initial_prestige_does_not_become_zero() {
    let schema = Schema::new(["unused"]).unwrap();
    let library = ScriptLibrary::new();
    let mut world = World::new(schema.clone());
    let aaa = world.add_entity("AAA", EntityKind::Country);
    world
        .set_metrics(Metrics {
            definitions: [(
                "prestige".into(),
                MetricDefinition {
                    kind: EntityKind::Country,
                    minimum: -100_000,
                    maximum: 100_000,
                    baseline: 0,
                    divisor: 100_000,
                    modifier: None,
                    unresolved_properties: Default::default(),
                    below_baseline: None,
                    growth_ceiling: false,
                },
            )]
            .into(),
            entities: [(aaa, [("prestige".into(), None)].into())].into(),
            ceilings: Default::default(),
        })
        .unwrap();
    let plan = compiler(&schema, &library, true)
        .compile(&document("add_prestige=1"), EntityKind::Country)
        .unwrap();
    world.execute(aaa, &plan.plan).unwrap();
    let prestige = NumericSlot::Metric("prestige".into());
    assert!(world.numeric_value(aaa, &prestige).is_err());
    assert_eq!(world.unbased_numeric_delta(aaa, &prestige), Some(1_000));
}

#[test]
fn unsupported_operands_scopes_and_unregistered_storage_keep_source_diagnostics() {
    let schema = Schema::new(["unused"]).unwrap();
    let library = ScriptLibrary::new();
    assert!(
        ScriptCompiler::new(&schema, &library, true)
            .compile(&document("add_prestige=1"), EntityKind::Country)
            .is_err()
    );
    for text in [
        "add_prestige=ROOT",
        "add_prestige={value=1}",
        "add_prestige=0.0000001",
        "capital_scope={add_prestige=1}",
        "capital_scope={if={limit={prestige=1} log=bad}}",
    ] {
        let error = compiler(&schema, &library, true)
            .compile(&document(text), EntityKind::Country)
            .unwrap_err();
        assert!(error.to_string().contains("prestige.txt:1:"));
    }
}

#[test]
fn original_entry_keeps_unknown_initial_prestige_and_tracks_startup_changes() {
    use augustus_eu4::VanillaWorld;
    use std::{fs, path::Path};
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world");
    let root = tempfile::tempdir().unwrap();
    for entry in walkdir::WalkDir::new(source.clone()) {
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
    let path = root.path().join("common/on_actions/control.txt");
    fs::write(&path, "on_startup={if={limit={tag=AAA} add_prestige=1}}").unwrap();
    let loaded = VanillaWorld::load(root.path(), "1444.11.11").unwrap();
    assert!(
        loaded
            .initial
            .world()
            .metrics()
            .entities
            .iter()
            .filter(
                |(entity, _)| loaded.initial.world().entity(**entity).unwrap().kind
                    == EntityKind::Country
            )
            .all(|(_, values)| values.get("prestige") == Some(&None))
    );
    assert!(loaded.report.estates.startup_unresolved.is_empty());
    let aaa = loaded.initial.country_entity("AAA").unwrap();
    let prestige = NumericSlot::Metric("prestige".into());
    assert!(
        loaded
            .initial
            .world()
            .numeric_value(aaa, &prestige)
            .is_err()
    );
    assert_eq!(
        loaded.initial.world().unbased_numeric_delta(aaa, &prestige),
        Some(1_000)
    );
    let saved = loaded.save().unwrap();
    fs::write(path, "on_startup={if={limit={tag=AAA} add_prestige=2}}").unwrap();
    let mut changed = VanillaWorld::load(root.path(), "1444.11.11").unwrap();
    assert!(changed.restore(&saved).is_err());
}

#[test]
fn external_country_metric_values_bind_atomically_and_restore_as_state() {
    use augustus_eu4::VanillaWorld;
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

    let mut loaded = VanillaWorld::load(root.path(), "1444.11.11").unwrap();
    loaded
        .set_country_metric_values(BTreeMap::from([
            (
                "AAA".into(),
                BTreeMap::from([("prestige".into(), Some(26_942))]),
            ),
            ("BBB".into(), BTreeMap::from([("prestige".into(), None)])),
        ]))
        .unwrap();
    assert_eq!(
        loaded.initial.world().metrics().entities[&loaded.initial.country_entity("AAA").unwrap()]["prestige"],
        Some(26_942)
    );
    assert_eq!(
        loaded.initial.world().metrics().entities[&loaded.initial.country_entity("BBB").unwrap()]["prestige"],
        None
    );

    let snapshot = loaded.save().unwrap();
    let mut restored = VanillaWorld::load(root.path(), "1444.11.11").unwrap();
    restored.restore(&snapshot).unwrap();
    assert_eq!(restored.initial.world(), loaded.initial.world());
}

#[test]
fn external_country_metric_values_reject_unknown_names_without_partial_writes() {
    use augustus_eu4::VanillaWorld;
    use std::{collections::BTreeMap, path::Path};

    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world");
    let mut loaded = VanillaWorld::load(source, "1444.11.11").unwrap();
    let before = loaded.initial.world().clone();
    let error = loaded
        .set_country_metric_values(BTreeMap::from([(
            "AAA".into(),
            BTreeMap::from([("missing_metric".into(), Some(1))]),
        )]))
        .unwrap_err()
        .to_string();
    assert!(error.contains("unknown metric missing_metric"));
    assert_eq!(loaded.initial.world(), &before);
}

#[test]
fn external_country_metric_values_are_bound_before_startup_effects() {
    use augustus_eu4::VanillaWorld;
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
        root.path().join("common/on_actions/metric_provider.txt"),
        "on_startup={if={limit={tag=AAA} add_prestige=1}}",
    )
    .unwrap();

    let values = BTreeMap::from([(
        "AAA".into(),
        BTreeMap::from([("prestige".into(), Some(26_942))]),
    )]);
    let loaded =
        VanillaWorld::load_with_country_metric_values(root.path(), "1444.11.11", &values).unwrap();
    let entity = loaded.initial.country_entity("AAA").unwrap();
    assert_eq!(
        loaded.initial.world().metrics().entities[&entity]["prestige"],
        Some(27_942)
    );
    assert!(
        !loaded
            .report
            .estates
            .startup_unresolved
            .iter()
            .any(|reason| reason.contains("prestige") && reason.contains("unknown"))
    );
}
