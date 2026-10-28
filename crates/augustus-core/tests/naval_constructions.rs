// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    Action, COUNTRY_LEDGER_EXPENSE_CATEGORY_COUNT, Calendar, CountryLedger, CountryResource,
    CountryResources, EntityId, EntityKind, EntityRef, NavalConstructionId, NavalConstructionInput,
    NumericOperation, NumericSlot, PaidNavalConstructionInput, Schema, ScopeContext, ScopeSelector,
    World, WorldDate,
};

fn fixture() -> (World, EntityId, EntityId) {
    let mut world = World::new(Schema::new(std::iter::empty::<String>()).unwrap());
    let country = world.add_entity("SWE", EntityKind::Country);
    let port = world.add_entity("1", EntityKind::Province);
    world.set_province_has_port(port, true).unwrap();
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
    (world, country, port)
}

fn input(
    owner: EntityId,
    location: EntityId,
    unit_type: &str,
    completion_day: u64,
) -> NavalConstructionInput {
    NavalConstructionInput {
        owner,
        location,
        unit_type: unit_type.into(),
        completion_day,
    }
}

fn paid_input(
    owner: EntityId,
    location: EntityId,
    unit_type: &str,
    completion_day: u64,
    cost_milli: i64,
) -> PaidNavalConstructionInput {
    PaidNavalConstructionInput {
        construction: input(owner, location, unit_type, completion_day),
        cost_milli,
    }
}

#[test]
fn construction_uses_explicit_day_and_round_trips_without_duplicate_completion() {
    let (mut continuous, owner, port) = fixture();
    assert_eq!(
        continuous
            .enqueue_naval_constructions(&[input(owner, port, "construction_hull", 3)])
            .unwrap(),
        [NavalConstructionId(0)]
    );

    continuous.advance_to(2).unwrap();
    assert!(continuous.naval_units().is_empty());
    assert_eq!(continuous.naval_constructions().len(), 1);
    let saved = continuous.save("naval-construction").unwrap();
    let mut resumed = World::restore(&saved, "naval-construction", continuous.schema()).unwrap();

    continuous.advance_to(3).unwrap();
    resumed.advance_to(3).unwrap();
    assert_eq!(resumed, continuous);
    assert!(continuous.naval_constructions().is_empty());
    assert_eq!(continuous.naval_units().len(), 1);
    assert_eq!(continuous.naval_units()[0].home_province, Some(port));
    assert_eq!(continuous.naval_units()[0].name, None);
    assert_eq!(continuous.naval_units()[0].morale_milli, None);
    assert_eq!(continuous.fleets().len(), 1);
    assert_eq!(continuous.fleets()[0].ships.len(), 1);

    continuous.advance_to(6).unwrap();
    assert_eq!(continuous.naval_units().len(), 1);
    let completed = continuous.save("naval-construction").unwrap();
    let saved_value: serde_json::Value = serde_json::from_str(&completed).unwrap();
    assert_eq!(saved_value["naval_constructions"]["next_id"], 1);
    assert_eq!(
        saved_value["naval_constructions"]["orders"]
            .as_array()
            .unwrap()
            .len(),
        0
    );

    let mut after_restore =
        World::restore(&completed, "naval-construction", continuous.schema()).unwrap();
    assert_eq!(
        after_restore
            .enqueue_naval_constructions(&[input(owner, port, "next_hull", 7)])
            .unwrap(),
        [NavalConstructionId(1)]
    );
}

