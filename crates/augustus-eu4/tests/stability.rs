// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{EntityId, EntityKind, EventChoice, PendingEventId, Schema, World};
use augustus_eu4::{ScriptCompiler, ScriptLibrary, TextDocument, TextEncoding, VanillaWorld};
use std::{fs, path::Path};

fn document(text: &str) -> TextDocument {
    TextDocument::from_bytes(
        "self-authored-stability.txt",
        text.as_bytes().to_vec(),
        TextEncoding::Utf8,
    )
}

fn world(schema: Schema) -> World {
    let mut world = World::new(schema);
    world.add_entity("AAA", EntityKind::Country);
    world.add_entity("Port", EntityKind::Province);
    world.set_owner(EntityId(1), EntityId(0)).unwrap();
    world.set_capital(EntityId(0), EntityId(1)).unwrap();
    world.set_metrics(serde_json::from_str(r#"{
        "definitions":{"stability":{"kind":"Country","minimum":-3,"maximum":3,
            "baseline":0,"divisor":1,"modifier":{"values_micros":{"global_tax_modifier":125000},"flags":{}},"unresolved_properties":[],
            "below_baseline":{"modifier":{"values_micros":{"global_tax_modifier":75000},"flags":{}},"unresolved_properties":[]}}},
        "entities":{"0":{"stability":0}}
    }"#).unwrap()).unwrap();
    world
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
fn literal_rounding_clamps_and_owner_queries_use_one_integer_state_in_both_modes() {
    let schema = Schema::new(["unused"]).unwrap();
    let library = ScriptLibrary::new();
    let text = document(
        r#"
        if={limit={stability=0.5 NOT={stability=1}} log=zero}
        add_stability=-100 add_stability=0.125 add_stability=0.875
        if={limit={stability=-3.001 NOT={stability=-2.999}} log=minimum}
        add_stability=2.125 add_stability=0.999
        if={limit={stability=-1 NOT={stability=0}} log=negative}
        add_stability=100 add_stability=-0.125 add_stability=-0.875
        if={limit={stability=3.001 NOT={stability=4}} log=maximum}
        add_stability=-2.125 add_stability=-0.999
        capital_scope={owner={if={limit={stability=1.5 NOT={stability=2}} log=owner}}}
    "#,
    );
    let mut saves = Vec::new();
    for optimise in [false, true] {
        let plan = ScriptCompiler::new(&schema, &library, optimise)
            .with_metric_ids(["stability".into()].into())
            .compile(&text, EntityKind::Country)
            .unwrap()
            .plan;
        let mut world = world(schema.clone());
        world.execute(EntityId(0), &plan).unwrap();
        assert_eq!(
            world
                .metrics()
                .value(&world, EntityId(0), "stability")
                .unwrap(),
            1
        );
        assert_eq!(
            world
                .events()
                .iter()
                .map(|e| e.2.as_str())
                .collect::<Vec<_>>(),
            ["zero", "minimum", "negative", "maximum", "owner"]
        );
        saves.push(world.save("stability").unwrap());
    }
    assert_eq!(saves[0], saves[1]);
}

#[test]
fn unsupported_operands_and_scope_fail_with_source_and_unknown_state_is_preserved() {
    let schema = Schema::new(["unused"]).unwrap();
    let library = ScriptLibrary::new();
    for optimise in [false, true] {
        for text in [
            "add_stability=ROOT",
            "add_stability={value=1}",
            "add_stability=1.0001",
            "add_stability=2147483648",
            "add_stability=-2147483649",
            "if={limit={stability=AAA} log=bad}",
            "capital_scope={add_stability=1}",
            "capital_scope={if={limit={stability=0} log=bad}}",
        ] {
            let error = ScriptCompiler::new(&schema, &library, optimise)
                .with_metric_ids(["stability".into()].into())
                .compile(&document(text), EntityKind::Country)
                .unwrap_err();
            assert!(error.location().is_some(), "{error}");
        }
    }
    assert!(
        ScriptCompiler::new(&schema, &library, true)
            .compile(&document("add_stability=1"), EntityKind::Country)
            .is_err()
    );
    let mut world = world(schema.clone());
    world
        .set_metric_value(EntityId(0), "stability", None)
        .unwrap();
    let before = world.clone();
    for text in ["add_stability=1", "if={limit={stability=0} log=bad}"] {
        let mut world = before.clone();
        let plan = ScriptCompiler::new(&schema, &library, true)
            .with_metric_ids(["stability".into()].into())
            .compile(&document(text), EntityKind::Country)
            .unwrap()
            .plan;
        assert!(world.execute(EntityId(0), &plan).is_err());
        assert_eq!(world.metrics(), before.metrics());
        assert!(world.events().is_empty());
        assert!(world.save("failed-stability").is_err());
    }
}

#[test]
fn startup_and_event_choices_feed_owned_attributes_and_survive_save_resume() {
    let temp = fixture();
    write(
        temp.path(),
        "common/on_actions/stability.txt",
        r#"on_startup={if={limit={tag=AAA}
        add_stability=2.999 if={limit={stability=2.5 NOT={stability=3}} log=startup}
        country_event={id=confidence.begin}}}"#,
    );
    write(
        temp.path(),
        "events/stability.txt",
        r#"
        country_event={id=confidence.begin is_triggered_only=yes immediate={add_stability=1}
            option={name=accept add_stability=-4 country_event={id=confidence.later days=1}}
            after={if={limit={stability=-1 NOT={stability=0}} log=after}}}
        country_event={id=confidence.later is_triggered_only=yes trigger={stability=-1}
            immediate={add_stability=2} option={name=accept} after={log=later}}
    "#,
    );
    let mut loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(
        loaded.report.estates.pending_startup_effects.is_empty(),
        "{:?}",
        loaded.report.estates.startup_unresolved
    );
    let country = loaded.initial.country_entity("AAA").unwrap();
    let province = loaded.initial.province_entity(2).unwrap();
    let contribution = |state: &VanillaWorld| {
        state
            .initial
            .world()
            .attributes()
            .evaluate(state.initial.world(), province, "tax_basis")
            .unwrap()
            .steps
            .iter()
            .flat_map(|step| &step.contributions)
            .find(|item| item.source == "metric:stability")
            .unwrap()
            .micros
    };
    assert_eq!(
        loaded.country("AAA").unwrap().metrics.unwrap()["stability"],
        Some(3)
    );
    assert_eq!(contribution(&loaded), 375000);
    loaded
        .initial
        .choose_event(&EventChoice {
            pending: PendingEventId(0),
            option: "0".into(),
        })
        .unwrap();
    assert_eq!(contribution(&loaded), -75000);
    let saved = loaded.save().unwrap();
    let mut resumed = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    resumed.restore(&saved).unwrap();
    for state in [&mut loaded, &mut resumed] {
        state.advance(1).unwrap();
        state
            .initial
            .choose_event(&EventChoice {
                pending: PendingEventId(1),
                option: "0".into(),
            })
            .unwrap();
        assert_eq!(
            state
                .initial
                .world()
                .metrics()
                .value(state.initial.world(), country, "stability")
                .unwrap(),
            1
        );
        assert_eq!(contribution(state), 125000);
        assert_eq!(
            state
                .initial
                .world()
                .events()
                .iter()
                .map(|e| e.2.as_str())
                .collect::<Vec<_>>(),
            ["startup", "after", "later"]
        );
    }
    assert_eq!(loaded.save().unwrap(), resumed.save().unwrap());
}

