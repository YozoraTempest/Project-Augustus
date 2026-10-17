// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    Economy, EntityKind, Schema, TradeGoodDefinition, TradeProductionInput, TradeProductionResult,
    VanillaDerivedModifierInput, VanillaModifierAggregationInput, VanillaProductionBaseValueInput,
    VanillaProductionInput, VanillaProductionResult, VanillaTradeGoodPriceInput,
    VanillaTradeGoodsSizeInput, World, aggregate_vanilla_modifier_contributions,
    calculate_trade_production, calculate_vanilla_derived_modifier, calculate_vanilla_production,
    calculate_vanilla_production_base_value, calculate_vanilla_trade_good_price,
    calculate_vanilla_trade_goods_size,
};

fn world() -> World {
    let mut world = World::new(Schema::new([] as [&str; 0]).unwrap());
    world.add_entity("AAA", EntityKind::Country);
    world.add_entity("1", EntityKind::Province);
    world.add_entity("2", EntityKind::Province);
    world
}

fn economy() -> Economy {
    serde_json::from_value(data()).unwrap()
}

fn data() -> serde_json::Value {
    serde_json::json!({
        "buildings":{"depot":{"cost_milli":87125,"construction_months":9}},
        "trade_goods":{"grain":{"runtime_index":1,"base_price_milli":2125}},
        "trade_nodes":{
            "river":{"location":1,"members":[1],"outgoing":["harbor"]},
            "harbor":{"location":2,"members":[2],"outgoing":[],"end":true}
        },
        "provinces":{"1":{"buildings":["depot"],"trade_good":"grain"}}
    })
}

#[test]
fn economic_state_roundtrips_and_older_demo_saves_remain_readable() {
    let mut world = world();
    let old = world.save("test").unwrap();
    assert!(!old.contains("economy"));
    assert_eq!(World::restore(&old, "test", world.schema()).unwrap(), world);
    let legacy: TradeGoodDefinition =
        serde_json::from_value(serde_json::json!({"base_price_milli": 2125})).unwrap();
    assert_eq!(legacy.runtime_index, None);
    world.set_economy(economy()).unwrap();
    assert_eq!(
        world.economy().province_node(augustus_core::EntityId(1)),
        Some("river")
    );
    assert_eq!(
        world
            .economy()
            .trade_good_by_runtime_index(1)
            .map(|(name, _)| name),
        Some("grain")
    );
    world
        .schedule(augustus_core::EntityId(1), 5, vec![])
        .unwrap();
    let mut resumed = World::restore(&world.save("test").unwrap(), "test", world.schema()).unwrap();
    world.advance_to(10).unwrap();
    resumed.advance_to(10).unwrap();
    assert_eq!(world, resumed);
}

#[test]
fn trade_good_runtime_indices_are_unique_and_saved_as_optional_metadata() {
    let mut world = world();
    let mut economy = economy();
    economy.trade_goods.insert(
        "cloth".into(),
        TradeGoodDefinition {
            runtime_index: Some(2),
            base_price_milli: Some(3_000),
            gold_type: None,
        },
    );
    world.set_economy(economy.clone()).unwrap();
    let restored = World::restore(
        &world.save("runtime-index").unwrap(),
        "runtime-index",
        world.schema(),
    )
    .unwrap();
    assert_eq!(restored.economy(), &economy);

    let mut duplicate = economy;
    duplicate
        .trade_goods
        .get_mut("grain")
        .unwrap()
        .runtime_index = Some(2);
    assert!(world.set_economy(duplicate).is_err());
    assert_eq!(
        world
            .economy()
            .trade_good_by_runtime_index(2)
            .map(|(name, _)| name),
        Some("cloth")
    );
}

