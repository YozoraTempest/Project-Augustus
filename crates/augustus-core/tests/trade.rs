// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    VanillaTradeAttachedModifierGroup, VanillaTradeCandidate, VanillaTradeCaravanInput,
    VanillaTradeCountryFlags, VanillaTradeDistributionCandidate,
    VanillaTradeDistributionCandidateInput, VanillaTradeDistributionInput,
    VanillaTradeDistributionRouteInput, VanillaTradeIncomeInput, VanillaTradeIncomePassInput,
    VanillaTradePowerAmountInput, VanillaTradePowerModifierInput, VanillaTradePropagationInput,
    VanillaTradeRetentionInput, VanillaTradeRetentionParticipantInput,
    VanillaTradeRetentionPassInput, VanillaTradeRouteWeightContributionInput,
    VanillaTradeRouteWeightInput, VanillaTradeRouteWeightPassInput,
    VanillaTradeSteeringEligibilityInput, VanillaTradeSteeringModifierInput,
    calculate_vanilla_trade_caravan, calculate_vanilla_trade_distribution,
    calculate_vanilla_trade_income, calculate_vanilla_trade_income_pass,
    calculate_vanilla_trade_power_amount, calculate_vanilla_trade_power_modifier,
    calculate_vanilla_trade_propagation, calculate_vanilla_trade_retention,
    calculate_vanilla_trade_retention_pass, calculate_vanilla_trade_route_weight_pass,
    calculate_vanilla_trade_route_weights, calculate_vanilla_trade_steering_modifier,
    is_vanilla_trade_collector, is_vanilla_trade_steering_eligible,
    select_vanilla_trade_distribution_candidates, sort_vanilla_trade_candidates,
};

#[test]
fn trade_income_matches_the_two_verified_syria_paths() {
    let no_mod = calculate_vanilla_trade_income(VanillaTradeIncomeInput {
        node_value_milli: 3_837,
        country_record_value_milli: 343,
        base_context_modifier_milli: 20,
        selected_context_modifier_milli: 100,
        virtual_context_modifier_milli: None,
        minimum_context_modifier_milli: 10,
        maximum_context_modifier_milli: 2_000,
        post_context_modifier_milli: 100,
    })
    .unwrap();
    assert_eq!(no_mod.context_modifier_before_clamp_milli, 120);
    assert_eq!(no_mod.context_modifier_milli, 120);
    assert_eq!(no_mod.stage_one_product, 1_316_091);
    assert_eq!(no_mod.stage_one_milli, 1_316);
    assert_eq!(no_mod.effective_multiplier_milli, 1_220);
    assert_eq!(no_mod.stage_two_product, 1_605_520);
    assert_eq!(no_mod.trade_income_milli, 1_605);

    let modded = calculate_vanilla_trade_income(VanillaTradeIncomeInput {
        node_value_milli: 3_845,
        country_record_value_milli: 342,
        base_context_modifier_milli: 20,
        selected_context_modifier_milli: 50,
        virtual_context_modifier_milli: None,
        minimum_context_modifier_milli: 10,
        maximum_context_modifier_milli: 2_000,
        post_context_modifier_milli: 100,
    })
    .unwrap();
    assert_eq!(modded.context_modifier_before_clamp_milli, 70);
    assert_eq!(modded.context_modifier_milli, 70);
    assert_eq!(modded.stage_one_product, 1_314_990);
    assert_eq!(modded.stage_one_milli, 1_314);
    assert_eq!(modded.effective_multiplier_milli, 1_170);
    assert_eq!(modded.stage_two_product, 1_537_380);
    assert_eq!(modded.trade_income_milli, 1_537);
}