#[test]
fn unverified_history_missing_definitions_and_failed_startup_do_not_invent_values() {
    let cases = [
        ("history/countries/AAA - Test.txt", "stability=2.125", false),
        (
            "history/countries/AAA - Test.txt",
            "1444.11.10={add_stability=2}",
            false,
        ),
        (
            "history/countries/AAA - Test.txt",
            "capital_scope={owner={add_stability=1}}",
            true,
        ),
        (
            "history/provinces/1 - Test.txt",
            "effect={owner={add_stability=1}}",
            true,
        ),
    ];
    for (path, text, all) in cases {
        let temp = fixture();
        // Append to the selected file, preserving the self-authored history prerequisites.
        let selected = if path.starts_with("history/countries") {
            fs::read_dir(temp.path().join("history/countries"))
                .unwrap()
                .map(|e| e.unwrap().path())
                .find(|p| p.file_name().unwrap().to_string_lossy().starts_with("AAA"))
                .unwrap()
        } else {
            fs::read_dir(temp.path().join("history/provinces"))
                .unwrap()
                .map(|e| e.unwrap().path())
                .find(|p| p.file_name().unwrap().to_string_lossy().starts_with("1 "))
                .unwrap()
        };
        let previous = fs::read_to_string(&selected).unwrap();
        fs::write(selected, format!("{previous}\n{text}\n")).unwrap();
        let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
        assert_eq!(
            loaded.country("AAA").unwrap().metrics.unwrap()["stability"],
            None
        );
        if all {
            assert_eq!(
                loaded.country("BBB").unwrap().metrics.unwrap()["stability"],
                None
            );
        }
    }
    let temp = fixture();
    write(
        temp.path(),
        "common/on_actions/stability.txt",
        "on_startup={add_stability=1 unsupported_effect=yes}",
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert_eq!(
        loaded.country("AAA").unwrap().metrics.unwrap()["stability"],
        None
    );
    assert!(!loaded.report.estates.pending_startup_effects.is_empty());
    let later = VanillaWorld::load(fixture().path(), "1450.1.1").unwrap();
    assert_eq!(
        later.country("AAA").unwrap().metrics.unwrap()["stability"],
        None
    );
}

#[test]
fn ambiguous_or_absent_stability_modifiers_leave_their_active_properties_unresolved() {
    let temp = fixture();
    write(
        temp.path(),
        "common/static_modifiers/duplicate.txt",
        "negative_stability={global_tax_modifier=0.2}",
    );
    let mut loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let country = loaded.initial.country_entity("AAA").unwrap();
    loaded
        .initial
        .execute(
            country,
            &[augustus_core::Action::UpdateNumeric {
                target: augustus_core::Target::Current,
                slot: augustus_core::NumericSlot::Metric("stability".into()),
                operation: augustus_core::NumericOperation::Set,
                value: -1,
            }],
        )
        .unwrap();
    let world = loaded.initial.world();
    assert!(
        world
            .metrics()
            .property_effects(world, country, "global_tax_modifier")
            .unwrap()
            .unresolved
            .contains("metric:stability:unresolved:global_tax_modifier")
    );
    assert!(
        loaded
            .report
            .attributes
            .ambiguous_definitions
            .contains_key("static_modifiers:negative_stability")
    );
}

#[test]
fn initial_country_and_capital_owner_conditions_read_registered_stability() {
    let temp = fixture();
    write(
        temp.path(),
        "common/estates/confidence.txt",
        "estate_confidence={trigger={stability=0.5 NOT={stability=1} capital_scope={owner={stability=0.5}}}}",
    );
    write(
        temp.path(),
        "common/ideas/confidence.txt",
        "confidence_ideas={free=yes start={} trigger={tag=AAA stability=0.5 NOT={stability=1}} bonus={} idea={global_tax_modifier=0.1}}",
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let country = loaded.country("AAA").unwrap();
    assert_eq!(
        country.estates.unwrap()["estate_confidence"].active,
        Some(true)
    );
    assert!(
        country
            .ideas
            .unwrap()
            .groups
            .as_ref()
            .unwrap()
            .contains_key("confidence_ideas")
    );
    assert_eq!(country.metrics.unwrap()["stability"], Some(0));
}

#[test]
fn duplicate_missing_and_indirect_history_keep_initial_values_unknown() {
    for case in 0..3 {
        let temp = fixture();
        let selected = temp.path().join("history/countries/AAA - Alpha.txt");
        match case {
            0 => {
                fs::copy(
                    &selected,
                    temp.path().join("history/countries/AAA - Duplicate.txt"),
                )
                .unwrap();
            }
            1 => {
                fs::remove_file(selected).unwrap();
            }
            _ => {
                write(
                    temp.path(),
                    "common/scripted_effects/confidence.txt",
                    "confidence_effect={add_stability=1}",
                );
                let text = fs::read_to_string(&selected).unwrap();
                fs::write(selected, format!("{text}\nconfidence_effect=yes\n")).unwrap();
            }
        }
        let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
        assert_eq!(
            loaded.country("AAA").unwrap().metrics.unwrap()["stability"],
            None
        );
        if case == 2 {
            assert_eq!(
                loaded.country("BBB").unwrap().metrics.unwrap()["stability"],
                None
            );
        }
    }
}

#[test]
fn unrelated_history_effect_does_not_hide_initial_stability() {
    let temp = fixture();
    write(
        temp.path(),
        "common/scripted_effects/unrelated.txt",
        "unrelated_country_setup={add_prestige=1}",
    );
    let selected = temp.path().join("history/countries/AAA - Alpha.txt");
    let previous = fs::read_to_string(&selected).unwrap();
    fs::write(
        selected,
        format!("{previous}\nunrelated_country_setup=yes\n"),
    )
    .unwrap();

    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert_eq!(
        loaded.country("AAA").unwrap().metrics.unwrap()["stability"],
        Some(0)
    );
    assert_eq!(
        loaded.country("BBB").unwrap().metrics.unwrap()["stability"],
        Some(0)
    );
}
