// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    Calendar, CountryResources, EntityId, EntityKind, Schema, Taxation, World, WorldDate,
};

fn scenario() -> World {
    let mut world = World::new(Schema::new(["base"]).unwrap());
    world.add_entity("A", EntityKind::Country);
    world.add_entity("B", EntityKind::Country);
    world.add_entity("P", EntityKind::Province);
    world.set_owner(EntityId(2), EntityId(0)).unwrap();
    world.set_ideas(serde_json::from_str(r#"{
        "groups":{"first":{"traditions":{"modifier":{"values_micros":{"yield":125000},"flags":{}},"unresolved_properties":[]},
            "ideas":[{"id":"one","effect":{"modifier":{"values_micros":{"yield":250000},"flags":{}},"unresolved_properties":[]}},
                {"id":"two","effect":{"modifier":{"values_micros":{"yield":-62500,"other":100000},"flags":{}},"unresolved_properties":[]}}],
            "completion":{"modifier":{"values_micros":{"yield":500000},"flags":{}},"unresolved_properties":[]}}},
        "countries":{"0":{"groups":{"first":0}},"1":{"groups":{}}}
    }"#).unwrap()).unwrap();
    world
        .set_attributes(
            serde_json::from_str(
                r#"{
        "static_modifiers":{},"building_modifiers":{},"trade_good_modifiers":{},
        "formulas":{"2":{"annual":{"base":{"kind":"constant","micros":12000000},
            "channels":[{"flat":[],"rates":[{"kind":"ideas","target":"Owner","property":"yield"}]}],
            "minimum_micros":null,"maximum_micros":null,"unresolved":[]}}}
    }"#,
            )
            .unwrap(),
        )
        .unwrap();
    world
}

fn progress(world: &mut World, count: Option<u16>) {
    let mut ideas = world.ideas().clone();
    ideas
        .countries
        .get_mut(&EntityId(0))
        .unwrap()
        .groups
        .as_mut()
        .unwrap()
        .insert("first".into(), count);
    world.set_ideas(ideas).unwrap();
}

fn value(world: &World) -> augustus_core::AttributeEvaluation {
    world
        .attributes()
        .evaluate(world, EntityId(2), "annual")
        .unwrap()
}

#[test]
fn traditions_ordered_unlocks_completion_and_owner_changes_use_live_state() {
    let mut world = scenario();
    assert_eq!(value(&world).value_micros, Some(13_500_000));
    progress(&mut world, Some(1));
    assert_eq!(value(&world).value_micros, Some(16_500_000));
    assert_eq!(value(&world).steps[0].contributions.len(), 2);
    progress(&mut world, Some(2));
    assert_eq!(value(&world).value_micros, Some(21_750_000));
    assert!(
        value(&world).steps[0]
            .contributions
            .iter()
            .any(|c| c.source == "ideas:completion:first")
    );
    world.set_owner(EntityId(2), EntityId(1)).unwrap();
    assert_eq!(value(&world).value_micros, Some(12_000_000));
}

#[test]
fn unknown_progress_selection_and_properties_preserve_known_sources() {
    let mut world = scenario();
    progress(&mut world, None);
    assert_eq!(value(&world).known_subtotal_micros, Some(13_500_000));
    assert_eq!(value(&world).value_micros, None);
    assert!(
        value(&world)
            .unresolved
            .contains("ideas:first:unknown_progress")
    );
    assert!(
        world
            .ideas()
            .property_effects(EntityId(0), "absent")
            .unwrap()
            .unresolved
            .is_empty()
    );
    progress(&mut world, Some(0));
    let mut ideas = world.ideas().clone();
    ideas.groups.get_mut("first").unwrap().ideas[0]
        .effect
        .unresolved_properties
        .insert("yield".into());
    ideas.groups.get_mut("first").unwrap().ideas[0]
        .effect
        .modifier
        .values_micros
        .remove("yield");
    world.set_ideas(ideas.clone()).unwrap();
    assert!(value(&world).unresolved.is_empty());
    progress(&mut world, Some(1));
    assert!(
        value(&world)
            .unresolved
            .contains("ideas:idea:first:one:unresolved:yield")
    );
    ideas.countries.get_mut(&EntityId(0)).unwrap().groups = None;
    world.set_ideas(ideas.clone()).unwrap();
    assert!(value(&world).unresolved.contains("ideas:unknown_selection"));
    ideas.countries.get_mut(&EntityId(0)).unwrap().groups =
        Some([("missing".into(), Some(0))].into());
    world.set_ideas(ideas).unwrap();
    assert!(
        value(&world)
            .unresolved
            .contains("ideas:missing:missing_definition")
    );
}

#[test]
fn has_idea_queries_progress_and_preserves_unknown_selection_or_progress() {
    let mut world = scenario();
    let selected = EntityId(0);
    let unselected = EntityId(1);
    let condition = |id: &str| augustus_core::Condition::HasIdea {
        target: augustus_core::Target::Current,
        id: id.into(),
    };
    assert!(!condition("one").evaluate(&world, selected).unwrap());
    assert!(!condition("one").evaluate(&world, unselected).unwrap());
    progress(&mut world, Some(1));
    assert!(condition("one").evaluate(&world, selected).unwrap());
    assert!(!condition("two").evaluate(&world, selected).unwrap());
    assert!(condition("unknown").evaluate(&world, selected).is_err());

    progress(&mut world, None);
    assert!(condition("one").evaluate(&world, selected).is_err());

    let mut ideas = world.ideas().clone();
    ideas.countries.get_mut(&selected).unwrap().groups = None;
    world.set_ideas(ideas).unwrap();
    assert!(condition("one").evaluate(&world, selected).is_err());
}

