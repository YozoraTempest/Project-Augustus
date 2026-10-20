// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::{BTreeMap, BTreeSet};

use augustus_core::{
    COUNTRY_LEDGER_EXPENSE_KEYS, COUNTRY_LEDGER_INCOME_KEYS,
    COUNTRY_LEDGER_NATIVE_FILTERED_EXPENSE_CATEGORIES,
    COUNTRY_LEDGER_NATIVE_FILTERED_INCOME_CATEGORIES, COUNTRY_LEDGER_SEND_OFFICERS_CATEGORY,
    COUNTRY_LEDGER_SUPPORT_LOYALISTS_CATEGORY, CountryLedger, CountryResources, EntityKind, Schema,
    World, country_ledger_expense_key, country_ledger_income_key,
};

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
        categories: [(38, Default::default())].into(),
        ..Default::default()
    };
    assert!(
        world
            .set_country_ledgers([(country, invalid_category)].into())
            .is_err()
    );
}

#[test]
fn expense_categories_extend_to_relation_slots_23_and_24() {
    let (mut world, country) = world();
    world.record_country_expense(country, 23, 10).unwrap();
    world
        .record_delayed_country_expense(country, 24, 12)
        .unwrap();
    assert_eq!(
        world.country_ledgers()[&country].categories[&23].expense_milli,
        10
    );
    assert_eq!(
        world.country_ledgers()[&country].categories[&24].expense_milli,
        12
    );
    assert_eq!(world.country_ledgers()[&country].pending_milli, -12);
}

#[test]
fn native_ledger_keys_expose_relation_slots_without_numeric_magic() {
    assert_eq!(
        COUNTRY_LEDGER_INCOME_KEYS,
        [
            "INCOMETAX",
            "INCOMEPROD",
            "INCOMETRADE",
            "INCOMEGOLD",
            "INCOMETARRIFFS",
            "INCOMEVASSAL",
            "INCOMEHARBORFEES",
            "INCOMESUBS",
            "INCOME_WAR_REPARATION",
            "INCOMELOAN",
            "INCOMEGIFT",
            "INCOMEEVENT",
            "SPOILS_OF_WAR_ECOVIEW",
            "INCOME_TREASURE_FLEET",
            "SIPHON_INCOME",
            "CONDOTTIERI",
            "KNOWLEDGE_SHARING",
            "INC_BLOCKADES",
            "INC_LOOTING",
        ]
    );
    assert_eq!(
        COUNTRY_LEDGER_EXPENSE_KEYS,
        [
            "EXPENSECOURTMAINTENANCE",
            "EXPENSEINTEREST",
            "EXPENSE_STATE_MAINTENANCE",
            "EXPENSEHARBORFEES",
            "EXPENSESUBS",
            "EXPENSE_WAR_REPARATIONS",
            "EXPENSEAM",
            "EXPENSEFM",
            "EXPENSEFORTM",
            "COLONIAL_MAINTENANCE",
            "EXPENSEMISSIONARY",
            "EXPENSEARMY",
            "EXPENSEFLEET",
            "EXPENSEFORT",
            "EXPENSEMAN",
            "EXPENSESUPPORTREBELS",
            "EXPENSELOAN",
            "EXPENSEGIFT",
            "EXPENSECOURT",
            "EXPENSEEVENTS",
            "EXPENSEPEACE",
            "EXPENSEVASSAL",
            "EXPENSETARIFFS",
            "SUPPORT_LOYALISTS",
            "SEND_OFFICERS",
            "EXPENSE_SIPHON_INCOME",
            "CONDOTTIERI",
            "EXPENSE_CORRUPTION",
            "EXPENSE_TECHNOLOGY",
            "EXPENSE_GREAT_POWER",
            "KNOWLEDGE_SHARING",
            "TRADE_COMPANY_INVESTMENTS",
            "NONE",
            "EXP_BLOCKADES",
            "EXP_LOOTING",
            "EXPENSEGREATPROJECTS",
            "CENTRE_OF_TRADE_UPGRADE",
            "COLONY_CHANGES",
        ]
    );
    assert_eq!(COUNTRY_LEDGER_EXPENSE_KEYS.len(), 38);
    assert_eq!(
        COUNTRY_LEDGER_NATIVE_FILTERED_INCOME_CATEGORIES,
        [9, 10, 11, 13, 14]
    );
    assert_eq!(country_ledger_income_key(0), Some("INCOMETAX"));
    assert_eq!(
        country_ledger_expense_key(COUNTRY_LEDGER_SUPPORT_LOYALISTS_CATEGORY),
        Some("SUPPORT_LOYALISTS")
    );
    assert_eq!(
        country_ledger_expense_key(COUNTRY_LEDGER_SEND_OFFICERS_CATEGORY),
        Some("SEND_OFFICERS")
    );
    assert_eq!(country_ledger_income_key(19), None);
    assert_eq!(country_ledger_expense_key(38), None);
}

#[test]
fn native_monthly_income_filter_excludes_the_verified_five_categories() {
    let mut ledger = CountryLedger {
        income_sources_milli: (1..=19).collect(),
        ..Default::default()
    };

    ledger.consolidate_monthly(&BTreeSet::new()).unwrap();

    assert_eq!(ledger.income_total_milli, 190);
    assert_eq!(ledger.income_filtered_total_milli, 128);
}

