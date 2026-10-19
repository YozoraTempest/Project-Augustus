// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    Action, AttributeBase, AttributeFormula, Attributes, Calendar, CountryResources, EntityId,
    EntityKind, Error, FieldId, MonthlyEconomy, MonthlyExternalCashflows, MonthlyPipeline,
    MonthlyProduction, MonthlySystem, MonthlyTax, Schema, Taxation, VanillaProductionInput,
    VanillaProductionInputState, World, WorldDate,
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
    let pipeline = MonthlyPipeline::new(
        "standard-economy-pipeline:v1",
        Some(&economy),
        Some(&parallel),
        Some(&serial),
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
        world.country_ledgers()[&country].income_sources_milli[0],
        1_000
    );
    assert_eq!(
        world.country_ledgers()[&country].income_sources_milli[1],
        84
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
}
