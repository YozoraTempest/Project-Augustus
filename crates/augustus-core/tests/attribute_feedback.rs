// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::*;

fn constant(amount: i64) -> AttributeTerm {
    AttributeTerm::Constant {
        source: "input".into(),
        micros: amount,
    }
}
fn formula(terms: Vec<AttributeTerm>) -> AttributeFormula {
    AttributeFormula {
        base: AttributeBase::Constant { micros: 0 },
        channels: vec![AttributeChannel {
            flat: terms,
            rates: vec![],
        }],
        minimum_micros: None,
        maximum_micros: None,
        result_quantum_micros: None,
        unresolved: Default::default(),
    }
}
fn numeric(slot: NumericSlot, scale: i64) -> AttributeTerm {
    AttributeTerm::Numeric {
        source: "quantity".into(),
        target: Target::Current,
        slot,
        micros_per_unit: scale,
    }
}
fn attributes(formula: AttributeFormula) -> Attributes {
    Attributes {
        formulas: [(EntityId(0), [("response".into(), formula)].into())].into(),
        ..Default::default()
    }
}

#[test]
fn scale_rounds_the_group_and_only_the_selected_branch_reads_state() {
    let mut world = World::new(Schema::new(["quantity"]).unwrap());
    let entity = world.add_entity("AAA", EntityKind::Country);
    world
        .set_attributes(attributes(formula(vec![
            AttributeTerm::Scale {
                factor_micros: 500000,
                term: Box::new(AttributeTerm::TruncatedSum {
                    source: "group".into(),
                    quantum_micros: 1,
                    terms: vec![constant(1), constant(1)],
                }),
            },
            AttributeTerm::Conditional {
                condition: Box::new(Condition::Always(true)),
                then: vec![constant(4)],
                otherwise: vec![numeric(
                    NumericSlot::Resource(CountryResource::Treasury),
                    1000,
                )],
            },
        ])))
        .unwrap();
    assert_eq!(
        world
            .attributes()
            .evaluate(&world, entity, "response")
            .unwrap()
            .value_micros,
        Some(5)
    );
    let mut changed = world.attributes().clone();
    changed
        .formulas
        .get_mut(&entity)
        .unwrap()
        .get_mut("response")
        .unwrap()
        .channels[0]
        .flat[1] = AttributeTerm::Conditional {
        condition: Box::new(Condition::Always(false)),
        then: vec![constant(4)],
        otherwise: vec![numeric(
            NumericSlot::Resource(CountryResource::Treasury),
            1000,
        )],
    };
    world.set_attributes(changed).unwrap();
    let evaluation = world
        .attributes()
        .evaluate(&world, entity, "response")
        .unwrap();
    assert_eq!(evaluation.value_micros, None);
    assert!(
        evaluation
            .unresolved
            .iter()
            .any(|reason| reason.contains("unknown_quantity"))
    );
}

#[test]
fn derived_attribute_dependencies_are_rejected_even_in_unselected_branches() {
    let mut world = World::new(Schema::new(["quantity"]).unwrap());
    world.add_entity("AAA", EntityKind::Country);
    let slot = NumericSlot::Attribute {
        name: "response".into(),
        divisor: 1,
    };
    assert!(
        world
            .set_attributes(attributes(formula(vec![numeric(slot.clone(), 1)])))
            .is_err()
    );
    let condition = Condition::Not(Box::new(Condition::All(vec![Condition::NumericCompare {
        target: Target::Current,
        slot,
        comparison: NumericComparison::AtLeast,
        value: 0,
    }])));
    assert!(
        world
            .set_attributes(attributes(formula(vec![AttributeTerm::Conditional {
                condition: Box::new(condition),
                then: vec![],
                otherwise: vec![]
            }])))
            .is_err()
    );
}