#[test]
fn trade_income_clamps_context_before_bonus_and_truncates_signed_stages() {
    let upper = calculate_vanilla_trade_income(VanillaTradeIncomeInput {
        node_value_milli: 1,
        country_record_value_milli: 1_000,
        base_context_modifier_milli: 20,
        selected_context_modifier_milli: 3_000,
        virtual_context_modifier_milli: Some(2_000),
        minimum_context_modifier_milli: 10,
        maximum_context_modifier_milli: 2_000,
        post_context_modifier_milli: 100,
    })
    .unwrap();
    assert_eq!(upper.context_modifier_before_clamp_milli, 2_020);
    assert_eq!(upper.context_modifier_milli, 2_000);
    assert_eq!(upper.effective_multiplier_milli, 3_100);
    assert_eq!(upper.trade_income_milli, 3);

    let negative = calculate_vanilla_trade_income(VanillaTradeIncomeInput {
        node_value_milli: 3,
        country_record_value_milli: -500,
        base_context_modifier_milli: 0,
        selected_context_modifier_milli: 0,
        virtual_context_modifier_milli: None,
        minimum_context_modifier_milli: 10,
        maximum_context_modifier_milli: 2_000,
        post_context_modifier_milli: 0,
    })
    .unwrap();
    assert_eq!(negative.stage_one_milli, -1);
    assert_eq!(negative.trade_income_milli, -1);

    let subunit = calculate_vanilla_trade_income(VanillaTradeIncomeInput {
        node_value_milli: 3,
        country_record_value_milli: -333,
        base_context_modifier_milli: 0,
        selected_context_modifier_milli: 0,
        virtual_context_modifier_milli: None,
        minimum_context_modifier_milli: 10,
        maximum_context_modifier_milli: 2_000,
        post_context_modifier_milli: 0,
    })
    .unwrap();
    assert_eq!(subunit.stage_one_product, -999);
    assert_eq!(subunit.stage_one_milli, 0);
    assert_eq!(subunit.trade_income_milli, 0);
}

#[test]
fn trade_income_preserves_native_i32_context_wrap_and_result_storage() {
    let wrapped_context = calculate_vanilla_trade_income(VanillaTradeIncomeInput {
        node_value_milli: 1,
        country_record_value_milli: 1_000,
        base_context_modifier_milli: i64::from(i32::MAX),
        selected_context_modifier_milli: 1,
        virtual_context_modifier_milli: None,
        minimum_context_modifier_milli: 10,
        maximum_context_modifier_milli: 2_000,
        post_context_modifier_milli: 0,
    })
    .unwrap();
    assert_eq!(
        wrapped_context.context_modifier_before_clamp_milli,
        i128::from(i32::MIN)
    );
    assert_eq!(wrapped_context.context_modifier_milli, 10);

    let wrapped_multiplier = calculate_vanilla_trade_income(VanillaTradeIncomeInput {
        node_value_milli: 1,
        country_record_value_milli: 1_000,
        base_context_modifier_milli: 1_000,
        selected_context_modifier_milli: 1_000,
        virtual_context_modifier_milli: None,
        minimum_context_modifier_milli: 10,
        maximum_context_modifier_milli: 2_000,
        post_context_modifier_milli: i64::from(i32::MAX),
    })
    .unwrap();
    assert_eq!(wrapped_multiplier.context_modifier_milli, 2_000);
    assert_eq!(
        wrapped_multiplier.effective_multiplier_milli,
        -2_147_480_649
    );

    let wrapped_result = calculate_vanilla_trade_income(VanillaTradeIncomeInput {
        node_value_milli: 2_000_000,
        country_record_value_milli: 2_000_000,
        base_context_modifier_milli: 0,
        selected_context_modifier_milli: 0,
        virtual_context_modifier_milli: None,
        minimum_context_modifier_milli: 10,
        maximum_context_modifier_milli: 2_000,
        post_context_modifier_milli: 0,
    })
    .unwrap();
    assert_eq!(wrapped_result.stage_one_product, 4_000_000_000_000);
    assert_eq!(wrapped_result.stage_one_milli, -294_967_296);
    assert_eq!(wrapped_result.effective_multiplier_milli, 1_010);
    assert_eq!(wrapped_result.stage_two_product, -297_916_968_960);
    assert_eq!(wrapped_result.trade_income_milli, -297_916_968);
}

