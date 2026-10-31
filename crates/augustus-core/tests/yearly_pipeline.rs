// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    Action, Calendar, CountryLedger, CountryLedgerAnnualRollover, CountryResources, EntityId,
    EntityKind, Error, Schema, World, WorldDate, YearlyPipeline, YearlySystem,
};
use std::collections::BTreeMap;

struct PipelinePhase {
    id: &'static str,
    country: EntityId,
    message: &'static str,
}

impl YearlySystem for PipelinePhase {
    fn id(&self) -> &str {
        self.id
    }

    fn validate(&self, world: &World) -> Result<(), Error> {
        if world.entity(self.country)?.kind != EntityKind::Country {
            return Err(Error::Invalid("yearly phase requires a country".into()));
        }
        Ok(())
    }

    fn settle(&self, world: &mut World) -> Result<(), Error> {
        world.execute(self.country, &[Action::Emit(self.message.into())])
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
fn yearly_pipeline_requires_a_phase() {
    assert!(YearlyPipeline::new("", None, None).is_err());
    assert!(YearlyPipeline::new("yearly-pipeline:test:v1", None, None).is_err());
}

#[test]
fn yearly_pipeline_runs_world_then_country_and_restores() {
    let (mut world, country) = world();
    let world_phase = PipelinePhase {
        id: "yearly-world:test:v1",
        country,
        message: "world",
    };
    let country_phase = PipelinePhase {
        id: "yearly-country:test:v1",
        country,
        message: "country",
    };
    let pipeline = YearlyPipeline::new(
        "yearly-pipeline:test:v1",
        Some(&world_phase),
        Some(&country_phase),
    )
    .unwrap();

    world.select_yearly_system(&pipeline).unwrap();
    world.advance_with_yearly_system(52, &pipeline).unwrap();
    assert_eq!(
        world
            .events()
            .iter()
            .map(|event| event.2.as_str())
            .collect::<Vec<_>>(),
        ["world", "country"]
    );

    let saved = world.save("yearly-pipeline:test:v1").unwrap();
    let mut resumed = World::restore(&saved, "yearly-pipeline:test:v1", world.schema()).unwrap();
    assert_eq!(resumed.yearly_system_id(), Some("yearly-pipeline:test:v1"));

    let mut continuous = world.clone();
    continuous
        .advance_with_yearly_system(417, &pipeline)
        .unwrap();
    resumed.advance_with_yearly_system(417, &pipeline).unwrap();
    assert_eq!(resumed.events(), continuous.events());
    assert_eq!(
        resumed.save("yearly-pipeline:test:v1").unwrap(),
        continuous.save("yearly-pipeline:test:v1").unwrap()
    );
}

#[test]
fn annual_ledger_rollover_is_a_selected_yearly_phase_and_resumes() {
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
    world.record_country_income(country, 2, 400).unwrap();
    world.record_country_expense(country, 5, 120).unwrap();

    let rollover = CountryLedgerAnnualRollover;
    let pipeline =
        YearlyPipeline::new("country-ledger-yearly:test:v1", None, Some(&rollover)).unwrap();
    world.select_yearly_system(&pipeline).unwrap();
    world.advance_with_yearly_system(52, &pipeline).unwrap();

    let ledger = &world.country_ledgers()[&country];
    assert_eq!(ledger.income_rollover_milli[2], 400);
    assert_eq!(ledger.expense_rollover_milli[5], 120);
    assert_eq!(ledger.income_rollover_total_milli, 400);
    assert_eq!(ledger.expense_rollover_total_milli, 120);
    assert!(
        ledger
            .income_accumulator_milli
            .iter()
            .all(|value| *value == 0)
    );
    assert!(
        ledger
            .expense_accumulator_milli
            .iter()
            .all(|value| *value == 0)
    );
    assert!(ledger.income_sources_milli.iter().all(|value| *value == 0));
    assert!(ledger.expense_sources_milli.iter().all(|value| *value == 0));
    assert_eq!(ledger.income_total_milli, 0);
    assert_eq!(ledger.expense_total_milli, 0);

    let saved = world.save("country-ledger-yearly:test:v1").unwrap();
    let mut resumed =
        World::restore(&saved, "country-ledger-yearly:test:v1", world.schema()).unwrap();
    assert_eq!(
        resumed.yearly_system_id(),
        Some("country-ledger-yearly:test:v1")
    );
    let mut continuous = world;
    continuous.record_country_income(country, 2, 50).unwrap();
    resumed.record_country_income(country, 2, 50).unwrap();
    continuous
        .advance_with_yearly_system(424, &pipeline)
        .unwrap();
    resumed.advance_with_yearly_system(424, &pipeline).unwrap();

    assert_eq!(resumed, continuous);
    assert_eq!(
        resumed.country_ledgers()[&country].income_rollover_milli[2],
        50
    );
    assert_eq!(
        resumed.country_ledgers()[&country].income_rollover_total_milli,
        50
    );
}

#[test]
fn preview_annual_ledger_rollover_preserves_live_state_and_clears_sources() {
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
            .into_iter()
            .collect(),
        )
        .unwrap();
    world.record_country_income(country, 2, 400).unwrap();
    world.record_country_expense(country, 5, 120).unwrap();
    let snapshot = world.save("annual-preview").unwrap();

    let preview = world.preview_country_ledgers_annual_rollover().unwrap();
    let ledger = &preview[&country];
    assert_eq!(ledger.income_rollover_milli[2], 400);
    assert_eq!(ledger.expense_rollover_milli[5], 120);
    assert_eq!(ledger.income_rollover_total_milli, 400);
    assert_eq!(ledger.expense_rollover_total_milli, 120);
    assert!(
        ledger
            .income_accumulator_milli
            .iter()
            .all(|value| *value == 0)
    );
    assert!(
        ledger
            .expense_accumulator_milli
            .iter()
            .all(|value| *value == 0)
    );
    assert!(ledger.income_sources_milli.iter().all(|value| *value == 0));
    assert!(ledger.expense_sources_milli.iter().all(|value| *value == 0));
    assert_eq!(world.save("annual-preview").unwrap(), snapshot);
    assert_eq!(
        world.country_ledgers()[&country].income_accumulator_milli[2],
        400
    );
    assert!(world.yearly_system_id().is_none());
}

#[test]
fn annual_ledger_rollover_does_not_partially_update_countries_on_overflow() {
    let (mut world, first_country) = world();
    let second_country = world.add_entity("BBB", EntityKind::Country);

    let mut first_income = vec![0; 19];
    first_income[2] = 45;
    let first_ledger = CountryLedger {
        income_accumulator_milli: first_income,
        income_rollover_milli: vec![7; 19],
        income_rollover_total_milli: 133,
        ..Default::default()
    };

    let mut second_income = vec![0; 19];
    second_income[0] = i64::MAX;
    second_income[1] = 1;
    let second_ledger = CountryLedger {
        income_accumulator_milli: second_income,
        income_rollover_milli: vec![9; 19],
        income_rollover_total_milli: 171,
        ..Default::default()
    };

    let ledgers = BTreeMap::from([
        (first_country, first_ledger),
        (second_country, second_ledger),
    ]);
    world.set_country_ledgers(ledgers.clone()).unwrap();

    let rollover = CountryLedgerAnnualRollover;
    let pipeline =
        YearlyPipeline::new("country-ledger-overflow:test:v1", None, Some(&rollover)).unwrap();
    world.select_yearly_system(&pipeline).unwrap();

    assert!(world.advance_with_yearly_system(52, &pipeline).is_err());
    assert_eq!(world.country_ledgers(), &ledgers);
}
