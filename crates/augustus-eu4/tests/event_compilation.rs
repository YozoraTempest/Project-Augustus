// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    Action, EntityId, EntityKind, EventCatalog, EventChoice, Politics, Schema, World,
};
use augustus_eu4::{ScriptCompiler, ScriptLibrary, TextDocument, TextEncoding};

fn document(text: &str) -> TextDocument {
    TextDocument::from_bytes(
        "self-authored-events.txt",
        text.as_bytes().to_vec(),
        TextEncoding::Utf8,
    )
}

#[test]
fn event_call_fields_compile_to_direct_calls_or_existing_scheduled_plans() {
    let schema = Schema::new(["treasury"]).unwrap();
    let library = ScriptLibrary::new();
    let compiled=ScriptCompiler::new(&schema,&library,true).compile(&document("country_event={id=probe.1} country_event={days=0 id=probe.2} country_event={id=probe.3 days=2} capital_scope={province_event={id=probe.4}}"),EntityKind::Country).unwrap();
    assert!(matches!(&compiled.plan[0],Action::TryEvent { event } if event=="probe.1"));
    assert!(matches!(&compiled.plan[1],Action::TryEvent { event } if event=="probe.2"));
    assert!(
        matches!(&compiled.plan[2],Action::Schedule { delay:2,plan } if matches!(&plan[0],Action::TryEvent {event} if event=="probe.3"))
    );
}

#[test]
fn set_emperor_compiles_only_for_country_scope_with_yes_value() {
    let schema = Schema::new([] as [&str; 0]).unwrap();
    let compiled = ScriptCompiler::new(&schema, &ScriptLibrary::new(), true)
        .compile(&document("set_emperor = yes"), EntityKind::Country)
        .unwrap();
    assert!(matches!(
        compiled.plan.as_slice(),
        [Action::SetOfficeHolder { office }] if office == "imperial_emperor"
    ));

    let mut world = World::new(schema.clone());
    let previous = world.add_entity("AAA", EntityKind::Country);
    let current = world.add_entity("BBB", EntityKind::Country);
    world
        .set_politics(Politics {
            offices: [("imperial_emperor".into(), previous)].into(),
            ..Default::default()
        })
        .unwrap();
    world.execute(current, &compiled.plan).unwrap();
    assert_eq!(
        world.politics().offices.get("imperial_emperor"),
        Some(&current)
    );

    for (text, scope) in [
        ("set_emperor = no", EntityKind::Country),
        ("set_emperor = yes", EntityKind::Province),
    ] {
        assert!(
            ScriptCompiler::new(&schema, &ScriptLibrary::new(), true)
                .compile(&document(text), scope)
                .is_err()
        );
    }
}

#[test]
fn hidden_event_metadata_is_validated_without_changing_simulation_effects() {
    let schema = Schema::new([] as [&str; 0]).unwrap();
    let compiled = ScriptCompiler::new(&schema, &ScriptLibrary::new(), true)
        .compile_events(&document(
            "country_event={id=probe.hidden is_triggered_only=yes hidden=yes title=none desc=none picture=none immediate={log=hidden_effect} option={ai_chance={factor=50} log=first} option={ai_chance={factor=50} log=second}}",
        ))
        .unwrap();
    assert!(matches!(
        compiled.definitions[0].immediate.as_slice(),
        [Action::Emit(message)] if message == "hidden_effect"
    ));
    assert!(ScriptCompiler::new(&schema, &ScriptLibrary::new(), true)
        .compile_events(&document(
            "country_event={id=probe.hidden is_triggered_only=yes hidden=no option={log=unnamed_option}}",
        ))
        .is_err());
    assert!(ScriptCompiler::new(&schema, &ScriptLibrary::new(), true)
        .compile_events(&document(
            "country_event={id=probe.hidden is_triggered_only=yes hidden=maybe option={name=accept}}",
        ))
        .is_err());
}

#[test]
fn once_policy_survives_compilation_and_call_simplification() {
    let schema = Schema::new(["treasury"]).unwrap();
    let library = ScriptLibrary::new();
    let text = document(
        "country_event={id=probe.once is_triggered_only=yes fire_only_once=yes immediate={log=once} option={name=accept country_event={id=probe.once}}} country_event={id=probe.repeat is_triggered_only=yes fire_only_once=no immediate={country_event={id=probe.once} country_event={id=probe.once}} option={name=accept}}",
    );
    let mut saved = Vec::new();
    for optimise in [false, true] {
        let compiled = ScriptCompiler::new(&schema, &library, optimise)
            .compile_events(&text)
            .unwrap();
        assert_eq!(compiled.once_events, ["probe.once".into()].into());
        let repeat = &compiled.definitions[1].immediate[0];
        if optimise {
            assert!(matches!(repeat, Action::TriggerEvent { .. }));
        } else {
            assert!(matches!(repeat, Action::TryEvent { .. }));
        }
        let catalog = EventCatalog::new(compiled.definitions)
            .unwrap()
            .with_conditions(compiled.conditions)
            .unwrap()
            .with_once_events(compiled.once_events)
            .unwrap();
        let mut world = World::new(schema.clone());
        let country = world.add_entity("Union", EntityKind::Country);
        world.set_event_catalog(catalog).unwrap();
        world.trigger_event("probe.repeat", country).unwrap();
        world
            .choose_event(&EventChoice {
                pending: world.pending_events()[0].id,
                option: "0".into(),
            })
            .unwrap();
        assert_eq!(world.events().len(), 1);
        assert_eq!(world.pending_events().len(), 1);
        assert_eq!(world.fired_events().len(), 1);
        saved.push(world.save("compiled-once").unwrap());
    }
    assert_eq!(saved[0], saved[1]);
}

