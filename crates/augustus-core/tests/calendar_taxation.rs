// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    Action, AttributeBase, AttributeFormula, Attributes, Calendar, CountryResources, EntityId,
    EntityKind, FieldId, MonthlySystem, Schema, Target, Taxation, World, WorldDate,
};
use std::collections::BTreeSet;

fn date(year: i16, month: u8, day: u8) -> WorldDate {
    WorldDate { year, month, day }
}

fn calendar(start: WorldDate) -> Calendar {
    Calendar {
        start,
        month_lengths: [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31],
    }
}

fn scenario() -> World {
    let mut world = World::new(Schema::new(["annual_tax"]).unwrap());
    let alpha = world.add_entity("Alpha", EntityKind::Country);
    let beta = world.add_entity("Beta", EntityKind::Country);
    let north = world.add_entity("North", EntityKind::Province);
    let south = world.add_entity("South", EntityKind::Province);
    let mut attributes = Attributes::default();
    for (province, owner, annual) in [(north, alpha, 12_000), (south, beta, 24_000)] {
        world.set_owner(province, owner).unwrap();
        world.set(province, FieldId(0), annual).unwrap();
        attributes.formulas.insert(
            province,
            [(
                "annual_tax".into(),
                AttributeFormula {
                    base: AttributeBase::Field {
                        field: FieldId(0),
                        micros_per_unit: 1000,
                    },
                    channels: vec![],
                    minimum_micros: None,
                    maximum_micros: None,
                    unresolved: BTreeSet::new(),
                },
            )]
            .into_iter()
            .collect(),
        );
    }
    world.set_attributes(attributes).unwrap();
    world
        .set_country_resources(
            [(alpha, 10_000), (beta, 20_000)]
                .into_iter()
                .map(|(id, treasury)| {
                    (
                        id,
                        CountryResources {
                            treasury_milli: Some(treasury),
                            ..Default::default()
                        },
                    )
                })
                .collect(),
        )
        .unwrap();
    world.set_calendar(calendar(date(1444, 11, 11))).unwrap();
    world
        .set_taxation(Taxation::new(
            "annual_tax",
            [alpha, beta].into_iter().collect(),
        ))
        .unwrap();
    world
}

fn balance(world: &World, id: usize) -> Option<i64> {
    world.country_resources()[&EntityId(id)].treasury_milli
}

#[test]
fn calendar_has_content_defined_month_lengths_and_checked_dates() {
    let c = calendar(date(1444, 2, 28));
    assert_eq!(c.date(1).unwrap(), date(1444, 3, 1));
    assert_eq!(c.date(365).unwrap(), date(1445, 2, 28));
    assert_eq!(c.next_month(0).unwrap(), 1);
    assert_eq!(
        calendar(date(1600, 2, 28)).date(1).unwrap(),
        date(1600, 3, 1)
    );
    let native = Calendar {
        start: date(1, 1, 30),
        month_lengths: [30; 12],
    };
    assert_eq!(native.date(1).unwrap(), date(1, 2, 1));
    assert_eq!(native.date(360).unwrap(), date(2, 1, 30));
    for invalid in [
        date(0, 1, 1),
        date(1444, 0, 1),
        date(1444, 13, 1),
        date(1444, 2, 29),
        date(1444, 1, 0),
    ] {
        assert!(calendar(invalid).date(0).is_err());
    }
    assert!(
        Calendar {
            start: date(1444, 1, 1),
            month_lengths: [0; 12]
        }
        .date(0)
        .is_err()
    );
    assert!(calendar(date(i16::MAX, 12, 31)).date(1).is_err());
    let mut world = scenario();
    let before = world.clone();
    assert!(world.advance_to(u64::MAX).is_err());
    assert_eq!(world, before);
}

