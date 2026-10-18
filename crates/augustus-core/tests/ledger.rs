// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::BTreeMap;

use augustus_core::{CountryLedger, CountryResources, EntityKind, Schema, World};

fn world() -> (World, augustus_core::EntityId) {
    let mut world = World::new(Schema::new(["unused"]).unwrap());
    let country = world.add_entity("AAA", EntityKind::Country);
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
    (world, country)
}

#[test]
fn immediate_income_and_expense_update_treasury_and_categories() {
    let (mut world, country) = world();
    world.record_country_income(country, 3, 50).unwrap();
    world.record_country_expense(country, 17, 20).unwrap();

    assert_eq!(
        world.country_resources()[&country].treasury_milli,
        Some(130)
    );
    assert_eq!(
        world.country_ledgers()[&country].categories[&3].income_milli,
        50
    );
    assert_eq!(
        world.country_ledgers()[&country].categories[&17].expense_milli,
        20
    );
    assert_eq!(world.country_ledgers()[&country].pending_milli, 0);
}

#[test]
fn delayed_flows_accumulate_then_flush_with_native_treasury_clamp() {
    let (mut world, country) = world();
    world.record_delayed_country_income(country, 1, 40).unwrap();
    world
        .record_delayed_country_expense(country, 18, 15)
        .unwrap();
    assert_eq!(
        world.country_resources()[&country].treasury_milli,
        Some(100)
    );
    assert_eq!(world.country_ledgers()[&country].pending_milli, 25);

    assert_eq!(world.flush_country_pending(country).unwrap(), 125);
    assert_eq!(world.country_ledgers()[&country].pending_milli, 0);

    world
        .record_delayed_country_expense(country, 18, 200)
        .unwrap();
    assert_eq!(world.flush_country_pending(country).unwrap(), -75);
    assert_eq!(
        world.country_ledgers()[&country].categories[&18].expense_milli,
        215
    );
}

#[test]
fn immediate_treasury_changes_clamp_and_ledger_reset_preserves_pending_balance() {
    let (mut world, country) = world();
    world
        .set_country_resources(
            [(
                country,
                CountryResources {
                    treasury_milli: Some(999_999_999),
                    ..Default::default()
                },
            )]
            .into(),
        )
        .unwrap();
    world.record_country_income(country, 0, 10).unwrap();
    assert_eq!(
        world.country_resources()[&country].treasury_milli,
        Some(1_000_000_000)
    );

    world.record_delayed_country_income(country, 2, 7).unwrap();
    world.reset_country_ledger(country).unwrap();
    assert_eq!(world.country_ledgers()[&country].pending_milli, 7);
    assert!(world.country_ledgers()[&country].categories.is_empty());
}

#[test]
fn ledger_state_round_trips_and_old_empty_state_is_valid() {
    let (mut world, country) = world();
    world.record_delayed_country_income(country, 5, 12).unwrap();
    let saved = world.save("ledger").unwrap();
    let restored = World::restore(&saved, "ledger", world.schema()).unwrap();
    assert_eq!(restored.country_ledgers(), world.country_ledgers());

    let mut old = serde_json::from_str::<serde_json::Value>(&saved).unwrap();
    old.as_object_mut().unwrap().remove("country_ledgers");
    let restored_old = World::restore(&old.to_string(), "ledger", world.schema()).unwrap();
    assert!(restored_old.country_ledgers().is_empty());
}

#[test]
fn explicit_ledger_replacement_validates_country_and_category() {
    let (mut world, country) = world();
    let mut valid = BTreeMap::new();
    valid.insert(country, CountryLedger::default());
    world.set_country_ledgers(valid).unwrap();

    let invalid_category = CountryLedger {
        categories: [(19, Default::default())].into(),
        ..Default::default()
    };
    assert!(
        world
            .set_country_ledgers([(country, invalid_category)].into())
            .is_err()
    );
}
