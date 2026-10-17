// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::*;

fn fixture() -> World {
    let mut world = World::new(Schema::new(["unused"]).unwrap());
    world.add_entity("AAA", EntityKind::Country);
    world.add_entity("BBB", EntityKind::Country);
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
        .set_politics(Politics {
            ruler_skill_bounds: Some(RulerSkillBounds {
                minimum: 0,
                maximum: 6,
            }),
            countries: [(
                EntityId(0),
                CountryPolitics {
                    national_focus: Some(NationalFocus::default()),
                    ruler: Some(PersonState {
                        administration: Some(2),
                        diplomacy: Some(5),
                        military: Some(1),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            )]
            .into(),
            ..Default::default()
        })
        .unwrap();
    world
        .set_metrics(Metrics {
            definitions: [(
                "power".into(),
                MetricDefinition {
                    kind: EntityKind::Country,
                    minimum: -100,
                    maximum: i64::MAX,
                    baseline: 0,
                    divisor: 1,
                    modifier: None,
                    unresolved_properties: Default::default(),
                    below_baseline: None,
                    growth_ceiling: true,
                },
            )]
            .into(),
            entities: [
                (EntityId(0), [("power".into(), Some(10))].into()),
                (EntityId(1), [("power".into(), Some(0))].into()),
            ]
            .into(),
            ceilings: [(EntityId(0), [("power".into(), Some(15))].into())].into(),
        })
        .unwrap();
    world
        .set_modifiers(Modifiers {
            definitions: [
                (
                    "first".into(),
                    ModifierDefinition {
                        values_micros: [("income".into(), 600_000)].into(),
                        ..Default::default()
                    },
                ),
                (
                    "second".into(),
                    ModifierDefinition {
                        values_micros: [("income".into(), 600_000)].into(),
                        ..Default::default()
                    },
                ),
                (
                    "negative".into(),
                    ModifierDefinition {
                        values_micros: [("income".into(), -100_600_000)].into(),
                        ..Default::default()
                    },
                ),
            ]
            .into(),
            ..Default::default()
        })
        .unwrap();
    let formula = AttributeFormula {
        base: AttributeBase::Constant { micros: 3_000_000 },
        channels: vec![AttributeChannel {
            flat: vec![
                AttributeTerm::RulerSkill {
                    target: Target::Current,
                    skill: RulerSkill::Administration,
                    micros_per_unit: 1_000_000,
                },
                AttributeTerm::NationalFocus {
                    target: Target::Current,
                    skill: RulerSkill::Administration,
                    increase_micros: 2_000_000,
                    decrease_micros: -1_000_000,
                },
                AttributeTerm::TruncatedSum {
                    source: "income_modifiers".into(),
                    quantum_micros: 1_000_000,
                    terms: vec![AttributeTerm::Modifiers {
                        target: Target::Current,
                        property: "income".into(),
                    }],
                },
            ],
            rates: vec![],
        }],
        minimum_micros: None,
        maximum_micros: None,
        result_quantum_micros: None,
        unresolved: Default::default(),
    };
    world
        .set_attributes(Attributes {
            formulas: [
                (EntityId(0), [("income".into(), formula.clone())].into()),
                (EntityId(1), [("income".into(), formula)].into()),
            ]
            .into(),
            ..Default::default()
        })
        .unwrap();
    world
        .set_monthly_accruals(MonthlyAccruals {
            rules: [(
                EntityId(0),
                [(
                    "power".into(),
                    AccrualRule {
                        attribute: "income".into(),
                        divisor: 1_000_000,
                    },
                )]
                .into(),
            )]
            .into(),
            ..Default::default()
        })
        .unwrap();
    world
}

fn income(world: &World) -> i64 {
    world
        .numeric_value(
            EntityId(0),
            &NumericSlot::Attribute {
                name: "income".into(),
                divisor: 1_000_000,
            },
        )
        .unwrap()
}

#[test]
fn grouped_truncation_live_people_focus_and_modifier_expiration_survive_resume() {
    let mut world = fixture();
    world
        .attach_modifier(
            EntityId(0),
            "first",
            Some(20),
            None,
            ModifierRenewal::Replace,
        )
        .unwrap();
    assert_eq!(income(&world), 5);
    world
        .attach_modifier(EntityId(0), "second", None, None, ModifierRenewal::Replace)
        .unwrap();
    assert_eq!(income(&world), 6);
    world
        .schedule(
            EntityId(0),
            20,
            vec![
                Action::If {
                    condition: Condition::NumericCompare {
                        target: Target::Current,
                        slot: NumericSlot::Metric("power".into()),
                        comparison: NumericComparison::Equal,
                        value: 10,
                    },
                    then: vec![Action::Emit("before_month".into())],
                    otherwise: vec![],
                },
                Action::ChangeRulerSkill {
                    target: Target::Current,
                    skill: RulerSkill::Administration,
                    delta: 1,
                },
                Action::ChangeNationalFocus {
                    target: Target::Current,
                    focus: NationalFocus {
                        skill: Some(RulerSkill::Administration),
                    },
                },
            ],
        )
        .unwrap();
    world.advance_to(19).unwrap();
    let snapshot = world.save("fixture").unwrap();
    let mut resumed = World::restore(&snapshot, "fixture", world.schema()).unwrap();
    for current in [&mut world, &mut resumed] {
        current.advance_to(20).unwrap();
        assert_eq!(income(current), 8);
        assert_eq!(
            current
                .metrics()
                .value(current, EntityId(0), "power")
                .unwrap(),
            15
        );
        let account = &current
            .monthly_accruals()
            .unwrap()
            .last_settlement
            .as_ref()
            .unwrap()
            .accounts[&EntityId(0)]["power"];
        assert_eq!((account.income, account.credited), (Some(8), Some(5)));
        assert_eq!(current.events()[0].2, "before_month");
        current
            .attach_modifier(
                EntityId(0),
                "negative",
                None,
                None,
                ModifierRenewal::Replace,
            )
            .unwrap();
        assert_eq!(income(current), -92);
        current.advance_to(82).unwrap();
        assert_eq!(
            current
                .metrics()
                .value(current, EntityId(0), "power")
                .unwrap(),
            -100
        );
    }
    assert_eq!(
        world.save("fixture").unwrap(),
        resumed.save("fixture").unwrap()
    );
}

#[test]
fn unknown_income_quantity_and_ceiling_never_produce_a_partial_credit() {
    let mut world = fixture();
    world
        .set_metric_ceiling(EntityId(0), "power", None)
        .unwrap();
    world.advance_to(20).unwrap();
    let account = &world
        .monthly_accruals()
        .unwrap()
        .last_settlement
        .as_ref()
        .unwrap()
        .accounts[&EntityId(0)]["power"];
    assert_eq!(account.income, Some(5));
    assert_eq!(account.quantity_after, None);
    assert!(account.unresolved.contains("unknown_growth_ceiling"));
    let snapshot = world.save("fixture").unwrap();
    let restored = World::restore(&snapshot, "fixture", world.schema()).unwrap();
    assert_eq!(restored.save("fixture").unwrap(), snapshot);

    let mut world = fixture();
    let mut accruals = world.monthly_accruals().unwrap().clone();
    accruals.rules.insert(
        EntityId(1),
        [(
            "power".into(),
            AccrualRule {
                attribute: "income".into(),
                divisor: 1_000_000,
            },
        )]
        .into(),
    );
    world.set_monthly_accruals(accruals).unwrap();
    world.advance_to(20).unwrap();
    assert_eq!(
        world.metrics().value(&world, EntityId(0), "power").unwrap(),
        15
    );
    assert!(world.metrics().value(&world, EntityId(1), "power").is_err());
    assert!(
        world
            .monthly_accruals()
            .unwrap()
            .last_settlement
            .as_ref()
            .unwrap()
            .accounts[&EntityId(1)]["power"]
            .income
            .is_none()
    );
}

#[test]
fn late_country_overflow_rolls_back_tax_and_all_accrual_balances() {
    let mut world = fixture();
    let mut politics = world.politics().clone();
    politics.countries.insert(
        EntityId(1),
        CountryPolitics {
            ruler: Some(PersonState {
                administration: Some(0),
                ..Default::default()
            }),
            ..Default::default()
        },
    );
    world.set_politics(politics).unwrap();
    world.add_entity("Taxpayer", EntityKind::Province);
    world.set_owner(EntityId(2), EntityId(0)).unwrap();
    let mut attributes = world.attributes().clone();
    attributes.formulas.get_mut(&EntityId(1)).unwrap().insert(
        "income".into(),
        AttributeFormula {
            base: AttributeBase::Constant { micros: i64::MAX },
            channels: vec![AttributeChannel {
                flat: vec![AttributeTerm::RulerSkill {
                    target: Target::Current,
                    skill: RulerSkill::Administration,
                    micros_per_unit: 1,
                }],
                rates: vec![],
            }],
            minimum_micros: None,
            maximum_micros: None,
            result_quantum_micros: None,
            unresolved: Default::default(),
        },
    );
    attributes.formulas.insert(
        EntityId(2),
        [(
            "tax".into(),
            AttributeFormula {
                base: AttributeBase::Constant { micros: 12_000 },
                channels: vec![],
                minimum_micros: None,
                maximum_micros: None,
                result_quantum_micros: None,
                unresolved: Default::default(),
            },
        )]
        .into(),
    );
    world.set_attributes(attributes).unwrap();
    world
        .set_metric_value(EntityId(1), "power", Some(-100))
        .unwrap();
    world
        .set_metric_ceiling(EntityId(1), "power", Some(i64::MAX))
        .unwrap();
    let mut accruals = world.monthly_accruals().unwrap().clone();
    accruals.rules.insert(
        EntityId(1),
        [(
            "power".into(),
            AccrualRule {
                attribute: "income".into(),
                divisor: 1,
            },
        )]
        .into(),
    );
    world.set_monthly_accruals(accruals).unwrap();
    world
        .set_country_resources(
            [(
                EntityId(0),
                CountryResources {
                    treasury_milli: Some(1000),
                    ..Default::default()
                },
            )]
            .into(),
        )
        .unwrap();
    world
        .set_taxation(Taxation::new("tax", [EntityId(0)].into()))
        .unwrap();
    world
        .schedule(
            EntityId(1),
            20,
            vec![Action::ChangeRulerSkill {
                target: Target::Current,
                skill: RulerSkill::Administration,
                delta: 6,
            }],
        )
        .unwrap();
    assert!(world.advance_to(20).is_err());
    assert_eq!(
        world.country_resources()[&EntityId(0)].treasury_milli,
        Some(1000)
    );
    assert_eq!(world.metrics().entities[&EntityId(0)]["power"], Some(10));
    assert_eq!(world.metrics().entities[&EntityId(1)]["power"], Some(-100));
    assert_eq!(world.taxation().unwrap().settlement_count, 0);
    assert_eq!(world.monthly_accruals().unwrap().settlement_count, 0);
}

#[test]
fn forged_receipts_invalid_groups_and_read_only_calculations_are_rejected() {
    let mut world = fixture();
    assert!(
        world
            .update_numeric(
                EntityId(0),
                &NumericSlot::Attribute {
                    name: "income".into(),
                    divisor: 1_000_000
                },
                NumericOperation::Set,
                1
            )
            .is_err()
    );
    assert!(
        world
            .numeric_value(
                EntityId(0),
                &NumericSlot::Attribute {
                    name: "income".into(),
                    divisor: 0
                }
            )
            .is_err()
    );
    let mut invalid = world.attributes().clone();
    let term = &mut invalid
        .formulas
        .get_mut(&EntityId(0))
        .unwrap()
        .get_mut("income")
        .unwrap()
        .channels[0]
        .flat[2];
    if let AttributeTerm::TruncatedSum { quantum_micros, .. } = term {
        *quantum_micros = 0;
    }
    assert!(world.set_attributes(invalid).is_err());
    world.advance_to(20).unwrap();
    let snapshot = world.save("fixture").unwrap();
    let mut forged: serde_json::Value = serde_json::from_str(&snapshot).unwrap();
    forged["monthly_accruals"]["last_settlement"]["accounts"]["0"]["power"]["credited"] = 0.into();
    assert!(World::restore(&forged.to_string(), "fixture", world.schema()).is_err());
    let mut forged: serde_json::Value = serde_json::from_str(&snapshot).unwrap();
    forged["monthly_system_id"] = "monthly-tax:1".into();
    assert!(World::restore(&forged.to_string(), "fixture", world.schema()).is_err());
}
