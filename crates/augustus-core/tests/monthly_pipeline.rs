// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    Action, AttributeBase, AttributeFormula, Attributes, Calendar, CashflowBreakdown,
    CountryLedger, CountryLedgerMonthlyConsolidation, CountryLedgerPendingFlush, CountryResources,
    EntityId, EntityKind, Error, FieldId, MonthlyEconomy, MonthlyExternalCashflows,
    MonthlyPipeline, MonthlyProduction, MonthlySystem, MonthlyTax, Schema, Taxation,
    VanillaProductionInput, VanillaProductionInputState, VanillaTradeIncomeInput,
    VanillaTradeIncomePassInput, World, WorldDate,
};
use std::collections::{BTreeMap, BTreeSet};

struct Recorder {
    id: &'static str,
    country: EntityId,
    message: &'static str,
}

impl MonthlySystem for Recorder {
    fn id(&self) -> &str {
        self.id
    }

    fn validate(&self, world: &World) -> Result<(), Error> {
        if world.entity(self.country)?.kind != EntityKind::Country {
            return Err(Error::Invalid("monthly recorder requires a country".into()));
        }
        Ok(())
    }

    fn settle(&self, world: &mut World) -> Result<(), Error> {
        world.execute(self.country, &[Action::Emit(self.message.into())])
    }
}

struct StandardTaxPhase;

impl MonthlySystem for StandardTaxPhase {
    fn id(&self) -> &str {
        "standard-tax-phase:v1"
    }

    fn validate(&self, world: &World) -> Result<(), Error> {
        MonthlyTax.validate(world)
    }

    fn settle(&self, world: &mut World) -> Result<(), Error> {
        MonthlyTax.settle(world)
    }
}

fn world() -> (World, EntityId) {
    let mut world = World::new(Schema::new([] as [&str; 0]).unwrap());
    let country = world.add_entity("AAA", EntityKind::Country);
    world
        .set_calendar(Calendar {
            start: WorldDate {
                year: 1444,
                month: 11,
                day: 11,
            },
            month_lengths: [31; 12],
        })
        .unwrap();
    (world, country)
}

#[test]
fn monthly_pipeline_runs_world_parallel_and_serial_phases_in_order() {
    let (mut world, country) = world();
    let world_phase = Recorder {
        id: "world:v1",
        country,
        message: "world",
    };
    let parallel_phase = Recorder {
        id: "parallel:v1",
        country,
        message: "parallel",
    };
    let serial_phase = Recorder {
        id: "serial:v1",
        country,
        message: "serial",
    };
    let pipeline = MonthlyPipeline::new(
        "eu4-monthly-chain:test:v1",
        Some(&world_phase),
        Some(&parallel_phase),
        Some(&serial_phase),
    )
    .unwrap();

    world.select_monthly_system(&pipeline).unwrap();
    world.advance_with_monthly_system(21, &pipeline).unwrap();
    assert_eq!(
        world
            .events()
            .iter()
            .map(|event| event.2.as_str())
            .collect::<Vec<_>>(),
        ["world", "parallel", "serial"]
    );

    let save = world.save("monthly-pipeline").unwrap();
    let restored = World::restore(&save, "monthly-pipeline", world.schema()).unwrap();
    assert_eq!(
        restored.monthly_system_id(),
        Some("eu4-monthly-chain:test:v1")
    );
}

#[test]
fn monthly_pipeline_requires_a_phase() {
    assert!(MonthlyPipeline::new("empty:v1", None, None, None).is_err());
    assert!(MonthlyPipeline::new(" ", None, None, None).is_err());
}

