// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    AttributeBase, AttributeFormula, Attributes, Calendar, CashflowBreakdown, CountryResources,
    EntityId, EntityKind, MonthlyExternalCashflows, MonthlyProduction, Schema, Taxation,
    VanillaProductionInput, VanillaProductionInputState, VanillaTradeIncomeInput,
    VanillaTradeIncomePassInput, World, WorldDate,
};
use std::collections::{BTreeMap, BTreeSet};

fn calendar() -> Calendar {
    Calendar {
        start: WorldDate {
            year: 1444,
            month: 11,
            day: 11,
        },
        month_lengths: [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31],
    }
}

fn known_input() -> VanillaProductionInputState {
    VanillaProductionInputState {
        input: Some(VanillaProductionInput {
            base_value_milli: 1000,
            country_modifier_milli: 20,
            legacy_province_modifier_milli: 0,
            queried_province_modifier_milli: -10,
        }),
        unresolved: BTreeSet::new(),
    }
}

fn world(input: VanillaProductionInputState) -> (World, augustus_core::EntityId) {
    let mut world = World::new(Schema::new([] as [&str; 0]).unwrap());
    let country = world.add_entity("AAA", EntityKind::Country);
    let province = world.add_entity("1", EntityKind::Province);
    world.set_owner(province, country).unwrap();
    world
        .set_country_resources(
            [(
                country,
                CountryResources {
                    treasury_milli: Some(10_000),
                    ..Default::default()
                },
            )]
            .into_iter()
            .collect(),
        )
        .unwrap();
    world.set_calendar(calendar()).unwrap();
    world
        .set_monthly_production(MonthlyProduction::new(BTreeMap::from([(province, input)])))
        .unwrap();
    (world, country)
}

#[test]
fn monthly_production_credits_known_income_and_round_trips() {
    let (mut world, country) = world(known_input());
    world.advance_to(20).unwrap();

    assert_eq!(
        world.country_resources()[&country].treasury_milli,
        Some(10_084)
    );
    let settlement = world
        .monthly_production()
        .unwrap()
        .last_settlement
        .as_ref()
        .unwrap();
    let account = &settlement.accounts[&country];
    assert_eq!(account.annual_value_milli, Some(1009));
    assert_eq!(account.monthly_value_milli, Some(84));
    assert_eq!(account.credited_milli, Some(84));

    let snapshot = world.save("production").unwrap();
    let restored = World::restore(&snapshot, "production", world.schema()).unwrap();
    assert_eq!(restored, world);
}

#[test]
fn monthly_production_inputs_refresh_after_a_settlement_without_resetting_receipts() {
    let (mut world, _) = world(known_input());
    world.advance_to(20).unwrap();
    let replacement = VanillaProductionInputState {
        input: Some(VanillaProductionInput {
            base_value_milli: 2_000,
            country_modifier_milli: 0,
            legacy_province_modifier_milli: 0,
            queried_province_modifier_milli: 0,
        }),
        unresolved: BTreeSet::new(),
    };

    world
        .set_monthly_production_inputs(BTreeMap::from([(EntityId(1), replacement.clone())]))
        .unwrap();

    let production = world.monthly_production().unwrap();
    assert_eq!(production.settlement_count, 1);
    assert_eq!(production.inputs[&EntityId(1)], replacement);
    assert!(
        world
            .set_monthly_production_inputs(BTreeMap::from([(EntityId(99), replacement.clone(),)]))
            .is_err()
    );
    assert_eq!(
        world.monthly_production().unwrap().inputs[&EntityId(1)],
        replacement
    );
    let saved = world.save("production-refresh").unwrap();
    let restored = World::restore(&saved, "production-refresh", world.schema()).unwrap();
    assert_eq!(restored, world);
}

#[test]
fn monthly_production_preserves_known_unknown_inputs_without_crediting_them() {
    let (mut world, country) = world(VanillaProductionInputState {
        input: None,
        unresolved: ["native_source_unresolved".into()].into(),
    });
    world.advance_to(20).unwrap();

    assert_eq!(
        world.country_resources()[&country].treasury_milli,
        Some(10_000)
    );
    let account = &world
        .monthly_production()
        .unwrap()
        .last_settlement
        .as_ref()
        .unwrap()
        .accounts[&country];
    assert_eq!(account.known_monthly_value_milli, 0);
    assert_eq!(account.monthly_value_milli, None);
    assert_eq!(account.credited_milli, None);
    assert!(
        account
            .unresolved
            .contains("province_input:native_source_unresolved")
    );
}