#[test]
fn invalid_economic_references_and_graphs_fail_atomically() {
    let mut world = world();
    world.set_economy(economy()).unwrap();
    let original = world.clone();
    let changes: [(&[&str], serde_json::Value); 7] = [
        (&["trade_nodes", "river", "location"], 0.into()),
        (
            &["trade_nodes", "river", "outgoing"],
            serde_json::json!(["missing"]),
        ),
        (
            &["trade_nodes", "harbor", "members"],
            serde_json::json!([1, 2]),
        ),
        (
            &["provinces", "1", "buildings"],
            serde_json::json!(["missing"]),
        ),
        (&["provinces", "1", "trade_good"], "missing".into()),
        (&["buildings", "depot", "cost_milli"], (-1).into()),
        (&["buildings", "depot", "construction_months"], 0.into()),
    ];
    for (path, replacement) in changes {
        let mut invalid = data();
        let mut slot = &mut invalid;
        for &key in path {
            slot = &mut slot[key];
        }
        *slot = replacement;
        let economy: Economy = serde_json::from_value(invalid).unwrap();
        assert!(world.set_economy(economy).is_err());
        assert_eq!(world, original);
    }
    let mut save: serde_json::Value = serde_json::from_str(&world.save("test").unwrap()).unwrap();
    save["economy"]["provinces"]["1"]["trade_good"] = "missing".into();
    assert!(World::restore(&save.to_string(), "test", world.schema()).is_err());
}

#[test]
fn trade_network_preserves_cycles_and_self_routes() {
    let mut world = world();
    let mut economy = economy();
    economy.trade_nodes.get_mut("harbor").unwrap().end = None;
    economy
        .trade_nodes
        .get_mut("harbor")
        .unwrap()
        .outgoing
        .insert("river".into());
    economy
        .trade_nodes
        .get_mut("river")
        .unwrap()
        .outgoing
        .insert("river".into());

    world.set_economy(economy.clone()).unwrap();
    assert_eq!(world.economy(), &economy);
    let restored = World::restore(&world.save("test").unwrap(), "test", world.schema()).unwrap();
    assert_eq!(restored.economy(), &economy);
}

#[test]
fn trade_production_scales_in_stages_and_truncates_toward_zero() {
    let output = calculate_trade_production(TradeProductionInput {
        province_base_quantity_milli: 300,
        country_base_quantity_milli: 100,
        external_base_quantity_milli: 0,
        province_quantity_modifier_milli: 250,
        country_quantity_modifier_milli: 250,
        external_quantity_modifier_milli: 0,
        trade_good_price_milli: 2500,
        province_fixed_value_milli: 100,
        external_fixed_value_milli: 25,
        province_value_modifier_milli: 200,
        external_value_modifier_milli: 100,
    });

    assert_eq!(
        output,
        TradeProductionResult {
            quantity_milli: 600,
            value_milli: 2112,
        }
    );
}

#[test]
fn trade_production_clamps_quantity_modifiers_but_preserves_negative_final_value() {
    let output = calculate_trade_production(TradeProductionInput {
        province_base_quantity_milli: 1,
        country_base_quantity_milli: 0,
        external_base_quantity_milli: 0,
        province_quantity_modifier_milli: -1500,
        country_quantity_modifier_milli: 0,
        external_quantity_modifier_milli: 0,
        trade_good_price_milli: 2000,
        province_fixed_value_milli: -501,
        external_fixed_value_milli: 0,
        province_value_modifier_milli: -5000,
        external_value_modifier_milli: 0,
    });

    assert_eq!(
        output,
        TradeProductionResult {
            quantity_milli: 0,
            value_milli: -5,
        }
    );
}

#[test]
fn trade_production_uses_wrapping_32_bit_intermediates() {
    let output = calculate_trade_production(TradeProductionInput {
        province_base_quantity_milli: i32::MAX,
        country_base_quantity_milli: 0,
        external_base_quantity_milli: 0,
        province_quantity_modifier_milli: i32::MAX,
        country_quantity_modifier_milli: 0,
        external_quantity_modifier_milli: 0,
        trade_good_price_milli: 1000,
        province_fixed_value_milli: i32::MAX,
        external_fixed_value_milli: 1,
        province_value_modifier_milli: 0,
        external_value_modifier_milli: 0,
    });

    assert_eq!(
        output,
        TradeProductionResult {
            quantity_milli: 0,
            value_milli: i32::MIN,
        }
    );
}