#[test]
fn trade_income_pass_preserves_record_results_and_wraps_country_accumulation() {
    let observed_row = VanillaTradeIncomeInput {
        node_value_milli: 3_817,
        country_record_value_milli: 349,
        base_context_modifier_milli: 20,
        selected_context_modifier_milli: 50,
        virtual_context_modifier_milli: None,
        minimum_context_modifier_milli: 10,
        maximum_context_modifier_milli: 2_000,
        post_context_modifier_milli: 100,
    };
    let zero_row = VanillaTradeIncomeInput {
        node_value_milli: 0,
        ..observed_row
    };
    let pass = calculate_vanilla_trade_income_pass(VanillaTradeIncomePassInput {
        eligible_records: vec![observed_row, zero_row],
    })
    .unwrap();
    assert_eq!(pass.records.len(), 2);
    assert_eq!(pass.records[0].trade_income_milli, 1_558);
    assert_eq!(pass.records[1].trade_income_milli, 0);
    assert_eq!(pass.total_income_milli, 1_558);

    let wrapping_row = VanillaTradeIncomeInput {
        node_value_milli: 1_500_000,
        country_record_value_milli: 1_000_000,
        base_context_modifier_milli: 10,
        selected_context_modifier_milli: 0,
        virtual_context_modifier_milli: None,
        minimum_context_modifier_milli: 10,
        maximum_context_modifier_milli: 2_000,
        post_context_modifier_milli: -10,
    };
    let wrapped = calculate_vanilla_trade_income_pass(VanillaTradeIncomePassInput {
        eligible_records: vec![wrapping_row; 3],
    })
    .unwrap();
    assert_eq!(wrapped.records[0].trade_income_milli, 1_500_000_000);
    assert_eq!(wrapped.total_income_milli, 205_032_704);
}

#[test]
fn trade_caravan_preserves_bounds_modifier_order_and_zero_floor() {
    let result = calculate_vanilla_trade_caravan(VanillaTradeCaravanInput {
        development_milli: 90_000,
        denominator_milli: 3_000,
        minimum_milli: 0,
        maximum_milli: 50_000,
        effective_modifier_milli: 250,
    });
    assert_eq!(result.development_value_milli, 30_000);
    assert_eq!(result.bounded_value_milli, 30_000);
    assert_eq!(result.factor_milli, 1_250);
    assert_eq!(result.value_milli, 37_500);

    let bounded_by_max = calculate_vanilla_trade_caravan(VanillaTradeCaravanInput {
        development_milli: 90_000,
        denominator_milli: 1_000,
        minimum_milli: 10_000,
        maximum_milli: 5_000,
        effective_modifier_milli: -1_500,
    });
    assert_eq!(bounded_by_max.development_value_milli, 90_000);
    assert_eq!(bounded_by_max.bounded_value_milli, 5_000);
    assert_eq!(bounded_by_max.factor_milli, -500);
    assert_eq!(bounded_by_max.value_milli, 0);

    let invalid_denominator = calculate_vanilla_trade_caravan(VanillaTradeCaravanInput {
        development_milli: 90_000,
        denominator_milli: 0,
        minimum_milli: 0,
        maximum_milli: 50_000,
        effective_modifier_milli: 0,
    });
    assert_eq!(invalid_denominator.value_milli, 0);
    assert_eq!(invalid_denominator.bounded_value_milli, 0);
}

#[test]
fn trade_caravan_uses_signed_truncation_and_wrapping_factor() {
    let result = calculate_vanilla_trade_caravan(VanillaTradeCaravanInput {
        development_milli: 33_333,
        denominator_milli: 5_000,
        minimum_milli: 0,
        maximum_milli: 20_000,
        effective_modifier_milli: -333,
    });
    assert_eq!(result.development_value_milli, 6_666);
    assert_eq!(result.bounded_value_milli, 6_666);
    assert_eq!(result.value_milli, 4_446);

    let wrapped = calculate_vanilla_trade_caravan(VanillaTradeCaravanInput {
        development_milli: i32::MAX,
        denominator_milli: 1_000,
        minimum_milli: -1_000,
        maximum_milli: i32::MAX,
        effective_modifier_milli: i32::MAX,
    });
    assert_eq!(wrapped.factor_milli, -2_147_482_649);
    assert_eq!(wrapped.value_milli, 0);
}

