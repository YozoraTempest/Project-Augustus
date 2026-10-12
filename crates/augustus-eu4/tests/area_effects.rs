// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::*;
use augustus_eu4::{ScriptCompiler, ScriptLibrary, TextDocument, TextEncoding, VanillaWorld};
use std::{collections::BTreeMap, fs, path::Path};

fn document(text: &str) -> TextDocument {
    TextDocument::from_bytes(
        "self-authored-area-effects.txt",
        text.as_bytes().to_vec(),
        TextEncoding::Utf8,
    )
}
fn compiler<'a>(
    schema: &'a Schema,
    library: &'a ScriptLibrary,
    optimise: bool,
) -> ScriptCompiler<'a> {
    ScriptCompiler::new(schema, library, optimise)
        .with_area_scopes(BTreeMap::from([("test_area".into(), vec![3, 1, 2])]))
        .with_metric_ids(["devastation".into()].into())
}
fn world() -> World {
    let mut world = World::new(Schema::new(["unused"]).unwrap());
    let owner = world.add_entity("AAA", EntityKind::Country);
    for id in 1..=3 {
        let province = world.add_entity(id.to_string(), EntityKind::Province);
        world.set_owner(province, owner).unwrap();
    }
    world
        .set_markers(Markers {
            entities: [(owner, Default::default())].into(),
            ..Default::default()
        })
        .unwrap();
    world
        .set_metrics(Metrics {
            definitions: [(
                "devastation".into(),
                MetricDefinition {
                    kind: EntityKind::Province,
                    minimum: 0,
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
            entities: (1..=3)
                .map(|id| (EntityId(id), [("devastation".into(), Some(0))].into()))
                .collect(),
            ceilings: Default::default(),
        })
        .unwrap();
    world
}

#[test]
fn translation_preserves_live_filters_declared_order_and_threshold_precision() {
    let text="country_event={id=area.start is_triggered_only=yes immediate={set_country_flag=gate
        test_area={limit={owner={has_country_flag=gate}} log=member owner={clr_country_flag=gate}}
        test_area={add_devastation=500 add_devastation=-50.999 if={limit={devastation=50 NOT={devastation=50.001}} log=exact}}
        1={add_devastation=-1000 add_devastation=12.999 if={limit={devastation=12 NOT={devastation=12.001}} log=integer}}
        log=return} option={name=ok}}";
    let mut outputs = vec![];
    for optimise in [false, true] {
        let mut world = world();
        let compiled = compiler(world.schema(), &ScriptLibrary::new(), optimise)
            .compile_events(&document(text))
            .unwrap();
        world
            .set_event_catalog(EventCatalog::new(compiled.definitions).unwrap())
            .unwrap();
        world.trigger_event("area.start", EntityId(0)).unwrap();
        assert_eq!(
            world
                .events()
                .iter()
                .map(|row| (row.1, row.2.as_str()))
                .collect::<Vec<_>>(),
            [
                (EntityId(3), "member"),
                (EntityId(3), "exact"),
                (EntityId(1), "exact"),
                (EntityId(2), "exact"),
                (EntityId(1), "integer"),
                (EntityId(0), "return")
            ]
        );
        assert_eq!(
            world
                .numeric_value(EntityId(1), &NumericSlot::Metric("devastation".into()))
                .unwrap(),
            12_000
        );
        outputs.push(world.save("area").unwrap());
    }
    assert_eq!(outputs[0], outputs[1]);
}

#[test]
fn invalid_scope_missing_metric_and_duplicate_limit_keep_source_diagnostics() {
    let schema = Schema::new(["unused"]).unwrap();
    let library = ScriptLibrary::new();
    for text in [
        "country_event={id=x immediate={add_devastation=10}}",
        "country_event={id=x trigger={devastation=1}}",
        "country_event={id=x trigger={province_id=1}}",
        "country_event={id=x immediate={test_area={limit={always=yes} limit={always=no}}}}",
        "country_event={id=x immediate={test_area={add_devastation=1.0001}}}",
        "province_event={id=x trigger={province_id=0}}",
        "country_event={id=x immediate={unknown_area={log=bad}}}",
    ] {
        let error = compiler(&schema, &library, true)
            .compile_events(&document(text))
            .unwrap_err();
        assert!(error.location().is_some(), "{error}");
        assert!(error.path().ends_with("self-authored-area-effects.txt"));
    }
    assert!(
        ScriptCompiler::new(&schema, &library, true)
            .compile_events(&document(
                "province_event={id=x immediate={add_devastation=1}}"
            ))
            .is_err()
    );
}

fn fixture() -> tempfile::TempDir {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world");
    let temp = tempfile::tempdir().unwrap();
    for entry in walkdir::WalkDir::new(&root) {
        let entry = entry.unwrap();
        let path = temp.path().join(entry.path().strip_prefix(&root).unwrap());
        if entry.file_type().is_dir() {
            fs::create_dir_all(path).unwrap();
        } else {
            fs::copy(entry.path(), path).unwrap();
        }
    }
    temp
}
fn write(root: &Path, name: &str, text: &str) {
    let path = root.join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

#[test]
fn bootstrap_retains_map_order_including_sea_and_discovers_nested_event_calls() {
    let temp = fixture();
    write(temp.path(), "map/area.txt", "test_area={2 1 3}");
    write(
        temp.path(),
        "common/on_actions/test.txt",
        "on_startup={events={area.start}}",
    );
    write(
        temp.path(),
        "events/test.txt",
        "country_event={id=area.start trigger={tag=AAA} is_triggered_only=yes immediate={test_area={log=member add_devastation=50 province_event={id=area.later days=1}} log=returned} option={name=ok}} province_event={id=area.later is_triggered_only=yes immediate={if={limit={devastation=50} log=later}} option={name=ok}}",
    );
    let mut loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(
        loaded.report.estates.startup_unresolved.is_empty(),
        "{:?}",
        loaded.report.estates.startup_unresolved
    );
    let ids = [2, 1, 3].map(|id| loaded.initial.province_entity(id).unwrap());
    let world = loaded.initial.world();
    assert_eq!(
        world
            .events()
            .iter()
            .filter(|row| row.2 == "member")
            .map(|row| row.1)
            .collect::<Vec<_>>(),
        ids
    );
    for entity in ids {
        assert_eq!(
            world
                .numeric_value(entity, &NumericSlot::Metric("devastation".into()))
                .unwrap(),
            50_000
        );
    }
    assert_eq!(
        world.events().last().unwrap().1,
        loaded.initial.country_entity("AAA").unwrap()
    );
    let saved = loaded.save().unwrap();
    let mut restored = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    restored.restore(&saved).unwrap();
    loaded.advance(1).unwrap();
    restored.advance(1).unwrap();
    assert_eq!(loaded.save().unwrap(), restored.save().unwrap());
    assert_eq!(
        loaded
            .initial
            .world()
            .events()
            .iter()
            .filter(|row| row.2 == "later")
            .map(|row| row.1)
            .collect::<Vec<_>>(),
        ids
    );
}

#[test]
fn startup_rollback_and_unimplemented_history_never_publish_zero_as_known_state() {
    let temp = fixture();
    write(
        temp.path(),
        "common/on_actions/test.txt",
        "on_startup={north_area={add_devastation=50} unknown_effect=yes}",
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(!loaded.report.estates.startup_unresolved.is_empty());
    assert!(
        loaded
            .initial
            .world()
            .metrics()
            .entities
            .values()
            .all(|values| values.values().all(Option::is_none))
    );
    fs::remove_file(temp.path().join("common/on_actions/test.txt")).unwrap();
    let path = temp.path().join("history/provinces/1 - North.txt");
    let previous = fs::read_to_string(&path).unwrap();
    fs::write(path, previous + "\nadd_devastation=10\n").unwrap();
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let first = loaded.initial.province_entity(1).unwrap();
    let second = loaded.initial.province_entity(2).unwrap();
    assert!(
        loaded
            .initial
            .world()
            .numeric_value(first, &NumericSlot::Metric("devastation".into()))
            .is_err()
    );
    assert_eq!(
        loaded
            .initial
            .world()
            .numeric_value(second, &NumericSlot::Metric("devastation".into()))
            .unwrap(),
        0
    );
    assert!(
        loaded
            .report
            .unapplied_province_fields
            .contains_key("add_devastation")
    );
    let later = VanillaWorld::load(temp.path(), "1500.1.1").unwrap();
    assert!(
        later
            .initial
            .world()
            .metrics()
            .entities
            .values()
            .all(|values| values.values().all(Option::is_none))
    );
    let path = temp.path().join("history/countries/AAA - Alpha.txt");
    let previous = fs::read_to_string(&path).unwrap();
    fs::write(path, previous + "\nnorth_area={add_devastation=5}\n").unwrap();
    let scoped = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(
        scoped
            .initial
            .world()
            .metrics()
            .entities
            .iter()
            .filter(
                |(entity, _)| scoped.initial.world().entity(**entity).unwrap().kind
                    == EntityKind::Province
            )
            .all(|(_, values)| values["devastation"].is_none())
    );
    assert_eq!(
        scoped.country("AAA").unwrap().metrics.unwrap()["stability"],
        Some(0)
    );
}