#[test]
fn unsupported_event_call_parameters_and_wrong_scopes_have_source_locations() {
    let schema = Schema::new(["treasury"]).unwrap();
    let library = ScriptLibrary::new();
    for text in [
        "country_event={}",
        "country_event={id=\"\"}",
        "country_event={id=x id=y}",
        "country_event={id=x days=-1}",
        "country_event={id=x days=1.5}",
        "country_event={id=x days=1 days=2}",
        "country_event={id=x random=1}",
        "country_event={id=x hours=1}",
        "country_event=x",
        "province_event={id=x}",
    ] {
        let error = ScriptCompiler::new(&schema, &library, true)
            .compile(&document(text), EntityKind::Country)
            .unwrap_err();
        assert!(error.location().is_some(), "{text}");
    }
}

#[test]
fn event_phases_share_script_calls_scopes_and_live_conditions() {
    let schema = Schema::new(["treasury", "variable/phase"]).unwrap();
    let mut library = ScriptLibrary::new();
    library
        .add_effects(&document(
            "prepare={set_variable={which=phase value=1} log=immediate}",
        ))
        .unwrap();
    let text = document(
        "namespace=probe country_event={id=probe.1 is_triggered_only=yes immediate={prepare=yes} option={name=accept if={limit={check_variable={which=phase value=1}} set_variable={which=phase value=2}} log=option} after={capital_scope={set_variable={which=phase value=3}} log=after}} province_event={id=probe.2 is_triggered_only=yes option={name=accept owner={add_treasury=0.25}}}",
    );
    let compiled = ScriptCompiler::new(&schema, &library, true)
        .compile_events(&text)
        .unwrap();
    assert!(compiled.sources.iter().any(|(_, source)| source.line == 1));
    let mut world = World::new(schema.clone());
    let country = world.add_entity("Union", EntityKind::Country);
    let province = world.add_entity("Port", EntityKind::Province);
    world.set_owner(province, country).unwrap();
    world.set_capital(country, province).unwrap();
    world
        .set_event_catalog(EventCatalog::new(compiled.definitions).unwrap())
        .unwrap();
    let first = world.trigger_event("probe.1", country).unwrap();
    assert_eq!(world.events().len(), 1);
    world
        .choose_event(&EventChoice {
            pending: first,
            option: "0".into(),
        })
        .unwrap();
    assert_eq!(
        world
            .value(country, schema.field("variable/phase").unwrap())
            .unwrap(),
        2000
    );
    assert_eq!(
        world
            .value(province, schema.field("variable/phase").unwrap())
            .unwrap(),
        3000
    );
    assert_eq!(
        world
            .events()
            .iter()
            .map(|event| event.2.as_str())
            .collect::<Vec<_>>(),
        ["immediate", "option", "after"]
    );
    let second = world.trigger_event("probe.2", province).unwrap();
    world
        .choose_event(&EventChoice {
            pending: second,
            option: "0".into(),
        })
        .unwrap();
    assert_eq!(
        world
            .value(EntityId(0), schema.field("treasury").unwrap())
            .unwrap(),
        250
    );
}

#[test]
fn unsupported_event_semantics_are_diagnostics_even_in_unselected_options() {
    let schema = Schema::new(["treasury"]).unwrap();
    let library = ScriptLibrary::new();
    for fragment in [
        "trigger={unsupported_trigger=yes}",
        "mean_time_to_happen={months=10 modifier={factor=1 unsupported_trigger=yes}}",
        "fire_only_once=invalid",
        "fire_only_once=yes fire_only_once=no",
        "option={name=other ai_chance={unknown=1} log=unsupported}",
        "option={name=other trigger={always=no unsupported_trigger=yes} log=unsupported}",
        "option={name=other ROOT={unsupported_effect=yes}}",
        "immediate={country_event={id=probe.2 random=1}}",
        "desc={text=dynamic}",
        "id=duplicate",
    ] {
        let text = format!(
            "\ncountry_event={{id=probe.1 is_triggered_only=yes option={{name=accept log=ok}} {fragment}}}"
        );
        let error = ScriptCompiler::new(&schema, &library, true)
            .compile_events(&document(&text))
            .unwrap_err();
        assert!(error.location().is_some());
        assert_eq!(error.path().to_string_lossy(), "self-authored-events.txt");
    }
    for text in [
        "country_event={id=x option={name=a}}",
        "country_event={id=x is_triggered_only=no option={name=a}}",
        "country_event={id=x is_triggered_only=yes}",
        "country_event={id=x is_triggered_only=yes option={log=x}}",
    ] {
        assert!(
            ScriptCompiler::new(&schema, &library, true)
                .compile_events(&document(text))
                .is_err()
        );
    }
}

#[test]
fn constant_folding_changes_the_plan_without_changing_event_behavior() {
    let schema = Schema::new(["treasury"]).unwrap();
    let library = ScriptLibrary::new();
    let text = document(
        "country_event={id=probe.1 is_triggered_only=yes option={name=a if={limit={always=yes} log=selected} else={log=incorrect}} after={log=after}}",
    );
    let mut results = Vec::new();
    for optimise in [false, true] {
        let compiled = ScriptCompiler::new(&schema, &library, optimise)
            .compile_events(&text)
            .unwrap();
        assert_eq!(compiled.folded_branches, usize::from(optimise));
        let mut world = World::new(schema.clone());
        let country = world.add_entity("Union", EntityKind::Country);
        world
            .set_event_catalog(EventCatalog::new(compiled.definitions).unwrap())
            .unwrap();
        let pending = world.trigger_event("probe.1", country).unwrap();
        world
            .choose_event(&EventChoice {
                pending,
                option: "0".into(),
            })
            .unwrap();
        results.push(world.events().to_vec());
    }
    assert_eq!(results[0], results[1]);
}