#[test]
fn trade_power_amount_preserves_wrapping_sources_and_signed_division() {
    let result = calculate_vanilla_trade_power_amount(VanillaTradePowerAmountInput {
        province_power_milli: 7_406,
        fleet_power_milli: -5_765,
        external_power_milli: 0,
        denominator_milli: 1_000,
        global_scale_milli: 677,
        country_modifier_milli: 1_651,
    });
    assert_eq!(result.combined_power_milli, 1_641);
    assert_eq!(result.scaled_power_milli, 1_110);
    assert_eq!(result.factor_milli, 2_651);
    assert_eq!(result.value_milli, 2_942);

    let negative_denominator = calculate_vanilla_trade_power_amount(VanillaTradePowerAmountInput {
        province_power_milli: 9_617,
        fleet_power_milli: 5_316,
        external_power_milli: 0,
        denominator_milli: -7,
        global_scale_milli: -400,
        country_modifier_milli: 457,
    });
    assert_eq!(negative_denominator.combined_power_milli, 14_933);
    assert_eq!(negative_denominator.scaled_power_milli, 853_314);
    assert_eq!(negative_denominator.factor_milli, 1_457);
    assert_eq!(negative_denominator.value_milli, 1_243_278);

    let zero_denominator = calculate_vanilla_trade_power_amount(VanillaTradePowerAmountInput {
        province_power_milli: i32::MAX,
        fleet_power_milli: 1,
        external_power_milli: 1,
        denominator_milli: 0,
        global_scale_milli: i32::MAX,
        country_modifier_milli: -1_500,
    });
    assert_eq!(zero_denominator.combined_power_milli, i32::MIN + 1);
    assert_eq!(zero_denominator.scaled_power_milli, 0);
    assert_eq!(zero_denominator.factor_milli, 0);
    assert_eq!(zero_denominator.value_milli, 0);
}

#[test]
fn trade_power_modifier_uses_one_embargo_factor_then_post_embargo_additions() {
    let result = calculate_vanilla_trade_power_modifier(VanillaTradePowerModifierInput {
        initial_modifier_milli: 100,
        selected_context_modifier_milli: 50,
        policy_modifier_milli: -25,
        embargo_amounts_milli: vec![200, 300, -10],
        noncapital_modifier_milli: -250,
        noncapital_active: true,
        home_bonus_milli: 75,
        home_bonus_active: true,
    });
    assert_eq!(result.base_modifier_milli, 125);
    assert_eq!(result.pre_embargo_factor_milli, 1_125);
    assert_eq!(result.embargo_modifier_milli, -562);
    assert_eq!(result.after_embargo_modifier_milli, -437);
    assert_eq!(result.noncapital_factor_milli, 563);
    assert_eq!(result.noncapital_contribution_milli, -140);
    assert_eq!(result.home_bonus_contribution_milli, 75);
    assert_eq!(result.value_milli, -502);

    let inactive = calculate_vanilla_trade_power_modifier(VanillaTradePowerModifierInput {
        initial_modifier_milli: i32::MAX,
        selected_context_modifier_milli: 1,
        policy_modifier_milli: 0,
        embargo_amounts_milli: vec![1],
        noncapital_modifier_milli: i32::MAX,
        noncapital_active: false,
        home_bonus_milli: i32::MAX,
        home_bonus_active: false,
    });
    assert_eq!(inactive.noncapital_factor_milli, 0);
    assert_eq!(inactive.noncapital_contribution_milli, 0);
    assert_eq!(inactive.home_bonus_contribution_milli, 0);
}

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
fn trade_collection_predicate_matches_all_country_flag_combinations() {
    for special_country in [false, true] {
        for ae in [false, true] {
            for ac in [false, true] {
                for ad in [false, true] {
                    let flags = VanillaTradeCountryFlags {
                        special_country,
                        ae,
                        ac,
                        ad,
                    };
                    let expected = special_country || (ae && !ac) || (ad && !(ae && ac));
                    assert_eq!(is_vanilla_trade_collector(flags), expected, "{flags:?}");
                }
            }
        }
    }
}

