// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use serde::{Deserialize, Serialize};

use crate::Error;

/// Inputs for the verified EU4 caravan-strength amount stage.
///
/// The compatibility layer resolves development, the global denominator,
/// bounds and the effective modifier after any service or plugin lookup. The
/// core preserves the native integer order without interpreting those sources.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaTradeCaravanInput {
    pub development_milli: i32,
    pub denominator_milli: i32,
    pub minimum_milli: i32,
    pub maximum_milli: i32,
    pub effective_modifier_milli: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VanillaTradeCaravanResult {
    pub development_value_milli: i32,
    pub bounded_value_milli: i32,
    pub factor_milli: i32,
    pub value_milli: i32,
}

/// Reproduces the verified caravan-strength arithmetic stage.
///
/// A non-positive denominator returns zero. Positive denominators use a
/// minimum divisor of 1000; the development quotient is then bounded by the
/// native lower-then-upper branch. The effective modifier is added with
/// 32-bit wrapping, the scaled result is truncated toward zero, and the
/// final value is clamped at zero after the native cast.
pub fn calculate_vanilla_trade_caravan(
    input: VanillaTradeCaravanInput,
) -> VanillaTradeCaravanResult {
    let factor = 1_000_i32.wrapping_add(input.effective_modifier_milli);
    if input.denominator_milli <= 0 {
        return VanillaTradeCaravanResult {
            development_value_milli: 0,
            bounded_value_milli: 0,
            factor_milli: factor,
            value_milli: 0,
        };
    }

    let divisor = input.denominator_milli.max(1_000);
    let development_value =
        ((i64::from(input.development_milli) * 1_000) / i64::from(divisor)) as i32;
    let bounded_value = if development_value < input.minimum_milli {
        input.minimum_milli
    } else if development_value > input.maximum_milli {
        input.maximum_milli
    } else {
        development_value
    };
    let value = ((i64::from(bounded_value) * i64::from(factor)) / 1_000) as i32;

    VanillaTradeCaravanResult {
        development_value_milli: development_value,
        bounded_value_milli: bounded_value,
        factor_milli: factor,
        value_milli: value.max(0),
    }
}

/// Inputs for the verified EU4 trade-income record calculation.
///
/// The compatibility layer resolves the node value, country record, base and
/// selected context values, an optional virtual replacement for the selected
/// value, and any post-context bonus. The core preserves native 32-bit storage
/// and fixed-point truncation without interpreting the context key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaTradeIncomeInput {
    pub node_value_milli: i64,
    pub country_record_value_milli: i64,
    pub base_context_modifier_milli: i64,
    pub selected_context_modifier_milli: i64,
    /// Optional transformed value that replaces `selected_context_modifier_milli`.
    pub virtual_context_modifier_milli: Option<i64>,
    pub minimum_context_modifier_milli: i64,
    pub maximum_context_modifier_milli: i64,
    pub post_context_modifier_milli: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VanillaTradeIncomeResult {
    pub context_modifier_before_clamp_milli: i128,
    pub context_modifier_milli: i64,
    pub stage_one_product: i128,
    pub stage_one_milli: i64,
    pub effective_multiplier_milli: i128,
    pub stage_two_product: i128,
    pub trade_income_milli: i64,
}

/// A country-level trade-income pass over records already selected by the
/// compatibility layer.
///
/// Record selection remains outside the core. The pass preserves each row's
/// calculation result and the native signed 32-bit accumulation order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaTradeIncomePassInput {
    pub eligible_records: Vec<VanillaTradeIncomeInput>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VanillaTradeIncomePassResult {
    pub records: Vec<VanillaTradeIncomeResult>,
    pub total_income_milli: i32,
}

