// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::*;

fn formula(base: AttributeBase) -> AttributeFormula {
    AttributeFormula {
        base,
        channels: vec![],
        minimum_micros: None,
        maximum_micros: None,
        result_quantum_micros: None,
        unresolved: Default::default(),
    }
}
fn fixture() -> World {
    let mut world = World::new(Schema::new(["unused"]).unwrap());
    for name in ["AAA", "BBB", "CCC"] {
        world.add_entity(name, EntityKind::Country);
    }
    let port = world.add_entity("Port", EntityKind::Province);
    world.set_owner(port, EntityId(0)).unwrap();
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
        .set_country_resources(
            (0..3)
                .map(|id| {
                    (
                        EntityId(id),
                        CountryResources {
                            treasury_milli: Some(5000),
                            manpower_milli: Some(if id == 1 { 101500000 } else { 1501000 }),
                            mercantilism_milli: None,
                        },
                    )
                })
                .collect(),
        )
        .unwrap();
    world
        .set_attributes(Attributes {
            formulas: (0..3)
                .map(|id| {
                    (
                        EntityId(id),
                        [
                            (
                                "income".into(),
                                formula(AttributeBase::Constant { micros: 640000000 }),
                            ),
                            (
                                "ceiling".into(),
                                formula(AttributeBase::Constant { micros: 3000000000 }),
                            ),
                        ]
                        .into(),
                    )
                })
                .collect(),
            ..Default::default()
        })
        .unwrap();
    world
        .set_monthly_stocks(Some(MonthlyStocks {
            rules: (0..3)
                .map(|id| {
                    (
                        EntityId(id),
                        [(
                            CountryResource::Manpower,
                            StockRule {
                                income_attribute: "income".into(),
                                divisor: 1000,
                                ceiling_attribute: Some("ceiling".into()),
                                condition: (id == 2).then_some(Condition::Always(false)),
                            },
                        )]
                        .into(),
                    )
                })
                .collect(),
            ..Default::default()
        }))
        .unwrap();
    world
}
fn quantity(world: &World, id: usize) -> Option<i64> {
    world.country_resources()[&EntityId(id)].manpower_milli
}

#[test]
fn monthly_growth_preserves_excess_and_due_plans_read_the_old_stock() {
    let mut whole = fixture();
    whole
        .schedule(
            EntityId(0),
            20,
            vec![Action::If {
                condition: Condition::NumericCompare {
                    target: Target::Current,
                    slot: NumericSlot::Resource(CountryResource::Manpower),
                    comparison: NumericComparison::Equal,
                    value: 1501000,
                },
                then: vec![Action::Emit("before_month".into())],
                otherwise: vec![],
            }],
        )
        .unwrap();
    whole.advance_to(45).unwrap();
    assert_eq!(
        [
            quantity(&whole, 0),
            quantity(&whole, 1),
            quantity(&whole, 2)
        ],
        [Some(2141000), Some(101500000), Some(1501000)]
    );
    let saved = whole.save("stocks:1").unwrap();
    let mut resumed = World::restore(&saved, "stocks:1", whole.schema()).unwrap();
    whole.advance_to(82).unwrap();
    resumed.advance_to(51).unwrap();
    resumed.advance_to(82).unwrap();
    assert_eq!(
        whole.save("stocks:1").unwrap(),
        resumed.save("stocks:1").unwrap()
    );
    assert_eq!(quantity(&whole, 0), Some(3000000));
    assert_eq!(quantity(&whole, 1), Some(101500000));
    assert_eq!(
        whole.country_resources()[&EntityId(0)].treasury_milli,
        Some(5000)
    );
    assert_eq!(
        whole
            .monthly_stocks()
            .unwrap()
            .last_settlement
            .as_ref()
            .unwrap()
            .accounts[&EntityId(1)][&CountryResource::Manpower]
            .credited,
        Some(0)
    );
    assert_eq!(whole.events()[0].2, "before_month");
    whole
        .update_numeric(
            EntityId(0),
            &NumericSlot::Resource(CountryResource::Manpower),
            NumericOperation::Add,
            100000000,
        )
        .unwrap();
    whole.advance_to(110).unwrap();
    assert_eq!(quantity(&whole, 0), Some(103000000));
}

