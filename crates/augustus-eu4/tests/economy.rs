// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    COUNTRY_LEDGER_SEND_OFFICERS_CATEGORY, COUNTRY_LEDGER_SUPPORT_LOYALISTS_CATEGORY,
    CountryLedger, CountryLedgerCategory, CountryResources, VanillaProductionInput, World,
};
use augustus_eu4::{
    CashflowBreakdown, VanillaContentProductionGoodsSizeInput, VanillaContentProductionInput,
    VanillaContentProductionValueInput, VanillaExternalProductionInput, VanillaExternalState,
    VanillaGoldProductionInput, VanillaGoldProductionInputState, VanillaProductionInputState,
    VanillaProvinceProductionBaseValueInput, VanillaTradeGoodsSizeInput, VanillaWorld,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world")
}

fn copy_fixture() -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    for entry in walkdir::WalkDir::new(fixture()) {
        let entry = entry.unwrap();
        let target = temp
            .path()
            .join(entry.path().strip_prefix(fixture()).unwrap());
        if entry.file_type().is_dir() {
            fs::create_dir_all(target).unwrap();
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
    temp
}

fn production_input(
    base_value_milli: i32,
    country_modifier_milli: i32,
    legacy_province_modifier_milli: i32,
    queried_province_modifier_milli: i32,
) -> VanillaProductionInputState {
    VanillaProductionInputState {
        input: Some(VanillaProductionInput {
            base_value_milli,
            country_modifier_milli,
            legacy_province_modifier_milli,
            queried_province_modifier_milli,
        }),
        unresolved: BTreeSet::new(),
    }
}

#[test]
fn country_ledgers_bind_by_eu4_tag_before_startup() {
    let ledger = CountryLedger {
        pending_milli: 12,
        categories: [(
            17,
            CountryLedgerCategory {
                expense_milli: 25,
                ..Default::default()
            },
        )]
        .into(),
        ..Default::default()
    };
    let loaded = VanillaWorld::load_with_country_ledgers(
        fixture(),
        "1444.11.11",
        &BTreeMap::from([("AAA".into(), ledger.clone())]),
    )
    .unwrap();
    let country = loaded.initial.country_entity("AAA").unwrap();
    assert_eq!(loaded.initial.world().country_ledgers()[&country], ledger);
}

#[test]
fn country_ledger_lifecycle_binds_filters_by_eu4_tag() {
    let mut ledger = CountryLedger {
        expense_sources_milli: vec![0; 38],
        expense_accumulator_milli: vec![0; 38],
        ..Default::default()
    };
    ledger.expense_sources_milli[23] = 10;
    ledger.expense_accumulator_milli[23] = 10;

    let mut loaded = VanillaWorld::load(fixture(), "1444.11.11").unwrap();
    loaded
        .set_country_ledgers(BTreeMap::from([("AAA".into(), ledger)]))
        .unwrap();
    loaded
        .consolidate_country_ledgers_monthly(BTreeMap::from([("AAA".into(), BTreeSet::from([23]))]))
        .unwrap();

    let country = loaded.initial.country_entity("AAA").unwrap();
    let consolidated = &loaded.initial.world().country_ledgers()[&country];
    assert_eq!(consolidated.expense_snapshot_milli[23], 10);
    assert_eq!(consolidated.expense_total_milli, 10);
    assert_eq!(consolidated.expense_filtered_total_milli, 0);
    assert_eq!(consolidated.expense_sources_milli[23], 0);

    loaded.rollover_country_ledgers_annual().unwrap();
    let rolled = &loaded.initial.world().country_ledgers()[&country];
    assert_eq!(rolled.expense_rollover_milli[23], 10);
    assert_eq!(rolled.expense_rollover_total_milli, 10);
}

#[test]
fn relationship_expenses_update_treasury_and_ledger_through_eu4_tags() {
    let state = VanillaExternalState {
        country_resources: BTreeMap::from([(
            "AAA".into(),
            CountryResources {
                treasury_milli: Some(1_000),
                ..Default::default()
            },
        )]),
        ..Default::default()
    };
    let mut loaded =
        VanillaWorld::load_with_external_state(fixture(), "1444.11.11", &state).unwrap();

    loaded
        .record_country_expense("AAA", COUNTRY_LEDGER_SUPPORT_LOYALISTS_CATEGORY, 125)
        .unwrap();
    loaded
        .record_country_expense("AAA", COUNTRY_LEDGER_SEND_OFFICERS_CATEGORY, 75)
        .unwrap();

    let country = loaded.initial.country_entity("AAA").unwrap();
    assert_eq!(
        loaded.initial.world().country_resources()[&country].treasury_milli,
        Some(800)
    );
    let ledger = &loaded.initial.world().country_ledgers()[&country];
    assert_eq!(ledger.pending_milli, 0);
    assert_eq!(
        ledger.expense_sources_milli[COUNTRY_LEDGER_SUPPORT_LOYALISTS_CATEGORY as usize],
        125
    );
    assert_eq!(
        ledger.expense_sources_milli[COUNTRY_LEDGER_SEND_OFFICERS_CATEGORY as usize],
        75
    );
    assert_eq!(
        ledger.expense_accumulator_milli[COUNTRY_LEDGER_SUPPORT_LOYALISTS_CATEGORY as usize],
        125
    );
    assert_eq!(
        ledger.expense_accumulator_milli[COUNTRY_LEDGER_SEND_OFFICERS_CATEGORY as usize],
        75
    );

    loaded
        .consolidate_country_ledgers_monthly(BTreeMap::new())
        .unwrap();
    let ledger = &loaded.initial.world().country_ledgers()[&country];
    assert_eq!(
        ledger.expense_snapshot_milli[COUNTRY_LEDGER_SUPPORT_LOYALISTS_CATEGORY as usize],
        125
    );
    assert_eq!(
        ledger.expense_snapshot_milli[COUNTRY_LEDGER_SEND_OFFICERS_CATEGORY as usize],
        75
    );
    assert_eq!(ledger.expense_total_milli, 200);
    assert_eq!(ledger.expense_filtered_total_milli, 200);
}

#[test]
fn country_cashflow_writers_bind_by_tag_and_flush_pending_at_month_boundary() {
    let state = VanillaExternalState {
        country_resources: BTreeMap::from([(
            "AAA".into(),
            CountryResources {
                treasury_milli: Some(1_000),
                ..Default::default()
            },
        )]),
        ..Default::default()
    };
    let mut loaded =
        VanillaWorld::load_with_external_state(fixture(), "1444.11.11", &state).unwrap();

    loaded.record_country_income("AAA", 3, 40).unwrap();
    loaded.record_delayed_country_income("AAA", 1, 50).unwrap();
    loaded.record_delayed_country_expense("AAA", 5, 30).unwrap();

    let country = loaded.initial.country_entity("AAA").unwrap();
    assert_eq!(
        loaded.initial.world().country_resources()[&country].treasury_milli,
        Some(1_040)
    );
    let ledger = &loaded.initial.world().country_ledgers()[&country];
    assert_eq!(ledger.pending_milli, 20);
    assert_eq!(ledger.income_sources_milli[3], 40);
    assert_eq!(ledger.income_sources_milli[1], 50);
    assert_eq!(ledger.expense_sources_milli[5], 30);

    let saved = loaded.save().unwrap();
    let mut resumed =
        VanillaWorld::load_with_external_state(fixture(), "1444.11.11", &state).unwrap();
    resumed.restore(&saved).unwrap();
    assert_eq!(
        resumed.initial.world().country_ledgers()[&country],
        loaded.initial.world().country_ledgers()[&country]
    );

    for world in [&mut loaded, &mut resumed] {
        let balances = world.flush_country_ledgers_pending().unwrap();
        assert_eq!(balances["AAA"], 1_060);
        let ledger = &world.initial.world().country_ledgers()[&country];
        assert_eq!(ledger.pending_milli, 0);
        assert_eq!(ledger.income_accumulator_milli[3], 40);
        assert_eq!(ledger.income_accumulator_milli[1], 50);
        assert_eq!(ledger.expense_accumulator_milli[5], 30);

        world
            .consolidate_country_ledgers_monthly(BTreeMap::new())
            .unwrap();
        let ledger = &world.initial.world().country_ledgers()[&country];
        assert_eq!(ledger.income_snapshot_milli[3], 40);
        assert_eq!(ledger.income_snapshot_milli[1], 50);
        assert_eq!(ledger.expense_snapshot_milli[5], 30);
        assert_eq!(ledger.income_total_milli, 90);
        assert_eq!(ledger.expense_total_milli, 30);
    }
    assert_eq!(loaded.save().unwrap(), resumed.save().unwrap());
}

#[test]
fn country_ledger_pending_flush_returns_balances_by_eu4_tag() {
    let mut loaded = VanillaWorld::load(fixture(), "1444.11.11").unwrap();
    loaded
        .set_country_resources(BTreeMap::from([(
            "AAA".into(),
            CountryResources {
                treasury_milli: Some(100),
                ..Default::default()
            },
        )]))
        .unwrap();
    loaded
        .set_country_ledgers(BTreeMap::from([(
            "AAA".into(),
            CountryLedger {
                pending_milli: 40,
                ..Default::default()
            },
        )]))
        .unwrap();

    let balances = loaded.flush_country_ledgers_pending().unwrap();
    assert_eq!(balances, BTreeMap::from([("AAA".into(), 140)]));
    let country = loaded.initial.country_entity("AAA").unwrap();
    assert_eq!(
        loaded.initial.world().country_ledgers()[&country].pending_milli,
        0
    );
}

fn gold_production_input(
    gold_size_milli: i32,
    mine_base_milli: i32,
    external_gold_size_milli: i32,
) -> VanillaGoldProductionInputState {
    VanillaGoldProductionInputState {
        input: Some(VanillaGoldProductionInput {
            gold_size_milli,
            mine_base_milli,
            external_gold_size_milli,
        }),
        unresolved: BTreeSet::new(),
    }
}

#[test]
fn buildings_goods_and_routes_reach_the_core_with_history_and_sources() {
    let loaded = VanillaWorld::load(fixture(), "1444.11.11").unwrap();
    let economy = loaded.initial.world().economy();
    assert_eq!(economy.buildings["test_market"].cost_milli, Some(87125));
    assert_eq!(
        economy.buildings["test_market"].construction_months,
        Some(9)
    );
    assert_eq!(economy.trade_goods["gold"].base_price_milli, Some(0));
    assert_eq!(economy.trade_goods["gold"].gold_type, Some(true));
    assert_eq!(economy.trade_goods["grain"].runtime_index, Some(1));
    assert_eq!(economy.trade_goods["cloth"].runtime_index, Some(2));
    assert_eq!(economy.trade_goods["gold"].runtime_index, Some(3));
    let north = loaded.province(1).unwrap();
    assert_eq!(north.buildings.unwrap(), ["test_fort"]);
    assert_eq!(
        north.trade_good_definition.unwrap().base_price_milli,
        Some(2125)
    );
    assert_eq!(north.trade_node, Some("river"));
    assert_eq!(north.trade_node_outgoing.unwrap(), ["harbor"]);
    assert_eq!(
        loaded.province(2).unwrap().buildings.unwrap(),
        ["test_market"]
    );
    assert!(loaded.province(3).unwrap().buildings.is_none());
    assert!(loaded.province(4).unwrap().buildings.unwrap().is_empty());
    assert_eq!(loaded.report.economy.building_instances, 2);
    assert_eq!(loaded.report.economy.trade_routes, 1);
    let origin = &loaded.report.economy.building_history_sources["1:test_market"];
    assert_eq!(origin.line, 20);
    assert!(origin.path.ends_with("history/provinces/1 - North.txt"));
    assert_eq!(
        loaded.report.economy.unapplied_definition_fields["buildings:modifier"],
        1
    );
    let core = loaded.initial.world();
    let restored =
        World::restore(&core.save("fixture").unwrap(), "fixture", core.schema()).unwrap();
    assert_eq!(&restored, core);
    let earlier = VanillaWorld::load(fixture(), "1430.1.1").unwrap();
    assert_eq!(
        earlier.province(1).unwrap().buildings.unwrap(),
        ["test_market"]
    );
    let future = VanillaWorld::load(fixture(), "1450.1.1").unwrap();
    assert!(future.province(1).unwrap().buildings.unwrap().is_empty());
    assert_eq!(
        future.province(1).unwrap().state.trade_good.as_deref(),
        Some("cloth")
    );
}

#[test]
fn province_production_value_uses_content_price_without_inventing_missing_goods() {
    let loaded = VanillaWorld::load(fixture(), "1444.11.11").unwrap();
    let result = loaded
        .calculate_vanilla_province_production_base_value(
            1,
            VanillaProvinceProductionBaseValueInput {
                goods_size_milli: 1_000,
                flat_modifier_milli: 100,
                external_flat_modifier_milli: -50,
                rate_modifier_milli: 0,
                external_rate_modifier_milli: 0,
            },
        )
        .unwrap()
        .unwrap();
    assert_eq!(result.priced_value_milli, 2_125);
    assert_eq!(result.flat_value_milli, 2_175);
    assert_eq!(result.value_milli, 2_175);
    assert_eq!(
        loaded
            .calculate_vanilla_province_production_base_value(
                4,
                VanillaProvinceProductionBaseValueInput {
                    goods_size_milli: 1_000,
                    flat_modifier_milli: 0,
                    external_flat_modifier_milli: 0,
                    rate_modifier_milli: 0,
                    external_rate_modifier_milli: 0,
                },
            )
            .unwrap(),
        None
    );
}

#[test]
fn external_production_value_inputs_bind_content_price_before_monthly_settlement() {
    let state = VanillaExternalState {
        country_resources: BTreeMap::from([(
            "AAA".into(),
            CountryResources {
                treasury_milli: Some(1_234),
                ..Default::default()
            },
        )]),
        production_value_inputs: BTreeMap::from([(
            2,
            VanillaExternalProductionInput {
                goods_size_milli: 1_000,
                flat_modifier_milli: 0,
                external_flat_modifier_milli: 0,
                rate_modifier_milli: 0,
                external_rate_modifier_milli: 0,
                country_modifier_milli: 0,
                legacy_province_modifier_milli: 0,
                queried_province_modifier_milli: 0,
            },
        )]),
        ..Default::default()
    };
    let mut loaded =
        VanillaWorld::load_with_external_state(fixture(), "1444.11.11", &state).unwrap();
    let entity = loaded.initial.province_entity(2).unwrap();
    let input = &loaded.initial.world().monthly_production().unwrap().inputs[&entity];
    assert_eq!(input.input.as_ref().unwrap().base_value_milli, 3_000);
    let country = loaded.initial.country_entity("AAA").unwrap();
    assert_eq!(
        loaded.initial.world().country_resources()[&country].treasury_milli,
        Some(1_234)
    );
    loaded.advance(30).unwrap();
    let account = &loaded
        .initial
        .world()
        .monthly_production()
        .unwrap()
        .last_settlement
        .as_ref()
        .unwrap()
        .accounts[&country];
    assert_eq!(account.monthly_value_milli, Some(250));
    assert!(account.unresolved.contains("unknown_treasury_after_tax"));
    assert_eq!(
        loaded.initial.world().country_resources()[&country].treasury_milli,
        None
    );
}

#[test]
fn external_price_impacts_rebuild_effective_price_before_production() {
    let state = VanillaExternalState {
        production_value_inputs: BTreeMap::from([(
            2,
            VanillaExternalProductionInput {
                goods_size_milli: 1_000,
                flat_modifier_milli: 0,
                external_flat_modifier_milli: 0,
                rate_modifier_milli: 0,
                external_rate_modifier_milli: 0,
                country_modifier_milli: 0,
                legacy_province_modifier_milli: 0,
                queried_province_modifier_milli: 0,
            },
        )]),
        trade_good_price_impacts_milli: BTreeMap::from([(2, 500)]),
        ..Default::default()
    };
    let loaded = VanillaWorld::load_with_external_state(fixture(), "1444.11.11", &state).unwrap();
    let entity = loaded.initial.province_entity(2).unwrap();
    let input = &loaded.initial.world().monthly_production().unwrap().inputs[&entity];

    assert_eq!(input.input.as_ref().unwrap().base_value_milli, 4_500);
}

#[test]
fn ordered_external_price_impacts_are_aggregated_before_production() {
    let state = VanillaExternalState {
        production_value_inputs: BTreeMap::from([(
            2,
            VanillaExternalProductionInput {
                goods_size_milli: 1_000,
                flat_modifier_milli: 0,
                external_flat_modifier_milli: 0,
                rate_modifier_milli: 0,
                external_rate_modifier_milli: 0,
                country_modifier_milli: 0,
                legacy_province_modifier_milli: 0,
                queried_province_modifier_milli: 0,
            },
        )]),
        trade_good_price_impact_entries_milli: BTreeMap::from([(2, vec![200, 300])]),
        ..Default::default()
    };
    let loaded = VanillaWorld::load_with_external_state(fixture(), "1444.11.11", &state).unwrap();
    let entity = loaded.initial.province_entity(2).unwrap();
    let input = &loaded.initial.world().monthly_production().unwrap().inputs[&entity];

    assert_eq!(input.input.as_ref().unwrap().base_value_milli, 4_500);
}

#[test]
fn saved_trade_good_price_overrides_content_price_before_production() {
    let state = VanillaExternalState {
        trade_good_prices_milli: BTreeMap::from([("cloth".into(), Some(3_500))]),
        production_value_inputs: BTreeMap::from([(
            2,
            VanillaExternalProductionInput {
                goods_size_milli: 1_000,
                flat_modifier_milli: 0,
                external_flat_modifier_milli: 0,
                rate_modifier_milli: 0,
                external_rate_modifier_milli: 0,
                country_modifier_milli: 0,
                legacy_province_modifier_milli: 0,
                queried_province_modifier_milli: 0,
            },
        )]),
        ..Default::default()
    };
    let loaded = VanillaWorld::load_with_external_state(fixture(), "1444.11.11", &state).unwrap();
    assert_eq!(
        loaded.province_trade_good_price_milli(2).unwrap(),
        Some(3_500)
    );
    let price_only = VanillaWorld::load_with_external_state(
        fixture(),
        "1444.11.11",
        &VanillaExternalState {
            trade_good_prices_milli: BTreeMap::from([("cloth".into(), Some(3_500))]),
            ..Default::default()
        },
    )
    .unwrap();
    let content_only = VanillaWorld::load(fixture(), "1444.11.11").unwrap();
    assert_eq!(
        price_only.initial.content_identity_state().unwrap(),
        content_only.initial.content_identity_state().unwrap()
    );
    let entity = loaded.initial.province_entity(2).unwrap();
    let input = &loaded.initial.world().monthly_production().unwrap().inputs[&entity];
    assert_eq!(input.input.as_ref().unwrap().base_value_milli, 3_500);
}

#[test]
fn external_country_annual_tax_binds_before_monthly_settlement() {
    let state = VanillaExternalState {
        country_resources: BTreeMap::from([(
            "AAA".into(),
            CountryResources {
                treasury_milli: Some(1_234),
                ..Default::default()
            },
        )]),
        country_annual_tax_milli: BTreeMap::from([("AAA".into(), Some(36_000))]),
        ..Default::default()
    };
    let mut loaded =
        VanillaWorld::load_with_external_state(fixture(), "1444.11.11", &state).unwrap();
    let country = loaded.initial.country_entity("AAA").unwrap();
    let annual_province_micros = loaded
        .initial
        .world()
        .attributes()
        .sum_owned(loaded.initial.world(), country, "tax_after_autonomy")
        .unwrap()
        .known_subtotal_micros;
    assert_eq!(
        loaded
            .initial
            .world()
            .taxation()
            .unwrap()
            .external_country_annual_milli[&country],
        Some(36_000)
    );

    loaded.advance(30).unwrap();
    let account = &loaded
        .initial
        .world()
        .taxation()
        .unwrap()
        .last_settlement
        .as_ref()
        .unwrap()
        .accounts[&country];
    let expected = (annual_province_micros + 36_000_000) / 12_000;
    assert_eq!(account.known_subtotal_milli, expected);
    assert_eq!(account.income_milli, None);
    assert!(account.unresolved.contains("city_status"));
    assert_eq!(account.treasury_after_milli, None);
}

#[test]
fn external_province_annual_tax_binds_by_eu4_province_id() {
    let state = VanillaExternalState {
        province_annual_tax_milli: BTreeMap::from([(2, Some(42_000))]),
        ..Default::default()
    };
    let loaded = VanillaWorld::load_with_external_state(fixture(), "1444.11.11", &state).unwrap();
    let province = loaded.initial.province_entity(2).unwrap();
    assert_eq!(
        loaded
            .initial
            .world()
            .taxation()
            .unwrap()
            .external_province_annual_milli[&province],
        Some(42_000)
    );
}

#[test]
fn external_country_cashflows_bind_after_production_and_preserve_unknown_tax_state() {
    let state = VanillaExternalState {
        country_resources: BTreeMap::from([(
            "AAA".into(),
            CountryResources {
                treasury_milli: Some(1_234),
                ..Default::default()
            },
        )]),
        country_monthly_cashflows_milli: BTreeMap::from([("AAA".into(), Some(75))]),
        production_value_inputs: BTreeMap::from([(
            2,
            VanillaExternalProductionInput {
                goods_size_milli: 1_000,
                flat_modifier_milli: 0,
                external_flat_modifier_milli: 0,
                rate_modifier_milli: 0,
                external_rate_modifier_milli: 0,
                country_modifier_milli: 0,
                legacy_province_modifier_milli: 0,
                queried_province_modifier_milli: 0,
            },
        )]),
        ..Default::default()
    };
    let mut loaded =
        VanillaWorld::load_with_external_state(fixture(), "1444.11.11", &state).unwrap();
    let country = loaded.initial.country_entity("AAA").unwrap();
    let cashflows = loaded.initial.world().monthly_cashflows().unwrap();
    assert_eq!(cashflows.amounts_milli[&country], Some(75));
    loaded.advance(30).unwrap();

    let account = &loaded
        .initial
        .world()
        .monthly_cashflows()
        .unwrap()
        .last_settlement
        .as_ref()
        .unwrap()
        .accounts[&country];
    assert!(
        account
            .unresolved
            .contains("unknown_treasury_after_production")
    );
    assert_eq!(account.amount_milli, Some(75));
    assert_eq!(
        loaded
            .country("AAA")
            .unwrap()
            .last_monthly_cashflow
            .unwrap()
            .amount_milli,
        Some(75)
    );
    assert_eq!(
        loaded.initial.world().country_resources()[&country].treasury_milli,
        None
    );
}

#[test]
fn external_cashflow_breakdowns_bind_category_slots_before_settlement() {
    let state = VanillaExternalState {
        country_monthly_cashflow_breakdowns: BTreeMap::from([(
            "AAA".into(),
            CashflowBreakdown {
                trade_milli: Some(100),
                gold_milli: Some(40),
                expenses_milli: Some(30),
                expense_categories_milli: BTreeMap::from([(11, 30)]),
            },
        )]),
        ..Default::default()
    };
    let loaded = VanillaWorld::load_with_external_state(fixture(), "1444.11.11", &state).unwrap();
    let country = loaded.initial.country_entity("AAA").unwrap();
    let cashflows = loaded.initial.world().monthly_cashflows().unwrap();
    assert_eq!(cashflows.amounts_milli[&country], Some(110));
    assert_eq!(
        cashflows.breakdowns[&country],
        CashflowBreakdown {
            trade_milli: Some(100),
            gold_milli: Some(40),
            expenses_milli: Some(30),
            expense_categories_milli: BTreeMap::from([(11, 30)]),
        }
    );
}

#[test]
fn gold_production_uses_the_gold_path_without_a_trade_good_price() {
    let root = copy_fixture();
    let goods = root.path().join("common/tradegoods/test_goods.txt");
    let text = fs::read_to_string(&goods).unwrap();
    fs::write(
        goods,
        text.replace(
            "gold = { color = { 1.0 0.9 0.0 } }",
            "gold = { color = { 1.0 0.9 0.0 } goldtype = yes }",
        ),
    )
    .unwrap();
    let province = root.path().join("history/provinces/2 - East.txt");
    let text = fs::read_to_string(&province).unwrap();
    fs::write(
        province,
        text.replace("trade_goods = cloth", "trade_goods = gold"),
    )
    .unwrap();

    let loaded = VanillaWorld::load(root.path(), "1444.11.11").unwrap();
    let base = loaded
        .calculate_vanilla_province_gold_base(
            2,
            augustus_eu4::VanillaGoldBaseInput {
                province_field_2df0: 0,
                province_field_428: 900,
                province_field_2df4: 100_000,
                global_base_milli: 40_000,
            },
        )
        .unwrap()
        .unwrap();
    assert_eq!(base.base_value_milli, 39_640);
    let result = loaded
        .calculate_vanilla_province_gold_production(
            2,
            augustus_eu4::VanillaGoldProductionInput {
                gold_size_milli: 800,
                mine_base_milli: 39_640,
                external_gold_size_milli: 0,
            },
        )
        .unwrap()
        .unwrap();
    assert_eq!(result.value_milli, 31_712);
    assert_eq!(result.monthly_value_milli, 2_642);
    let country = loaded
        .country_vanilla_gold_production(
            "AAA",
            &BTreeMap::from([(
                2,
                augustus_eu4::VanillaGoldProductionInputState {
                    input: Some(augustus_eu4::VanillaGoldProductionInput {
                        gold_size_milli: 800,
                        mine_base_milli: 39_640,
                        external_gold_size_milli: 0,
                    }),
                    unresolved: BTreeSet::new(),
                },
            )]),
        )
        .unwrap();
    assert_eq!(country.value_milli, Some(31_712));
    assert_eq!(country.monthly_value_milli, Some(2_642));
    assert_eq!(country.provinces.len(), 1);
    assert!(
        loaded
            .calculate_vanilla_province_gold_production(
                1,
                augustus_eu4::VanillaGoldProductionInput {
                    gold_size_milli: 800,
                    mine_base_milli: 39_640,
                    external_gold_size_milli: 0,
                },
            )
            .unwrap()
            .is_none()
    );
}

#[test]
fn content_production_inputs_derive_country_efficiency_from_administrative_technology() {
    let fixture = copy_fixture();
    let technology = fixture.path().join("common/technologies/test_adm.txt");
    let text = fs::read_to_string(&technology).unwrap().replace(
        "technology = { year = 1430 test_capability = yes }",
        "technology = { year = 1430 test_capability = yes production_efficiency = 0.02 }",
    );
    fs::write(technology, text).unwrap();

    let mut loaded = VanillaWorld::load(fixture.path(), "1444.11.11").unwrap();
    loaded
        .set_vanilla_production_inputs_from_content(BTreeMap::from([(
            2,
            VanillaContentProductionInput {
                base_value_milli: 1_000,
                legacy_province_modifier_milli: 0,
                queried_province_modifier_milli: 0,
            },
        )]))
        .unwrap();

    let province = loaded.initial.province_entity(2).unwrap();
    let input = loaded.initial.world().monthly_production().unwrap().inputs[&province]
        .input
        .as_ref()
        .unwrap();
    assert_eq!(input.country_modifier_milli, 20);
    assert!(
        loaded.initial.world().monthly_production().unwrap().inputs[&province]
            .unresolved
            .is_empty()
    );
}

#[test]
fn loaded_content_production_inputs_derive_price_value_and_country_efficiency() {
    let fixture = copy_fixture();
    let technology = fixture.path().join("common/technologies/test_adm.txt");
    let text = fs::read_to_string(&technology).unwrap().replace(
        "technology = { year = 1430 test_capability = yes }",
        "technology = { year = 1430 test_capability = yes production_efficiency = 0.02 }",
    );
    fs::write(technology, text).unwrap();

    let mut loaded = VanillaWorld::load(fixture.path(), "1444.11.11").unwrap();
    loaded
        .set_vanilla_production_inputs_from_loaded_content(BTreeMap::from([(
            2,
            VanillaContentProductionValueInput {
                goods_size_milli: 1_000,
                flat_modifier_milli: 0,
                external_flat_modifier_milli: 0,
                rate_modifier_milli: 0,
                external_rate_modifier_milli: 0,
                legacy_province_modifier_milli: 0,
                queried_province_modifier_milli: 0,
            },
        )]))
        .unwrap();

    let province = loaded.initial.province_entity(2).unwrap();
    let input = loaded.initial.world().monthly_production().unwrap().inputs[&province]
        .input
        .as_ref()
        .unwrap();
    assert_eq!(input.base_value_milli, 3_000);
    assert_eq!(input.country_modifier_milli, 20);
    assert!(
        loaded.initial.world().monthly_production().unwrap().inputs[&province]
            .unresolved
            .is_empty()
    );
}

#[test]
fn content_goods_size_stage_feeds_loaded_content_production() {
    let mut loaded = VanillaWorld::load(fixture(), "1444.11.11").unwrap();
    loaded
        .set_vanilla_production_inputs_from_content_goods_size(BTreeMap::from([(
            2,
            VanillaContentProductionGoodsSizeInput {
                goods_size_input: VanillaTradeGoodsSizeInput {
                    province_base_milli: 800,
                    trade_good_base_milli: 0,
                    external_base_milli: 0,
                    province_modifier_milli: 44,
                    trade_good_modifier_milli: 0,
                    external_modifier_milli: 0,
                },
                flat_modifier_milli: 0,
                external_flat_modifier_milli: 0,
                rate_modifier_milli: 0,
                external_rate_modifier_milli: 0,
                legacy_province_modifier_milli: 0,
                queried_province_modifier_milli: 0,
            },
        )]))
        .unwrap();

    let province = loaded.initial.province_entity(2).unwrap();
    let input = loaded.initial.world().monthly_production().unwrap().inputs[&province]
        .input
        .as_ref()
        .unwrap();
    assert_eq!(input.base_value_milli, 2_505);
    assert_eq!(input.country_modifier_milli, 0);
    assert!(
        loaded.initial.world().monthly_production().unwrap().inputs[&province]
            .unresolved
            .is_empty()
    );
}

#[test]
fn external_production_input_formats_cannot_overlap_a_province() {
    let state = VanillaExternalState {
        production_inputs: BTreeMap::from([(2, production_input(1_000, 0, 0, 0))]),
        production_value_inputs: BTreeMap::from([(
            2,
            VanillaExternalProductionInput {
                goods_size_milli: 1_000,
                flat_modifier_milli: 0,
                external_flat_modifier_milli: 0,
                rate_modifier_milli: 0,
                external_rate_modifier_milli: 0,
                country_modifier_milli: 0,
                legacy_province_modifier_milli: 0,
                queried_province_modifier_milli: 0,
            },
        )]),
        ..Default::default()
    };
    let error = VanillaWorld::load_with_external_state(fixture(), "1444.11.11", &state)
        .unwrap_err()
        .to_string();
    assert!(error.contains("both production_inputs and production_value_inputs"));
}

#[test]
fn runtime_production_value_inputs_refresh_without_resetting_monthly_receipts() {
    let mut loaded = VanillaWorld::load(fixture(), "1444.11.11").unwrap();
    let first = VanillaExternalProductionInput {
        goods_size_milli: 1_000,
        flat_modifier_milli: 0,
        external_flat_modifier_milli: 0,
        rate_modifier_milli: 0,
        external_rate_modifier_milli: 0,
        country_modifier_milli: 0,
        legacy_province_modifier_milli: 0,
        queried_province_modifier_milli: 0,
    };
    loaded
        .set_vanilla_production_value_inputs(BTreeMap::from([(2, first)]))
        .unwrap();
    loaded.advance(30).unwrap();
    let second = VanillaExternalProductionInput {
        goods_size_milli: 2_000,
        ..first
    };
    loaded
        .set_vanilla_production_value_inputs(BTreeMap::from([(2, second)]))
        .unwrap();
    let entity = loaded.initial.province_entity(2).unwrap();
    let production = loaded.initial.world().monthly_production().unwrap();
    assert_eq!(production.settlement_count, 1);
    assert_eq!(
        production.inputs[&entity]
            .input
            .as_ref()
            .unwrap()
            .base_value_milli,
        6_000
    );
}

#[test]
fn production_value_inputs_round_trip_as_resolved_core_state() {
    let state = VanillaExternalState {
        production_value_inputs: BTreeMap::from([(
            2,
            VanillaExternalProductionInput {
                goods_size_milli: 1_000,
                flat_modifier_milli: 0,
                external_flat_modifier_milli: 0,
                rate_modifier_milli: 0,
                external_rate_modifier_milli: 0,
                country_modifier_milli: 0,
                legacy_province_modifier_milli: 0,
                queried_province_modifier_milli: 0,
            },
        )]),
        ..Default::default()
    };
    let loaded = VanillaWorld::load_with_external_state(fixture(), "1444.11.11", &state).unwrap();
    let snapshot = loaded.save().unwrap();
    let mut restored =
        VanillaWorld::load_with_external_state(fixture(), "1444.11.11", &state).unwrap();
    restored.restore(&snapshot).unwrap();
    assert_eq!(
        restored.initial.world().monthly_production(),
        loaded.initial.world().monthly_production()
    );
}

#[test]
fn later_building_definition_replaces_the_earlier_definition_and_keeps_its_source() {
    let temp = copy_fixture();
    let path = temp.path().join("common/buildings/zz_override.txt");
    fs::write(
        path,
        "test_market = { cost = 90 modifier = { test_power = 0.25 } }\n",
    )
    .unwrap();

    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let building = &loaded.initial.world().economy().buildings["test_market"];
    assert_eq!(building.cost_milli, Some(90_000));
    assert_eq!(building.construction_months, None);

    let report = &loaded.report.economy;
    let key = "buildings:test_market";
    assert!(
        report.definition_sources[key]
            .path
            .ends_with("zz_override.txt")
    );
    assert_eq!(report.overridden_definition_sources[key].len(), 1);
    assert!(
        report.overridden_definition_sources[key][0]
            .path
            .ends_with("test_buildings.txt")
    );

    let attributes = &loaded.initial.world().attributes().building_modifiers["test_market"];
    assert_eq!(attributes.values_micros["test_power"], 250_000);
    assert!(!attributes.values_micros.contains_key("local_tax_modifier"));
    let attribute_key = "buildings:test_market:modifier";
    assert!(
        loaded.report.attributes.definition_sources[attribute_key]
            .path
            .ends_with("zz_override.txt")
    );
    assert_eq!(
        loaded.report.attributes.overridden_definition_sources[attribute_key].len(),
        1
    );
}

#[test]
fn country_production_query_aggregates_live_owned_provinces() {
    let fixture = copy_fixture();
    fs::write(
        fixture.path().join("history/provinces/4 - Highland.txt"),
        "owner = AAA",
    )
    .unwrap();
    let loaded = VanillaWorld::load(fixture.path(), "1444.11.11").unwrap();
    let inputs = BTreeMap::from([
        (2, production_input(2000, 20, 0, -20)),
        (4, production_input(500, 20, 0, 0)),
    ]);

    let result = loaded.country_vanilla_production("AAA", &inputs).unwrap();
    assert_eq!(result.known_annual_value_milli, 2509);
    assert_eq!(result.known_monthly_value_milli, 208);
    assert_eq!(result.annual_value_milli, Some(2509));
    assert_eq!(result.monthly_value_milli, Some(208));
    assert!(result.unresolved.is_empty());
    assert_eq!(result.provinces.len(), 2);
    assert_eq!(result.provinces[0].annual_value_milli, Some(1999));
    assert_eq!(result.provinces[1].annual_value_milli, Some(510));
}

#[test]
fn country_production_query_keeps_known_subtotals_when_an_input_is_unknown() {
    let fixture = copy_fixture();
    fs::write(
        fixture.path().join("history/provinces/4 - Highland.txt"),
        "owner = AAA",
    )
    .unwrap();
    let loaded = VanillaWorld::load(fixture.path(), "1444.11.11").unwrap();
    let inputs = BTreeMap::from([(2, production_input(2000, 20, 0, -20))]);

    let result = loaded.country_vanilla_production("AAA", &inputs).unwrap();
    assert_eq!(result.known_annual_value_milli, 1999);
    assert_eq!(result.known_monthly_value_milli, 166);
    assert_eq!(result.annual_value_milli, None);
    assert_eq!(result.monthly_value_milli, None);
    assert!(
        result
            .unresolved
            .contains("province:4:missing_production_input")
    );
    assert_eq!(result.provinces[1].annual_value_milli, None);
}

#[test]
fn country_gold_production_query_aggregates_only_owned_gold_provinces() {
    let fixture = copy_fixture();
    fs::write(
        fixture.path().join("common/tradegoods/test_goods.txt"),
        "grain = { color = { 0.9 0.8 0.3 } }\ncloth = { color = { 0.2 0.3 0.8 } }\ngold = { color = { 1.0 0.9 0.0 } goldtype = yes }\n",
    )
    .unwrap();
    fs::write(
        fixture.path().join("history/provinces/2 - East.txt"),
        "owner = AAA\ntrade_goods = gold",
    )
    .unwrap();
    fs::write(
        fixture.path().join("history/provinces/4 - Highland.txt"),
        "owner = AAA\ntrade_goods = gold",
    )
    .unwrap();

    let loaded = VanillaWorld::load(fixture.path(), "1444.11.11").unwrap();
    let inputs = BTreeMap::from([
        (2, gold_production_input(800, 39_640, 0)),
        (4, gold_production_input(500, 39_640, 0)),
    ]);

    let result = loaded
        .country_vanilla_gold_production("AAA", &inputs)
        .unwrap();
    assert_eq!(result.known_value_milli, 51_532);
    assert_eq!(result.known_monthly_value_milli, 4_293);
    assert_eq!(result.value_milli, Some(51_532));
    assert_eq!(result.monthly_value_milli, Some(4_293));
    assert!(result.unresolved.is_empty());
    assert_eq!(
        result
            .provinces
            .iter()
            .map(|province| province.province_id)
            .collect::<Vec<_>>(),
        vec![2, 4]
    );
}

#[test]
fn country_gold_production_query_preserves_known_subtotals_when_gold_input_is_unknown() {
    let fixture = copy_fixture();
    fs::write(
        fixture.path().join("common/tradegoods/test_goods.txt"),
        "grain = { color = { 0.9 0.8 0.3 } }\ncloth = { color = { 0.2 0.3 0.8 } }\ngold = { color = { 1.0 0.9 0.0 } goldtype = yes }\n",
    )
    .unwrap();
    fs::write(
        fixture.path().join("history/provinces/2 - East.txt"),
        "owner = AAA\ntrade_goods = gold",
    )
    .unwrap();
    fs::write(
        fixture.path().join("history/provinces/4 - Highland.txt"),
        "owner = AAA\ntrade_goods = gold",
    )
    .unwrap();

    let loaded = VanillaWorld::load(fixture.path(), "1444.11.11").unwrap();
    let inputs = BTreeMap::from([(2, gold_production_input(800, 39_640, 0))]);

    let result = loaded
        .country_vanilla_gold_production("AAA", &inputs)
        .unwrap();
    assert_eq!(result.known_value_milli, 31_712);
    assert_eq!(result.known_monthly_value_milli, 2_642);
    assert_eq!(result.value_milli, None);
    assert_eq!(result.monthly_value_milli, None);
    assert!(
        result
            .unresolved
            .contains("province:4:missing_gold_production_input")
    );
    assert_eq!(result.provinces[1].value_milli, None);
}

#[test]
fn external_production_inputs_bind_by_eu4_province_id_and_restore() {
    let fixture = copy_fixture();
    let mut loaded = VanillaWorld::load(fixture.path(), "1444.11.11").unwrap();
    let inputs = BTreeMap::from([(2, production_input(2000, 20, 0, -20))]);
    loaded
        .set_vanilla_production_inputs(inputs.clone())
        .unwrap();

    let entity = loaded.initial.province_entity(2).unwrap();
    assert_eq!(
        loaded.initial.world().monthly_production().unwrap().inputs,
        BTreeMap::from([(entity, inputs[&2].clone())])
    );

    loaded.advance(30).unwrap();
    let updated = BTreeMap::from([(2, production_input(3000, 0, 0, 0))]);
    loaded
        .set_vanilla_production_inputs(updated.clone())
        .unwrap();
    assert_eq!(
        loaded
            .initial
            .world()
            .monthly_production()
            .unwrap()
            .settlement_count,
        1
    );
    assert_eq!(
        loaded.initial.world().monthly_production().unwrap().inputs,
        BTreeMap::from([(entity, updated[&2].clone())])
    );

    let snapshot = loaded.save().unwrap();
    let mut restored = VanillaWorld::load(fixture.path(), "1444.11.11").unwrap();
    restored
        .set_vanilla_production_inputs(updated.clone())
        .unwrap();
    restored.restore(&snapshot).unwrap();
    assert_eq!(
        restored
            .initial
            .world()
            .monthly_production()
            .unwrap()
            .inputs,
        BTreeMap::from([(entity, updated[&2].clone())])
    );
}

#[test]
fn external_production_inputs_reject_unknown_eu4_province_ids() {
    let fixture = copy_fixture();
    let mut loaded = VanillaWorld::load(fixture.path(), "1444.11.11").unwrap();
    let error = loaded
        .set_vanilla_production_inputs(BTreeMap::from([(999, production_input(2000, 20, 0, -20))]))
        .unwrap_err()
        .to_string();
    assert!(error.contains("unknown EU4 production province 999"));
}

#[test]
fn duplicate_catalogs_unknown_goods_and_invalid_history_fail_with_locations() {
    for (path, from, to, message) in [
        (
            "common/tradegoods/test_goods.txt",
            "grain =",
            "cloth =",
            "duplicate economic definition",
        ),
        (
            "common/buildings/test_buildings.txt",
            "cost = 87.125",
            "cost = -1",
            "nonnegative value",
        ),
        (
            "history/provinces/1 - North.txt",
            "test_fort = yes",
            "test_fort = 1",
            "expected yes or no",
        ),
        (
            "history/provinces/1 - North.txt",
            "trade_goods = grain",
            "trade_goods = missing",
            "unknown trade good missing",
        ),
        (
            "common/prices/test_prices.txt",
            "grain =",
            "missing =",
            "unknown trade good missing",
        ),
        (
            "common/tradenodes/test_nodes.txt",
            "members = { 1 }",
            "members = { 1 1 }",
            "duplicate trade node member",
        ),
        (
            "common/tradenodes/test_nodes.txt",
            "members = { 1 }",
            "members = { 999 }",
            "unknown province 999",
        ),
        (
            "common/tradenodes/test_nodes.txt",
            "name = harbor",
            "name = missing",
            "unknown trade route destination",
        ),
    ] {
        let temp = copy_fixture();
        let path = temp.path().join(path);
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains(from));
        fs::write(path, text.replace(from, to)).unwrap();
        let error = VanillaWorld::load(temp.path(), "1444.11.11")
            .unwrap_err()
            .to_string();
        assert!(error.contains(message), "{error}");
        if matches!(
            message,
            "duplicate economic definition"
                | "nonnegative value"
                | "expected yes or no"
                | "duplicate trade node member"
        ) {
            assert!(error.contains(".txt:"), "missing input location: {error}");
        }
    }
}