/// Reproduces the observed trade-income calculation order.
///
/// A virtual context result replaces the selected context value. The base and
/// selected values are added with 32-bit wrapping before the lower-then-upper
/// clamp. The node and country record are multiplied and truncated to
/// thousandths, then stored as a signed 32-bit value. The result is multiplied
/// by the 1000 baseline, clamped context modifier, and post-context modifier;
/// these additions and the final stored result also use signed 32-bit storage.
pub fn calculate_vanilla_trade_income(
    input: VanillaTradeIncomeInput,
) -> Result<VanillaTradeIncomeResult, Error> {
    let native_i32 = |value| i32::try_from(value).map_err(|_| Error::Overflow);
    let node_value_milli = native_i32(input.node_value_milli)?;
    let country_record_value_milli = native_i32(input.country_record_value_milli)?;
    let base_context_modifier_milli = native_i32(input.base_context_modifier_milli)?;
    let selected_context_modifier_milli = native_i32(
        input
            .virtual_context_modifier_milli
            .unwrap_or(input.selected_context_modifier_milli),
    )?;
    let minimum_context_modifier_milli = native_i32(input.minimum_context_modifier_milli)?;
    let maximum_context_modifier_milli = native_i32(input.maximum_context_modifier_milli)?;
    let post_context_modifier_milli = native_i32(input.post_context_modifier_milli)?;

    let context_modifier_before_clamp_milli =
        base_context_modifier_milli.wrapping_add(selected_context_modifier_milli);
    let context_modifier_milli =
        if context_modifier_before_clamp_milli < minimum_context_modifier_milli {
            minimum_context_modifier_milli
        } else if context_modifier_before_clamp_milli > maximum_context_modifier_milli {
            maximum_context_modifier_milli
        } else {
            context_modifier_before_clamp_milli
        };

    let stage_one_product = i64::from(node_value_milli) * i64::from(country_record_value_milli);
    let stage_one_milli = (stage_one_product / 1_000) as i32;
    let effective_multiplier_milli = 1_000_i32
        .wrapping_add(context_modifier_milli)
        .wrapping_add(post_context_modifier_milli);
    let stage_two_product = i64::from(stage_one_milli) * i64::from(effective_multiplier_milli);
    let trade_income_milli = (stage_two_product / 1_000) as i32;

    Ok(VanillaTradeIncomeResult {
        context_modifier_before_clamp_milli: i128::from(context_modifier_before_clamp_milli),
        context_modifier_milli: i64::from(context_modifier_milli),
        stage_one_product: i128::from(stage_one_product),
        stage_one_milli: i64::from(stage_one_milli),
        effective_multiplier_milli: i128::from(effective_multiplier_milli),
        stage_two_product: i128::from(stage_two_product),
        trade_income_milli: i64::from(trade_income_milli),
    })
}

/// Calculates selected node records and accumulates their income writes.
///
/// Each row is evaluated in input order. The total matches repeated writes to
/// EU4's signed 32-bit delayed-cash field; node eligibility and source values
/// must already be resolved by the compatibility layer.
pub fn calculate_vanilla_trade_income_pass(
    input: VanillaTradeIncomePassInput,
) -> Result<VanillaTradeIncomePassResult, Error> {
    let records = input
        .eligible_records
        .into_iter()
        .map(calculate_vanilla_trade_income)
        .collect::<Result<Vec<_>, _>>()?;
    let total_income_milli = records.iter().try_fold(0_i32, |total, record| {
        let amount = i32::try_from(record.trade_income_milli).map_err(|_| Error::Overflow)?;
        Ok::<_, Error>(total.wrapping_add(amount))
    })?;

    Ok(VanillaTradeIncomePassResult {
        records,
        total_income_milli,
    })
}

/// Inputs for the verified province/trade-power amount stage.
///
/// The compatibility layer resolves the province, fleet and any conditional
/// external contribution, along with the global scale and country modifier.
/// The core only preserves their observed arithmetic and fixed-point order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaTradePowerAmountInput {
    pub province_power_milli: i32,
    pub fleet_power_milli: i32,
    pub external_power_milli: i32,
    pub denominator_milli: i32,
    pub global_scale_milli: i32,
    pub country_modifier_milli: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VanillaTradePowerAmountResult {
    pub combined_power_milli: i32,
    pub scaled_power_milli: i32,
    pub factor_milli: i32,
    pub value_milli: i32,
}

/// Reproduces the verified trade-power amount stage used by the trade chain.
///
/// A zero denominator returns zero. Other signed denominators are preserved,
/// matching the native division path. The three power sources are merged with
/// 32-bit wrapping, then scaled and finally multiplied by a nonnegative
/// country factor derived from `1000 + country_modifier_milli`.
pub fn calculate_vanilla_trade_power_amount(
    input: VanillaTradePowerAmountInput,
) -> VanillaTradePowerAmountResult {
    let combined = input
        .province_power_milli
        .wrapping_add(input.fleet_power_milli)
        .wrapping_add(input.external_power_milli);
    let factor = 1_000_i32.wrapping_add(input.country_modifier_milli).max(0);
    if input.denominator_milli == 0 {
        return VanillaTradePowerAmountResult {
            combined_power_milli: combined,
            scaled_power_milli: 0,
            factor_milli: factor,
            value_milli: 0,
        };
    }

    let scaled = ((i64::from(combined) * i64::from(input.global_scale_milli))
        / i64::from(input.denominator_milli)) as i32;
    let value = ((i64::from(scaled) * i64::from(factor)) / 1_000) as i32;
    VanillaTradePowerAmountResult {
        combined_power_milli: combined,
        scaled_power_milli: scaled,
        factor_milli: factor,
        value_milli: value,
    }
}