#[test]
fn vanilla_production_matches_the_verified_eu4_order() {
    let province_72 = calculate_vanilla_production(VanillaProductionInput {
        base_value_milli: 2_424,
        country_modifier_milli: 20,
        legacy_province_modifier_milli: 100,
        queried_province_modifier_milli: -35,
    });
    assert_eq!(
        province_72,
        VanillaProductionResult {
            pre_query_value_milli: 2_714,
            annual_value_milli: 2_619,
            monthly_value_milli: 218,
        }
    );

    let province_128 = calculate_vanilla_production(VanillaProductionInput {
        base_value_milli: 1_878,
        country_modifier_milli: 20,
        legacy_province_modifier_milli: 0,
        queried_province_modifier_milli: -10,
    });
    assert_eq!(
        province_128,
        VanillaProductionResult {
            pre_query_value_milli: 1_915,
            annual_value_milli: 1_895,
            monthly_value_milli: 157,
        }
    );
}

#[test]
fn vanilla_production_truncates_negative_intermediates_toward_zero() {
    let result = calculate_vanilla_production(VanillaProductionInput {
        base_value_milli: -1_001,
        country_modifier_milli: 0,
        legacy_province_modifier_milli: 0,
        queried_province_modifier_milli: 0,
    });
    assert_eq!(result.pre_query_value_milli, -1_001);
    assert_eq!(result.annual_value_milli, -1_001);
    assert_eq!(result.monthly_value_milli, -83);
}

#[test]
fn vanilla_trade_goods_size_merges_sources_and_clamps_negative_factor() {
    let result = calculate_vanilla_trade_goods_size(VanillaTradeGoodsSizeInput {
        province_base_milli: 1_000,
        trade_good_base_milli: 200,
        external_base_milli: 300,
        province_modifier_milli: -800,
        trade_good_modifier_milli: -300,
        external_modifier_milli: 50,
    });

    assert_eq!(result.combined_base_milli, 1_500);
    assert_eq!(result.combined_modifier_milli, -1_050);
    assert_eq!(result.quantity_milli, 0);
}

#[test]
fn vanilla_trade_goods_size_wraps_before_clamping_negative_quantity() {
    let result = calculate_vanilla_trade_goods_size(VanillaTradeGoodsSizeInput {
        province_base_milli: i32::MAX,
        trade_good_base_milli: 0,
        external_base_milli: 0,
        province_modifier_milli: 1_000,
        trade_good_modifier_milli: 0,
        external_modifier_milli: 0,
    });

    assert_eq!(result.combined_base_milli, i32::MAX);
    assert_eq!(result.combined_modifier_milli, 1_000);
    assert_eq!(result.quantity_milli, 0);
}

#[test]
fn vanilla_production_base_value_preserves_stage_order_and_rate_clamp() {
    let result = calculate_vanilla_production_base_value(VanillaProductionBaseValueInput {
        goods_size_milli: 1_500,
        trade_good_price_milli: 2_424,
        flat_modifier_milli: 100,
        external_flat_modifier_milli: -50,
        rate_modifier_milli: -2_000,
        external_rate_modifier_milli: 0,
    });

    assert_eq!(result.priced_value_milli, 3_636);
    assert_eq!(result.flat_value_milli, 3_686);
    assert_eq!(result.rate_factor_milli, 10);
    assert_eq!(result.value_milli, 36);
}

#[test]
fn vanilla_trade_good_price_rebuilds_runtime_impact_before_production() {
    let result = calculate_vanilla_trade_good_price(VanillaTradeGoodPriceInput {
        base_price_milli: 2_500,
        price_impact_milli: 500,
    });

    assert_eq!(result.factor_milli, 1_500);
    assert_eq!(result.effective_price_milli, 3_750);
}

#[test]
fn vanilla_trade_good_price_uses_native_lower_factor_bound() {
    let result = calculate_vanilla_trade_good_price(VanillaTradeGoodPriceInput {
        base_price_milli: 2_500,
        price_impact_milli: -2_000,
    });

    assert_eq!(result.factor_milli, 10);
    assert_eq!(result.effective_price_milli, 25);
}