#[test]
fn unbound_sources_are_scoped_and_invalid_saved_feedback_is_rejected() {
    let mut world = World::new(Schema::new(["quantity"]).unwrap());
    let aaa = world.add_entity("AAA", EntityKind::Country);
    world
        .set_attributes(attributes(formula(vec![AttributeTerm::Conditional {
            condition: Box::new(Condition::HasOwnedProvince {
                target: Target::Current,
            }),
            then: vec![AttributeTerm::Unknown {
                reason: "source_not_bound".into(),
            }],
            otherwise: vec![constant(7)],
        }])))
        .unwrap();
    assert_eq!(
        world
            .attributes()
            .evaluate(&world, aaa, "response")
            .unwrap()
            .value_micros,
        Some(7)
    );
    let saved = world.save("feedback:1").unwrap();
    let mut corrupt: serde_json::Value = serde_json::from_str(&saved).unwrap();
    corrupt["attributes"]["formulas"]["0"]["response"]["channels"][0]["flat"][0]["then"][0]["reason"] =
        serde_json::json!("");
    assert!(World::restore(&corrupt.to_string(), "feedback:1", world.schema()).is_err());
    let province = world.add_entity("Port", EntityKind::Province);
    world.set_owner(province, aaa).unwrap();
    let evaluation = world
        .attributes()
        .evaluate(&world, aaa, "response")
        .unwrap();
    assert_eq!(evaluation.value_micros, None);
    assert!(evaluation.unresolved.contains("source_not_bound"));
    world.set_capital(aaa, province).unwrap();
    let invalid = AttributeTerm::Numeric {
        source: "cash".into(),
        target: Target::Capital,
        slot: NumericSlot::Resource(CountryResource::Treasury),
        micros_per_unit: 1000,
    };
    assert!(
        world
            .set_attributes(attributes(formula(vec![invalid])))
            .is_err()
    );
}

#[test]
fn monthly_feedback_reads_live_quantities_and_inputs_after_restore() {
    let mut whole = World::new(Schema::new(["annual_extra", "decay_extra"]).unwrap());
    let aaa = whole.add_entity("AAA", EntityKind::Country);
    let province = whole.add_entity("Port", EntityKind::Province);
    whole.set_owner(province, aaa).unwrap();
    whole
        .set_calendar(Calendar {
            start: WorldDate {
                year: 1444,
                month: 11,
                day: 11,
            },
            month_lengths: [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31],
        })
        .unwrap();
    whole
        .set_metrics(Metrics {
            definitions: [(
                "stock".into(),
                MetricDefinition {
                    kind: EntityKind::Country,
                    minimum: -100000,
                    maximum: 100000,
                    baseline: 0,
                    divisor: 100000,
                    modifier: None,
                    unresolved_properties: Default::default(),
                    below_baseline: None,
                    growth_ceiling: false,
                },
            )]
            .into(),
            entities: [(aaa, [("stock".into(), Some(50000))].into())].into(),
            ceilings: Default::default(),
        })
        .unwrap();
    let metric = NumericSlot::Metric("stock".into());
    let decay = numeric(NumericSlot::Field(FieldId(1)), 1000);
    let formula = AttributeFormula {
        base: AttributeBase::Constant { micros: 0 },
        channels: vec![
            AttributeChannel {
                flat: vec![numeric(metric.clone(), -1000)],
                rates: vec![
                    constant(-950000),
                    AttributeTerm::Conditional {
                        condition: Box::new(Condition::NumericCompare {
                            target: Target::Current,
                            slot: metric,
                            comparison: NumericComparison::AtLeast,
                            value: 0,
                        }),
                        then: vec![decay.clone()],
                        otherwise: vec![AttributeTerm::Scale {
                            factor_micros: -1000000,
                            term: Box::new(decay),
                        }],
                    },
                ],
            },
            AttributeChannel {
                flat: vec![
                    constant(1000000),
                    numeric(NumericSlot::Field(FieldId(0)), 1000),
                ],
                rates: vec![],
            },
        ],
        minimum_micros: None,
        maximum_micros: None,
        result_quantum_micros: None,
        unresolved: Default::default(),
    };
    whole.set_attributes(attributes(formula)).unwrap();
    whole
        .set_monthly_accruals(MonthlyAccruals {
            rules: [(
                aaa,
                [(
                    "stock".into(),
                    AccrualRule {
                        attribute: "response".into(),
                        divisor: 12000,
                    },
                )]
                .into(),
            )]
            .into(),
            ..Default::default()
        })
        .unwrap();
    whole.advance_to(20).unwrap();
    assert_eq!(whole.metrics().entities[&aaa]["stock"], Some(49875));
    let saved = whole.save("feedback:1").unwrap();
    let mut resumed = World::restore(&saved, "feedback:1", whole.schema()).unwrap();
    for world in [&mut whole, &mut resumed] {
        world.advance_to(51).unwrap();
        assert_eq!(world.metrics().entities[&aaa]["stock"], Some(49751));
        world.set_metric_value(aaa, "stock", Some(-50000)).unwrap();
        world.set(aaa, FieldId(0), 1200).unwrap();
        world.set(aaa, FieldId(1), -20).unwrap();
        world.advance_to(82).unwrap();
        assert_eq!(world.metrics().entities[&aaa]["stock"], Some(-49525));
    }
    assert_eq!(
        whole.save("feedback:1").unwrap(),
        resumed.save("feedback:1").unwrap()
    );
}