/// Inputs for the verified final trade-power modifier stage.
///
/// The compatibility layer resolves the selected context modifier, policy,
/// embargo amounts and the two conditional post-embargo contributions. The
/// core preserves the native order without interpreting country or node
/// objects.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaTradePowerModifierInput {
    pub initial_modifier_milli: i32,
    pub selected_context_modifier_milli: i32,
    pub policy_modifier_milli: i32,
    pub embargo_amounts_milli: Vec<i32>,
    pub noncapital_modifier_milli: i32,
    pub noncapital_active: bool,
    pub home_bonus_milli: i32,
    pub home_bonus_active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VanillaTradePowerModifierResult {
    pub base_modifier_milli: i32,
    pub pre_embargo_factor_milli: i32,
    pub embargo_modifier_milli: i32,
    pub after_embargo_modifier_milli: i32,
    pub noncapital_factor_milli: i32,
    pub noncapital_contribution_milli: i32,
    pub home_bonus_contribution_milli: i32,
    pub value_milli: i32,
}

/// Reproduces the verified final trade-power modifier order.
///
/// All positive embargo amounts use the same pre-embargo factor. The
/// non-capital contribution uses the factor after embargo deductions and is
/// clamped at zero before multiplication. The final home contribution is
/// added directly. Intermediate additions retain signed 32-bit wrapping.
pub fn calculate_vanilla_trade_power_modifier(
    input: VanillaTradePowerModifierInput,
) -> VanillaTradePowerModifierResult {
    let base = input
        .initial_modifier_milli
        .wrapping_add(input.selected_context_modifier_milli)
        .wrapping_add(input.policy_modifier_milli);
    let pre_embargo_factor = base.wrapping_add(1_000);
    let embargo_modifier = input
        .embargo_amounts_milli
        .into_iter()
        .filter(|amount| *amount > 0)
        .map(|amount| ((-i64::from(amount) * i64::from(pre_embargo_factor)) / 1_000) as i32)
        .fold(0_i32, i32::wrapping_add);
    let after_embargo = base.wrapping_add(embargo_modifier);
    let noncapital_factor = if input.noncapital_active {
        after_embargo.wrapping_add(1_000).max(0)
    } else {
        0
    };
    let noncapital_contribution = if input.noncapital_active {
        ((i64::from(noncapital_factor) * i64::from(input.noncapital_modifier_milli)) / 1_000) as i32
    } else {
        0
    };
    let with_noncapital = after_embargo.wrapping_add(noncapital_contribution);
    let home_bonus_contribution = if input.home_bonus_active {
        input.home_bonus_milli
    } else {
        0
    };
    let value = with_noncapital.wrapping_add(home_bonus_contribution);

    VanillaTradePowerModifierResult {
        base_modifier_milli: base,
        pre_embargo_factor_milli: pre_embargo_factor,
        embargo_modifier_milli: embargo_modifier,
        after_embargo_modifier_milli: after_embargo,
        noncapital_factor_milli: noncapital_factor,
        noncapital_contribution_milli: noncapital_contribution,
        home_bonus_contribution_milli: home_bonus_contribution,
        value_milli: value,
    }
}

/// Explicit inputs for the verified EU4 trade-propagation amount stage.
///
/// The compatibility layer resolves the two record values, the country
/// modifier, the global divisor and the threshold. The core keeps the native
/// fixed-point order and does not decide where those values come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaTradePropagationInput {
    /// The current node amount read from the source record.
    pub base_amount_milli: i32,
    /// The positive source amount before the country modifier is applied.
    pub source_amount_milli: i32,
    /// Country-side modifier in thousandths.
    pub country_modifier_milli: i32,
    /// Global propagation divisor. EU4 supplies a positive value here.
    pub divisor_milli: u32,
    /// Results below this threshold are discarded.
    pub threshold_milli: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VanillaTradePropagationResult {
    pub scaled_source_milli: i32,
    pub pre_threshold_milli: i32,
    pub amount_milli: i32,
    pub below_threshold: bool,
}