#[test]
fn trade_steering_eligibility_excludes_collectors_and_uses_the_route_cache() {
    let collector = VanillaTradeCountryFlags {
        special_country: false,
        ae: true,
        ac: false,
        ad: false,
    };
    assert!(!is_vanilla_trade_steering_eligible(
        VanillaTradeSteeringEligibilityInput {
            country_flags: collector,
            effective_amount_milli: 100,
            cached_route_eligible: true,
        }
    ));

    let ordinary = VanillaTradeCountryFlags {
        special_country: false,
        ae: false,
        ac: false,
        ad: false,
    };
    assert!(!is_vanilla_trade_steering_eligible(
        VanillaTradeSteeringEligibilityInput {
            country_flags: ordinary,
            effective_amount_milli: 0,
            cached_route_eligible: true,
        }
    ));
    assert!(!is_vanilla_trade_steering_eligible(
        VanillaTradeSteeringEligibilityInput {
            country_flags: ordinary,
            effective_amount_milli: 100,
            cached_route_eligible: false,
        }
    ));
    assert!(is_vanilla_trade_steering_eligible(
        VanillaTradeSteeringEligibilityInput {
            country_flags: ordinary,
            effective_amount_milli: 100,
            cached_route_eligible: true,
        }
    ));

    let ae_and_ac = VanillaTradeCountryFlags {
        special_country: false,
        ae: true,
        ac: true,
        ad: false,
    };
    assert!(is_vanilla_trade_steering_eligible(
        VanillaTradeSteeringEligibilityInput {
            country_flags: ae_and_ac,
            effective_amount_milli: 0,
            cached_route_eligible: true,
        }
    ));
}

#[test]
fn trade_retention_pass_aggregates_only_valid_participating_records() {
    let collected = VanillaTradeCountryFlags {
        special_country: false,
        ae: true,
        ac: false,
        ad: false,
    };
    let ordinary = VanillaTradeCountryFlags {
        special_country: false,
        ae: false,
        ac: false,
        ad: false,
    };
    let result = calculate_vanilla_trade_retention_pass(VanillaTradeRetentionPassInput {
        participants: vec![
            VanillaTradeRetentionParticipantInput {
                reference_valid: false,
                participation_balance_milli: 1,
                effective_amount_milli: 999,
                country_flags: collected,
                cached_route_eligible: false,
            },
            VanillaTradeRetentionParticipantInput {
                reference_valid: true,
                participation_balance_milli: 0,
                effective_amount_milli: 888,
                country_flags: collected,
                cached_route_eligible: false,
            },
            VanillaTradeRetentionParticipantInput {
                reference_valid: true,
                participation_balance_milli: 1,
                effective_amount_milli: 1_200,
                country_flags: collected,
                cached_route_eligible: false,
            },
            VanillaTradeRetentionParticipantInput {
                reference_valid: true,
                participation_balance_milli: -1,
                effective_amount_milli: 800,
                country_flags: ordinary,
                cached_route_eligible: true,
            },
            VanillaTradeRetentionParticipantInput {
                reference_valid: true,
                participation_balance_milli: 1,
                effective_amount_milli: 500,
                country_flags: ordinary,
                cached_route_eligible: false,
            },
        ],
    });

    assert_eq!(result.total_amount_milli, 2_000);
    assert_eq!(result.collected_amount_milli, 1_200);
    assert_eq!(result.transferred_amount_milli, 800);
    assert_eq!(result.retention.transfer_ratio_milli, 400);
    assert_eq!(result.retention.retention_milli, 600);
}