#[test]
fn unexecuted_unlock_scripts_taint_arbitrary_properties_only_when_relevant() {
    let mut world = scenario();
    let mut ideas = world.ideas().clone();
    ideas.groups.get_mut("first").unwrap().ideas[0]
        .effect
        .unresolved_effects
        .insert("custom_unlock".into());
    world.set_ideas(ideas).unwrap();
    assert!(value(&world).unresolved.is_empty());
    progress(&mut world, None);
    assert!(
        world
            .ideas()
            .property_effects(EntityId(0), "unlisted_property")
            .unwrap()
            .unresolved
            .contains("ideas:first:unknown_progress")
    );
    progress(&mut world, Some(1));
    assert!(
        world
            .ideas()
            .property_effects(EntityId(0), "unlisted_property")
            .unwrap()
            .unresolved
            .contains("ideas:idea:first:one:unexecuted:custom_unlock")
    );
    assert_eq!(value(&world).value_micros, None);
    assert_eq!(value(&world).known_subtotal_micros, Some(16_500_000));
}

#[test]
fn invalid_replacements_and_corrupt_progress_leave_state_intact() {
    let mut world = scenario();
    let before = world.clone();
    let mut bad = world.ideas().clone();
    bad.countries.get_mut(&EntityId(0)).unwrap().groups = Some([("first".into(), Some(3))].into());
    assert!(world.set_ideas(bad).is_err());
    let mut bad = world.ideas().clone();
    bad.countries.insert(EntityId(2), Default::default());
    assert!(world.set_ideas(bad).is_err());
    let mut bad = world.ideas().clone();
    bad.groups.get_mut("first").unwrap().ideas[1].id = "one".into();
    assert!(world.set_ideas(bad).is_err());
    let mut bad = world.ideas().clone();
    let m = &mut bad.groups.get_mut("first").unwrap().traditions.modifier;
    m.values_micros.remove("yield");
    m.flags.insert("yield".into(), true);
    assert!(world.set_ideas(bad).is_err());
    let mut bad = world.ideas().clone();
    bad.countries.get_mut(&EntityId(0)).unwrap().groups = Some([("first".into(), Some(1))].into());
    bad.groups
        .get_mut("first")
        .unwrap()
        .traditions
        .modifier
        .values_micros
        .insert("yield".into(), i64::MAX);
    assert!(world.set_ideas(bad).is_err());
    assert_eq!(world, before);
    let mut saved: serde_json::Value = serde_json::from_str(&world.save("ideas").unwrap()).unwrap();
    saved["ideas"]["countries"]["0"]["groups"]["first"] = serde_json::json!(3);
    assert!(World::restore(&saved.to_string(), "ideas", world.schema()).is_err());
}

#[test]
fn multiple_groups_stack_and_missing_component_in_existing_saves_remains_readable() {
    let mut world = scenario();
    let mut ideas = world.ideas().clone();
    ideas
        .groups
        .insert("second".into(), ideas.groups["first"].clone());
    ideas
        .countries
        .get_mut(&EntityId(0))
        .unwrap()
        .groups
        .as_mut()
        .unwrap()
        .insert("second".into(), Some(1));
    world.set_ideas(ideas).unwrap();
    assert_eq!(value(&world).value_micros, Some(18_000_000));
    let saved = world.save("ideas").unwrap();
    assert_eq!(
        World::restore(&saved, "ideas", world.schema()).unwrap(),
        world
    );
    let empty = World::new(Schema::new(["base"]).unwrap());
    let saved = empty.save("old").unwrap();
    assert!(!saved.contains("ideas"));
    assert_eq!(
        World::restore(&saved, "old", empty.schema()).unwrap(),
        empty
    );
}

#[test]
fn changing_progress_changes_real_monthly_credit_and_resumed_run_matches() {
    let mut world = scenario();
    world
        .set_country_resources(
            [(
                EntityId(0),
                CountryResources {
                    treasury_milli: Some(10_000),
                    ..Default::default()
                },
            )]
            .into(),
        )
        .unwrap();
    world
        .set_calendar(Calendar {
            start: WorldDate {
                year: 1444,
                month: 11,
                day: 11,
            },
            month_lengths: [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31],
        })
        .unwrap();
    world
        .set_taxation(Taxation::new("annual", [EntityId(0)].into()))
        .unwrap();
    world.advance_to(20).unwrap();
    assert_eq!(
        world.country_resources()[&EntityId(0)].treasury_milli,
        Some(11_125)
    );
    progress(&mut world, Some(1));
    let saved = world.save("fiscal-ideas").unwrap();
    let mut resumed = World::restore(&saved, "fiscal-ideas", world.schema()).unwrap();
    world.advance_to(51).unwrap();
    resumed.advance_to(51).unwrap();
    assert_eq!(
        world.country_resources()[&EntityId(0)].treasury_milli,
        Some(12_500)
    );
    assert_eq!(world, resumed);
    progress(&mut world, None);
    world.advance_to(82).unwrap();
    let receipt = &world
        .taxation()
        .unwrap()
        .last_settlement
        .as_ref()
        .unwrap()
        .accounts[&EntityId(0)];
    assert_eq!(receipt.income_milli, None);
    assert_eq!(receipt.credited_milli, None);
    assert_eq!(world.country_resources()[&EntityId(0)].treasury_milli, None);
}