#[test]
fn monthly_production_adds_after_a_known_tax_settlement() {
    let (mut world, country) = world(known_input());
    let mut attributes = Attributes::default();
    attributes.formulas.insert(
        EntityId(1),
        [(
            "annual_tax".into(),
            AttributeFormula {
                base: AttributeBase::Constant { micros: 12_000_000 },
                channels: Vec::new(),
                minimum_micros: None,
                maximum_micros: None,
                result_quantum_micros: None,
                unresolved: BTreeSet::new(),
            },
        )]
        .into_iter()
        .collect(),
    );
    world.set_attributes(attributes).unwrap();
    world
        .set_taxation(Taxation::new("annual_tax", [EntityId(0)].into()))
        .unwrap();
    world.advance_to(20).unwrap();

    assert_eq!(
        world.country_resources()[&country].treasury_milli,
        Some(11_084)
    );
    let production = &world
        .monthly_production()
        .unwrap()
        .last_settlement
        .as_ref()
        .unwrap()
        .accounts[&country];
    assert_eq!(production.treasury_before_milli, Some(11_000));
    assert_eq!(production.credited_milli, Some(84));
}

#[test]
fn external_cashflows_settle_after_production_and_round_trip() {
    let (mut world, country) = world(known_input());
    world
        .set_monthly_cashflows(Some(MonthlyExternalCashflows::new(BTreeMap::from([(
            country,
            Some(250),
        )]))))
        .unwrap();
    world.advance_to(20).unwrap();

    assert_eq!(
        world.country_resources()[&country].treasury_milli,
        Some(10_334)
    );
    let account = &world
        .monthly_cashflows()
        .unwrap()
        .last_settlement
        .as_ref()
        .unwrap()
        .accounts[&country];
    assert_eq!(account.treasury_before_milli, Some(10_084));
    assert_eq!(account.treasury_after_milli, Some(10_334));
    assert_eq!(account.credited_milli, Some(250));

    world
        .set_monthly_cashflow_inputs(BTreeMap::from([(country, Some(500))]))
        .unwrap();
    assert_eq!(
        world.monthly_cashflows().unwrap().amounts_milli[&country],
        Some(500)
    );

    let saved = world.save("external-cashflows").unwrap();
    let restored = World::restore(&saved, "external-cashflows", world.schema()).unwrap();
    assert_eq!(restored, world);
}

#[test]
fn cashflow_breakdowns_derive_net_receipts_and_round_trip() {
    let (mut world, country) = world(known_input());
    let breakdown = CashflowBreakdown {
        trade_milli: Some(100),
        trade_income_pass: None,
        gold_milli: Some(40),
        expenses_milli: Some(30),
        expense_categories_milli: BTreeMap::new(),
    };
    world
        .set_monthly_cashflows(Some(
            MonthlyExternalCashflows::new(BTreeMap::from([(country, None)]))
                .with_breakdowns(BTreeMap::from([(country, breakdown.clone())])),
        ))
        .unwrap();
    world.advance_to(20).unwrap();

    assert_eq!(
        world.country_resources()[&country].treasury_milli,
        Some(10_194)
    );
    let account = &world
        .monthly_cashflows()
        .unwrap()
        .last_settlement
        .as_ref()
        .unwrap()
        .accounts[&country];
    assert_eq!(account.amount_milli, Some(110));
    assert_eq!(account.breakdown, Some(breakdown));
    assert_eq!(account.credited_milli, Some(110));

    let saved = world.save("cashflow-breakdown").unwrap();
    let restored = World::restore(&saved, "cashflow-breakdown", world.schema()).unwrap();
    assert_eq!(restored, world);
}

#[test]
fn cashflow_breakdown_rejects_a_conflicting_explicit_net_amount() {
    let (mut world, country) = world(known_input());
    let result = world.set_monthly_cashflows(Some(
        MonthlyExternalCashflows::new(BTreeMap::from([(country, Some(111))])).with_breakdowns(
            BTreeMap::from([(
                country,
                CashflowBreakdown {
                    trade_milli: Some(100),
                    trade_income_pass: None,
                    gold_milli: Some(40),
                    expenses_milli: Some(30),
                    expense_categories_milli: BTreeMap::new(),
                },
            )]),
        ),
    ));
    assert!(result.is_err());
}

