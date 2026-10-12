// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    Action, Condition, EntityId, EntityKind, Governments, NumericOperation, NumericSlot, Schema,
    Target, World,
};

fn world() -> World {
    let mut world = World::new(Schema::new(["base"]).unwrap());
    world.add_entity("A", EntityKind::Country);
    world.add_entity("B", EntityKind::Country);
    world.add_entity("P", EntityKind::Province);
    world.set_owner(EntityId(2), EntityId(0)).unwrap();
    world.set_governments(serde_json::from_str(r#"{
        "reforms": {
            "first": {"modifier":{"values_micros":{"yield":125000},"flags":{}},"unresolved_properties":[]},
            "second": {"modifier":{"values_micros":{"yield":-250000},"flags":{}},"unresolved_properties":[]}
        },
        "metrics": {"authority":{"minimum_milli":0,"maximum_milli":100000,"baseline_milli":25000,"divisor_milli":50000,
            "modifier":{"values_micros":{"yield":400000,"other":-750000},"flags":{}},"unresolved_properties":[]}},
        "countries": {
            "0":{"kind":"first_kind","reforms":["first"],"metrics":{"authority":75125}},
            "1":{"kind":"second_kind","reforms":["second"],"metrics":{}}
        }
    }"#).unwrap()).unwrap();
    world.set_attributes(serde_json::from_str(r#"{
        "static_modifiers":{},"building_modifiers":{},"trade_good_modifiers":{},
        "formulas":{"2":{"yield":{"base":{"kind":"constant","micros":1000000},
            "channels":[{"flat":[],"rates":[{"kind":"government","target":"Owner","property":"yield"}]}],
            "minimum_micros":null,"maximum_micros":null,"unresolved":[]}}}
    }"#).unwrap()).unwrap();
    world
}

fn yield_value(world: &World) -> augustus_core::AttributeEvaluation {
    world
        .attributes()
        .evaluate(world, EntityId(2), "yield")
        .unwrap()
}

fn metric(operation: NumericOperation, value: i64) -> Action {
    Action::UpdateNumeric {
        target: Target::Current,
        slot: NumericSlot::Government("authority".into()),
        operation,
        value,
    }
}

fn transition_world() -> World {
    let mut world = world();
    let mut governments = world.governments().clone();
    governments.forms = [
        (
            "first_kind".into(),
            augustus_core::GovernmentForm {
                reforms: ["first".into()].into(),
                metrics: [("authority".into(), Some(50_000))].into(),
                ..Default::default()
            },
        ),
        (
            "second_kind".into(),
            augustus_core::GovernmentForm {
                reforms: ["second".into()].into(),
                inactive_metrics: [("authority".into(), Some(100_000))].into(),
                ..Default::default()
            },
        ),
    ]
    .into();
    governments
        .countries
        .get_mut(&EntityId(0))
        .unwrap()
        .reforms
        .as_mut()
        .unwrap()
        .insert("second".into());
    world.set_governments(governments).unwrap();
    world
}

fn change(kind: &str, before: augustus_core::Plan) -> Action {
    Action::ChangeGovernment {
        target: Target::Current,
        kind: kind.into(),
        before,
    }
}

#[test]
fn transitions_run_callbacks_on_old_state_and_same_form_preserves_reforms() {
    let mut world = transition_world();
    let before = vec![Action::If {
        condition: Condition::All(vec![
            Condition::GovernmentKind {
                target: Target::Current,
                kind: "first_kind".into(),
            },
            Condition::GovernmentReform {
                target: Target::Current,
                id: "second".into(),
            },
        ]),
        then: vec![
            Action::Emit("old state".into()),
            metric(NumericOperation::Subtract, 3000),
        ],
        otherwise: vec![],
    }];
    world
        .execute(EntityId(0), &[change("first_kind", before.clone())])
        .unwrap();
    assert_eq!(
        world
            .government_metric_value(EntityId(0), "authority")
            .unwrap(),
        72_125
    );
    assert!(
        world
            .government_reform_selected(EntityId(0), "second")
            .unwrap()
    );
    world
        .execute(EntityId(0), &[change("second_kind", before)])
        .unwrap();
    assert_eq!(
        world
            .events()
            .iter()
            .filter(|(_, _, text)| text == "old state")
            .count(),
        2
    );
    assert_eq!(world.government_kind(EntityId(0)).unwrap(), "second_kind");
    assert!(
        !world
            .government_reform_selected(EntityId(0), "first")
            .unwrap()
    );
    assert_eq!(
        world
            .government_metric_stored_value(EntityId(0), "authority")
            .unwrap(),
        100_000
    );
    assert_eq!(yield_value(&world).value_micros, Some(750_000));
    world
        .execute(EntityId(0), &[change("first_kind", vec![])])
        .unwrap();
    assert_eq!(
        world
            .government_metric_value(EntityId(0), "authority")
            .unwrap(),
        50_000
    );
    assert!(
        !world.governments().countries[&EntityId(0)]
            .inactive_metrics
            .contains_key("authority")
    );
    assert_eq!(yield_value(&world).value_micros, Some(1_325_000));
}

#[test]
fn transitioned_values_and_deferred_callbacks_survive_restore() {
    let mut world = transition_world();
    let mut governments = world.governments().clone();
    governments
        .forms
        .get_mut("second_kind")
        .unwrap()
        .inactive_metrics
        .clear();
    world.set_governments(governments).unwrap();
    world
        .execute(
            EntityId(0),
            &[Action::Schedule {
                delay: 2,
                plan: vec![change(
                    "second_kind",
                    vec![metric(NumericOperation::Subtract, 3000)],
                )],
            }],
        )
        .unwrap();
    let saved = world.save("transition").unwrap();
    let mut restored = World::restore(&saved, "transition", world.schema()).unwrap();
    world.advance_to(2).unwrap();
    restored.advance_to(2).unwrap();
    assert_eq!(world, restored);
    assert_eq!(
        world
            .government_metric_stored_value(EntityId(0), "authority")
            .unwrap(),
        72_125
    );
    assert_eq!(
        World::restore(
            &world.save("transition").unwrap(),
            "transition",
            world.schema()
        )
        .unwrap(),
        world
    );
}

#[test]
fn form_validation_checks_references_bounds_and_pending_dependencies() {
    let mut world = transition_world();
    for bad in ["missing_reform", "out_of_range", "duplicate_metric"] {
        let before = world.clone();
        let mut governments = world.governments().clone();
        let form = governments.forms.get_mut("first_kind").unwrap();
        match bad {
            "missing_reform" => {
                form.reforms.insert("missing".into());
            }
            "out_of_range" => {
                form.metrics.insert("authority".into(), Some(100_001));
            }
            _ => {
                form.inactive_metrics
                    .insert("authority".into(), Some(100_000));
            }
        }
        assert!(world.set_governments(governments).is_err());
        assert_eq!(world, before);
    }
    world
        .execute(
            EntityId(0),
            &[Action::Schedule {
                delay: 1,
                plan: vec![change("second_kind", vec![])],
            }],
        )
        .unwrap();
    let before = world.clone();
    let mut governments = world.governments().clone();
    governments.forms.remove("second_kind");
    assert!(world.set_governments(governments).is_err());
    assert_eq!(world, before);
}

#[test]
fn callback_failure_preserves_prior_writes_but_does_not_install_form() {
    let mut world = transition_world();
    let error = world
        .execute(
            EntityId(0),
            &[change(
                "second_kind",
                vec![
                    metric(NumericOperation::Subtract, 3000),
                    metric(NumericOperation::Divide { scale: 1000 }, 0),
                ],
            )],
        )
        .unwrap_err();
    assert!(error.to_string().contains("zero"));
    assert_eq!(world.government_kind(EntityId(0)).unwrap(), "first_kind");
    assert_eq!(
        world
            .government_metric_value(EntityId(0), "authority")
            .unwrap(),
        72_125
    );
    assert!(world.save("transition").is_err());
    assert!(world.execute(EntityId(0), &[]).is_err());
}

#[test]
fn transitions_share_instruction_budget_and_resolve_country_scope() {
    let mut scoped = transition_world();
    scoped
        .execute(
            EntityId(2),
            &[Action::ChangeGovernment {
                target: Target::Owner,
                kind: "second_kind".into(),
                before: vec![Action::Emit("owner callback".into())],
            }],
        )
        .unwrap();
    assert_eq!(scoped.events().last().unwrap().1, EntityId(0));
    let mut world = transition_world();
    let error = world
        .execute(
            EntityId(0),
            &[change(
                "second_kind",
                vec![Action::Emit("budget".into()); 100_000],
            )],
        )
        .unwrap_err();
    assert!(error.to_string().contains("instruction limit"));
    assert_eq!(world.government_kind(EntityId(0)).unwrap(), "first_kind");
    assert!(world.save("transition").is_err());
}

#[test]
fn numeric_operations_clamp_each_step_and_recalculate_live_attributes() {
    let mut world = world();
    let slot = NumericSlot::Government("authority".into());
    world
        .execute(
            EntityId(0),
            &[
                metric(NumericOperation::Add, 100_000),
                metric(NumericOperation::Subtract, 50_000),
            ],
        )
        .unwrap();
    assert_eq!(world.numeric_value(EntityId(0), &slot).unwrap(), 50_000);
    assert_eq!(yield_value(&world).value_micros, Some(1_325_000));
    for (operation, value, expected) in [
        (NumericOperation::Set, -1, 0),
        (NumericOperation::Add, 75_125, 75_125),
        (NumericOperation::Multiply { scale: 1000 }, 1250, 93_906),
        (NumericOperation::Divide { scale: 1000 }, 1250, 75_124),
        (NumericOperation::Add, i64::MAX, 100_000),
        (NumericOperation::Multiply { scale: 1 }, i64::MAX, 100_000),
        (NumericOperation::Subtract, i64::MAX, 0),
    ] {
        world
            .update_numeric(EntityId(0), &slot, operation, value)
            .unwrap();
        assert_eq!(world.numeric_value(EntityId(0), &slot).unwrap(), expected);
    }
    let before = world.clone();
    assert!(
        world
            .update_numeric(
                EntityId(0),
                &slot,
                NumericOperation::Divide { scale: 1000 },
                0
            )
            .is_err()
    );
    assert_eq!(world, before);
}

#[test]
fn metric_selection_unknown_value_and_unknown_selection_are_distinct() {
    let mut world = world();
    let slot = NumericSlot::Government("authority".into());
    assert!(
        world
            .government_metric_active(EntityId(0), "authority")
            .unwrap()
    );
    assert!(
        !world
            .government_metric_active(EntityId(1), "authority")
            .unwrap()
    );
    assert!(world.numeric_value(EntityId(1), &slot).is_err());
    assert!(
        world
            .update_numeric(EntityId(1), &slot, NumericOperation::Set, 1)
            .is_err()
    );
    assert!(
        world
            .government_metric_active(EntityId(2), "authority")
            .is_err()
    );
    assert!(
        world
            .government_metric_active(EntityId(0), "unknown")
            .is_err()
    );
    let mut governments = world.governments().clone();
    governments
        .countries
        .get_mut(&EntityId(0))
        .unwrap()
        .metrics
        .as_mut()
        .unwrap()
        .insert("authority".into(), None);
    world.set_governments(governments).unwrap();
    assert!(
        world
            .government_metric_active(EntityId(0), "authority")
            .unwrap()
    );
    assert!(
        world
            .update_numeric(EntityId(0), &slot, NumericOperation::Add, 1)
            .is_err()
    );
    world
        .update_numeric(EntityId(0), &slot, NumericOperation::Set, 50_000)
        .unwrap();
    assert_eq!(yield_value(&world).value_micros, Some(1_325_000));
    let mut governments = world.governments().clone();
    governments.countries.get_mut(&EntityId(0)).unwrap().metrics = None;
    world.set_governments(governments).unwrap();
    let before = world.clone();
    assert!(
        world
            .government_metric_active(EntityId(0), "authority")
            .is_err()
    );
    assert!(
        world
            .update_numeric(EntityId(0), &slot, NumericOperation::Set, 1)
            .is_err()
    );
    assert_eq!(world, before);
}

#[test]
fn scheduled_metric_reads_are_live_after_save_and_catalog_replacement_is_checked() {
    let mut world = world();
    let plan = vec![Action::If {
        condition: Condition::GovernmentMetricActive {
            target: Target::Current,
            name: "authority".into(),
        },
        then: vec![metric(NumericOperation::Subtract, 25_125)],
        otherwise: vec![],
    }];
    world.schedule(EntityId(0), 1, plan.clone()).unwrap();
    let before = world.clone();
    let mut bad = world.governments().clone();
    bad.metrics.clear();
    for state in bad.countries.values_mut() {
        state.metrics = Some(Default::default());
    }
    assert!(world.set_governments(bad.clone()).is_err());
    assert_eq!(world, before);
    let mut resumed = World::restore(
        &world.save("numeric-government").unwrap(),
        "numeric-government",
        world.schema(),
    )
    .unwrap();
    for world in [&mut world, &mut resumed] {
        world.advance_to(1).unwrap();
    }
    assert_eq!(world, resumed);
    assert_eq!(
        world
            .government_metric_value(EntityId(0), "authority")
            .unwrap(),
        50_000
    );
    world
        .set_event_catalog(
            augustus_core::EventCatalog::new(vec![augustus_core::EventDefinition {
                id: "government".into(),
                scope: EntityKind::Country,
                immediate: plan,
                options: vec![augustus_core::EventOption {
                    id: "0".into(),
                    effects: vec![],
                }],
                after: vec![],
            }])
            .unwrap(),
        )
        .unwrap();
    let before = world.clone();
    assert!(world.set_governments(bad).is_err());
    assert_eq!(world, before);
    let mut saved: serde_json::Value =
        serde_json::from_str(&world.save("numeric-government").unwrap()).unwrap();
    saved["governments"]["metrics"]
        .as_object_mut()
        .unwrap()
        .clear();
    saved["governments"]["countries"]["0"]["metrics"] = serde_json::json!({});
    assert!(
        World::restore_with_events(
            &saved.to_string(),
            "numeric-government",
            world.schema(),
            vec![],
            world.event_catalog().clone()
        )
        .is_err()
    );
}

#[test]
fn metric_contribution_overflow_rolls_back_before_write() {
    let mut world = world();
    let mut governments = world.governments().clone();
    let definition = governments.metrics.get_mut("authority").unwrap();
    definition.baseline_milli = 75_125;
    definition.divisor_milli = 1;
    definition
        .modifier
        .as_mut()
        .unwrap()
        .values_micros
        .insert("other".into(), i64::MAX);
    world.set_governments(governments).unwrap();
    let before = world.clone();
    assert!(
        world
            .update_numeric(
                EntityId(0),
                &NumericSlot::Government("authority".into()),
                NumericOperation::Add,
                2
            )
            .is_err()
    );
    assert_eq!(world, before);
}

#[test]
fn due_metric_effects_feed_monthly_settlement_in_the_shared_world() {
    use augustus_core::{Calendar, CountryResources, Taxation, WorldDate};
    let mut world = world();
    world
        .set_calendar(Calendar {
            start: WorldDate {
                year: 1444,
                month: 11,
                day: 30,
            },
            month_lengths: [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31],
        })
        .unwrap();
    world
        .set_country_resources(
            [(
                EntityId(0),
                CountryResources {
                    treasury_milli: Some(0),
                    ..Default::default()
                },
            )]
            .into(),
        )
        .unwrap();
    world
        .set_taxation(Taxation::new("yield", [EntityId(0)].into()))
        .unwrap();
    world
        .schedule(EntityId(0), 1, vec![metric(NumericOperation::Set, 50_000)])
        .unwrap();
    let saved = world.save("monthly-government").unwrap();
    let mut resumed = World::restore(&saved, "monthly-government", world.schema()).unwrap();
    for state in [&mut world, &mut resumed] {
        state.advance_to(1).unwrap();
        assert_eq!(yield_value(state).value_micros, Some(1_325_000));
        assert_eq!(
            state.country_resources()[&EntityId(0)].treasury_milli,
            Some(110)
        );
    }
    assert_eq!(
        world.save("monthly-government").unwrap(),
        resumed.save("monthly-government").unwrap()
    );
}

#[test]
fn institutional_features_follow_selected_reforms_and_preserve_local_uncertainty() {
    let mut world = world();
    let mut governments = world.governments().clone();
    governments.reforms.get_mut("first").unwrap().features = [
        ("charter".into(), Some(true)),
        ("elections".into(), Some(false)),
        ("disputed".into(), None),
    ]
    .into_iter()
    .collect();
    governments
        .reforms
        .get_mut("second")
        .unwrap()
        .features
        .insert("charter".into(), Some(false));
    world.set_governments(governments.clone()).unwrap();
    assert_eq!(
        world.governments().feature(EntityId(0), "charter"),
        Some(true)
    );
    assert_eq!(
        world.governments().feature(EntityId(0), "elections"),
        Some(false)
    );
    assert_eq!(
        world.governments().feature(EntityId(0), "absent"),
        Some(false)
    );
    assert_eq!(world.governments().feature(EntityId(0), "disputed"), None);
    assert_eq!(world.governments().feature(EntityId(2), "charter"), None);
    assert_eq!(
        world,
        World::restore(&world.save("features").unwrap(), "features", world.schema()).unwrap()
    );
    governments
        .countries
        .get_mut(&EntityId(0))
        .unwrap()
        .reforms
        .as_mut()
        .unwrap()
        .insert("second".into());
    world.set_governments(governments.clone()).unwrap();
    assert_eq!(world.governments().feature(EntityId(0), "charter"), None);
    governments.countries.get_mut(&EntityId(0)).unwrap().reforms =
        Some(["second".into()].into_iter().collect());
    world.set_governments(governments.clone()).unwrap();
    assert_eq!(
        world.governments().feature(EntityId(0), "charter"),
        Some(false)
    );
    governments.countries.get_mut(&EntityId(0)).unwrap().reforms =
        Some(["missing".into()].into_iter().collect());
    world.set_governments(governments.clone()).unwrap();
    assert_eq!(world.governments().feature(EntityId(0), "absent"), None);
    let before = world.clone();
    governments
        .reforms
        .get_mut("first")
        .unwrap()
        .features
        .insert(String::new(), None);
    assert!(world.set_governments(governments).is_err());
    assert_eq!(world, before);
}

#[test]
fn reform_metric_and_owner_changes_recalculate_with_signed_fixed_point_values() {
    let mut world = world();
    assert_eq!(yield_value(&world).value_micros, Some(1_526_000));
    let effects = world.governments().declared_effects(EntityId(0)).unwrap();
    assert_eq!(effects.known_totals_micros["other"], -751_875);
    assert_eq!(effects.contributions.len(), 3);
    for (metric, expected) in [
        (0, 925_000),
        (25_000, 1_125_000),
        (50_000, 1_325_000),
        (100_000, 1_725_000),
    ] {
        let mut governments = world.governments().clone();
        *governments
            .countries
            .get_mut(&EntityId(0))
            .unwrap()
            .metrics
            .as_mut()
            .unwrap()
            .get_mut("authority")
            .unwrap() = Some(metric);
        world.set_governments(governments).unwrap();
        assert_eq!(yield_value(&world).value_micros, Some(expected));
    }
    let mut governments = world.governments().clone();
    governments
        .countries
        .get_mut(&EntityId(0))
        .unwrap()
        .reforms
        .as_mut()
        .unwrap()
        .clear();
    world.set_governments(governments).unwrap();
    assert_eq!(yield_value(&world).value_micros, Some(1_600_000));
    world.set_owner(EntityId(2), EntityId(1)).unwrap();
    assert_eq!(yield_value(&world).value_micros, Some(750_000));
}

#[test]
fn unknown_inputs_preserve_known_contributions_and_property_scope() {
    let mut world = world();
    let mut governments = world.governments().clone();
    governments
        .metrics
        .get_mut("authority")
        .unwrap()
        .unresolved_properties
        .insert("other".into());
    world.set_governments(governments.clone()).unwrap();
    assert_eq!(yield_value(&world).value_micros, Some(1_526_000));
    governments
        .countries
        .get_mut(&EntityId(0))
        .unwrap()
        .metrics
        .as_mut()
        .unwrap()
        .insert("authority".into(), None);
    world.set_governments(governments.clone()).unwrap();
    let value = yield_value(&world);
    assert_eq!(value.known_subtotal_micros, Some(1_125_000));
    assert_eq!(value.value_micros, None);
    assert!(
        value
            .unresolved
            .contains("government:metric:authority:unknown_value:yield")
    );
    governments.countries.get_mut(&EntityId(0)).unwrap().metrics = Some(Default::default());
    governments
        .countries
        .get_mut(&EntityId(0))
        .unwrap()
        .reforms
        .as_mut()
        .unwrap()
        .insert("missing".into());
    world.set_governments(governments.clone()).unwrap();
    assert!(
        yield_value(&world)
            .unresolved
            .contains("government:reform:missing:missing_definition")
    );
    governments.countries.get_mut(&EntityId(0)).unwrap().reforms = None;
    world.set_governments(governments).unwrap();
    assert!(
        yield_value(&world)
            .unresolved
            .contains("government:unknown_reforms")
    );
    assert_eq!(yield_value(&world).value_micros, None);
}

#[test]
fn invalid_replacements_overflow_and_corrupt_saves_fail_before_replacing_state() {
    let mut world = world();
    let before = world.clone();
    let mut bad = world.governments().clone();
    bad.countries
        .insert(EntityId(2), bad.countries[&EntityId(0)].clone());
    assert!(world.set_governments(bad).is_err());
    let mut bad = world.governments().clone();
    bad.metrics.get_mut("authority").unwrap().divisor_milli = 0;
    assert!(world.set_governments(bad).is_err());
    let mut bad = world.governments().clone();
    bad.countries
        .get_mut(&EntityId(0))
        .unwrap()
        .metrics
        .as_mut()
        .unwrap()
        .insert("authority".into(), Some(100_001));
    assert!(world.set_governments(bad).is_err());
    let mut bad = world.governments().clone();
    let metric = bad.metrics.get_mut("authority").unwrap();
    metric.divisor_milli = 1;
    metric
        .modifier
        .as_mut()
        .unwrap()
        .values_micros
        .insert("yield".into(), i64::MAX);
    assert!(world.set_governments(bad).is_err());
    assert_eq!(world, before);
    let mut bad = world.governments().clone();
    let reform = bad.reforms.get_mut("first").unwrap();
    reform.modifier.values_micros.remove("yield");
    reform.modifier.flags.insert("yield".into(), true);
    assert!(world.set_governments(bad).is_err());
    assert_eq!(world, before);
    let mut saved: serde_json::Value =
        serde_json::from_str(&world.save("government").unwrap()).unwrap();
    saved["governments"]["countries"]["0"]["metrics"]["authority"] = serde_json::json!(-1);
    assert!(World::restore(&saved.to_string(), "government", world.schema()).is_err());
}

#[test]
fn government_state_roundtrips_and_existing_empty_snapshots_remain_readable() {
    let world = world();
    let saved = world.save("government").unwrap();
    let restored = World::restore(&saved, "government", world.schema()).unwrap();
    assert_eq!(restored, world);
    assert_eq!(yield_value(&restored), yield_value(&world));
    let empty = World::new(Schema::new(["base"]).unwrap());
    let saved = empty.save("old").unwrap();
    assert!(!saved.contains("governments"));
    assert_eq!(
        World::restore(&saved, "old", empty.schema()).unwrap(),
        empty
    );
    let mut replacement = world.governments().clone();
    replacement.metrics.get_mut("authority").unwrap().modifier = None;
    let mut changed = world;
    changed.set_governments(replacement).unwrap();
    assert!(
        yield_value(&changed)
            .unresolved
            .contains("government:metric:authority:missing_definition")
    );
    let _: Governments =
        serde_json::from_str(&serde_json::to_string(changed.governments()).unwrap()).unwrap();
}

#[test]
fn retained_inactive_values_are_readable_without_effects_or_numeric_writes() {
    let mut world = world();
    let mut governments = world.governments().clone();
    let state = governments.countries.get_mut(&EntityId(0)).unwrap();
    state.metrics.as_mut().unwrap().clear();
    state
        .inactive_metrics
        .insert("authority".into(), Some(100_000));
    world.set_governments(governments).unwrap();
    assert!(
        !world
            .government_metric_active(EntityId(0), "authority")
            .unwrap()
    );
    assert!(
        world
            .government_metric_stored(EntityId(0), "authority")
            .unwrap()
    );
    assert_eq!(
        world
            .government_metric_stored_value(EntityId(0), "authority")
            .unwrap(),
        100_000
    );
    assert_eq!(yield_value(&world).value_micros, Some(1_125_000));
    assert!(
        world
            .government_metric_value(EntityId(0), "authority")
            .is_err()
    );
    assert!(
        Condition::NumericCompare {
            target: Target::Owner,
            slot: NumericSlot::StoredGovernment("authority".into()),
            comparison: augustus_core::NumericComparison::AtLeast,
            value: 100_000,
        }
        .evaluate(&world, EntityId(2))
        .unwrap()
    );
    let before = world.clone();
    for slot in [
        NumericSlot::Government("authority".into()),
        NumericSlot::StoredGovernment("authority".into()),
    ] {
        assert!(
            world
                .update_numeric(EntityId(0), &slot, NumericOperation::Set, 50_000)
                .is_err()
        );
    }
    assert_eq!(world, before);
    let mut governments = world.governments().clone();
    let state = governments.countries.get_mut(&EntityId(0)).unwrap();
    let value = state.inactive_metrics.remove("authority").unwrap();
    state
        .metrics
        .as_mut()
        .unwrap()
        .insert("authority".into(), value);
    world.set_governments(governments).unwrap();
    assert!(
        world
            .government_metric_active(EntityId(0), "authority")
            .unwrap()
    );
    assert_eq!(
        world
            .government_metric_stored_value(EntityId(0), "authority")
            .unwrap(),
        100_000
    );
    assert_eq!(yield_value(&world).value_micros, Some(1_725_000));
}

#[test]
fn stored_metric_presence_unknown_values_and_unknown_selection_are_distinct() {
    let mut world = world();
    let condition = Condition::GovernmentMetricStored {
        target: Target::Current,
        name: "authority".into(),
    };
    assert!(!condition.evaluate(&world, EntityId(1)).unwrap());
    assert!(
        world
            .government_metric_stored_value(EntityId(1), "authority")
            .is_err()
    );
    let mut governments = world.governments().clone();
    governments
        .countries
        .get_mut(&EntityId(1))
        .unwrap()
        .inactive_metrics
        .insert("authority".into(), None);
    world.set_governments(governments).unwrap();
    assert!(condition.evaluate(&world, EntityId(1)).unwrap());
    assert!(
        world
            .government_metric_stored_value(EntityId(1), "authority")
            .is_err()
    );
    assert!(
        world
            .government_metric_stored(EntityId(2), "authority")
            .is_err()
    );
    assert!(
        world
            .government_metric_stored(EntityId(1), "missing")
            .is_err()
    );
    let mut governments = world.governments().clone();
    let state = governments.countries.get_mut(&EntityId(1)).unwrap();
    state.metrics = None;
    state
        .inactive_metrics
        .insert("authority".into(), Some(25_000));
    world.set_governments(governments).unwrap();
    assert!(condition.evaluate(&world, EntityId(1)).is_err());
    assert!(
        Condition::Not(Box::new(condition))
            .evaluate(&world, EntityId(1))
            .is_err()
    );
    assert!(
        world
            .government_metric_stored_value(EntityId(1), "authority")
            .is_err()
    );
}

#[test]
fn retained_values_roundtrip_and_delayed_reads_keep_registry_dependencies() {
    let mut world = world();
    let mut governments = world.governments().clone();
    let state = governments.countries.get_mut(&EntityId(0)).unwrap();
    state.metrics.as_mut().unwrap().clear();
    state
        .inactive_metrics
        .insert("authority".into(), Some(25_000));
    world.set_governments(governments).unwrap();
    world
        .schedule(
            EntityId(0),
            1,
            vec![Action::If {
                condition: Condition::All(vec![
                    Condition::GovernmentMetricStored {
                        target: Target::Current,
                        name: "authority".into(),
                    },
                    Condition::NumericCompare {
                        target: Target::Current,
                        slot: NumericSlot::StoredGovernment("authority".into()),
                        comparison: augustus_core::NumericComparison::AtLeast,
                        value: 75_125,
                    },
                ]),
                then: vec![Action::Emit("retained".into())],
                otherwise: vec![],
            }],
        )
        .unwrap();
    let saved = world.save("retained-government").unwrap();
    let mut resumed = World::restore(&saved, "retained-government", world.schema()).unwrap();
    assert_eq!(resumed.save("retained-government").unwrap(), saved);
    let before = world.clone();
    let mut bad = world.governments().clone();
    bad.metrics.clear();
    for state in bad.countries.values_mut() {
        state.metrics.as_mut().unwrap().clear();
        state.inactive_metrics.clear();
    }
    assert!(world.set_governments(bad).is_err());
    assert_eq!(world, before);
    for state in [&mut world, &mut resumed] {
        let mut governments = state.governments().clone();
        governments
            .countries
            .get_mut(&EntityId(0))
            .unwrap()
            .inactive_metrics
            .insert("authority".into(), Some(75_125));
        state.set_governments(governments).unwrap();
        state.advance_to(1).unwrap();
        assert_eq!(state.events().last().unwrap().2, "retained");
    }
    assert_eq!(world, resumed);
    let mut bad: serde_json::Value = serde_json::from_str(&saved).unwrap();
    bad["governments"]["countries"]["0"]["inactive_metrics"]["authority"] =
        serde_json::json!(100_001);
    assert!(World::restore(&bad.to_string(), "retained-government", world.schema()).is_err());
}

#[test]
fn invalid_inactive_values_and_read_only_plans_leave_the_world_unchanged() {
    let mut world = world();
    let before = world.clone();
    assert!(
        !world
            .save("old-government")
            .unwrap()
            .contains("inactive_metrics")
    );
    for (entity, name, value) in [
        (0, "authority", 10),
        (1, "authority", -1),
        (1, "authority", 100_001),
        (1, "missing", 10),
    ] {
        let mut bad = world.governments().clone();
        bad.countries
            .get_mut(&EntityId(entity))
            .unwrap()
            .inactive_metrics
            .insert(name.into(), Some(value));
        assert!(world.set_governments(bad).is_err());
        assert_eq!(world, before);
    }
    let plan = vec![Action::If {
        condition: Condition::Always(false),
        then: vec![Action::UpdateNumeric {
            target: Target::Current,
            slot: NumericSlot::StoredGovernment("authority".into()),
            operation: NumericOperation::Set,
            value: 1,
        }],
        otherwise: vec![],
    }];
    assert!(world.schedule(EntityId(0), 1, plan).is_err());
    assert_eq!(world, before);
    let saved = world.save("old-government").unwrap();
    assert_eq!(
        World::restore(&saved, "old-government", world.schema())
            .unwrap()
            .save("old-government")
            .unwrap(),
        saved
    );
}