#[test]
fn missing_inputs_stay_unknown_and_inactive_rules_preserve_the_stock() {
    let mut world = fixture();
    let mut attrs = world.attributes().clone();
    attrs.formulas.get_mut(&EntityId(0)).unwrap().insert(
        "income".into(),
        formula(AttributeBase::Unknown {
            reason: "missing_production".into(),
        }),
    );
    attrs.formulas.get_mut(&EntityId(2)).unwrap().insert(
        "income".into(),
        formula(AttributeBase::Unknown {
            reason: "dormant_production".into(),
        }),
    );
    world.set_attributes(attrs).unwrap();
    world.advance_to(20).unwrap();
    assert_eq!(quantity(&world, 0), None);
    assert_eq!(quantity(&world, 2), Some(1501000));
    let receipt = world
        .monthly_stocks()
        .unwrap()
        .last_settlement
        .as_ref()
        .unwrap();
    assert!(
        receipt.accounts[&EntityId(0)][&CountryResource::Manpower]
            .unresolved
            .contains("unknown_income")
    );
    assert!(
        receipt.accounts[&EntityId(2)][&CountryResource::Manpower]
            .unresolved
            .is_empty()
    );
    let saved = world.save("stocks:1").unwrap();
    assert_eq!(
        World::restore(&saved, "stocks:1", world.schema())
            .unwrap()
            .save("stocks:1")
            .unwrap(),
        saved
    );
}

#[test]
fn stock_failure_prevents_all_standard_monthly_commits() {
    let mut world = fixture();
    world.set_monthly_stocks(None).unwrap();
    world
        .set_metrics(Metrics {
            definitions: [(
                "power".into(),
                MetricDefinition {
                    kind: EntityKind::Country,
                    minimum: 0,
                    maximum: i64::MAX,
                    baseline: 0,
                    divisor: 1,
                    modifier: None,
                    below_baseline: None,
                    unresolved_properties: Default::default(),
                    growth_ceiling: false,
                },
            )]
            .into(),
            entities: [(EntityId(0), [("power".into(), Some(10))].into())].into(),
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
                        divisor: 1000,
                    },
                )]
                .into(),
            )]
            .into(),
            ..Default::default()
        })
        .unwrap();
    world
        .update_numeric(
            EntityId(1),
            &NumericSlot::Resource(CountryResource::Manpower),
            NumericOperation::Set,
            i64::MAX,
        )
        .unwrap();
    world
        .set_monthly_stocks(Some(MonthlyStocks {
            rules: [(
                EntityId(1),
                [(
                    CountryResource::Manpower,
                    StockRule {
                        income_attribute: "income".into(),
                        divisor: 1000,
                        ceiling_attribute: None,
                        condition: None,
                    },
                )]
                .into(),
            )]
            .into(),
            ..Default::default()
        }))
        .unwrap();
    assert!(world.advance_to(20).is_err());
    assert_eq!(world.metrics().entities[&EntityId(0)]["power"], Some(10));
    assert_eq!(world.monthly_accruals().unwrap().settlement_count, 0);
    assert_eq!(world.monthly_stocks().unwrap().settlement_count, 0);
    assert!(world.save("stocks:1").is_err());
}

#[test]
fn missing_calculations_and_corrupt_receipts_are_rejected() {
    let mut world = fixture();
    let before = world.clone();
    assert!(world.set_attributes(Attributes::default()).is_err());
    assert_eq!(world, before);
    world.advance_to(20).unwrap();
    let saved: serde_json::Value = serde_json::from_str(&world.save("stocks:1").unwrap()).unwrap();
    for (pointer, value) in [
        ("/monthly_stocks/settlement_count", serde_json::json!(2)),
        (
            "/monthly_stocks/rules/0/Manpower/divisor",
            serde_json::json!(0),
        ),
        (
            "/monthly_stocks/last_settlement/accounts/0/Manpower/quantity_after",
            serde_json::json!(1),
        ),
        (
            "/monthly_stocks/last_settlement/accounts/2/Manpower/income",
            serde_json::json!(1000),
        ),
    ] {
        let mut corrupt = saved.clone();
        *corrupt.pointer_mut(pointer).unwrap() = value;
        assert!(
            World::restore(&corrupt.to_string(), "stocks:1", world.schema()).is_err(),
            "{pointer}"
        );
    }
}