#[test]
fn crossings_settle_once_and_a_year_has_twelve_receipts() {
    let mut world = scenario();
    world.advance_to(19).unwrap();
    assert_eq!(world.date().unwrap(), Some(date(1444, 11, 30)));
    assert_eq!(world.taxation().unwrap().settlement_count, 0);
    assert_eq!(balance(&world, 0), Some(10_000));
    world.advance_to(20).unwrap();
    assert_eq!(world.date().unwrap(), Some(date(1444, 12, 1)));
    assert_eq!(balance(&world, 0), Some(11_000));
    assert_eq!(balance(&world, 1), Some(22_000));
    let once = world.clone();
    world.advance_to(20).unwrap();
    assert_eq!(world, once);
    world.advance_to(51).unwrap();
    assert_eq!(world.date().unwrap(), Some(date(1445, 1, 1)));
    world.advance_to(365).unwrap();
    assert_eq!(world.date().unwrap(), Some(date(1445, 11, 11)));
    assert_eq!(world.taxation().unwrap().settlement_count, 12);
    assert_eq!(balance(&world, 0), Some(22_000));
    assert_eq!(balance(&world, 1), Some(44_000));
    assert!(world.advance_to(364).is_err());
}

#[test]
fn first_of_month_start_does_not_generate_an_initial_receipt() {
    let mut world = scenario();
    world.set_calendar(calendar(date(1444, 11, 1))).unwrap();
    world.advance_to(0).unwrap();
    assert_eq!(world.taxation().unwrap().settlement_count, 0);
    world.advance_to(30).unwrap();
    assert_eq!(world.taxation().unwrap().settlement_count, 1);
    assert_eq!(world.date().unwrap(), Some(date(1444, 12, 1)));
}

#[test]
fn taxation_reads_current_ownership_and_rounds_after_aggregation() {
    let mut world = scenario();
    world.set_owner(EntityId(3), EntityId(0)).unwrap();
    world.set(EntityId(2), FieldId(0), 1006).unwrap();
    world.set(EntityId(3), FieldId(0), 1006).unwrap();
    world.advance_to(20).unwrap();
    assert_eq!(balance(&world, 0), Some(10_167));
    assert_eq!(balance(&world, 1), Some(20_000));
    world.set_owner(EntityId(2), EntityId(1)).unwrap();
    world.set(EntityId(2), FieldId(0), 36_000).unwrap();
    world.advance_to(51).unwrap();
    assert_eq!(balance(&world, 0), Some(10_250));
    assert_eq!(balance(&world, 1), Some(23_000));
}

#[test]
fn due_tasks_precede_tax_and_resumed_execution_matches_daily_and_continuous_runs() {
    let mut continuous = scenario();
    for annual in [24_000, 36_000] {
        continuous
            .schedule(
                EntityId(2),
                20,
                vec![Action::Set {
                    target: Target::Current,
                    field: FieldId(0),
                    value: annual,
                }],
            )
            .unwrap();
    }
    continuous
        .schedule(
            EntityId(0),
            51,
            vec![Action::RandomBranch {
                then: vec![Action::Emit("branch a".into())],
                otherwise: vec![Action::Emit("branch b".into())],
            }],
        )
        .unwrap();
    let mut daily = continuous.clone();
    let mut resumed = continuous.clone();
    resumed.advance_to(20).unwrap();
    assert_eq!(balance(&resumed, 0), Some(13_000));
    let snapshot = resumed.save("independent-tax-fixture:1").unwrap();
    resumed = World::restore(&snapshot, "independent-tax-fixture:1", resumed.schema()).unwrap();
    resumed.advance_to(365).unwrap();
    continuous.advance_to(365).unwrap();
    for day in 1..=365 {
        daily.advance_to(day).unwrap();
    }
    assert_eq!(resumed, continuous);
    assert_eq!(daily, continuous);
    assert_eq!(balance(&resumed, 0), Some(46_000));
}

#[test]
fn unknown_income_taints_the_balance_and_known_subtotals_are_never_credited() {
    let mut world = scenario();
    let mut attributes = world.attributes().clone();
    attributes
        .formulas
        .get_mut(&EntityId(2))
        .unwrap()
        .get_mut("annual_tax")
        .unwrap()
        .unresolved
        .insert("pending_rule".into());
    world.set_attributes(attributes.clone()).unwrap();
    world.advance_to(20).unwrap();
    let tax = &world
        .taxation()
        .unwrap()
        .last_settlement
        .as_ref()
        .unwrap()
        .accounts[&EntityId(0)];
    assert_eq!(tax.known_subtotal_milli, 1000);
    assert_eq!(tax.income_milli, None);
    assert_eq!(tax.credited_milli, None);
    assert_eq!(tax.treasury_before_milli, Some(10_000));
    assert_eq!(balance(&world, 0), None);
    assert_eq!(balance(&world, 1), Some(22_000));
    attributes
        .formulas
        .get_mut(&EntityId(2))
        .unwrap()
        .get_mut("annual_tax")
        .unwrap()
        .unresolved
        .clear();
    world.set_attributes(attributes).unwrap();
    world.advance_to(51).unwrap();
    let tax = &world
        .taxation()
        .unwrap()
        .last_settlement
        .as_ref()
        .unwrap()
        .accounts[&EntityId(0)];
    assert_eq!(tax.income_milli, Some(1000));
    assert_eq!(tax.credited_milli, None);
    assert!(tax.unresolved.contains("unknown_treasury"));
    assert_eq!(balance(&world, 0), None);
    let saved = world.save("unknown-fixture").unwrap();
    assert_eq!(
        World::restore(&saved, "unknown-fixture", world.schema()).unwrap(),
        world
    );
}