#[test]
fn paid_batch_subtracts_once_round_trips_and_completes_without_ledger_changes() {
    let (mut continuous, owner, port) = fixture();
    let other = continuous.add_entity("NOR", EntityKind::Country);
    continuous
        .set_country_resources(
            [(
                owner,
                CountryResources {
                    treasury_milli: Some(100),
                    ..Default::default()
                },
            )]
            .into(),
        )
        .unwrap();
    continuous
        .set_country_ledgers([(owner, CountryLedger::default())].into())
        .unwrap();
    let starting_ledger = continuous.country_ledgers()[&owner].clone();
    let treasury = NumericSlot::Resource(CountryResource::Treasury);
    continuous
        .update_numeric(other, &treasury, NumericOperation::Add, 20)
        .unwrap();

    assert_eq!(
        continuous
            .enqueue_paid_naval_constructions(&[
                paid_input(owner, port, "first_hull", 3, 75),
                paid_input(owner, port, "second_hull", 3, 50),
                paid_input(other, port, "third_hull", 3, 40),
            ])
            .unwrap(),
        [
            NavalConstructionId(0),
            NavalConstructionId(1),
            NavalConstructionId(2)
        ]
    );
    assert_eq!(
        continuous.country_resources()[&owner].treasury_milli,
        Some(-25)
    );
    assert_eq!(continuous.country_resources().get(&other), None);
    assert_eq!(
        continuous.unbased_numeric_delta(other, &treasury),
        Some(-20)
    );
    assert_eq!(continuous.country_ledgers()[&owner], starting_ledger);
    assert_eq!(continuous.naval_constructions().len(), 3);

    let saved = continuous.save("paid-naval-construction").unwrap();
    let mut resumed =
        World::restore(&saved, "paid-naval-construction", continuous.schema()).unwrap();
    assert_eq!(resumed, continuous);
    continuous.advance_to(3).unwrap();
    resumed.advance_to(3).unwrap();
    assert_eq!(resumed, continuous);
    assert!(continuous.naval_constructions().is_empty());
    assert_eq!(continuous.naval_units().len(), 3);
    assert_eq!(
        continuous.country_resources()[&owner].treasury_milli,
        Some(-25)
    );
    assert_eq!(
        continuous.unbased_numeric_delta(other, &treasury),
        Some(-20)
    );
    assert_eq!(continuous.country_ledgers()[&owner], starting_ledger);
}

#[test]
fn categorized_paid_batch_records_once_and_preserves_pending_through_restore_and_flush() {
    let (mut continuous, owner, port) = fixture();
    let expense_category = 27;
    continuous
        .set_country_resources(
            [(
                owner,
                CountryResources {
                    treasury_milli: Some(1_000),
                    ..Default::default()
                },
            )]
            .into(),
        )
        .unwrap();
    continuous
        .set_country_ledgers(
            [(
                owner,
                CountryLedger {
                    pending_milli: 40,
                    ..Default::default()
                },
            )]
            .into(),
        )
        .unwrap();

    assert_eq!(
        continuous
            .enqueue_naval_constructions_with_expenses(
                &[
                    paid_input(owner, port, "first_hull", 3, 75),
                    paid_input(owner, port, "second_hull", 3, 25),
                ],
                expense_category,
            )
            .unwrap(),
        [NavalConstructionId(0), NavalConstructionId(1)]
    );
    let ledger = &continuous.country_ledgers()[&owner];
    assert_eq!(ledger.pending_milli, 40);
    assert_eq!(ledger.expense_sources_milli[expense_category as usize], 100);
    assert_eq!(
        ledger.expense_accumulator_milli[expense_category as usize],
        100
    );
    assert_eq!(
        ledger.expense_auxiliary_milli[expense_category as usize],
        100
    );
    assert_eq!(ledger.categories[&expense_category].expense_milli, 100);
    assert_eq!(
        continuous.country_resources()[&owner].treasury_milli,
        Some(900)
    );
    assert_eq!(continuous.naval_constructions().len(), 2);

    let saved = continuous.save("categorized-naval-construction").unwrap();
    let mut resumed = World::restore(
        &saved,
        "categorized-naval-construction",
        continuous.schema(),
    )
    .unwrap();
    continuous.advance_to(3).unwrap();
    resumed.advance_to(3).unwrap();
    assert_eq!(resumed, continuous);
    assert_eq!(continuous.naval_constructions().len(), 0);
    assert_eq!(continuous.naval_units().len(), 2);
    assert_eq!(
        continuous.country_ledgers()[&owner].categories[&expense_category].expense_milli,
        100
    );
    assert_eq!(continuous.country_ledgers()[&owner].pending_milli, 40);

    assert_eq!(continuous.flush_country_pending(owner).unwrap(), 940);
    assert_eq!(resumed.flush_country_pending(owner).unwrap(), 940);
    assert_eq!(resumed, continuous);
    assert_eq!(continuous.country_ledgers()[&owner].pending_milli, 0);
    assert_eq!(
        continuous.country_resources()[&owner].treasury_milli,
        Some(940)
    );
    assert_eq!(
        continuous.country_ledgers()[&owner].categories[&expense_category].expense_milli,
        100
    );
}

