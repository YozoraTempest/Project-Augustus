// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    VanillaTradeCandidate, VanillaTradeDistributionCandidate, VanillaTradeDistributionInput,
    VanillaTradeDistributionRouteInput, VanillaTradePropagationInput, VanillaTradeRetentionInput,
    VanillaTradeRouteWeightInput, calculate_vanilla_trade_distribution,
    calculate_vanilla_trade_propagation, calculate_vanilla_trade_retention,
    calculate_vanilla_trade_route_weights, sort_vanilla_trade_candidates,
};

#[test]
fn trade_propagation_preserves_fixed_point_order_and_threshold() {
    let result = calculate_vanilla_trade_propagation(VanillaTradePropagationInput {
        base_amount_milli: 12_000,
        source_amount_milli: 8_000,
        country_modifier_milli: 250,
        divisor_milli: 5_000,
        threshold_milli: 2_000,
    })
    .unwrap();

    assert_eq!(result.scaled_source_milli, 2_000);
    assert_eq!(result.pre_threshold_milli, 14_000);
    assert_eq!(result.amount_milli, 2_800);
    assert!(!result.below_threshold);

    let discarded = calculate_vanilla_trade_propagation(VanillaTradePropagationInput {
        base_amount_milli: 1_000,
        source_amount_milli: 500,
        country_modifier_milli: 100,
        divisor_milli: 5_000,
        threshold_milli: 2_000,
    })
    .unwrap();
    assert_eq!(discarded.amount_milli, 0);
    assert!(discarded.below_threshold);
    assert!(
        calculate_vanilla_trade_propagation(VanillaTradePropagationInput {
            base_amount_milli: 1,
            source_amount_milli: 1,
            country_modifier_milli: 1_000,
            divisor_milli: 0,
            threshold_milli: 0,
        })
        .is_err()
    );
}

#[test]
fn trade_propagation_ignores_negative_source_and_wraps_the_base_sum() {
    let result = calculate_vanilla_trade_propagation(VanillaTradePropagationInput {
        base_amount_milli: i32::MAX,
        source_amount_milli: -2_000,
        country_modifier_milli: 500,
        divisor_milli: 1,
        threshold_milli: i32::MIN,
    })
    .unwrap();

    assert_eq!(result.scaled_source_milli, -1_000);
    assert_eq!(result.pre_threshold_milli, i32::MAX);
    assert_eq!(result.amount_milli, -1_000);
}

#[test]
fn trade_retention_keeps_zero_transfer_and_preserves_zero_denominator_sentinel() {
    assert_eq!(
        calculate_vanilla_trade_retention(VanillaTradeRetentionInput {
            total_amount_milli: 12_000,
            transferred_amount_milli: 3_000,
        }),
        augustus_core::VanillaTradeRetentionResult {
            transfer_ratio_milli: 250,
            retention_milli: 750,
        }
    );
    assert_eq!(
        calculate_vanilla_trade_retention(VanillaTradeRetentionInput {
            total_amount_milli: 0,
            transferred_amount_milli: 0,
        })
        .retention_milli,
        1_000
    );
    assert_eq!(
        calculate_vanilla_trade_retention(VanillaTradeRetentionInput {
            total_amount_milli: 0,
            transferred_amount_milli: 1,
        }),
        augustus_core::VanillaTradeRetentionResult {
            transfer_ratio_milli: -1,
            retention_milli: 1_001,
        }
    );
}

#[test]
fn trade_route_weights_normalize_nonzero_totals_and_split_marked_zero_totals() {
    let normalized = calculate_vanilla_trade_route_weights(VanillaTradeRouteWeightInput {
        contributions_milli: vec![600, 400],
        marked_routes: vec![true, true],
    })
    .unwrap();
    assert_eq!(normalized.total_milli, 1_000);
    assert_eq!(normalized.weights_milli, vec![600, 400]);

    let zero_total = calculate_vanilla_trade_route_weights(VanillaTradeRouteWeightInput {
        contributions_milli: vec![100, -100, 0],
        marked_routes: vec![true, false, true],
    })
    .unwrap();
    assert_eq!(zero_total.total_milli, 0);
    assert_eq!(zero_total.weights_milli, vec![500, 0, 500]);

    assert!(
        calculate_vanilla_trade_route_weights(VanillaTradeRouteWeightInput {
            contributions_milli: vec![1],
            marked_routes: vec![],
        })
        .is_err()
    );
}

#[test]
fn trade_candidates_use_descending_keys_and_stable_ties() {
    let mut candidates = vec![
        VanillaTradeCandidate {
            country_id: 5,
            adjusted_amount_milli: 100,
            unadjusted_amount_milli: 100,
        },
        VanillaTradeCandidate {
            country_id: 7,
            adjusted_amount_milli: 200,
            unadjusted_amount_milli: 100,
        },
        VanillaTradeCandidate {
            country_id: 6,
            adjusted_amount_milli: 200,
            unadjusted_amount_milli: 100,
        },
        VanillaTradeCandidate {
            country_id: 4,
            adjusted_amount_milli: 200,
            unadjusted_amount_milli: 100,
        },
        VanillaTradeCandidate {
            country_id: 3,
            adjusted_amount_milli: 200,
            unadjusted_amount_milli: 90,
        },
    ];

    sort_vanilla_trade_candidates(&mut candidates);
    assert_eq!(
        candidates
            .iter()
            .map(|candidate| candidate.country_id)
            .collect::<Vec<_>>(),
        vec![7, 6, 4, 3, 5]
    );
}

#[test]
fn trade_distribution_applies_top_five_modifiers_and_source_debit() {
    let result = calculate_vanilla_trade_distribution(VanillaTradeDistributionInput {
        source_value_milli: 10_000,
        source_debit_milli: 3_000,
        routes: vec![VanillaTradeDistributionRouteInput {
            weight_milli: 500,
            node_value_milli: 12_000,
            candidates: (1..=6)
                .map(|country_id| VanillaTradeDistributionCandidate {
                    country_id,
                    adjusted_amount_milli: i32::from(country_id) * 100,
                    unadjusted_amount_milli: 100,
                    transfer_modifier_milli: i32::from(country_id) * 100,
                    global_multiplier_milli: 1_000,
                })
                .collect(),
        }],
    });

    assert_eq!(result.source_value_milli, 7_000);
    assert_eq!(result.distributed_value_milli, 12_588);
    assert_eq!(result.routes[0].base_value_milli, 6_000);
    assert_eq!(
        result.routes[0].applied_modifiers_milli,
        vec![600, 250, 133, 75, 40]
    );
    assert_eq!(result.routes[0].factor_milli, 2_098);
    assert_eq!(result.routes[0].value_milli, 12_588);
}

#[test]
fn trade_distribution_does_not_debit_source_when_all_routes_are_zero() {
    let result = calculate_vanilla_trade_distribution(VanillaTradeDistributionInput {
        source_value_milli: 4_000,
        source_debit_milli: 999,
        routes: vec![VanillaTradeDistributionRouteInput {
            weight_milli: 0,
            node_value_milli: 4_000,
            candidates: Vec::new(),
        }],
    });

    assert_eq!(result.source_value_milli, 4_000);
    assert_eq!(result.distributed_value_milli, 0);
    assert_eq!(result.routes[0].value_milli, 0);
}
