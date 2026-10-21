// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    Economy, EntityKind, Error, Schema, TradeGoodDefinition, TradeProductionInput,
    TradeProductionResult, VanillaDerivedModifierInput, VanillaGoldBaseInput,
    VanillaGoldProductionInput, VanillaModifierAggregationInput, VanillaProductionBaseValueInput,
    VanillaProductionInput, VanillaProductionResult, VanillaTradeDistributionInput,
    VanillaTradeDistributionRouteInput, VanillaTradeGoodPriceInput,
    VanillaTradeGoodPriceTableInput, VanillaTradeGoodsSizeInput, World,
    aggregate_vanilla_modifier_contributions, calculate_trade_production,
    calculate_vanilla_derived_modifier, calculate_vanilla_gold_base,
    calculate_vanilla_gold_production, calculate_vanilla_production,
    calculate_vanilla_production_base_value, calculate_vanilla_trade_distribution,
    calculate_vanilla_trade_good_price, calculate_vanilla_trade_good_price_table,
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

fn trade_node_route_orders() -> std::collections::BTreeMap<String, Vec<String>> {
    std::collections::BTreeMap::from([
        ("river".into(), vec!["harbor".into()]),
        ("harbor".into(), Vec::new()),
    ])
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
fn current_trade_good_prices_are_runtime_state_and_restore() {
    let mut runtime_world = world();
    let mut economy = economy();
    economy
        .current_trade_good_prices_milli
        .insert("grain".into(), Some(3_500));
    runtime_world.set_economy(economy.clone()).unwrap();
    assert_eq!(
        runtime_world
            .economy()
            .current_trade_good_price_milli("grain"),
        Some(Some(3_500))
    );
    assert_eq!(
        runtime_world
            .economy()
            .current_trade_good_price_milli("cloth"),
        None
    );

    let restored = World::restore(
        &runtime_world.save("runtime-price").unwrap(),
        "runtime-price",
        runtime_world.schema(),
    )
    .unwrap();
    assert_eq!(restored.economy(), runtime_world.economy());
}

#[test]
fn current_trade_node_values_are_runtime_state_and_restore() {
    let mut runtime_world = world();
    let economy = economy();
    runtime_world.set_economy(economy).unwrap();
    runtime_world
        .set_trade_node_runtime_states(&std::collections::BTreeMap::from([
            (
                "river".into(),
                augustus_core::TradeNodeRuntimeState {
                    current_milli: Some(4_748),
                    local_value_milli: Some(5_945),
                    outgoing_milli: Some(5_626),
                    ..Default::default()
                },
            ),
            ("harbor".into(), Default::default()),
        ]))
        .unwrap();

    assert_eq!(
        runtime_world.trade_node_current_value_milli("river"),
        Some(Some(4_748))
    );
    assert_eq!(
        runtime_world.trade_node_current_value_milli("harbor"),
        Some(None)
    );
    assert_eq!(
        runtime_world.trade_node_current_value_milli("missing"),
        None
    );

    let restored = World::restore(
        &runtime_world.save("runtime-trade-node").unwrap(),
        "runtime-trade-node",
        runtime_world.schema(),
    )
    .unwrap();
    assert_eq!(restored.economy(), runtime_world.economy());
    assert_eq!(
        restored
            .trade_node_runtime_state("river")
            .flatten()
            .unwrap()
            .local_value_milli,
        Some(5_945)
    );
    assert_eq!(
        restored
            .trade_node_runtime_state("river")
            .flatten()
            .unwrap()
            .outgoing_milli,
        Some(5_626)
    );

    let mut legacy: serde_json::Value =
        serde_json::from_str(&runtime_world.save("runtime-trade-node-legacy").unwrap()).unwrap();
    legacy
        .as_object_mut()
        .unwrap()
        .remove("trade_node_runtime_states");
    let restored_legacy = World::restore(
        &legacy.to_string(),
        "runtime-trade-node-legacy",
        runtime_world.schema(),
    )
    .unwrap();
    assert_eq!(
        restored_legacy.trade_node_current_value_milli("river"),
        Some(Some(4_748))
    );
}

#[test]
fn vanilla_trade_distribution_updates_source_and_destination_node_values() {
    let mut runtime_world = world();
    runtime_world.set_economy(economy()).unwrap();
    runtime_world
        .set_trade_node_route_orders(&trade_node_route_orders())
        .unwrap();
    let invalid_route_orders = std::collections::BTreeMap::from([
        ("river".into(), Vec::new()),
        ("harbor".into(), Vec::new()),
    ]);
    assert!(
        runtime_world
            .set_trade_node_route_orders(&invalid_route_orders)
            .is_err()
    );
    assert_eq!(
        runtime_world
            .trade_node_route_order("river")
            .flatten()
            .unwrap()
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["harbor"]
    );
    runtime_world
        .set_trade_node_current_values_milli(&std::collections::BTreeMap::from([
            ("river".into(), Some(10_000)),
            ("harbor".into(), Some(5_000)),
        ]))
        .unwrap();

    let distribution = calculate_vanilla_trade_distribution(VanillaTradeDistributionInput {
        source_value_milli: 10_000,
        source_debit_milli: 2_000,
        routes: vec![VanillaTradeDistributionRouteInput {
            weight_milli: 1_000,
            node_value_milli: 1_000,
            candidates: Vec::new(),
        }],
    });
    runtime_world
        .apply_vanilla_trade_node_distribution("river", &["harbor".to_owned()], &distribution)
        .unwrap();

    assert_eq!(
        runtime_world.trade_node_current_value_milli("river"),
        Some(Some(8_000))
    );
    assert_eq!(
        runtime_world.trade_node_current_value_milli("harbor"),
        Some(Some(6_000))
    );

    let restored = World::restore(
        &runtime_world.save("trade-node-distribution").unwrap(),
        "trade-node-distribution",
        runtime_world.schema(),
    )
    .unwrap();
    assert_eq!(restored, runtime_world);
    assert_eq!(
        restored
            .trade_node_route_order("river")
            .flatten()
            .unwrap()
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["harbor"]
    );
}

#[test]
fn vanilla_trade_distribution_with_unknown_destination_is_atomic() {
    let mut runtime_world = world();
    runtime_world.set_economy(economy()).unwrap();
    runtime_world
        .set_trade_node_current_values_milli(&std::collections::BTreeMap::from([
            ("river".into(), Some(10_000)),
            ("harbor".into(), None),
        ]))
        .unwrap();
    let distribution = calculate_vanilla_trade_distribution(VanillaTradeDistributionInput {
        source_value_milli: 8_000,
        source_debit_milli: 2_000,
        routes: vec![VanillaTradeDistributionRouteInput {
            weight_milli: 1_000,
            node_value_milli: 1_000,
            candidates: Vec::new(),
        }],
    });

    assert!(
        runtime_world
            .apply_vanilla_trade_node_distribution("river", &["harbor".to_owned()], &distribution,)
            .is_err()
    );
    assert_eq!(
        runtime_world.trade_node_current_value_milli("river"),
        Some(Some(10_000))
    );
    assert_eq!(
        runtime_world.trade_node_current_value_milli("harbor"),
        Some(None)
    );
}

#[test]
fn vanilla_trade_nodes_run_in_caller_order_and_observe_prior_inflows() {
    let mut runtime_world = world();
    runtime_world.set_economy(economy()).unwrap();
    runtime_world
        .set_trade_node_route_orders(&trade_node_route_orders())
        .unwrap();
    runtime_world
        .set_trade_node_current_values_milli(&std::collections::BTreeMap::from([
            ("river".into(), Some(10_000)),
            ("harbor".into(), Some(5_000)),
        ]))
        .unwrap();

    let order = ["river".to_owned(), "harbor".to_owned()];
    let mut visited = Vec::new();
    runtime_world
        .run_vanilla_trade_node_distributions_in_order(&order, |world, source_node| {
            visited.push(source_node.to_owned());
            let source_value = world
                .trade_node_current_value_milli(source_node)
                .flatten()
                .unwrap() as i32;
            let (destinations, routes) = match source_node {
                "river" => (
                    vec!["harbor".to_owned()],
                    vec![VanillaTradeDistributionRouteInput {
                        weight_milli: 1_000,
                        node_value_milli: 1_000,
                        candidates: Vec::new(),
                    }],
                ),
                "harbor" => {
                    assert_eq!(source_value, 6_000);
                    (Vec::new(), Vec::new())
                }
                _ => unreachable!("only scheduled nodes are visited"),
            };
            Ok((
                destinations,
                calculate_vanilla_trade_distribution(VanillaTradeDistributionInput {
                    source_value_milli: source_value,
                    source_debit_milli: 0,
                    routes,
                }),
            ))
        })
        .unwrap();

    assert_eq!(visited, ["river", "harbor"]);
    assert_eq!(
        runtime_world.trade_node_current_value_milli("river"),
        Some(Some(10_000))
    );
    assert_eq!(
        runtime_world.trade_node_current_value_milli("harbor"),
        Some(Some(6_000))
    );
}

#[test]
fn vanilla_trade_node_sequence_rolls_back_when_a_later_node_fails() {
    let mut runtime_world = world();
    runtime_world.set_economy(economy()).unwrap();
    runtime_world
        .set_trade_node_route_orders(&trade_node_route_orders())
        .unwrap();
    runtime_world
        .set_trade_node_current_values_milli(&std::collections::BTreeMap::from([
            ("river".into(), Some(10_000)),
            ("harbor".into(), Some(5_000)),
        ]))
        .unwrap();

    let order = ["river".to_owned(), "harbor".to_owned()];
    let result = runtime_world.run_vanilla_trade_node_distributions_in_order(
        &order,
        |world, source_node| {
            let source_value = world
                .trade_node_current_value_milli(source_node)
                .flatten()
                .unwrap() as i32;
            let destination = if source_node == "river" {
                "harbor"
            } else {
                "river"
            };
            Ok((
                vec![destination.to_owned()],
                calculate_vanilla_trade_distribution(VanillaTradeDistributionInput {
                    source_value_milli: source_value,
                    source_debit_milli: 0,
                    routes: vec![VanillaTradeDistributionRouteInput {
                        weight_milli: 1_000,
                        node_value_milli: 1_000,
                        candidates: Vec::new(),
                    }],
                }),
            ))
        },
    );

    assert!(matches!(result, Err(Error::Invalid(_))));
    assert_eq!(
        runtime_world.trade_node_current_value_milli("river"),
        Some(Some(10_000))
    );
    assert_eq!(
        runtime_world.trade_node_current_value_milli("harbor"),
        Some(Some(5_000))
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
    assert!(
        world
            .set_trade_node_current_values_milli(&std::collections::BTreeMap::from([(
                "missing".into(),
                Some(1_000),
            )]))
            .is_err()
    );
    assert_eq!(world, original);
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
fn vanilla_trade_good_price_table_accumulates_impacts_before_the_lower_bound() {
    let result = calculate_vanilla_trade_good_price_table(VanillaTradeGoodPriceTableInput {
        base_price_milli: 2_500,
        price_impacts_milli: vec![200, 300],
    });
    assert_eq!(result.combined_impact_milli, 500);
    assert_eq!(result.factor_milli, 1_500);
    assert_eq!(result.effective_price_milli, 3_750);

    let wrapped = calculate_vanilla_trade_good_price_table(VanillaTradeGoodPriceTableInput {
        base_price_milli: 2_500,
        price_impacts_milli: vec![i32::MAX, 1],
    });
    assert_eq!(wrapped.combined_impact_milli, i32::MIN);
    assert_eq!(wrapped.factor_milli, 10);
    assert_eq!(wrapped.effective_price_milli, 25);
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
fn vanilla_gold_production_matches_the_confirmed_hab_path() {
    let result = calculate_vanilla_gold_production(VanillaGoldProductionInput {
        gold_size_milli: 800,
        mine_base_milli: 39_640,
        external_gold_size_milli: 0,
    });

    assert_eq!(result.combined_size_milli, 800);
    assert_eq!(result.value_milli, 31_712);
    assert_eq!(result.monthly_value_milli, 2_642);
}

#[test]
fn vanilla_gold_base_matches_the_confirmed_a11b80_order() {
    let result = calculate_vanilla_gold_base(VanillaGoldBaseInput {
        province_field_2df0: 0,
        province_field_428: 900,
        province_field_2df4: 100_000,
        global_base_milli: 40_000,
    });

    assert_eq!(
        result,
        augustus_core::VanillaGoldBaseResult {
            selected_province_field_milli: 900,
            bounded_province_field_milli: 900,
            reduction_milli: 9,
            base_value_milli: 39_640,
        }
    );
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