#[test]
fn categorized_paid_batch_errors_preserve_full_world_state() {
    let (mut world, owner, port) = fixture();
    let other = world.add_entity("NOR", EntityKind::Country);
    let expense_category = 27;
    world
        .set_country_resources(
            [
                (
                    owner,
                    CountryResources {
                        treasury_milli: Some(100),
                        ..Default::default()
                    },
                ),
                (
                    other,
                    CountryResources {
                        treasury_milli: Some(100),
                        ..Default::default()
                    },
                ),
            ]
            .into(),
        )
        .unwrap();

    let before = world.save("categorized-naval-construction").unwrap();
    assert!(
        world
            .enqueue_naval_constructions_with_expenses(
                &[paid_input(owner, port, "first_hull", 3, 10)],
                COUNTRY_LEDGER_EXPENSE_CATEGORY_COUNT,
            )
            .is_err()
    );
    assert_eq!(
        world.save("categorized-naval-construction").unwrap(),
        before
    );

    let mut unknown_treasury = world.clone();
    unknown_treasury
        .set_country_resources([(owner, CountryResources::default())].into())
        .unwrap();
    let unknown_before = unknown_treasury
        .save("categorized-naval-construction")
        .unwrap();
    assert!(
        unknown_treasury
            .enqueue_naval_constructions_with_expenses(
                &[paid_input(owner, port, "first_hull", 3, 10)],
                expense_category,
            )
            .is_err()
    );
    assert_eq!(
        unknown_treasury
            .save("categorized-naval-construction")
            .unwrap(),
        unknown_before
    );

    let mut overflowing_ledger = CountryLedger {
        expense_sources_milli: vec![0; COUNTRY_LEDGER_EXPENSE_CATEGORY_COUNT as usize],
        ..Default::default()
    };
    overflowing_ledger.expense_sources_milli[expense_category as usize] = i64::MAX;
    world
        .set_country_ledgers(
            [
                (owner, CountryLedger::default()),
                (other, overflowing_ledger),
            ]
            .into(),
        )
        .unwrap();
    let ledger_before = world.save("categorized-naval-construction").unwrap();
    assert!(
        world
            .enqueue_naval_constructions_with_expenses(
                &[
                    paid_input(owner, port, "first_hull", 3, 10),
                    paid_input(other, port, "second_hull", 3, 10),
                ],
                expense_category,
            )
            .is_err()
    );
    assert_eq!(
        world.save("categorized-naval-construction").unwrap(),
        ledger_before
    );
    assert_eq!(world.country_ledgers()[&owner], CountryLedger::default());
    assert_eq!(world.country_resources()[&owner].treasury_milli, Some(100));
    assert!(world.naval_constructions().is_empty());

    let mut exhausted: serde_json::Value = ledger_before.parse().unwrap();
    exhausted["naval_constructions"] = serde_json::json!({
        "next_id": u64::MAX,
        "orders": []
    });
    let mut exhausted = World::restore(
        &exhausted.to_string(),
        "categorized-naval-construction",
        world.schema(),
    )
    .unwrap();
    let exhausted_before = exhausted.save("categorized-naval-construction").unwrap();
    assert!(
        exhausted
            .enqueue_naval_constructions_with_expenses(
                &[paid_input(owner, port, "first_hull", 3, 10)],
                expense_category,
            )
            .is_err()
    );
    assert_eq!(
        exhausted.save("categorized-naval-construction").unwrap(),
        exhausted_before
    );
    assert_eq!(
        exhausted.country_ledgers()[&owner],
        CountryLedger::default()
    );
    assert_eq!(
        exhausted.country_resources()[&owner].treasury_milli,
        Some(100)
    );
}