#[test]
fn trade_retention_pass_preserves_the_native_zero_total_sentinel() {
    let result = calculate_vanilla_trade_retention_pass(VanillaTradeRetentionPassInput {
        participants: vec![
            VanillaTradeRetentionParticipantInput {
                reference_valid: true,
                participation_balance_milli: 1,
                effective_amount_milli: 5,
                country_flags: VanillaTradeCountryFlags {
                    special_country: false,
                    ae: true,
                    ac: false,
                    ad: false,
                },
                cached_route_eligible: false,
            },
            VanillaTradeRetentionParticipantInput {
                reference_valid: true,
                participation_balance_milli: 1,
                effective_amount_milli: -5,
                country_flags: VanillaTradeCountryFlags {
                    special_country: false,
                    ae: true,
                    ac: true,
                    ad: false,
                },
                cached_route_eligible: true,
            },
        ],
    });

    assert_eq!(result.total_amount_milli, 0);
    assert_eq!(result.collected_amount_milli, 5);
    assert_eq!(result.transferred_amount_milli, -5);
    assert_eq!(result.retention.transfer_ratio_milli, -1);
    assert_eq!(result.retention.retention_milli, 1_001);
}

#[test]
fn trade_retention_pass_wraps_country_amount_aggregates() {
    let result = calculate_vanilla_trade_retention_pass(VanillaTradeRetentionPassInput {
        participants: [i32::MAX, 1]
            .into_iter()
            .map(
                |effective_amount_milli| VanillaTradeRetentionParticipantInput {
                    reference_valid: true,
                    participation_balance_milli: 1,
                    effective_amount_milli,
                    country_flags: VanillaTradeCountryFlags {
                        special_country: false,
                        ae: true,
                        ac: false,
                        ad: false,
                    },
                    cached_route_eligible: false,
                },
            )
            .collect(),
    });

    assert_eq!(result.total_amount_milli, i32::MIN);
    assert_eq!(result.collected_amount_milli, i32::MIN);
    assert_eq!(result.transferred_amount_milli, 0);
    assert_eq!(result.retention.retention_milli, 1_000);
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
fn trade_route_weight_pass_applies_modifiers_and_maps_routes_past_the_table_to_zero() {
    let result = calculate_vanilla_trade_route_weight_pass(VanillaTradeRouteWeightPassInput {
        marked_routes: vec![true, false, true],
        contributions: vec![
            VanillaTradeRouteWeightContributionInput {
                selected_route_id: 0,
                effective_amount_milli: 400,
                steering_modifier_milli: 250,
            },
            VanillaTradeRouteWeightContributionInput {
                selected_route_id: 2,
                effective_amount_milli: 300,
                steering_modifier_milli: 0,
            },
            VanillaTradeRouteWeightContributionInput {
                selected_route_id: 9,
                effective_amount_milli: 1_000,
                steering_modifier_milli: 0,
            },
        ],
    })
    .unwrap();

    assert_eq!(result.route_contributions_milli, [1_500, 0, 300]);
    assert_eq!(result.total_milli, 1_800);
    assert_eq!(result.weights_milli, [833, 0, 166]);
}

#[test]
fn trade_route_weight_pass_splits_marked_zero_total_and_rejects_unknown_negative_routes() {
    let zero_total = calculate_vanilla_trade_route_weight_pass(VanillaTradeRouteWeightPassInput {
        marked_routes: vec![true, false, true],
        contributions: Vec::new(),
    })
    .unwrap();
    assert_eq!(zero_total.route_contributions_milli, [0, 0, 0]);
    assert_eq!(zero_total.weights_milli, [500, 0, 500]);

    assert!(
        calculate_vanilla_trade_route_weight_pass(VanillaTradeRouteWeightPassInput {
            marked_routes: vec![true],
            contributions: vec![VanillaTradeRouteWeightContributionInput {
                selected_route_id: -1,
                effective_amount_milli: 1,
                steering_modifier_milli: 0,
            }],
        })
        .is_err()
    );
    assert!(
        calculate_vanilla_trade_route_weight_pass(VanillaTradeRouteWeightPassInput {
            marked_routes: Vec::new(),
            contributions: vec![VanillaTradeRouteWeightContributionInput {
                selected_route_id: 0,
                effective_amount_milli: 1,
                steering_modifier_milli: 0,
            }],
        })
        .is_err()
    );
}

#[test]
fn trade_route_weight_pass_wraps_route_contribution_accumulation() {
    let result = calculate_vanilla_trade_route_weight_pass(VanillaTradeRouteWeightPassInput {
        marked_routes: vec![true],
        contributions: [i32::MAX, 1]
            .into_iter()
            .map(
                |effective_amount_milli| VanillaTradeRouteWeightContributionInput {
                    selected_route_id: 0,
                    effective_amount_milli,
                    steering_modifier_milli: 0,
                },
            )
            .collect(),
    })
    .unwrap();

    assert_eq!(result.route_contributions_milli, [i32::MIN]);
    assert_eq!(result.total_milli, i32::MIN);
    assert_eq!(result.weights_milli, [1_000]);
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
fn trade_distribution_candidate_filter_checks_route_flags_and_multiplier() {
    let candidate = |country_id, reference_valid, ae, ac, route_id, global_multiplier_milli| {
        VanillaTradeDistributionCandidateInput {
            country_id,
            reference_valid,
            country_flags: VanillaTradeCountryFlags {
                special_country: false,
                ae,
                ac,
                ad: false,
            },
            route_id,
            adjusted_amount_milli: i32::from(country_id) * 10,
            unadjusted_amount_milli: 100,
            transfer_modifier_milli: 1_000,
            global_multiplier_milli,
        }
    };

    let selected = select_vanilla_trade_distribution_candidates(
        7,
        vec![
            candidate(829, true, true, true, 7, 50),
            candidate(1, false, true, true, 7, 50),
            candidate(2, true, false, true, 7, 50),
            candidate(3, true, true, false, 7, 50),
            candidate(4, true, true, true, 8, 50),
            candidate(5, true, true, true, 7, 0),
            candidate(6, true, true, true, 7, -1),
            candidate(145, true, true, true, 7, 50),
        ],
    );

    assert_eq!(
        selected
            .iter()
            .map(|candidate| candidate.country_id)
            .collect::<Vec<_>>(),
        [829, 145]
    );
    assert_eq!(selected[0].global_multiplier_milli, 50);
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

#[test]
fn trade_steering_modifier_uses_the_first_matching_country_group() {
    let result = calculate_vanilla_trade_steering_modifier(VanillaTradeSteeringModifierInput {
        country_id: 145,
        country_context_modifier_milli: 10,
        attached_modifiers_active: true,
        attached_modifier_groups: vec![
            VanillaTradeAttachedModifierGroup {
                country_id: 829,
                modifiers_milli: vec![100],
            },
            VanillaTradeAttachedModifierGroup {
                country_id: 145,
                modifiers_milli: vec![2, 3],
            },
            VanillaTradeAttachedModifierGroup {
                country_id: 145,
                modifiers_milli: vec![900],
            },
        ],
    });

    assert_eq!(result.country_context_modifier_milli, 10);
    assert_eq!(result.attached_modifier_milli, 5);
    assert_eq!(result.modifier_milli, 15);
}

#[test]
fn trade_steering_modifier_skips_disabled_and_unmatched_groups() {
    let disabled = calculate_vanilla_trade_steering_modifier(VanillaTradeSteeringModifierInput {
        country_id: 145,
        country_context_modifier_milli: 25,
        attached_modifiers_active: false,
        attached_modifier_groups: vec![VanillaTradeAttachedModifierGroup {
            country_id: 145,
            modifiers_milli: vec![400],
        }],
    });
    assert_eq!(disabled.attached_modifier_milli, 0);
    assert_eq!(disabled.modifier_milli, 25);

    let unmatched = calculate_vanilla_trade_steering_modifier(VanillaTradeSteeringModifierInput {
        country_id: 145,
        country_context_modifier_milli: 25,
        attached_modifiers_active: true,
        attached_modifier_groups: vec![VanillaTradeAttachedModifierGroup {
            country_id: 829,
            modifiers_milli: vec![400],
        }],
    });
    assert_eq!(unmatched.attached_modifier_milli, 0);
    assert_eq!(unmatched.modifier_milli, 25);
}

#[test]
fn trade_steering_modifier_wraps_attached_value_accumulation() {
    let attached_wrap =
        calculate_vanilla_trade_steering_modifier(VanillaTradeSteeringModifierInput {
            country_id: 145,
            country_context_modifier_milli: 0,
            attached_modifiers_active: true,
            attached_modifier_groups: vec![VanillaTradeAttachedModifierGroup {
                country_id: 145,
                modifiers_milli: vec![i32::MAX, 1],
            }],
        });

    assert_eq!(attached_wrap.attached_modifier_milli, i32::MIN);
    assert_eq!(attached_wrap.modifier_milli, i32::MIN);

    let context_wrap =
        calculate_vanilla_trade_steering_modifier(VanillaTradeSteeringModifierInput {
            country_id: 145,
            country_context_modifier_milli: i32::MAX,
            attached_modifiers_active: true,
            attached_modifier_groups: vec![VanillaTradeAttachedModifierGroup {
                country_id: 145,
                modifiers_milli: vec![1],
            }],
        });
    assert_eq!(context_wrap.modifier_milli, i32::MIN);
}

#[test]
fn trade_distribution_replays_observed_pest_and_krakow_inflows_to_wien() {
    let candidates = || {
        let transfer_factor = |country_id| {
            let country_context_modifier_milli = if country_id == 145 { 25 } else { 0 };
            let modifier =
                calculate_vanilla_trade_steering_modifier(VanillaTradeSteeringModifierInput {
                    country_id,
                    country_context_modifier_milli,
                    attached_modifiers_active: true,
                    attached_modifier_groups: vec![VanillaTradeAttachedModifierGroup {
                        country_id: 145,
                        modifiers_milli: vec![0],
                    }],
                });
            1_000_i32.wrapping_add(modifier.modifier_milli)
        };
        assert_eq!(transfer_factor(145), 1_025);

        [
            (145, 67_299, 65_658),
            (829, 12_732, 12_732),
            (830, 10_963, 10_963),
            (836, 10_729, 10_729),
            (828, 10_610, 10_610),
        ]
        .into_iter()
        .map(
            |(country_id, adjusted_amount_milli, unadjusted_amount_milli)| {
                VanillaTradeDistributionCandidate {
                    country_id,
                    adjusted_amount_milli,
                    unadjusted_amount_milli,
                    transfer_modifier_milli: transfer_factor(country_id),
                    global_multiplier_milli: 50,
                }
            },
        )
        .collect()
    };
    let route = |weight_milli, node_value_milli| VanillaTradeDistributionRouteInput {
        weight_milli,
        node_value_milli,
        candidates: candidates(),
    };

    let pest = calculate_vanilla_trade_distribution(VanillaTradeDistributionInput {
        source_value_milli: 5_666,
        source_debit_milli: 3_427,
        routes: vec![route(612, 3_427)],
    });
    assert_eq!(pest.source_value_milli, 2_239);
    assert_eq!(pest.routes[0].base_value_milli, 2_097);
    assert_eq!(pest.routes[0].applied_modifiers_milli, [51, 25, 16, 12, 10]);
    assert_eq!(pest.routes[0].factor_milli, 1_114);
    assert_eq!(pest.routes[0].value_milli, 2_336);

    let krakow = calculate_vanilla_trade_distribution(VanillaTradeDistributionInput {
        source_value_milli: 8_008,
        source_debit_milli: 5_261,
        routes: vec![route(352, 5_261)],
    });
    assert_eq!(krakow.source_value_milli, 2_747);
    assert_eq!(krakow.routes[0].base_value_milli, 1_851);
    assert_eq!(
        krakow.routes[0].applied_modifiers_milli,
        [51, 25, 16, 12, 10]
    );
    assert_eq!(krakow.routes[0].factor_milli, 1_114);
    assert_eq!(krakow.routes[0].value_milli, 2_062);

    let wien_before_outgoing = 5_945_i32
        .wrapping_add(pest.routes[0].value_milli)
        .wrapping_add(krakow.routes[0].value_milli);
    let wien_current = wien_before_outgoing.wrapping_sub(5_626);
    assert_eq!(wien_before_outgoing, 10_343);
    assert_eq!(wien_current, 4_717);
}