/// Reproduces the verified propagation amount stage.
///
/// The source contribution is scaled and truncated toward zero, negative
/// contributions are ignored, the base sum uses 32-bit wrapping storage, and
/// the final divisor is applied before the threshold check.
pub fn calculate_vanilla_trade_propagation(
    input: VanillaTradePropagationInput,
) -> Result<VanillaTradePropagationResult, Error> {
    if input.divisor_milli == 0 {
        return Err(Error::Invalid(
            "trade propagation divisor must be positive".into(),
        ));
    }
    let scaled_source = ((i64::from(input.source_amount_milli)
        * i64::from(input.country_modifier_milli))
        / 1_000) as i32;
    let positive_source = scaled_source.max(0);
    let pre_threshold = input.base_amount_milli.wrapping_add(positive_source);
    let amount = (i64::from(pre_threshold) * 1_000 / i64::from(input.divisor_milli)) as i32;
    let below_threshold = amount < input.threshold_milli;

    Ok(VanillaTradePropagationResult {
        scaled_source_milli: scaled_source,
        pre_threshold_milli: pre_threshold,
        amount_milli: if below_threshold { 0 } else { amount },
        below_threshold,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaTradeRetentionInput {
    pub total_amount_milli: i32,
    pub transferred_amount_milli: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VanillaTradeRetentionResult {
    pub transfer_ratio_milli: i32,
    pub retention_milli: i32,
}

/// Calculates the verified trade-retention ratio.
///
/// A zero transfer keeps the full 1000-unit retention. A nonzero transfer with
/// a zero denominator follows the observed native sentinel ratio of `-1`.
/// No extra clamp is applied because the original stage does not establish one.
pub fn calculate_vanilla_trade_retention(
    input: VanillaTradeRetentionInput,
) -> VanillaTradeRetentionResult {
    let transfer_ratio = if input.transferred_amount_milli == 0 {
        0
    } else if input.total_amount_milli == 0 {
        -1
    } else {
        (i64::from(input.transferred_amount_milli) * 1_000 / i64::from(input.total_amount_milli))
            as i32
    };

    VanillaTradeRetentionResult {
        transfer_ratio_milli: transfer_ratio,
        retention_milli: 1_000_i32.wrapping_sub(transfer_ratio),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaTradeRouteWeightInput {
    /// Already-filtered route contributions in declaration order.
    pub contributions_milli: Vec<i32>,
    /// Routes marked by the upstream trade-state stage.
    pub marked_routes: Vec<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VanillaTradeRouteWeightResult {
    pub total_milli: i32,
    pub weights_milli: Vec<i32>,
}

/// Normalizes the verified trade-route weight stage.
///
/// The compatibility layer performs route eligibility, route-id mapping and
/// modifier production before calling this function. A nonzero total
/// normalizes every contribution with the original fixed-point division. A
/// zero total distributes 1000 units only across marked routes; unmarked routes
/// remain zero and integer remainder is discarded.
pub fn calculate_vanilla_trade_route_weights(
    input: VanillaTradeRouteWeightInput,
) -> Result<VanillaTradeRouteWeightResult, Error> {
    if input.contributions_milli.len() != input.marked_routes.len() {
        return Err(Error::Invalid(
            "trade route contributions and markers must have equal lengths".into(),
        ));
    }

    let total = input
        .contributions_milli
        .iter()
        .copied()
        .fold(0_i32, i32::wrapping_add);
    let weights = if total != 0 {
        input
            .contributions_milli
            .iter()
            .map(|&contribution| (i64::from(contribution) * 1_000 / i64::from(total)) as i32)
            .collect()
    } else {
        let marked_count = input.marked_routes.iter().filter(|marked| **marked).count();
        let per_route = if marked_count == 0 {
            0
        } else {
            1_000 / i32::try_from(marked_count).expect("route marker count fits in i32")
        };
        input
            .marked_routes
            .iter()
            .map(|&marked| if marked { per_route } else { 0 })
            .collect()
    };

    Ok(VanillaTradeRouteWeightResult {
        total_milli: total,
        weights_milli: weights,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaTradeCandidate {
    /// Signed EU4 country identifier used by the final tie breaker.
    pub country_id: i16,
    pub adjusted_amount_milli: i32,
    pub unadjusted_amount_milli: i32,
}

/// Orders trade candidates using the verified stable three-key comparison.
/// Equal keys retain their input order, matching the native insertion sort.
pub fn sort_vanilla_trade_candidates(candidates: &mut [VanillaTradeCandidate]) {
    candidates.sort_by(|left, right| {
        right
            .adjusted_amount_milli
            .cmp(&left.adjusted_amount_milli)
            .then_with(|| {
                right
                    .unadjusted_amount_milli
                    .cmp(&left.unadjusted_amount_milli)
            })
            .then_with(|| right.country_id.cmp(&left.country_id))
    });
}

/// A candidate already resolved by the compatibility layer for one outgoing
/// route. The layer supplies the effective amount used by the native ordering
/// and the two modifiers that the distribution stage combines. It does not
/// need to expose the source game's object layout to the core.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaTradeDistributionCandidate {
    pub country_id: i16,
    pub adjusted_amount_milli: i32,
    pub unadjusted_amount_milli: i32,
    pub transfer_modifier_milli: i32,
    pub global_multiplier_milli: i32,
}

/// Inputs for one outgoing route in the verified value-distribution stage.
///
/// Route eligibility, route identity, and candidate construction stay in the
/// EU4 compatibility layer. The core only receives candidates that already
/// match this route.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaTradeDistributionRouteInput {
    pub weight_milli: i32,
    pub node_value_milli: i32,
    pub candidates: Vec<VanillaTradeDistributionCandidate>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VanillaTradeDistributionRouteResult {
    pub base_value_milli: i32,
    pub factor_milli: i32,
    pub value_milli: i32,
    pub applied_modifiers_milli: Vec<i32>,
}

/// Inputs for the verified EU4 value-distribution pass.
///
/// `source_debit_milli` is the already-resolved `bc` amount written by the
/// native node object. The stage applies it only when at least one route has a
/// nonzero weight; this preserves the observed negative-weight trigger without
/// guessing how the compatibility layer produced that field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaTradeDistributionInput {
    pub source_value_milli: i32,
    pub source_debit_milli: i32,
    pub routes: Vec<VanillaTradeDistributionRouteInput>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VanillaTradeDistributionResult {
    pub source_value_milli: i32,
    pub distributed_value_milli: i32,
    pub routes: Vec<VanillaTradeDistributionRouteResult>,
}

/// Reproduces the verified arithmetic portion of EU4's trade-value
/// distribution stage.
///
/// Each route first computes `weight * node_value / 1000` with truncation
/// toward zero and a signed 32-bit result. Candidates are stably ordered by
/// the same three keys as [`sort_vanilla_trade_candidates`]. At most five are
/// applied; for candidate rank `n` (starting at one), the effective modifier is
/// `trunc(transfer_modifier * global_multiplier / 1000) / n`. The route factor
/// starts at 1000 and accumulates those modifiers with 32-bit wrapping before
/// the route value is calculated. The source debit is applied once if any
/// route weight is nonzero.
pub fn calculate_vanilla_trade_distribution(
    input: VanillaTradeDistributionInput,
) -> VanillaTradeDistributionResult {
    let has_nonzero_weight = input.routes.iter().any(|route| route.weight_milli != 0);
    let source_value = input
        .source_value_milli
        .wrapping_sub(if has_nonzero_weight {
            input.source_debit_milli
        } else {
            0
        });

    let mut distributed_value = 0_i32;
    let routes = input
        .routes
        .into_iter()
        .map(|route| {
            let base_value = ((i64::from(route.weight_milli) * i64::from(route.node_value_milli))
                / 1_000) as i32;

            let mut candidates = route.candidates;
            candidates.sort_by(|left, right| {
                right
                    .adjusted_amount_milli
                    .cmp(&left.adjusted_amount_milli)
                    .then_with(|| {
                        right
                            .unadjusted_amount_milli
                            .cmp(&left.unadjusted_amount_milli)
                    })
                    .then_with(|| right.country_id.cmp(&left.country_id))
            });

            let applied_modifiers_milli = candidates
                .into_iter()
                .take(5)
                .enumerate()
                .map(|(rank, candidate)| {
                    let rank = i64::try_from(rank + 1).expect("candidate rank fits in i64");
                    let combined = ((i64::from(candidate.transfer_modifier_milli)
                        * i64::from(candidate.global_multiplier_milli))
                        / 1_000) as i32;
                    (i64::from(combined) / rank) as i32
                })
                .collect::<Vec<_>>();

            let factor = applied_modifiers_milli
                .iter()
                .copied()
                .fold(1_000_i32, i32::wrapping_add);
            let value = ((i64::from(base_value) * i64::from(factor)) / 1_000) as i32;
            distributed_value = distributed_value.wrapping_add(value);

            VanillaTradeDistributionRouteResult {
                base_value_milli: base_value,
                factor_milli: factor,
                value_milli: value,
                applied_modifiers_milli,
            }
        })
        .collect();

    VanillaTradeDistributionResult {
        source_value_milli: source_value,
        distributed_value_milli: distributed_value,
        routes,
    }
}