#[test]
fn vanilla_production_base_value_clamps_wrapped_price_before_fixed_modifiers() {
    let result = calculate_vanilla_production_base_value(VanillaProductionBaseValueInput {
        goods_size_milli: i32::MAX,
        trade_good_price_milli: 2_000,
        flat_modifier_milli: 7,
        external_flat_modifier_milli: 0,
        rate_modifier_milli: 0,
        external_rate_modifier_milli: 0,
    });

    assert_eq!(result.priced_value_milli, 0);
    assert_eq!(result.flat_value_milli, 7);
    assert_eq!(result.rate_factor_milli, 1_000);
    assert_eq!(result.value_milli, 7);
}

#[test]
fn vanilla_production_base_value_clamps_rate_factor_at_upper_bound() {
    let result = calculate_vanilla_production_base_value(VanillaProductionBaseValueInput {
        goods_size_milli: 1_000,
        trade_good_price_milli: 1_000,
        flat_modifier_milli: 1_000,
        external_flat_modifier_milli: 0,
        rate_modifier_milli: 5_000,
        external_rate_modifier_milli: 0,
    });

    assert_eq!(result.priced_value_milli, 1_000);
    assert_eq!(result.flat_value_milli, 2_000);
    assert_eq!(result.rate_factor_milli, 2_000);
    assert_eq!(result.value_milli, 4_000);
}

#[test]
fn vanilla_production_base_value_keeps_negative_final_value() {
    let result = calculate_vanilla_production_base_value(VanillaProductionBaseValueInput {
        goods_size_milli: 0,
        trade_good_price_milli: 1_000,
        flat_modifier_milli: -5_000,
        external_flat_modifier_milli: 0,
        rate_modifier_milli: -2_000,
        external_rate_modifier_milli: 0,
    });

    assert_eq!(result.priced_value_milli, 0);
    assert_eq!(result.flat_value_milli, -5_000);
    assert_eq!(result.rate_factor_milli, 10);
    assert_eq!(result.value_milli, -50);
}

#[test]
fn vanilla_derived_modifier_clamps_before_scaling() {
    let lower = calculate_vanilla_derived_modifier(VanillaDerivedModifierInput {
        base_value_milli: 0,
        context_modifier_milli: -18,
        scale_milli: 200,
    });
    assert_eq!(lower.effective_value_milli, 10);
    assert_eq!(lower.derived_value_milli, 2);

    let ordinary = calculate_vanilla_derived_modifier(VanillaDerivedModifierInput {
        base_value_milli: 20,
        context_modifier_milli: 0,
        scale_milli: 200,
    });
    assert_eq!(ordinary.effective_value_milli, 20);
    assert_eq!(ordinary.derived_value_milli, 4);

    let upper = calculate_vanilla_derived_modifier(VanillaDerivedModifierInput {
        base_value_milli: 1_900,
        context_modifier_milli: 500,
        scale_milli: 200,
    });
    assert_eq!(upper.effective_value_milli, 2_000);
    assert_eq!(upper.derived_value_milli, 400);
}

#[test]
fn vanilla_derived_modifier_truncates_scaled_values_toward_zero() {
    let result = calculate_vanilla_derived_modifier(VanillaDerivedModifierInput {
        base_value_milli: 15,
        context_modifier_milli: 0,
        scale_milli: 333,
    });

    assert_eq!(result.effective_value_milli, 15);
    assert_eq!(result.derived_value_milli, 4);
}

#[test]
fn vanilla_modifier_aggregation_preserves_order_and_32_bit_storage() {
    let result = aggregate_vanilla_modifier_contributions(VanillaModifierAggregationInput {
        initial_value_milli: 0,
        contributions_milli: vec![20, -5, 10],
    });
    assert_eq!(result.value_milli, 25);

    let wrapped = aggregate_vanilla_modifier_contributions(VanillaModifierAggregationInput {
        initial_value_milli: i32::MAX,
        contributions_milli: vec![1],
    });
    assert_eq!(wrapped.value_milli, i32::MIN);
}
