// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{EntityId, EntityKind, Schema, World};
use augustus_eu4::{ScriptCompiler, ScriptLibrary, TextDocument, TextEncoding, VanillaWorld};
use std::{fs, path::Path};

fn document(text: &str) -> TextDocument {
    TextDocument::from_bytes(
        "self-authored-metrics.txt",
        text.as_bytes().to_vec(),
        TextEncoding::Utf8,
    )
}

fn world(schema: Schema) -> World {
    let mut world = World::new(schema);
    world.add_entity("AAA", EntityKind::Country);
    world.add_entity("BBB", EntityKind::Country);
    world.add_entity("Port", EntityKind::Province);
    world.set_owner(EntityId(2), EntityId(0)).unwrap();
    world.set_capital(EntityId(0), EntityId(2)).unwrap();
    world.set_governments(serde_json::from_str(r#"{
        "reforms":{}, "metrics":{"legitimacy":{"minimum_milli":0,"maximum_milli":100000,"baseline_milli":50000,"divisor_milli":100000,"modifier":{"values_micros":{},"flags":{}},"unresolved_properties":[]}},
        "countries":{"0":{"kind":"monarchy","reforms":[],"metrics":{"legitimacy":100000}},"1":{"kind":"republic","reforms":[],"metrics":{}}}
    }"#).unwrap()).unwrap();
    world
}

#[test]
fn scoped_fractional_operations_and_inactive_queries_use_shared_numeric_state() {
    let schema = Schema::new(["unused"]).unwrap();
    let library = ScriptLibrary::new();
    let script = document(
        r#"
        add_legitimacy=-24.875
        if={limit={legitimacy=75.125 NOT={legitimacy=75.126}} log=fraction}
        add_legitimacy=1000000 add_legitimacy=-1000000 add_legitimacy=10
        add_legitimacy=0.125 add_legitimacy=0.125 add_legitimacy=0.125
        capital_scope={owner={if={limit={legitimacy=10.375 NOT={legitimacy=10.376}} log=owner}}}
        add_legitimacy=49.625
        if={limit={legitimacy=60 NOT={legitimacy=60.001}} log=sixty}
    "#,
    );
    let mut snapshots = Vec::new();
    for optimise in [false, true] {
        let plan = ScriptCompiler::new(&schema, &library, optimise)
            .with_government_metric_ids(["legitimacy".into()].into())
            .compile(&script, EntityKind::Country)
            .unwrap()
            .plan;
        let mut world = world(schema.clone());
        world.execute(EntityId(0), &plan).unwrap();
        assert_eq!(
            world
                .government_metric_value(EntityId(0), "legitimacy")
                .unwrap(),
            60_000
        );
        assert_eq!(
            world
                .events()
                .iter()
                .map(|e| e.2.as_str())
                .collect::<Vec<_>>(),
            ["fraction", "owner", "sixty"]
        );
        let inactive=ScriptCompiler::new(&schema,&library,optimise).with_government_metric_ids(["legitimacy".into()].into()).compile(&document("add_legitimacy=-30 add_legitimacy=100 if={limit={legitimacy=0 NOT={legitimacy=0.001}} log=inactive}"),EntityKind::Country).unwrap().plan;
        world.execute(EntityId(1), &inactive).unwrap();
        assert!(
            world.governments().countries[&EntityId(1)]
                .metrics
                .as_ref()
                .unwrap()
                .is_empty()
        );
        assert_eq!(world.events().last().unwrap().2, "inactive");
        snapshots.push(world.save("government-metrics").unwrap());
    }
    assert_eq!(snapshots[0], snapshots[1]);
}

#[test]
fn unsupported_metric_forms_fail_with_source_and_unknown_activation_is_preserved() {
    let schema = Schema::new(["unused"]).unwrap();
    let library = ScriptLibrary::new();
    for text in [
        "add_legitimacy={value=10}",
        "add_legitimacy=ROOT",
        "add_legitimacy=1.0001",
        "capital_scope={add_legitimacy=10}",
        "if={limit={legitimacy=100.001} log=bad}",
        "if={limit={legitimacy=ROOT} log=bad}",
        "add_republican_tradition=10",
    ] {
        let error = ScriptCompiler::new(&schema, &library, true)
            .with_government_metric_ids(["legitimacy".into()].into())
            .compile(&document(text), EntityKind::Country)
            .unwrap_err();
        assert!(error.location().is_some(), "{error}");
    }
    assert!(
        ScriptCompiler::new(&schema, &library, true)
            .compile(&document("add_legitimacy=10"), EntityKind::Country)
            .is_err()
    );
    let plan = ScriptCompiler::new(&schema, &library, true)
        .with_government_metric_ids(["legitimacy".into()].into())
        .compile(&document("add_legitimacy=10"), EntityKind::Country)
        .unwrap()
        .plan;
    let mut world = world(schema);
    let mut governments = world.governments().clone();
    governments.countries.get_mut(&EntityId(0)).unwrap().metrics = None;
    world.set_governments(governments).unwrap();
    assert!(world.execute(EntityId(0), &plan).is_err());
    assert!(
        world.governments().countries[&EntityId(0)]
            .metrics
            .is_none()
    );
}

fn fixture() -> tempfile::TempDir {
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
    temp
}
fn write(root: &Path, name: &str, text: &str) {
    let path = root.join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

#[test]
fn startup_events_and_selection_conditions_share_metric_state_across_restore() {
    let temp = fixture();
    write(
        temp.path(),
        "common/estates/metrics.txt",
        "estate_metric={trigger={legitimacy=75.125 NOT={legitimacy=75.126}}}",
    );
    write(
        temp.path(),
        "common/ideas/metrics.txt",
        "metric_ideas={free=yes start={} trigger={tag=AAA legitimacy=75.125} bonus={} idea={global_tax_modifier=0.1}}",
    );
    write(
        temp.path(),
        "common/on_actions/metrics.txt",
        r#"on_startup={
        if={limit={tag=AAA} add_legitimacy=-24.875
            if={limit={legitimacy=50.25 NOT={legitimacy=50.251}} log=startup}
            country_event={id=metric.active} add_legitimacy=2
        }
        if={limit={tag=BBB} add_legitimacy=20 if={limit={legitimacy=0 NOT={legitimacy=0.001}} log=inactive}}
    }"#,
    );
    write(
        temp.path(),
        "events/metrics.txt",
        r#"
        country_event={id=metric.active is_triggered_only=yes immediate={add_legitimacy=49.625}
            option={name=accept add_legitimacy=-89.875 country_event={id=metric.delayed days=1}}
            after={if={limit={legitimacy=10.125 NOT={legitimacy=10.126}} log=after}}}
        country_event={id=metric.delayed is_triggered_only=yes trigger={legitimacy=10.125}
            immediate={add_legitimacy=0.25} option={name=accept} after={log=delayed}}
    "#,
    );
    let mut loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(
        loaded.report.estates.pending_startup_effects.is_empty(),
        "{:?}",
        loaded.report.estates.startup_unresolved
    );
    let country = loaded.initial.country_entity("AAA").unwrap();
    assert_eq!(
        loaded
            .initial
            .world()
            .government_metric_value(country, "legitimacy")
            .unwrap(),
        100_000
    );
    assert_eq!(
        loaded.country("AAA").unwrap().estates.unwrap()["estate_metric"].active,
        Some(true)
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
            .contains_key("metric_ideas")
    );
    let saved = loaded.save().unwrap();
    let mut resumed = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    resumed.restore(&saved).unwrap();
    for state in [&mut loaded, &mut resumed] {
        state
            .initial
            .choose_event(&augustus_core::EventChoice {
                pending: augustus_core::PendingEventId(0),
                option: "0".into(),
            })
            .unwrap();
        state.advance(1).unwrap();
        state
            .initial
            .choose_event(&augustus_core::EventChoice {
                pending: augustus_core::PendingEventId(1),
                option: "0".into(),
            })
            .unwrap();
        assert_eq!(
            state
                .initial
                .world()
                .government_metric_value(country, "legitimacy")
                .unwrap(),
            10_375
        );
        assert_eq!(
            state
                .initial
                .world()
                .events()
                .iter()
                .map(|e| e.2.as_str())
                .collect::<Vec<_>>(),
            ["startup", "inactive", "after", "delayed"]
        );
    }
    assert_eq!(loaded.save().unwrap(), resumed.save().unwrap());
}

#[test]
fn incomplete_startup_discards_metric_changes_and_marks_active_values_unknown() {
    let temp = fixture();
    write(
        temp.path(),
        "common/on_actions/metrics.txt",
        "on_startup={if={limit={tag=AAA} add_legitimacy=-10 unimplemented_effect=yes}}",
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert_eq!(loaded.report.estates.pending_startup_effects.len(), 1);
    let world = loaded.initial.world();
    let country = loaded.initial.country_entity("AAA").unwrap();
    assert!(
        world
            .government_metric_active(country, "legitimacy")
            .unwrap()
    );
    assert!(
        world
            .government_metric_value(country, "legitimacy")
            .is_err()
    );
    assert!(
        !world
            .government_metric_active(loaded.initial.country_entity("BBB").unwrap(), "legitimacy")
            .unwrap()
    );
}
