// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    Action, Condition, CountryController, CountryResource, EmptyWeightChoice, EntityId, EntityKind,
    Error, EventCatalog, EventChoice, EventDecisionSystem, EventDefinition, EventOption,
    EventWeight, FieldId, NumericOperation, NumericSlot, PendingEvent, PendingEventId, Schema,
    Target, WeightModifier, WeightedEventDecisions, World,
};
use std::{collections::BTreeMap, sync::Arc};

fn event(id: &str) -> EventDefinition {
    EventDefinition {
        id: id.into(),
        scope: EntityKind::Country,
        immediate: vec![Action::Set {
            target: Target::Current,
            field: FieldId(0),
            value: 1,
        }],
        options: vec![
            EventOption {
                id: "yes".into(),
                effects: vec![Action::Emit("yes".into())],
            },
            EventOption {
                id: "no".into(),
                effects: vec![Action::Emit("no".into())],
            },
        ],
        after: vec![Action::Emit("after".into())],
    }
}

fn gate() -> Condition {
    Condition::AtLeast {
        target: Target::Current,
        field: FieldId(0),
        value: 1,
    }
}
fn setup(controller: CountryController, empty: EmptyWeightChoice) -> World {
    let mut world = World::new(Schema::new(["gate"]).unwrap());
    let country = world.add_entity("Union", EntityKind::Country);
    world.set_country_controller(country, controller).unwrap();
    let catalog = EventCatalog::new([event("council")])
        .unwrap()
        .with_option_conditions(BTreeMap::from([(
            "council".into(),
            BTreeMap::from([
                ("yes".into(), gate()),
                ("no".into(), Condition::Not(Box::new(gate()))),
            ]),
        )]))
        .unwrap()
        .with_decision_system(Arc::new(WeightedEventDecisions { empty }))
        .unwrap();
    world.set_event_catalog(catalog).unwrap();
    world
}

#[test]
fn decision_is_captured_after_immediate_and_command_does_not_recheck_it() {
    let mut world = setup(CountryController::Ai, EmptyWeightChoice::FirstDeclared);
    world.trigger_event("council", EntityId(0)).unwrap();
    assert!(world.events().is_empty());
    assert_eq!(world.queued_event_choices()[0].option, "yes");
    world.set(EntityId(0), FieldId(0), 0).unwrap();
    world.process_event_commands().unwrap();
    assert_eq!(
        world
            .events()
            .iter()
            .map(|row| row.2.as_str())
            .collect::<Vec<_>>(),
        ["yes", "after"]
    );
    assert!(world.pending_events().is_empty());
    assert!(world.queued_event_choices().is_empty());
}

#[test]
fn queued_unknown_resource_adjustments_keep_the_base_unknown() {
    let mut world = World::new(Schema::new(["unused"]).unwrap());
    let country = world.add_entity("AAA", EntityKind::Country);
    world
        .set_country_controller(country, CountryController::Ai)
        .unwrap();
    world
        .set_event_catalog(
            EventCatalog::new([EventDefinition {
                id: "startup_recruitment".into(),
                scope: EntityKind::Country,
                immediate: Vec::new(),
                options: vec![EventOption {
                    id: "raise_troops".into(),
                    effects: vec![Action::UpdateNumeric {
                        target: Target::Current,
                        slot: NumericSlot::Resource(CountryResource::Manpower),
                        operation: NumericOperation::Add,
                        value: 1_000,
                    }],
                }],
                after: Vec::new(),
            }])
            .unwrap()
            .with_decision_system(Arc::new(WeightedEventDecisions {
                empty: EmptyWeightChoice::FirstDeclared,
            }))
            .unwrap(),
        )
        .unwrap();

    world.trigger_event("startup_recruitment", country).unwrap();
    world.process_event_commands().unwrap();

    let manpower = NumericSlot::Resource(CountryResource::Manpower);
    assert!(world.numeric_value(country, &manpower).is_err());
    assert_eq!(world.unbased_numeric_delta(country, &manpower), Some(1_000));
    assert!(world.pending_events().is_empty());
    assert!(world.queued_event_choices().is_empty());
}

#[test]
fn human_choices_remain_pending_without_consuming_randomness() {
    let mut world = setup(CountryController::Human, EmptyWeightChoice::FirstDeclared);
    let before: serde_json::Value = serde_json::from_str(&world.save("scene").unwrap()).unwrap();
    world.trigger_event("council", EntityId(0)).unwrap();
    world.advance_to(0).unwrap();
    assert_eq!(world.pending_events().len(), 1);
    assert!(world.queued_event_choices().is_empty());
    let after: serde_json::Value = serde_json::from_str(&world.save("scene").unwrap()).unwrap();
    assert_eq!(before["random"], after["random"]);
}