#[test]
fn overflow_cannot_partially_credit_other_countries_and_stops_the_run() {
    let mut world = scenario();
    let mut resources = world.country_resources().clone();
    resources.get_mut(&EntityId(1)).unwrap().treasury_milli = Some(i64::MAX);
    world.set_country_resources(resources.clone()).unwrap();
    assert!(world.advance_to(20).is_err());
    assert_eq!(world.country_resources(), &resources);
    assert_eq!(world.taxation().unwrap().settlement_count, 0);
    assert!(world.save("overflow-fixture").is_err());
    assert!(world.advance_to(21).is_err());
}

#[test]
fn corrupted_receipts_and_provider_selections_are_rejected() {
    let mut world = scenario();
    world.advance_to(51).unwrap();
    let snapshot: serde_json::Value =
        serde_json::from_str(&world.save("fixture").unwrap()).unwrap();
    let mut bad = snapshot.clone();
    bad["taxation"]["settlement_count"] = 1.into();
    assert!(World::restore(&bad.to_string(), "fixture", world.schema()).is_err());
    bad = snapshot.clone();
    bad["taxation"]["last_settlement"]["day"] = 20.into();
    assert!(World::restore(&bad.to_string(), "fixture", world.schema()).is_err());
    bad = snapshot.clone();
    bad["taxation"]["last_settlement"]["accounts"]["0"]["credited_milli"] = 999.into();
    assert!(World::restore(&bad.to_string(), "fixture", world.schema()).is_err());
    bad = snapshot;
    bad["monthly_system_id"] = "different-system:1".into();
    assert!(World::restore(&bad.to_string(), "fixture", world.schema()).is_err());
}

struct NativeMonthly;

impl MonthlySystem for NativeMonthly {
    fn id(&self) -> &str {
        "native-monthly-fixture:1"
    }
    fn validate(&self, world: &World) -> Result<(), augustus_core::Error> {
        world.value(EntityId(0), FieldId(0)).map(|_| ())
    }
    fn settle(&self, world: &mut World) -> Result<(), augustus_core::Error> {
        world.execute(
            EntityId(0),
            &[Action::Add {
                target: Target::Current,
                field: FieldId(0),
                value: 777,
            }],
        )
    }
}

#[test]
fn native_executor_replaces_the_whole_monthly_system_and_is_identified_in_saves() {
    let mut world = scenario();
    world.select_monthly_system(&NativeMonthly).unwrap();
    assert!(world.taxation().is_none());
    assert!(world.advance_to(20).is_err());
    world
        .advance_with_monthly_system(20, &NativeMonthly)
        .unwrap();
    assert_eq!(world.value(EntityId(0), FieldId(0)).unwrap(), 777);
    assert_eq!(balance(&world, 0), Some(10_000));
    let saved = world.save("native-monthly-content").unwrap();
    let mut restored = World::restore(&saved, "native-monthly-content", world.schema()).unwrap();
    assert_eq!(
        restored.monthly_system_id(),
        Some("native-monthly-fixture:1")
    );
    assert!(
        restored
            .advance_with_monthly_system(51, &augustus_core::MonthlyTax)
            .is_err()
    );
    restored
        .advance_with_monthly_system(365, &NativeMonthly)
        .unwrap();
    world
        .advance_with_monthly_system(365, &NativeMonthly)
        .unwrap();
    assert_eq!(restored, world);
    assert_eq!(world.value(EntityId(0), FieldId(0)).unwrap(), 12 * 777);
    assert!(world.select_monthly_system(&NativeMonthly).is_err());
}