#[test]
fn paid_batch_errors_preserve_balances_deltas_and_order_ids() {
    let (mut world, owner, port) = fixture();
    let other = world.add_entity("NOR", EntityKind::Country);
    world
        .set_country_resources(
            [(
                owner,
                CountryResources {
                    treasury_milli: Some(100),
                    ..Default::default()
                },
            )]
            .into(),
        )
        .unwrap();
    let treasury = NumericSlot::Resource(CountryResource::Treasury);
    world
        .update_numeric(other, &treasury, NumericOperation::Subtract, i64::MAX)
        .unwrap();
    let before = world.save("paid-naval-construction").unwrap();

    assert!(
        world
            .enqueue_paid_naval_constructions(&[
                paid_input(owner, port, "first_hull", 3, 10),
                paid_input(port, port, "invalid_owner", 3, 1),
            ])
            .is_err()
    );
    assert_eq!(world.save("paid-naval-construction").unwrap(), before);

    assert!(
        world
            .enqueue_paid_naval_constructions(&[
                paid_input(owner, port, "first_hull", 3, -1),
                paid_input(owner, port, "second_hull", 3, 1),
            ])
            .is_err()
    );
    assert_eq!(world.save("paid-naval-construction").unwrap(), before);

    assert!(
        world
            .enqueue_paid_naval_constructions(&[
                paid_input(owner, port, "first_hull", 3, i64::MAX),
                paid_input(owner, port, "second_hull", 3, 1),
            ])
            .is_err()
    );
    assert_eq!(world.save("paid-naval-construction").unwrap(), before);

    assert!(
        world
            .enqueue_paid_naval_constructions(&[
                paid_input(owner, port, "first_hull", 3, 10),
                paid_input(other, port, "second_hull", 3, 2),
            ])
            .is_err()
    );
    assert_eq!(world.save("paid-naval-construction").unwrap(), before);
    assert_eq!(world.naval_constructions().len(), 0);
    assert_eq!(world.country_resources()[&owner].treasury_milli, Some(100));
    assert_eq!(
        world.unbased_numeric_delta(other, &treasury),
        Some(-i64::MAX)
    );
    assert_eq!(
        world
            .enqueue_paid_naval_constructions(&[paid_input(owner, port, "valid_hull", 3, 10)])
            .unwrap(),
        [NavalConstructionId(0)]
    );

    let mut exhausted: serde_json::Value = before.parse().unwrap();
    exhausted["naval_constructions"] = serde_json::json!({
        "next_id": u64::MAX,
        "orders": []
    });
    let mut exhausted = World::restore(
        &exhausted.to_string(),
        "paid-naval-construction",
        world.schema(),
    )
    .unwrap();
    let exhausted_before = exhausted.save("paid-naval-construction").unwrap();
    assert!(
        exhausted
            .enqueue_paid_naval_constructions(&[paid_input(owner, port, "cannot_allocate", 3, 25)])
            .is_err()
    );
    assert_eq!(
        exhausted.save("paid-naval-construction").unwrap(),
        exhausted_before
    );
    assert_eq!(
        exhausted.country_resources()[&owner].treasury_milli,
        Some(100)
    );
}

#[test]
fn due_construction_runs_before_same_day_scheduled_tasks() {
    let (mut world, owner, port) = fixture();
    world
        .enqueue_naval_constructions(&[input(owner, port, "construction_hull", 2)])
        .unwrap();
    world
        .schedule_in_context(
            ScopeContext {
                current: port,
                root: owner,
            },
            2,
            vec![Action::CreateNavalUnit {
                owner: EntityRef::Selected(ScopeSelector::Root),
                unit_type: "task_hull".into(),
            }],
        )
        .unwrap();

    world.advance_to(2).unwrap();

    assert_eq!(world.naval_units().len(), 2);
    assert_eq!(world.naval_units()[0].unit_type, "construction_hull");
    assert_eq!(world.naval_units()[1].unit_type, "task_hull");
}

#[test]
fn invalid_batch_input_and_id_overflow_do_not_mutate_orders() {
    let (mut world, owner, port) = fixture();
    let before = world.save("naval-construction").unwrap();
    assert!(
        world
            .enqueue_naval_constructions(&[
                input(owner, port, "valid_hull", 3),
                input(port, port, "invalid_owner", 4),
            ])
            .is_err()
    );
    assert_eq!(world.save("naval-construction").unwrap(), before);
    assert!(
        world
            .enqueue_naval_constructions(&[input(owner, port, "too_late", u64::MAX)])
            .is_err()
    );
    assert_eq!(world.save("naval-construction").unwrap(), before);
    assert_eq!(
        world
            .enqueue_naval_constructions(&[input(owner, port, "valid_hull", 3)])
            .unwrap(),
        [NavalConstructionId(0)]
    );

    let mut exhausted: serde_json::Value = before.parse().unwrap();
    exhausted["naval_constructions"] = serde_json::json!({
        "next_id": u64::MAX,
        "orders": []
    });
    let mut exhausted =
        World::restore(&exhausted.to_string(), "naval-construction", world.schema()).unwrap();
    let exhausted_before = exhausted.save("naval-construction").unwrap();
    assert!(
        exhausted
            .enqueue_naval_constructions(&[input(owner, port, "cannot_allocate", 3)])
            .is_err()
    );
    assert_eq!(
        exhausted.save("naval-construction").unwrap(),
        exhausted_before
    );
}