#[test]
fn cashflow_breakdown_rejects_a_trade_total_that_conflicts_with_its_records() {
    let (mut world, country) = world(known_input());
    let result = world.set_monthly_cashflows(Some(
        MonthlyExternalCashflows::new(BTreeMap::from([(country, None)])).with_breakdowns(
            BTreeMap::from([(
                country,
                CashflowBreakdown {
                    trade_milli: Some(1_557),
                    trade_income_pass: Some(VanillaTradeIncomePassInput {
                        eligible_records: vec![VanillaTradeIncomeInput {
                            node_value_milli: 3_817,
                            country_record_value_milli: 349,
                            base_context_modifier_milli: 20,
                            selected_context_modifier_milli: 50,
                            virtual_context_modifier_milli: None,
                            minimum_context_modifier_milli: 10,
                            maximum_context_modifier_milli: 2_000,
                            post_context_modifier_milli: 100,
                        }],
                    }),
                    gold_milli: Some(0),
                    expenses_milli: Some(0),
                    expense_categories_milli: BTreeMap::new(),
                },
            )]),
        ),
    ));
    assert!(result.is_err());
    assert!(world.monthly_cashflows().is_none());
}

#[test]
fn cashflow_breakdown_rejects_trade_values_outside_the_native_integer_range() {
    let (mut world, country) = world(known_input());
    let result = world.set_monthly_cashflows(Some(
        MonthlyExternalCashflows::new(BTreeMap::from([(country, None)])).with_breakdowns(
            BTreeMap::from([(
                country,
                CashflowBreakdown {
                    trade_milli: None,
                    trade_income_pass: Some(VanillaTradeIncomePassInput {
                        eligible_records: vec![VanillaTradeIncomeInput {
                            node_value_milli: i64::from(i32::MAX) + 1,
                            country_record_value_milli: 1,
                            base_context_modifier_milli: 0,
                            selected_context_modifier_milli: 0,
                            virtual_context_modifier_milli: None,
                            minimum_context_modifier_milli: 0,
                            maximum_context_modifier_milli: 2_000,
                            post_context_modifier_milli: 0,
                        }],
                    }),
                    gold_milli: Some(0),
                    expenses_milli: Some(0),
                    expense_categories_milli: BTreeMap::new(),
                },
            )]),
        ),
    ));
    assert!(matches!(result, Err(augustus_core::Error::Overflow)));
    assert!(world.monthly_cashflows().is_none());
}

#[test]
fn cashflow_breakdown_rejects_invalid_expense_categories() {
    let (mut world, country) = world(known_input());
    for (category, amount, total) in [(38, 1, 1), (11, 2, 1)] {
        let breakdown = CashflowBreakdown {
            trade_milli: Some(0),
            trade_income_pass: None,
            gold_milli: Some(0),
            expenses_milli: Some(total),
            expense_categories_milli: BTreeMap::from([(category, amount)]),
        };
        let result = world.set_monthly_cashflows(Some(
            MonthlyExternalCashflows::new(BTreeMap::from([(country, Some(-total))]))
                .with_breakdowns(BTreeMap::from([(country, breakdown)])),
        ));
        assert!(result.is_err());
    }
}

#[test]
fn unknown_external_cashflow_keeps_treasury_unknown_after_known_production() {
    let (mut world, country) = world(known_input());
    world
        .set_monthly_cashflows(Some(MonthlyExternalCashflows::new(BTreeMap::from([(
            country, None,
        )]))))
        .unwrap();
    world.advance_to(20).unwrap();

    assert_eq!(world.country_resources()[&country].treasury_milli, None);
    let account = &world
        .monthly_cashflows()
        .unwrap()
        .last_settlement
        .as_ref()
        .unwrap()
        .accounts[&country];
    assert!(account.unresolved.contains("unknown_cashflow"));
    assert_eq!(account.treasury_before_milli, Some(10_084));
    assert_eq!(account.treasury_after_milli, None);
    assert_eq!(account.credited_milli, None);
}