struct Replacement;

#[test]
fn tax_and_stock_settlements_compose_and_duplicate_treasury_writes_are_rejected() {
    let mut world = fixture();
    let mut attributes = world.attributes().clone();
    attributes.formulas.insert(
        EntityId(3),
        [(
            "annual_tax".into(),
            formula(AttributeBase::Constant { micros: 12000000 }),
        )]
        .into(),
    );
    world.set_attributes(attributes.clone()).unwrap();
    let tax = Taxation::new("annual_tax", [EntityId(0)].into());
    world.set_taxation(tax.clone()).unwrap();
    let before = world.clone();
    let mut duplicate = world.monthly_stocks().unwrap().clone();
    let rule = duplicate.rules[&EntityId(0)][&CountryResource::Manpower].clone();
    duplicate
        .rules
        .get_mut(&EntityId(0))
        .unwrap()
        .insert(CountryResource::Treasury, rule.clone());
    assert!(world.set_monthly_stocks(Some(duplicate)).is_err());
    assert_eq!(world, before);
    world.advance_to(20).unwrap();
    assert_eq!(
        world.country_resources()[&EntityId(0)].treasury_milli,
        Some(6000)
    );
    assert_eq!(quantity(&world, 0), Some(2141000));
    let mut other = fixture();
    other.set_attributes(attributes).unwrap();
    other
        .set_monthly_stocks(Some(MonthlyStocks {
            rules: [(EntityId(0), [(CountryResource::Treasury, rule)].into())].into(),
            ..Default::default()
        }))
        .unwrap();
    let before = other.clone();
    assert!(other.set_taxation(tax).is_err());
    assert_eq!(other, before);
    other
        .set_monthly_stocks(Some(MonthlyStocks {
            rules: [(
                EntityId(1),
                [(
                    CountryResource::Treasury,
                    StockRule {
                        income_attribute: "income".into(),
                        divisor: 1000,
                        ceiling_attribute: None,
                        condition: None,
                    },
                )]
                .into(),
            )]
            .into(),
            ..Default::default()
        }))
        .unwrap();
    other
        .set_taxation(Taxation::new("annual_tax", [EntityId(0)].into()))
        .unwrap();
    other.advance_to(20).unwrap();
    assert_eq!(
        other.country_resources()[&EntityId(0)].treasury_milli,
        Some(6000)
    );
    assert_eq!(
        other.country_resources()[&EntityId(1)].treasury_milli,
        Some(645000)
    );
}

impl MonthlySystem for Replacement {
    fn id(&self) -> &str {
        "test-resource-month:1"
    }
    fn validate(&self, _: &World) -> Result<(), Error> {
        Ok(())
    }
    fn settle(&self, world: &mut World) -> Result<(), Error> {
        world.update_numeric(
            EntityId(0),
            &NumericSlot::Resource(CountryResource::Manpower),
            NumericOperation::Add,
            1000,
        )
    }
}

#[test]
fn whole_monthly_replacement_disables_the_standard_stock_executor() {
    let mut world = fixture();
    world.select_monthly_system(&Replacement).unwrap();
    assert!(world.monthly_stocks().is_none());
    world.advance_with_monthly_system(20, &Replacement).unwrap();
    assert_eq!(quantity(&world, 0), Some(1502000));
    let saved = world.save("stocks:1").unwrap();
    let mut resumed = World::restore(&saved, "stocks:1", world.schema()).unwrap();
    resumed
        .advance_with_monthly_system(51, &Replacement)
        .unwrap();
    assert_eq!(quantity(&resumed, 0), Some(1503000));
}