#[test]
fn invalid_due_order_leaves_the_whole_completion_batch_unchanged() {
    let (mut world, owner, first_port) = fixture();
    let second_port = world.add_entity("2", EntityKind::Province);
    world.set_province_has_port(second_port, true).unwrap();
    world
        .enqueue_naval_constructions(&[
            input(owner, first_port, "first_hull", 2),
            input(owner, second_port, "second_hull", 2),
        ])
        .unwrap();
    world.set_province_has_port(second_port, false).unwrap();

    assert!(world.advance_to(2).is_err());
    assert!(world.naval_units().is_empty());
    assert!(world.fleets().is_empty());
    assert_eq!(world.naval_constructions().len(), 2);
}

#[test]
fn completion_identifier_overflow_does_not_partially_create_ships() {
    let (mut world, owner, port) = fixture();
    world
        .enqueue_naval_constructions(&[
            input(owner, port, "first_hull", 2),
            input(owner, port, "second_hull", 2),
        ])
        .unwrap();
    let mut saved: serde_json::Value = world.save("naval-construction").unwrap().parse().unwrap();
    saved["naval_units"] = serde_json::json!({
        "next_id": u64::MAX - 1,
        "units": []
    });
    let mut world =
        World::restore(&saved.to_string(), "naval-construction", world.schema()).unwrap();

    assert!(world.advance_to(2).is_err());
    assert!(world.naval_units().is_empty());
    assert!(world.fleets().is_empty());
    assert_eq!(world.naval_constructions().len(), 2);
}

#[test]
fn old_empty_snapshot_shape_and_restore_validation_are_preserved() {
    let (empty, _, _) = fixture();
    let empty_saved = empty.save("naval-construction").unwrap();
    assert!(!empty_saved.contains("naval_constructions"));
    let mut legacy: serde_json::Value = serde_json::from_str(&empty_saved).unwrap();
    legacy
        .as_object_mut()
        .unwrap()
        .remove("naval_constructions");
    assert_eq!(
        World::restore(&legacy.to_string(), "naval-construction", empty.schema()).unwrap(),
        empty
    );

    let (mut world, owner, port) = fixture();
    world
        .enqueue_naval_constructions(&[input(owner, port, "valid_hull", 3)])
        .unwrap();
    let saved: serde_json::Value =
        serde_json::from_str(&world.save("naval-construction").unwrap()).unwrap();

    let mut corrupt = saved.clone();
    corrupt["naval_constructions"]["orders"][0]["owner"] = serde_json::json!(port.0);
    assert!(World::restore(&corrupt.to_string(), "naval-construction", world.schema()).is_err());

    let mut corrupt = saved.clone();
    corrupt["naval_constructions"]["orders"][0]["location"] = serde_json::json!(owner.0);
    assert!(World::restore(&corrupt.to_string(), "naval-construction", world.schema()).is_err());

    let mut corrupt = saved.clone();
    corrupt["entities"][port.0]["province_has_port"] = serde_json::json!(false);
    assert!(World::restore(&corrupt.to_string(), "naval-construction", world.schema()).is_err());

    let mut corrupt = saved.clone();
    corrupt["naval_constructions"]["orders"][0]["unit_type"] = serde_json::json!("invalid type");
    assert!(World::restore(&corrupt.to_string(), "naval-construction", world.schema()).is_err());

    let mut corrupt = saved;
    corrupt["naval_constructions"]["orders"][0]["completion_day"] = serde_json::json!(0);
    assert!(World::restore(&corrupt.to_string(), "naval-construction", world.schema()).is_err());
}