#[test]
fn zero_weights_have_an_explicit_content_policy() {
    for empty in [
        EmptyWeightChoice::KeepPending,
        EmptyWeightChoice::FirstDeclared,
    ] {
        let mut world = setup(CountryController::Ai, empty);
        let catalog = world
            .event_catalog()
            .clone()
            .with_option_weights(BTreeMap::from([(
                "council".into(),
                BTreeMap::from([(
                    "yes".into(),
                    EventWeight {
                        factor: 0,
                        ..Default::default()
                    },
                )]),
            )]))
            .unwrap();
        world.set_event_catalog(catalog).unwrap();
        world.trigger_event("council", EntityId(0)).unwrap();
        world.advance_to(0).unwrap();
        assert_eq!(
            world.pending_events().len(),
            usize::from(empty == EmptyWeightChoice::KeepPending)
        );
        assert_eq!(
            world.events().len(),
            if empty == EmptyWeightChoice::FirstDeclared {
                2
            } else {
                0
            }
        );
    }
}

#[test]
fn weighted_ai_keeps_events_pending_when_an_option_is_unavailable() {
    let mut world = setup(CountryController::Ai, EmptyWeightChoice::FirstDeclared);
    let catalog = world
        .event_catalog()
        .clone()
        .with_unavailable_options(BTreeMap::from([(
            "council".into(),
            BTreeMap::from([("no".into(), "unsupported effect update_sprite_pack".into())]),
        )]))
        .unwrap();
    world.set_event_catalog(catalog).unwrap();

    let before: serde_json::Value = serde_json::from_str(&world.save("scene").unwrap()).unwrap();
    world.trigger_event("council", EntityId(0)).unwrap();

    assert!(world.queued_event_choices().is_empty());
    assert_eq!(world.pending_events().len(), 1);
    assert_eq!(
        world
            .available_event_options(PendingEventId(0))
            .unwrap()
            .iter()
            .map(|option| option.id.as_str())
            .collect::<Vec<_>>(),
        ["yes"]
    );
    assert_eq!(
        world
            .event_catalog()
            .unavailable_option_reason("council", "no"),
        Some("unsupported effect update_sprite_pack")
    );

    let after: serde_json::Value = serde_json::from_str(&world.save("scene").unwrap()).unwrap();
    assert_eq!(before["random"], after["random"]);
    let before_invalid_choice = world.save("scene").unwrap();
    assert!(
        world
            .choose_event(&EventChoice {
                pending: PendingEventId(0),
                option: "no".into(),
            })
            .unwrap_err()
            .to_string()
            .contains("unavailable")
    );
    assert_eq!(world.save("scene").unwrap(), before_invalid_choice);
}

#[test]
fn declared_first_policy_preserves_an_unavailable_option_identifier() {
    let mut world = setup(CountryController::Ai, EmptyWeightChoice::FirstDeclared);
    let conditions = BTreeMap::from([(
        "council".into(),
        BTreeMap::from([
            ("yes".into(), Condition::Always(false)),
            ("no".into(), Condition::Always(false)),
        ]),
    )]);
    world
        .set_event_catalog(
            world
                .event_catalog()
                .clone()
                .with_option_conditions(conditions)
                .unwrap(),
        )
        .unwrap();
    world.trigger_event("council", EntityId(0)).unwrap();
    assert_eq!(world.queued_event_choices()[0].option, "yes");
    world.process_event_commands().unwrap();
    assert_eq!(world.events()[0].2, "yes");
}

#[test]
fn queued_choice_and_random_stream_restore_together() {
    let mut world = setup(CountryController::Ai, EmptyWeightChoice::FirstDeclared);
    world
        .set_event_catalog(
            world
                .event_catalog()
                .clone()
                .with_option_conditions(BTreeMap::new())
                .unwrap(),
        )
        .unwrap();
    world.trigger_event("council", EntityId(0)).unwrap();
    let saved = world.save("scene").unwrap();
    let mut restored = World::restore_with_events(
        &saved,
        "scene",
        world.schema(),
        Vec::new(),
        world.event_catalog().clone(),
    )
    .unwrap();
    assert_eq!(restored, world);
    world.process_event_commands().unwrap();
    restored.process_event_commands().unwrap();
    for _ in 0..16 {
        world.trigger_event("council", EntityId(0)).unwrap();
        restored.trigger_event("council", EntityId(0)).unwrap();
        world.process_event_commands().unwrap();
        restored.process_event_commands().unwrap();
    }
    assert_eq!(
        world.save("scene").unwrap(),
        restored.save("scene").unwrap()
    );
}

