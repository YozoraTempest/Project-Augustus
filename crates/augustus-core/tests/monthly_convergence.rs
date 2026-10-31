// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::*;

fn fixture() -> World {
    let mut world = World::new(Schema::new(["unused"]).unwrap());
    for name in ["AAA", "BBB", "CCC"] {
        world.add_entity(name, EntityKind::Country);
    }
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
        .set_metrics(Metrics {
            definitions: [(
                "influence".into(),
                MetricDefinition {
                    kind: EntityKind::Country,
                    minimum: i64::MIN,
                    maximum: i64::MAX,
                    baseline: 0,
                    divisor: 1000,
                    modifier: None,
                    below_baseline: None,
                    unresolved_properties: Default::default(),
                    growth_ceiling: false,
                },
            )]
            .into(),
            entities: [
                (EntityId(0), [("influence".into(), Some(250))].into()),
                (EntityId(1), [("influence".into(), Some(-150))].into()),
                (EntityId(2), [("influence".into(), Some(0))].into()),
            ]
            .into(),
            ceilings: Default::default(),
        })
        .unwrap();
    world
        .set_monthly_convergence(Some(MonthlyConvergence {
            rules: (0..3)
                .map(|id| {
                    (
                        EntityId(id),
                        [(
                            "influence".into(),
                            ConvergenceRule {
                                target_value: 0,
                                step: Some(100),
                                condition: None,
                                reset_when_inactive: false,
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
    world.metrics().entities[&EntityId(id)]["influence"]
}

#[test]
fn preview_monthly_convergence_reuses_next_settlement_without_mutation() {
    let world = fixture();
    let snapshot = world.save("preview").unwrap();
    let settlement = world.preview_monthly_convergence().unwrap();
    assert_eq!(settlement.day, 0);
    let account = &settlement.accounts[&EntityId(0)]["influence"];
    assert_eq!(
        (
            account.applies,
            account.quantity_before,
            account.quantity_after,
            account.changed,
        ),
        (Some(true), Some(250), Some(150), Some(-100))
    );
    assert!(account.unresolved.is_empty());
    assert_eq!(world.save("preview").unwrap(), snapshot);
    assert_eq!(world.monthly_convergence().unwrap().settlement_count, 0);
    assert!(
        world
            .monthly_convergence()
            .unwrap()
            .last_settlement
            .is_none()
    );
}

#[test]
fn both_signs_reach_zero_and_split_restore_preserves_months_and_receipts() {
    let mut whole = fixture();
    whole.advance_to(19).unwrap();
    assert_eq!(quantity(&whole, 0), Some(250));
    whole.advance_to(20).unwrap();
    assert_eq!(
        [
            quantity(&whole, 0),
            quantity(&whole, 1),
            quantity(&whole, 2)
        ],
        [Some(150), Some(-50), Some(0)]
    );
    whole.advance_to(51).unwrap();
    assert_eq!(
        [quantity(&whole, 0), quantity(&whole, 1)],
        [Some(50), Some(0)]
    );
    let saved = whole.save("convergence").unwrap();
    let mut restored = World::restore(&saved, "convergence", whole.schema()).unwrap();
    whole.advance_to(120).unwrap();
    restored.advance_to(79).unwrap();
    restored.advance_to(120).unwrap();
    assert_eq!(whole, restored);
    assert_eq!(
        [quantity(&whole, 0), quantity(&whole, 1)],
        [Some(0), Some(0)]
    );
    assert_eq!(whole.monthly_convergence().unwrap().settlement_count, 4);
    assert_eq!(
        whole.save("convergence").unwrap(),
        restored.save("convergence").unwrap()
    );
}

#[test]
fn a_due_write_is_visible_to_the_same_months_convergence() {
    let mut world = fixture();
    world
        .schedule(
            EntityId(0),
            20,
            vec![Action::UpdateNumeric {
                target: Target::Current,
                slot: NumericSlot::Metric("influence".into()),
                operation: NumericOperation::Set,
                value: 25,
            }],
        )
        .unwrap();
    world.advance_to(20).unwrap();
    assert_eq!(quantity(&world, 0), Some(0));
    let row = &world
        .monthly_convergence()
        .unwrap()
        .last_settlement
        .as_ref()
        .unwrap()
        .accounts[&EntityId(0)]["influence"];
    assert_eq!(
        (row.quantity_before, row.quantity_after, row.changed),
        (Some(25), Some(0), Some(-25))
    );
}

#[test]
fn unknown_rate_or_quantity_propagates_while_an_inactive_rule_preserves_its_value() {
    let mut world = fixture();
    world
        .set_metric_value(EntityId(0), "influence", None)
        .unwrap();
    let mut state = world.monthly_convergence().unwrap().clone();
    state
        .rules
        .get_mut(&EntityId(1))
        .unwrap()
        .get_mut("influence")
        .unwrap()
        .step = None;
    state
        .rules
        .get_mut(&EntityId(2))
        .unwrap()
        .get_mut("influence")
        .unwrap()
        .condition = Some(Condition::Always(false));
    world.set_monthly_convergence(Some(state)).unwrap();
    world.advance_to(20).unwrap();
    assert_eq!(
        [
            quantity(&world, 0),
            quantity(&world, 1),
            quantity(&world, 2)
        ],
        [None, None, Some(0)]
    );
    let accounts = &world
        .monthly_convergence()
        .unwrap()
        .last_settlement
        .as_ref()
        .unwrap()
        .accounts;
    assert!(
        accounts[&EntityId(0)]["influence"]
            .unresolved
            .contains("unknown_quantity")
    );
    assert!(
        accounts[&EntityId(1)]["influence"]
            .unresolved
            .contains("unknown_step")
    );
    assert!(accounts[&EntityId(2)]["influence"].unresolved.is_empty());
    let saved = world.save("unknown").unwrap();
    assert_eq!(
        World::restore(&saved, "unknown", world.schema()).unwrap(),
        world
    );
}

#[test]
fn receipt_corruption_and_removing_a_registered_metric_are_rejected() {
    let mut world = fixture();
    world.advance_to(20).unwrap();
    let saved = world.save("receipt").unwrap();
    for path in ["count", "day", "quantity", "changed", "names"] {
        let mut value: serde_json::Value = serde_json::from_str(&saved).unwrap();
        let state = &mut value["monthly_convergence"];
        match path {
            "count" => state["settlement_count"] = 2.into(),
            "day" => state["last_settlement"]["day"] = 19.into(),
            "quantity" => {
                state["last_settlement"]["accounts"]["0"]["influence"]["quantity_after"] =
                    149.into()
            }
            "changed" => {
                state["last_settlement"]["accounts"]["0"]["influence"]["changed"] = (-99).into()
            }
            _ => {
                state["last_settlement"]["accounts"]["0"]
                    .as_object_mut()
                    .unwrap()
                    .remove("influence");
            }
        }
        assert!(
            World::restore(&value.to_string(), "receipt", world.schema()).is_err(),
            "{path}"
        );
    }
    let before = world.clone();
    assert!(world.set_metrics(Metrics::default()).is_err());
    assert_eq!(world, before);
}

#[test]
fn approach_handles_wide_distances_and_rejects_invalid_steps_without_writes() {
    let mut world = fixture();
    let slot = NumericSlot::Metric("influence".into());
    world
        .set_metric_value(EntityId(0), "influence", Some(i64::MIN))
        .unwrap();
    world
        .update_numeric(
            EntityId(0),
            &slot,
            NumericOperation::Approach { step: i64::MAX },
            i64::MAX,
        )
        .unwrap();
    assert_eq!(quantity(&world, 0), Some(-1));
    world
        .update_numeric(
            EntityId(0),
            &slot,
            NumericOperation::Approach { step: i64::MAX },
            i64::MAX,
        )
        .unwrap();
    assert_eq!(quantity(&world, 0), Some(i64::MAX - 1));
    world
        .update_numeric(
            EntityId(0),
            &slot,
            NumericOperation::Approach { step: 100 },
            i64::MAX,
        )
        .unwrap();
    assert_eq!(quantity(&world, 0), Some(i64::MAX));
    let before = world.clone();
    world
        .update_numeric(
            EntityId(0),
            &slot,
            NumericOperation::Approach { step: 0 },
            0,
        )
        .unwrap();
    assert_eq!(world, before);
    assert!(
        world
            .update_numeric(
                EntityId(0),
                &slot,
                NumericOperation::Approach { step: -1 },
                0
            )
            .is_err()
    );
    assert_eq!(world, before);
}

#[test]
fn late_prepare_error_commits_no_convergence_balances() {
    let mut world = fixture();
    let mut metrics = world.metrics().clone();
    metrics.definitions.get_mut("influence").unwrap().modifier = Some(ModifierDefinition {
        values_micros: [("output".into(), i64::MAX)].into(),
        ..Default::default()
    });
    world
        .set_metric_value(EntityId(1), "influence", Some(-1000))
        .unwrap();
    metrics.entities = world.metrics().entities.clone();
    world.set_metrics(metrics).unwrap();
    let mut rules = world.monthly_convergence().unwrap().clone();
    rules
        .rules
        .get_mut(&EntityId(1))
        .unwrap()
        .get_mut("influence")
        .unwrap()
        .target_value = i64::MIN;
    world.set_monthly_convergence(Some(rules)).unwrap();
    assert!(world.advance_to(20).is_err());
    assert_eq!(
        [quantity(&world, 0), quantity(&world, 1)],
        [Some(250), Some(-1000)]
    );
    assert_eq!(world.monthly_convergence().unwrap().settlement_count, 0);
    assert!(world.save("failed").is_err());
}

#[test]
fn selecting_an_entire_replacement_discards_standard_convergence() {
    struct Replacement;
    impl MonthlySystem for Replacement {
        fn id(&self) -> &str {
            "replacement:1"
        }
        fn validate(&self, _: &World) -> Result<(), Error> {
            Ok(())
        }
        fn settle(&self, _: &mut World) -> Result<(), Error> {
            Ok(())
        }
    }
    let mut world = fixture();
    world.select_monthly_system(&Replacement).unwrap();
    assert!(world.monthly_convergence().is_none());
    world.advance_with_monthly_system(51, &Replacement).unwrap();
    assert_eq!(quantity(&world, 0), Some(250));
    let saved = world.save("replacement").unwrap();
    assert!(!saved.contains("monthly_convergence"));
    assert_eq!(
        World::restore(&saved, "replacement", world.schema()).unwrap(),
        world
    );
}

#[test]
fn inactive_reset_recovers_unknown_stock_and_fixed_points_do_not_need_a_rate() {
    let mut world = fixture();
    world
        .set_metric_value(EntityId(0), "influence", None)
        .unwrap();
    let mut rules = world.monthly_convergence().unwrap().clone();
    let reset = rules
        .rules
        .get_mut(&EntityId(0))
        .unwrap()
        .get_mut("influence")
        .unwrap();
    reset.condition = Some(Condition::Always(false));
    reset.reset_when_inactive = true;
    let fixed = rules
        .rules
        .get_mut(&EntityId(2))
        .unwrap()
        .get_mut("influence")
        .unwrap();
    fixed.step = None;
    world.set_monthly_convergence(Some(rules)).unwrap();
    world.advance_to(20).unwrap();
    assert_eq!(
        [quantity(&world, 0), quantity(&world, 2)],
        [Some(0), Some(0)]
    );
    let rows = &world
        .monthly_convergence()
        .unwrap()
        .last_settlement
        .as_ref()
        .unwrap()
        .accounts;
    assert_eq!(rows[&EntityId(0)]["influence"].changed, None);
    assert!(rows[&EntityId(2)]["influence"].unresolved.is_empty());
    let saved = world.save("reset").unwrap();
    assert_eq!(
        World::restore(&saved, "reset", world.schema()).unwrap(),
        world
    );
}

#[test]
fn accrual_and_convergence_cannot_own_the_same_stock_in_either_install_order() {
    let mut world = fixture();
    world
        .set_attributes(Attributes {
            formulas: [(
                EntityId(0),
                [(
                    "income".into(),
                    AttributeFormula {
                        base: AttributeBase::Constant { micros: 1_000_000 },
                        channels: vec![],
                        minimum_micros: None,
                        maximum_micros: None,
                        result_quantum_micros: None,
                        unresolved: Default::default(),
                    },
                )]
                .into(),
            )]
            .into(),
            ..Default::default()
        })
        .unwrap();
    let accrual = MonthlyAccruals {
        rules: [(
            EntityId(0),
            [(
                "influence".into(),
                AccrualRule {
                    attribute: "income".into(),
                    divisor: 1_000_000,
                },
            )]
            .into(),
        )]
        .into(),
        ..Default::default()
    };
    let convergence = world.monthly_convergence().unwrap().clone();
    let before = world.clone();
    assert!(world.set_monthly_accruals(accrual.clone()).is_err());
    assert_eq!(world, before);
    world.set_monthly_convergence(None).unwrap();
    world.set_monthly_accruals(accrual).unwrap();
    let before = world.clone();
    assert!(world.set_monthly_convergence(Some(convergence)).is_err());
    assert_eq!(world, before);
}