#[test]
fn native_monthly_expense_filter_excludes_the_verified_categories() {
    assert_eq!(
        COUNTRY_LEDGER_NATIVE_FILTERED_EXPENSE_CATEGORIES,
        [11, 12, 13, 14, 16, 17, 18, 19, 20, 28, 32, 35, 36]
    );

    let mut ledger = CountryLedger {
        expense_sources_milli: (1..=38).collect(),
        ..Default::default()
    };
    let excluded = COUNTRY_LEDGER_NATIVE_FILTERED_EXPENSE_CATEGORIES
        .into_iter()
        .collect();
    ledger.consolidate_monthly(&excluded).unwrap();

    assert_eq!(ledger.expense_total_milli, 741);
    assert_eq!(ledger.expense_filtered_total_milli, 457);
}

#[test]
fn monthly_consolidation_runs_after_relation_expense_and_annual_rollover_is_explicit() {
    let (mut world, country) = world();
    world.record_delayed_country_income(country, 1, 40).unwrap();
    assert_eq!(
        world.flush_country_ledgers_pending().unwrap()[&country],
        140
    );

    world.record_country_expense(country, 23, 10).unwrap();
    world
        .consolidate_country_ledgers_monthly(&BTreeMap::new())
        .unwrap();

    let ledger = &world.country_ledgers()[&country];
    assert_eq!(
        world.country_resources()[&country].treasury_milli,
        Some(130)
    );
    assert_eq!(ledger.expense_snapshot_milli[23], 10);
    assert_eq!(ledger.expense_total_milli, 10);
    assert_eq!(ledger.expense_filtered_total_milli, 10);
    assert_eq!(ledger.expense_sources_milli[23], 0);

    world.rollover_country_ledgers_annual().unwrap();
    let ledger = &world.country_ledgers()[&country];
    assert_eq!(ledger.income_rollover_milli[1], 40);
    assert_eq!(ledger.expense_rollover_milli[23], 10);
    assert_eq!(ledger.income_rollover_total_milli, 40);
    assert_eq!(ledger.expense_rollover_total_milli, 10);
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
}

#[test]
fn monthly_consolidation_uses_the_native_filtered_expense_set_by_default() {
    let (mut world, country) = world();
    world.record_country_expense(country, 11, 10).unwrap();
    world
        .consolidate_country_ledgers_monthly(&BTreeMap::new())
        .unwrap();

    let ledger = &world.country_ledgers()[&country];
    assert!(COUNTRY_LEDGER_NATIVE_FILTERED_EXPENSE_CATEGORIES.contains(&11));
    assert_eq!(ledger.expense_total_milli, 10);
    assert_eq!(ledger.expense_filtered_total_milli, 0);
}

#[test]
fn monthly_consolidation_allows_an_explicit_empty_filter_override() {
    let (mut world, country) = world();
    world.record_country_expense(country, 11, 10).unwrap();
    world
        .consolidate_country_ledgers_monthly(&[(country, BTreeSet::new())].into())
        .unwrap();

    assert_eq!(
        world.country_ledgers()[&country].expense_filtered_total_milli,
        10
    );
}

#[test]
fn batch_pending_flush_is_atomic_when_one_country_lacks_a_treasury() {
    let (mut world, country) = world();
    let other = world.add_entity("BBB", EntityKind::Country);
    world.record_delayed_country_income(country, 1, 40).unwrap();
    world.record_delayed_country_income(other, 1, 30).unwrap();

    assert!(world.flush_country_ledgers_pending().is_err());
    assert_eq!(
        world.country_resources()[&country].treasury_milli,
        Some(100)
    );
    assert_eq!(world.country_ledgers()[&country].pending_milli, 40);
    assert_eq!(world.country_ledgers()[&other].pending_milli, 30);
}

#[test]
fn monthly_and_annual_ledger_vectors_consolidate_in_their_native_order() {
    let mut ledger = CountryLedger {
        income_sources_milli: vec![0; 19],
        expense_sources_milli: vec![0; 38],
        income_accumulator_milli: vec![0; 19],
        expense_accumulator_milli: vec![0; 38],
        ..Default::default()
    };
    ledger.income_sources_milli[3] = 40;
    ledger.income_sources_milli[9] = 7;
    ledger.expense_sources_milli[17] = 25;
    ledger.expense_sources_milli[1] = 6;
    ledger.income_accumulator_milli[3] = 90;
    ledger.expense_accumulator_milli[17] = 55;

    ledger
        .consolidate_monthly(&[1_u32].into_iter().collect())
        .unwrap();
    assert_eq!(ledger.income_total_milli, 47);
    assert_eq!(ledger.income_filtered_total_milli, 40);
    assert_eq!(ledger.expense_total_milli, 31);
    assert_eq!(ledger.expense_filtered_total_milli, 25);
    assert!(ledger.income_sources_milli.iter().all(|value| *value == 0));
    assert!(ledger.expense_sources_milli.iter().all(|value| *value == 0));

    ledger.rollover_annual().unwrap();
    assert_eq!(ledger.income_rollover_milli[3], 90);
    assert_eq!(ledger.income_rollover_total_milli, 90);
    assert_eq!(ledger.expense_rollover_milli[17], 55);
    assert_eq!(ledger.expense_rollover_total_milli, 55);
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
}