#[test]
fn malformed_saved_commands_are_rejected_before_resume() {
    let mut world = setup(CountryController::Ai, EmptyWeightChoice::FirstDeclared);
    world.trigger_event("council", EntityId(0)).unwrap();
    let data: serde_json::Value = serde_json::from_str(&world.save("scene").unwrap()).unwrap();
    for invalid in [
        serde_json::json!([{"pending": 99, "option": "yes"}]),
        serde_json::json!([{"pending": 0, "option": "missing"}]),
        serde_json::json!([{"pending": 0, "option": "yes"}, {"pending": 0, "option": "yes"}]),
    ] {
        let mut damaged = data.clone();
        damaged["event_queue"]["choices"] = invalid;
        assert!(
            World::restore_with_events(
                &damaged.to_string(),
                "scene",
                world.schema(),
                Vec::new(),
                world.event_catalog().clone()
            )
            .is_err()
        );
    }
}

#[test]
fn an_accepted_choice_cannot_be_overwritten_or_executed_twice() {
    let mut world = setup(CountryController::Ai, EmptyWeightChoice::FirstDeclared);
    world.trigger_event("council", EntityId(0)).unwrap();
    let before = world.save("scene").unwrap();
    assert!(
        world
            .choose_event(&EventChoice {
                pending: PendingEventId(0),
                option: "no".into()
            })
            .is_err()
    );
    assert!(
        world
            .queue_event_choice(world.queued_event_choices()[0].clone())
            .is_err()
    );
    assert_eq!(world.save("scene").unwrap(), before);
    let choice = world.queued_event_choices()[0].clone();
    world.choose_event(&choice).unwrap();
    world.process_event_commands().unwrap();
    assert_eq!(world.events().len(), 2);
}

#[test]
fn fixed_point_weight_factors_use_live_conditions_order_and_zero_truncation() {
    let mut world = setup(CountryController::Human, EmptyWeightChoice::KeepPending);
    let weight = EventWeight {
        base: -5,
        unit: 1000,
        factor: -1000,
        modifiers: vec![
            WeightModifier {
                condition: gate(),
                factor: 500,
            },
            WeightModifier {
                condition: Condition::Always(true),
                factor: 500,
            },
        ],
    };
    assert_eq!(weight.evaluate(&world, EntityId(0)).unwrap(), 2);
    world.set(EntityId(0), FieldId(0), 1).unwrap();
    assert_eq!(weight.evaluate(&world, EntityId(0)).unwrap(), 1);
    let zero = EventWeight {
        base: 1,
        unit: 1000,
        factor: 10000,
        modifiers: vec![
            WeightModifier {
                condition: Condition::Always(true),
                factor: 100,
            },
            WeightModifier {
                condition: Condition::Always(true),
                factor: 10000,
            },
        ],
    };
    assert_eq!(zero.evaluate(&world, EntityId(0)).unwrap(), 0);
}

#[test]
fn whole_selection_system_can_be_replaced_without_replacing_effect_execution() {
    #[derive(Debug)]
    struct AlwaysNo;
    impl EventDecisionSystem for AlwaysNo {
        fn content_key(&self) -> String {
            "test-always-no:1".into()
        }
        fn select(&self, _: &mut World, _: &PendingEvent) -> Result<Option<String>, Error> {
            Ok(Some("no".into()))
        }
    }
    let mut world = setup(CountryController::Ai, EmptyWeightChoice::FirstDeclared);
    world
        .set_event_catalog(
            world
                .event_catalog()
                .clone()
                .with_decision_system(Arc::new(AlwaysNo))
                .unwrap(),
        )
        .unwrap();
    world.trigger_event("council", EntityId(0)).unwrap();
    world.advance_to(0).unwrap();
    assert_eq!(world.events()[0].2, "no");
    assert_eq!(world.events()[1].2, "after");
}

#[test]
fn unbounded_nested_choices_stop_at_one_shared_instruction_budget() {
    let mut world = setup(CountryController::Ai, EmptyWeightChoice::FirstDeclared);
    let mut definition = event("loop");
    definition.options.truncate(1);
    definition.options[0].effects = vec![Action::TriggerEvent {
        event: "loop".into(),
    }];
    world
        .set_event_catalog(
            EventCatalog::new([definition])
                .unwrap()
                .with_decision_system(Arc::new(WeightedEventDecisions::default()))
                .unwrap(),
        )
        .unwrap();
    world.trigger_event("loop", EntityId(0)).unwrap();
    assert!(world.process_event_commands().is_err());
    assert!(world.save("scene").is_err());
}