#[test]
fn custom_pipeline_can_rebind_standard_monthly_state_before_start() {
    let mut world = World::new(Schema::new(["annual_tax"]).unwrap());
    let country = world.add_entity("AAA", EntityKind::Country);
    let province = world.add_entity("North", EntityKind::Province);
    world.set_owner(province, country).unwrap();
    world.set(province, FieldId(0), 12_000).unwrap();
    let mut attributes = Attributes::default();
    attributes.formulas.insert(
        province,
        [(
            "annual_tax".into(),
            AttributeFormula {
                base: AttributeBase::Field {
                    field: FieldId(0),
                    micros_per_unit: 1_000,
                },
                channels: vec![],
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
    world
        .set_calendar(Calendar {
            start: WorldDate {
                year: 1444,
                month: 11,
                day: 11,
            },
            month_lengths: [31; 12],
        })
        .unwrap();
    let taxation = Taxation::new("annual_tax", [country].into_iter().collect());
    world.set_taxation(taxation.clone()).unwrap();

    let pipeline =
        MonthlyPipeline::new("custom-economy:v1", Some(&StandardTaxPhase), None, None).unwrap();
    world.select_monthly_system(&pipeline).unwrap();
    assert!(world.taxation().is_none());
    world.set_taxation(taxation).unwrap();
    assert_eq!(world.monthly_system_id(), Some("custom-economy:v1"));

    world.advance_with_monthly_system(21, &pipeline).unwrap();
    let account = &world
        .taxation()
        .unwrap()
        .last_settlement
        .as_ref()
        .unwrap()
        .accounts[&country];
    assert_eq!(account.income_milli, Some(1_000));
    assert_eq!(account.treasury_before_milli, Some(10_000));
    assert_eq!(account.treasury_after_milli, Some(11_000));

    let saved = world.save("custom-economy").unwrap();
    let restored = World::restore(&saved, "custom-economy", world.schema()).unwrap();
    assert_eq!(restored.monthly_system_id(), Some("custom-economy:v1"));
    assert_eq!(restored.taxation(), world.taxation());
}

#[test]
fn standard_monthly_economy_runs_inside_the_world_pipeline_and_resumes() {
    let mut world = World::new(Schema::new(["annual_tax"]).unwrap());
    let country = world.add_entity("AAA", EntityKind::Country);
    let province = world.add_entity("North", EntityKind::Province);
    world.set_owner(province, country).unwrap();
    world.set(province, FieldId(0), 12_000).unwrap();
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
    world
        .set_attributes(Attributes {
            formulas: [(
                province,
                [(
                    "annual_tax".into(),
                    AttributeFormula {
                        base: AttributeBase::Field {
                            field: FieldId(0),
                            micros_per_unit: 1_000,
                        },
                        channels: Vec::new(),
                        minimum_micros: None,
                        maximum_micros: None,
                        result_quantum_micros: None,
                        unresolved: BTreeSet::new(),
                    },
                )]
                .into_iter()
                .collect(),
            )]
            .into_iter()
            .collect(),
            ..Default::default()
        })
        .unwrap();
    world
        .set_calendar(Calendar {
            start: WorldDate {
                year: 1444,
                month: 11,
                day: 11,
            },
            month_lengths: [31; 12],
        })
        .unwrap();

    let taxation = Taxation::new("annual_tax", [country].into_iter().collect());
    let production = MonthlyProduction::new(BTreeMap::from([(
        province,
        VanillaProductionInputState {
            input: Some(VanillaProductionInput {
                base_value_milli: 1_000,
                country_modifier_milli: 20,
                legacy_province_modifier_milli: 0,
                queried_province_modifier_milli: -10,
            }),
            unresolved: BTreeSet::new(),
        },
    )]));
    let cashflows = MonthlyExternalCashflows::new(BTreeMap::from([(country, Some(250))]));
    world.set_taxation(taxation.clone()).unwrap();
    world.set_monthly_production(production.clone()).unwrap();
    world
        .set_monthly_cashflows(Some(cashflows.clone()))
        .unwrap();

    let economy = MonthlyEconomy;
    let parallel = Recorder {
        id: "parallel-after-economy:v1",
        country,
        message: "parallel",
    };
    let serial = Recorder {
        id: "serial-after-economy:v1",
        country,
        message: "serial",
    };
    let ledger_consolidation = CountryLedgerMonthlyConsolidation::default();
    let serial_pipeline = MonthlyPipeline::new(
        "standard-economy-country-tail:v1",
        None,
        Some(&serial),
        Some(&ledger_consolidation),
    )
    .unwrap();
    let pipeline = MonthlyPipeline::new(
        "standard-economy-pipeline:v1",
        Some(&economy),
        Some(&parallel),
        Some(&serial_pipeline),
    )
    .unwrap();

    world.select_monthly_system(&pipeline).unwrap();
    world.set_taxation(taxation).unwrap();
    world.set_monthly_production(production).unwrap();
    world.set_monthly_cashflows(Some(cashflows)).unwrap();
    world.advance_with_monthly_system(21, &pipeline).unwrap();

    assert_eq!(
        world.country_resources()[&country].treasury_milli,
        Some(11_334)
    );
    assert_eq!(
        world.country_ledgers()[&country].income_accumulator_milli[0],
        1_000
    );
    assert_eq!(
        world.country_ledgers()[&country].income_accumulator_milli[1],
        84
    );
    assert_eq!(
        world.country_ledgers()[&country].income_snapshot_milli[0],
        1_000
    );
    assert_eq!(
        world.country_ledgers()[&country].income_snapshot_milli[1],
        84
    );
    assert_eq!(world.country_ledgers()[&country].income_total_milli, 1_084);
    assert!(
        world.country_ledgers()[&country]
            .income_sources_milli
            .iter()
            .all(|value| *value == 0)
    );
    assert!(
        world.country_ledgers()[&country]
            .expense_sources_milli
            .iter()
            .all(|value| *value == 0)
    );
    assert_eq!(
        world
            .events()
            .iter()
            .map(|event| event.2.as_str())
            .collect::<Vec<_>>(),
        ["parallel", "serial"]
    );

    let saved = world.save("standard-economy-pipeline").unwrap();
    let mut resumed = World::restore(&saved, "standard-economy-pipeline", world.schema()).unwrap();
    let mut continuous = world.clone();
    continuous
        .advance_with_monthly_system(52, &pipeline)
        .unwrap();
    resumed.advance_with_monthly_system(52, &pipeline).unwrap();
    assert_eq!(
        resumed.save("standard-economy-pipeline").unwrap(),
        continuous.save("standard-economy-pipeline").unwrap()
    );
    assert_eq!(
        resumed.country_ledgers()[&country].income_total_milli,
        1_084
    );
    assert_eq!(
        resumed.country_ledgers()[&country].income_accumulator_milli[0],
        2_000
    );
    assert_eq!(
        resumed.country_ledgers()[&country].income_accumulator_milli[1],
        168
    );
}

#[test]
fn monthly_tax_production_and_cashflow_share_the_native_treasury_bounds() {
    let mut world = World::new(Schema::new([] as [&str; 0]).unwrap());
    let country = world.add_entity("AAA", EntityKind::Country);
    let province = world.add_entity("North", EntityKind::Province);
    world.set_owner(province, country).unwrap();
    world
        .set_country_resources(
            [(
                country,
                CountryResources {
                    treasury_milli: Some(999_999_900),
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
            month_lengths: [31; 12],
        })
        .unwrap();

    let taxation = Taxation::new("annual_tax", [country].into_iter().collect())
        .with_external_country_annual_milli(BTreeMap::from([(country, Some(2_400))]))
        .with_external_province_annual_milli(BTreeMap::from([(province, Some(0))]));
    let production = MonthlyProduction::new(BTreeMap::from([(
        province,
        VanillaProductionInputState {
            input: Some(VanillaProductionInput {
                base_value_milli: 1_200,
                country_modifier_milli: 0,
                legacy_province_modifier_milli: 0,
                queried_province_modifier_milli: 0,
            }),
            unresolved: BTreeSet::new(),
        },
    )]));
    let cashflows = MonthlyExternalCashflows::new(BTreeMap::from([(country, Some(-100))]));
    world.set_taxation(taxation).unwrap();
    world.set_monthly_production(production).unwrap();
    world.set_monthly_cashflows(Some(cashflows)).unwrap();
    world.advance_to(21).unwrap();

    let tax = &world
        .taxation()
        .unwrap()
        .last_settlement
        .as_ref()
        .unwrap()
        .accounts[&country];
    assert_eq!(tax.income_milli, Some(200));
    assert_eq!(tax.treasury_after_milli, Some(1_000_000_000));

    let production = &world
        .monthly_production()
        .unwrap()
        .last_settlement
        .as_ref()
        .unwrap()
        .accounts[&country];
    assert!(
        production
            .monthly_value_milli
            .is_some_and(|value| value > 0)
    );
    assert_eq!(production.treasury_before_milli, Some(1_000_000_000));
    assert_eq!(production.treasury_after_milli, Some(1_000_000_000));

    let cashflow = &world
        .monthly_cashflows()
        .unwrap()
        .last_settlement
        .as_ref()
        .unwrap()
        .accounts[&country];
    assert_eq!(cashflow.amount_milli, Some(-100));
    assert_eq!(cashflow.treasury_before_milli, Some(1_000_000_000));
    assert_eq!(cashflow.treasury_after_milli, Some(999_999_900));
    assert_eq!(
        world.country_resources()[&country].treasury_milli,
        Some(999_999_900)
    );
    let snapshot = world.save("treasury-stage-bounds").unwrap();
    assert!(World::restore(&snapshot, "treasury-stage-bounds", world.schema()).is_ok());
}

#[test]
fn classified_cashflow_expenses_enter_their_native_ledger_slots() {
    let (mut world, country) = world();
    world
        .set_country_resources(
            [(
                country,
                CountryResources {
                    treasury_milli: Some(100_000),
                    ..Default::default()
                },
            )]
            .into(),
        )
        .unwrap();
    let breakdown = CashflowBreakdown {
        trade_milli: Some(0),
        trade_income_pass: None,
        gold_milli: Some(0),
        expenses_milli: Some(10_000),
        expense_categories_milli: BTreeMap::from([(11, 10_000)]),
    };
    let cashflows = MonthlyExternalCashflows::new(BTreeMap::from([(country, Some(-10_000))]))
        .with_breakdowns(BTreeMap::from([(country, breakdown)]));
    let production = MonthlyProduction::new(BTreeMap::new());
    world.set_monthly_production(production.clone()).unwrap();
    world
        .set_monthly_cashflows(Some(cashflows.clone()))
        .unwrap();
    let economy = MonthlyEconomy;
    let consolidation = CountryLedgerMonthlyConsolidation::default();
    let pipeline = MonthlyPipeline::new(
        "classified-cashflow-expenses:test:v1",
        Some(&economy),
        None,
        Some(&consolidation),
    )
    .unwrap();
    world.select_monthly_system(&pipeline).unwrap();
    world.set_monthly_production(production).unwrap();
    world.set_monthly_cashflows(Some(cashflows)).unwrap();
    world.advance_with_monthly_system(21, &pipeline).unwrap();

    let ledger = &world.country_ledgers()[&country];
    assert_eq!(
        augustus_core::COUNTRY_LEDGER_EXPENSE_KEYS[11],
        "EXPENSEARMY"
    );
    assert_eq!(ledger.expense_sources_milli[11], 0);
    assert_eq!(ledger.expense_snapshot_milli[11], 10_000);
    assert_eq!(ledger.expense_total_milli, 10_000);
    assert_eq!(
        world.country_resources()[&country].treasury_milli,
        Some(90_000)
    );

    let snapshot = world.save("classified-cashflow-expense").unwrap();
    let restored =
        World::restore(&snapshot, "classified-cashflow-expense", world.schema()).unwrap();
    assert_eq!(restored, world);
}

#[test]
fn trade_income_records_feed_monthly_cashflow_and_native_ledger() {
    let (mut world, country) = world();
    world
        .set_country_resources(
            [(
                country,
                CountryResources {
                    treasury_milli: Some(100_000),
                    ..Default::default()
                },
            )]
            .into(),
        )
        .unwrap();
    let breakdown = CashflowBreakdown {
        trade_milli: None,
        trade_income_pass: Some(VanillaTradeIncomePassInput {
            eligible_records: vec![
                VanillaTradeIncomeInput {
                    node_value_milli: 3_817,
                    country_record_value_milli: 349,
                    base_context_modifier_milli: 20,
                    selected_context_modifier_milli: 50,
                    virtual_context_modifier_milli: None,
                    minimum_context_modifier_milli: 10,
                    maximum_context_modifier_milli: 2_000,
                    post_context_modifier_milli: 100,
                },
                VanillaTradeIncomeInput {
                    node_value_milli: 5_390,
                    country_record_value_milli: 305,
                    base_context_modifier_milli: 0,
                    selected_context_modifier_milli: 70,
                    virtual_context_modifier_milli: None,
                    minimum_context_modifier_milli: 10,
                    maximum_context_modifier_milli: 2_000,
                    post_context_modifier_milli: 0,
                },
            ],
        }),
        gold_milli: Some(0),
        expenses_milli: Some(0),
        expense_categories_milli: BTreeMap::new(),
    };
    let cashflows = MonthlyExternalCashflows::new(BTreeMap::from([(country, None)]))
        .with_breakdowns(BTreeMap::from([(country, breakdown)]));
    let production = MonthlyProduction::new(BTreeMap::new());
    world.set_monthly_production(production.clone()).unwrap();
    world
        .set_monthly_cashflows(Some(cashflows.clone()))
        .unwrap();
    let economy = MonthlyEconomy;
    let consolidation = CountryLedgerMonthlyConsolidation::default();
    let pipeline = MonthlyPipeline::new(
        "trade-income-cashflow:test:v1",
        Some(&economy),
        None,
        Some(&consolidation),
    )
    .unwrap();
    world.select_monthly_system(&pipeline).unwrap();
    world.set_monthly_production(production).unwrap();
    world.set_monthly_cashflows(Some(cashflows)).unwrap();
    world.advance_with_monthly_system(21, &pipeline).unwrap();

    let account = &world
        .monthly_cashflows()
        .unwrap()
        .last_settlement
        .as_ref()
        .unwrap()
        .accounts[&country];
    assert_eq!(account.amount_milli, Some(3_316));
    assert_eq!(account.credited_milli, Some(3_316));
    assert_eq!(
        world.country_resources()[&country].treasury_milli,
        Some(103_316)
    );
    assert_eq!(
        world.country_ledgers()[&country].income_snapshot_milli[2],
        3_316
    );

    let saved = world.save("trade-income-cashflow").unwrap();
    let restored = World::restore(&saved, "trade-income-cashflow", world.schema()).unwrap();
    assert_eq!(restored, world);
}

#[test]
fn delayed_country_balances_flush_before_monthly_ledger_consolidation_and_resume() {
    let (mut world, country) = world();
    world
        .set_country_resources(
            [(
                country,
                CountryResources {
                    treasury_milli: Some(100),
                    ..Default::default()
                },
            )]
            .into(),
        )
        .unwrap();
    world.record_delayed_country_income(country, 1, 40).unwrap();

    let pending_flush = CountryLedgerPendingFlush;
    let consolidation = CountryLedgerMonthlyConsolidation::default();
    let pipeline = MonthlyPipeline::new(
        "country-ledger-monthly-lifecycle:test:v1",
        Some(&pending_flush),
        None,
        Some(&consolidation),
    )
    .unwrap();
    world.select_monthly_system(&pipeline).unwrap();
    world.advance_with_monthly_system(21, &pipeline).unwrap();

    assert_eq!(
        world.country_resources()[&country].treasury_milli,
        Some(140)
    );
    let ledger = &world.country_ledgers()[&country];
    assert_eq!(ledger.pending_milli, 0);
    assert_eq!(ledger.income_snapshot_milli[1], 40);
    assert_eq!(ledger.income_total_milli, 40);
    assert_eq!(ledger.income_accumulator_milli[1], 40);
    assert!(ledger.income_sources_milli.iter().all(|value| *value == 0));

    let saved = world.save("country-ledger-monthly-lifecycle").unwrap();
    let mut resumed =
        World::restore(&saved, "country-ledger-monthly-lifecycle", world.schema()).unwrap();
    let mut continuous = world;
    continuous
        .record_delayed_country_income(country, 1, 10)
        .unwrap();
    resumed
        .record_delayed_country_income(country, 1, 10)
        .unwrap();
    continuous
        .advance_with_monthly_system(52, &pipeline)
        .unwrap();
    resumed.advance_with_monthly_system(52, &pipeline).unwrap();

    assert_eq!(resumed, continuous);
    assert_eq!(
        resumed.country_resources()[&country].treasury_milli,
        Some(150)
    );
    let ledger = &resumed.country_ledgers()[&country];
    assert_eq!(ledger.pending_milli, 0);
    assert_eq!(ledger.income_snapshot_milli[1], 10);
    assert_eq!(ledger.income_total_milli, 10);
    assert_eq!(ledger.income_accumulator_milli[1], 50);
    assert!(ledger.income_sources_milli.iter().all(|value| *value == 0));
}

#[test]
fn monthly_economy_runs_before_the_pending_treasury_flush() {
    let (mut world, country) = world();
    world
        .set_country_resources(
            [(
                country,
                CountryResources {
                    treasury_milli: Some(-1_000_000_000),
                    ..Default::default()
                },
            )]
            .into(),
        )
        .unwrap();
    let cashflows = MonthlyExternalCashflows::new(BTreeMap::from([(country, Some(50))]));
    let production = MonthlyProduction::new(BTreeMap::new());
    world
        .set_monthly_cashflows(Some(cashflows.clone()))
        .unwrap();
    world.set_monthly_production(production.clone()).unwrap();
    world
        .record_delayed_country_expense(country, 23, 100)
        .unwrap();

    let economy = MonthlyEconomy;
    let pending_flush = CountryLedgerPendingFlush;
    let consolidation = CountryLedgerMonthlyConsolidation::default();
    let ledger_tail = MonthlyPipeline::new(
        "monthly-ledger-tail:test:v1",
        Some(&pending_flush),
        None,
        Some(&consolidation),
    )
    .unwrap();
    let pipeline = MonthlyPipeline::new(
        "monthly-economy-before-ledger-flush:test:v1",
        Some(&economy),
        None,
        Some(&ledger_tail),
    )
    .unwrap();

    world.select_monthly_system(&pipeline).unwrap();
    world.set_monthly_cashflows(Some(cashflows)).unwrap();
    world.set_monthly_production(production).unwrap();
    world.advance_with_monthly_system(21, &pipeline).unwrap();

    assert_eq!(
        world.country_resources()[&country].treasury_milli,
        Some(-1_000_000_000)
    );
    assert_eq!(world.country_ledgers()[&country].pending_milli, 0);
    assert_eq!(
        world.country_ledgers()[&country].expense_snapshot_milli[23],
        100
    );
}

#[test]
fn monthly_ledger_compatibility_filter_overrides_native_categories() {
    let (mut world, country) = world();
    world
        .set_country_resources(
            [(
                country,
                CountryResources {
                    treasury_milli: Some(1_000),
                    ..Default::default()
                },
            )]
            .into(),
        )
        .unwrap();
    world.record_country_expense(country, 23, 15).unwrap();

    assert!(CountryLedgerMonthlyConsolidation::new(" ", BTreeMap::new()).is_err());
    let consolidation = CountryLedgerMonthlyConsolidation::new(
        "country-ledger-custom-filter:test:v1",
        BTreeMap::from([(country, BTreeSet::from([23]))]),
    )
    .unwrap();
    assert_eq!(consolidation.id(), "country-ledger-custom-filter:test:v1");
    let pipeline = MonthlyPipeline::new(
        "country-ledger-custom-filter:test:v1",
        None,
        None,
        Some(&consolidation),
    )
    .unwrap();
    world.select_monthly_system(&pipeline).unwrap();
    world.advance_with_monthly_system(21, &pipeline).unwrap();

    let ledger = &world.country_ledgers()[&country];
    assert_eq!(ledger.expense_snapshot_milli[23], 15);
    assert_eq!(ledger.expense_total_milli, 15);
    assert_eq!(ledger.expense_filtered_total_milli, 0);
    assert!(ledger.expense_sources_milli.iter().all(|value| *value == 0));
}

#[test]
fn monthly_ledger_consolidation_does_not_partially_update_countries_on_overflow() {
    let (mut world, first_country) = world();
    let second_country = world.add_entity("BBB", EntityKind::Country);

    let mut first_income = vec![0; 19];
    first_income[0] = 12;
    let first_ledger = CountryLedger {
        income_sources_milli: first_income,
        income_snapshot_milli: vec![3; 19],
        income_total_milli: 57,
        ..Default::default()
    };

    let mut second_income = vec![0; 19];
    second_income[0] = i64::MAX;
    second_income[1] = 1;
    let second_ledger = CountryLedger {
        income_sources_milli: second_income,
        income_snapshot_milli: vec![5; 19],
        income_total_milli: 95,
        ..Default::default()
    };

    let ledgers = BTreeMap::from([
        (first_country, first_ledger),
        (second_country, second_ledger),
    ]);
    world.set_country_ledgers(ledgers.clone()).unwrap();

    let consolidation = CountryLedgerMonthlyConsolidation::default();
    let pipeline = MonthlyPipeline::new(
        "country-ledger-monthly-overflow:test:v1",
        None,
        None,
        Some(&consolidation),
    )
    .unwrap();
    world.select_monthly_system(&pipeline).unwrap();

    assert!(world.advance_with_monthly_system(21, &pipeline).is_err());
    assert_eq!(world.country_ledgers(), &ledgers);
}
