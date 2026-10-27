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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaTradeRetentionPoolsInput {
    pub transferred_amount_milli: i32,
    pub other_amount_milli: i32,
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

/// Calculates retention directly from the two native accumulator outputs.
///
/// Their denominator is added in 32-bit storage before the fixed-point ratio
/// is evaluated. The other pool remains an explicit input; this stage does not
/// infer which country rows produce it.
pub fn calculate_vanilla_trade_retention_from_pools(
    input: VanillaTradeRetentionPoolsInput,
) -> VanillaTradeRetentionResult {
    calculate_vanilla_trade_retention(VanillaTradeRetentionInput {
        total_amount_milli: input
            .transferred_amount_milli
            .wrapping_add(input.other_amount_milli),
        transferred_amount_milli: input.transferred_amount_milli,
    })
}

/// Normalized country-state flags used by EU4's trade collection rules.
///
/// The compatibility layer resolves these flags from the current country
/// state; they are predicate inputs rather than a copy of the source record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaTradeCountryFlags {
    pub special_country: bool,
    pub ae: bool,
    pub ac: bool,
    pub ad: bool,
}

/// Applies the verified trade-collection predicate.
pub fn is_vanilla_trade_collector(flags: VanillaTradeCountryFlags) -> bool {
    flags.special_country || (flags.ae && !flags.ac) || (flags.ad && !(flags.ae && flags.ac))
}

/// Inputs for the verified country-to-route eligibility predicate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaTradeSteeringEligibilityInput {
    pub country_flags: VanillaTradeCountryFlags,
    pub effective_amount_milli: i32,
    /// The compatibility layer resolves the country/node cache entry.
    pub cached_route_eligible: bool,
}

/// Determines whether a country can transfer trade power along a route.
///
/// Collectors are excluded first. Ordinary countries require positive
/// effective power; special countries and countries with `ae` bypass that
/// threshold. All remaining cases use the compatibility-resolved cache bit.
pub fn is_vanilla_trade_steering_eligible(input: VanillaTradeSteeringEligibilityInput) -> bool {
    if is_vanilla_trade_collector(input.country_flags) {
        return false;
    }
    if !input.country_flags.special_country
        && !input.country_flags.ae
        && input.effective_amount_milli <= 0
    {
        return false;
    }

    input.cached_route_eligible
}

/// One country record normalized for the native trade-retention pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaTradeRetentionParticipantInput {
    pub reference_valid: bool,
    /// Native participation balance after compatibility-layer field lookup.
    pub participation_balance_milli: i32,
    /// Effective country amount produced by the trade-power amount stage.
    pub effective_amount_milli: i32,
    pub country_flags: VanillaTradeCountryFlags,
    /// The compatibility layer resolves the country/node cache entry.
    pub cached_route_eligible: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaTradeRetentionPassInput {
    pub participants: Vec<VanillaTradeRetentionParticipantInput>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VanillaTradeRetentionPassResult {
    pub total_amount_milli: i32,
    pub collected_amount_milli: i32,
    pub transferred_amount_milli: i32,
    pub retention: VanillaTradeRetentionResult,
}

/// Aggregates collecting and transferring amounts before calculating retention.
///
/// Invalid references and records with a zero participation balance are
/// ignored. Eligible amounts accumulate with 32-bit wrapping; records that are
/// neither collecting nor eligible to transfer do not enter the ratio.
pub fn calculate_vanilla_trade_retention_pass(
    input: VanillaTradeRetentionPassInput,
) -> VanillaTradeRetentionPassResult {
    let mut collected_amount = 0_i32;
    let mut transferred_amount = 0_i32;

    for participant in input.participants {
        if !participant.reference_valid || participant.participation_balance_milli == 0 {
            continue;
        }

        if is_vanilla_trade_collector(participant.country_flags) {
            collected_amount = collected_amount.wrapping_add(participant.effective_amount_milli);
        } else if is_vanilla_trade_steering_eligible(VanillaTradeSteeringEligibilityInput {
            country_flags: participant.country_flags,
            effective_amount_milli: participant.effective_amount_milli,
            cached_route_eligible: participant.cached_route_eligible,
        }) {
            transferred_amount =
                transferred_amount.wrapping_add(participant.effective_amount_milli);
        }
    }

    let total_amount = transferred_amount.wrapping_add(collected_amount);

    VanillaTradeRetentionPassResult {
        total_amount_milli: total_amount,
        collected_amount_milli: collected_amount,
        transferred_amount_milli: transferred_amount,
        retention: calculate_vanilla_trade_retention_from_pools(VanillaTradeRetentionPoolsInput {
            transferred_amount_milli: transferred_amount,
            other_amount_milli: collected_amount,
        }),
    }
}

/// A normalized contribution row used to build the transfer pool.
///
/// The compatibility layer resolves the row's reference and transfer
/// eligibility. The core applies the verified fixed-point contribution and
/// limit order to the supplied row values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaTradeRetentionContributionRowInput {
    pub reference_valid: bool,
    pub transfer_eligible: bool,
    pub contribution_base_milli: i32,
    pub contribution_limit_milli: i32,
    pub contribution_scale_milli: i32,
    pub previous_amount_milli: i32,
    pub current_amount_milli: i32,
}

/// One compatibility-filtered value from the node's other-pool sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaTradeRetentionSequenceEntryInput {
    pub included_in_other_pool: bool,
    pub amount_milli: i32,
}

/// Source rows for the two independently accumulated retention pools.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaTradeRetentionSourcesInput {
    pub transfer_rows: Vec<VanillaTradeRetentionContributionRowInput>,
    pub other_pool_entries: Vec<VanillaTradeRetentionSequenceEntryInput>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VanillaTradeRetentionSourcesResult {
    pub transfer_amount_milli: i32,
    pub other_pool_amount_milli: i32,
    pub total_amount_milli: i32,
    pub retention: VanillaTradeRetentionResult,
}

/// Accumulates the verified transfer rows and the separate node sequence.
///
/// Each transfer row first computes the signed delta, skips a zero active
/// amount, scales and clamps its contribution, then adds the delta. Both pools
/// use native signed 32-bit wrapping accumulation before the retention ratio.
pub fn calculate_vanilla_trade_retention_from_sources(
    input: VanillaTradeRetentionSourcesInput,
) -> VanillaTradeRetentionSourcesResult {
    let transfer_amount = input
        .transfer_rows
        .into_iter()
        .filter_map(vanilla_trade_retention_row_amount)
        .fold(0_i32, i32::wrapping_add);
    let other_pool_amount = input
        .other_pool_entries
        .into_iter()
        .filter(|entry| entry.included_in_other_pool)
        .map(|entry| entry.amount_milli)
        .fold(0_i32, i32::wrapping_add);
    let total_amount = transfer_amount.wrapping_add(other_pool_amount);
    let retention = calculate_vanilla_trade_retention_from_pools(VanillaTradeRetentionPoolsInput {
        transferred_amount_milli: transfer_amount,
        other_amount_milli: other_pool_amount,
    });

    VanillaTradeRetentionSourcesResult {
        transfer_amount_milli: transfer_amount,
        other_pool_amount_milli: other_pool_amount,
        total_amount_milli: total_amount,
        retention,
    }
}

fn vanilla_trade_retention_row_amount(
    row: VanillaTradeRetentionContributionRowInput,
) -> Option<i32> {
    if !row.reference_valid || !row.transfer_eligible {
        return None;
    }

    let delta = row
        .current_amount_milli
        .wrapping_sub(row.previous_amount_milli);
    if row.contribution_limit_milli.wrapping_add(delta) == 0 {
        return None;
    }

    let scaled = ((i64::from(row.contribution_base_milli)
        * i64::from(row.contribution_scale_milli))
        / 1_000) as i32;
    let contribution = if scaled < 0 {
        0
    } else if scaled > row.contribution_limit_milli {
        row.contribution_limit_milli
    } else {
        scaled
    };
    Some(delta.wrapping_add(contribution))
}

/// Inputs for one country's fixed-point share of collected trade power.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaTradeCountryPowerShareInput {
    /// Effective amount after the compatibility layer applies native caps.
    pub effective_amount_milli: i32,
    pub collected_total_milli: i32,
}

/// Calculates one country row's share in thousandths.
///
/// The native stage divides the effective row amount by total collected power,
/// multiplying by 1000 before truncating toward zero. A zero total uses the
/// observed `-1` sentinel; results are not clamped to a percentage range.
pub fn calculate_vanilla_trade_country_power_share(
    input: VanillaTradeCountryPowerShareInput,
) -> i32 {
    calculate_vanilla_trade_amount_share_milli(
        input.effective_amount_milli,
        input.collected_total_milli,
    )
}

fn calculate_vanilla_trade_amount_share_milli(amount_milli: i32, total_milli: i32) -> i32 {
    if total_milli == 0 {
        -1
    } else {
        (i64::from(amount_milli) * 1_000 / i64::from(total_milli)) as i32
    }
}

/// Inputs for a country's production-derived share of one trade node.
///
/// The compatibility layer resolves the native row-selection gate from its
/// indices and flags. Missing values remain unknown instead of being treated
/// as zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaTradeNodeCountryValueShareInput {
    pub row_is_eligible: Option<bool>,
    pub base_production_value_milli: Option<i32>,
    pub production_modifier_milli: Option<i32>,
    pub production_cap_milli: Option<i32>,
    pub value_added_milli: Option<i32>,
    pub value_subtracted_milli: Option<i32>,
    pub node_total_value_milli: Option<i32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VanillaTradeNodeCountryValueShareResult {
    /// `None` means row eligibility or a required input is unknown.
    pub write_applied: Option<bool>,
    pub production_flow_milli: Option<i32>,
    pub effective_value_milli: Option<i32>,
    /// A zero node total produces the native `-1` sentinel.
    pub country_share_milli: Option<i32>,
}

/// Calculates one country row's production value and share in a trade node.
///
/// The eligibility threshold uses the row's cap plus added value minus
/// subtracted value. The produced flow is then multiplied and truncated,
/// floored at zero, capped, and adjusted before dividing by the node total.
pub fn calculate_vanilla_trade_node_country_value_share(
    input: VanillaTradeNodeCountryValueShareInput,
) -> VanillaTradeNodeCountryValueShareResult {
    let not_written = || VanillaTradeNodeCountryValueShareResult {
        write_applied: Some(false),
        production_flow_milli: None,
        effective_value_milli: None,
        country_share_milli: None,
    };
    let unresolved = || VanillaTradeNodeCountryValueShareResult {
        write_applied: None,
        production_flow_milli: None,
        effective_value_milli: None,
        country_share_milli: None,
    };

    match input.row_is_eligible {
        Some(false) => return not_written(),
        None => return unresolved(),
        Some(true) => {}
    }

    let (
        Some(base_production_value_milli),
        Some(production_modifier_milli),
        Some(production_cap_milli),
        Some(value_added_milli),
        Some(value_subtracted_milli),
        Some(node_total_value_milli),
    ) = (
        input.base_production_value_milli,
        input.production_modifier_milli,
        input.production_cap_milli,
        input.value_added_milli,
        input.value_subtracted_milli,
        input.node_total_value_milli,
    )
    else {
        return unresolved();
    };

    let eligibility_value = production_cap_milli
        .wrapping_add(value_added_milli)
        .wrapping_sub(value_subtracted_milli);
    if eligibility_value <= 0 {
        return not_written();
    }

    let multiplied_flow = (i64::from(base_production_value_milli)
        * i64::from(production_modifier_milli)
        / 1_000) as i32;
    let production_flow_milli = multiplied_flow.max(0).min(production_cap_milli);
    let effective_value_milli = production_flow_milli
        .wrapping_add(value_added_milli)
        .wrapping_sub(value_subtracted_milli);
    let country_share_milli =
        calculate_vanilla_trade_amount_share_milli(effective_value_milli, node_total_value_milli);

    VanillaTradeNodeCountryValueShareResult {
        write_applied: Some(true),
        production_flow_milli: Some(production_flow_milli),
        effective_value_milli: Some(effective_value_milli),
        country_share_milli: Some(country_share_milli),
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

/// One already-eligible country's contribution to a selected trade route.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaTradeRouteWeightContributionInput {
    pub selected_route_id: i16,
    /// Amount resolved from the current country record before steering bonus.
    pub effective_amount_milli: i32,
    pub steering_modifier_milli: i32,
}

/// Inputs for route contribution assembly and normalization.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaTradeRouteWeightPassInput {
    /// Eligibility and modifier-source selection are resolved upstream.
    pub contributions: Vec<VanillaTradeRouteWeightContributionInput>,
    pub marked_routes: Vec<bool>,
}

/// One node-country row used to select trade-route weight contributions.
///
/// The compatibility layer resolves the row values, route, and cached route
/// bit. The core applies the positive participation-delta gate and the shared
/// country steering-eligibility rule before accumulating the contribution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaTradeRouteWeightSourceRowInput {
    pub participation_balance_milli: i32,
    pub previous_amount_milli: i32,
    pub current_amount_milli: i32,
    pub steering_eligibility: VanillaTradeSteeringEligibilityInput,
    pub selected_route_id: i16,
    pub steering_modifier_milli: i32,
}

/// Candidate rows and route markers for the complete route-weight pass.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaTradeRouteWeightPassFromRowsInput {
    pub rows: Vec<VanillaTradeRouteWeightSourceRowInput>,
    pub marked_routes: Vec<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VanillaTradeRouteWeightResult {
    pub total_milli: i32,
    pub weights_milli: Vec<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VanillaTradeRouteWeightPassResult {
    pub route_contributions_milli: Vec<i32>,
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

/// Accumulates eligible country contributions and normalizes the route weights.
///
/// The compatibility layer supplies already-eligible records and resolved
/// amounts. A selected route past the route table maps to slot zero. Negative
/// route identifiers are rejected because their native behavior has not been
/// established.
pub fn calculate_vanilla_trade_route_weight_pass(
    input: VanillaTradeRouteWeightPassInput,
) -> Result<VanillaTradeRouteWeightPassResult, Error> {
    let route_count = input.marked_routes.len();
    let mut contributions_milli = vec![0_i32; route_count];

    for contribution in input.contributions {
        let route_id = usize::try_from(contribution.selected_route_id)
            .map_err(|_| Error::Invalid("trade route identifier must be nonnegative".into()))?;
        if route_count == 0 {
            return Err(Error::Invalid(
                "trade route contributions require at least one route".into(),
            ));
        }
        let route_index = if route_id >= route_count { 0 } else { route_id };
        let factor_milli = 1_000_i32.wrapping_add(contribution.steering_modifier_milli);
        let route_amount = ((i64::from(contribution.effective_amount_milli)
            * i64::from(factor_milli))
            / 1_000) as i32;
        contributions_milli[route_index] =
            contributions_milli[route_index].wrapping_add(route_amount);
    }

    let normalized = calculate_vanilla_trade_route_weights(VanillaTradeRouteWeightInput {
        contributions_milli: contributions_milli.clone(),
        marked_routes: input.marked_routes,
    })?;

    Ok(VanillaTradeRouteWeightPassResult {
        route_contributions_milli: contributions_milli,
        total_milli: normalized.total_milli,
        weights_milli: normalized.weights_milli,
    })
}

/// Selects eligible country rows before accumulating and normalizing weights.
///
/// The participation gate uses signed 32-bit wrapping for `balance + current -
/// previous`. Collection and cached route eligibility are resolved by the
/// same core predicate used by retention.
pub fn calculate_vanilla_trade_route_weight_pass_from_rows(
    input: VanillaTradeRouteWeightPassFromRowsInput,
) -> Result<VanillaTradeRouteWeightPassResult, Error> {
    let contributions = input
        .rows
        .into_iter()
        .filter(|row| {
            row.participation_balance_milli.wrapping_add(
                row.current_amount_milli
                    .wrapping_sub(row.previous_amount_milli),
            ) > 0
        })
        .filter(|row| is_vanilla_trade_steering_eligible(row.steering_eligibility))
        .map(|row| VanillaTradeRouteWeightContributionInput {
            selected_route_id: row.selected_route_id,
            effective_amount_milli: row.steering_eligibility.effective_amount_milli,
            steering_modifier_milli: row.steering_modifier_milli,
        })
        .collect();

    calculate_vanilla_trade_route_weight_pass(VanillaTradeRouteWeightPassInput {
        contributions,
        marked_routes: input.marked_routes,
    })
}

/// Inputs for the verified trade-node outflow stage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaTradeOutflowInput {
    pub has_downstream_routes: bool,
    pub route_weights_milli: Vec<i32>,
    pub total_effective_amount_milli: i32,
    /// Node value used by the native outflow gate.
    pub node_value_milli: i32,
    /// The local value field included in the outgoing value pool.
    pub base_outgoing_value_milli: i32,
    pub incoming_transfer_values_milli: Vec<i32>,
    pub retention_milli: i32,
    pub additional_outgoing_multiplier_milli: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VanillaTradeOutflowResult {
    pub applied: bool,
    pub value_pool_milli: Option<i32>,
    pub outgoing_amount_milli: i32,
    pub distribution_amount_milli: i32,
}

/// Calculates node outflow and the value supplied to route distribution.
///
/// Outflow runs only with downstream routes, empty weights or a positive
/// weight, and nonzero total effective amount or node value. The value pool
/// accumulates with 32-bit wrapping; retention is not clamped.
pub fn calculate_vanilla_trade_outflow(
    input: VanillaTradeOutflowInput,
) -> VanillaTradeOutflowResult {
    let has_positive_weight = input.route_weights_milli.iter().any(|weight| *weight > 0);
    let should_apply = input.has_downstream_routes
        && (input.route_weights_milli.is_empty() || has_positive_weight)
        && (input.total_effective_amount_milli != 0 || input.node_value_milli != 0);
    if !should_apply {
        return VanillaTradeOutflowResult {
            applied: false,
            value_pool_milli: None,
            outgoing_amount_milli: 0,
            distribution_amount_milli: 0,
        };
    }

    let value_pool = input
        .incoming_transfer_values_milli
        .into_iter()
        .fold(input.base_outgoing_value_milli, i32::wrapping_add);
    let retention_factor = 1_000_i32.wrapping_sub(input.retention_milli);
    let outgoing_amount = ((i64::from(retention_factor) * i64::from(value_pool)) / 1_000) as i32;
    let additional_amount = ((i64::from(input.additional_outgoing_multiplier_milli)
        * i64::from(value_pool))
        / 1_000) as i32;
    let distribution_amount = outgoing_amount.wrapping_add(additional_amount);

    VanillaTradeOutflowResult {
        applied: true,
        value_pool_milli: Some(value_pool),
        outgoing_amount_milli: outgoing_amount,
        distribution_amount_milli: distribution_amount,
    }
}

/// One outgoing route in a trade-node pass, in route declaration order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaTradeNodeRouteInput {
    pub route_id: i16,
    pub candidates: Vec<VanillaTradeDistributionCandidateInput>,
}

/// Inputs for a single native-order trade-node calculation pass.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaTradeNodePassInput {
    /// Node current value before this node's outgoing distribution.
    pub source_value_milli: i32,
    pub retention: VanillaTradeRetentionPassInput,
    /// Eligible country contributions and route markers for this node.
    pub route_weights: VanillaTradeRouteWeightPassInput,
    pub has_downstream_routes: bool,
    /// Node value operand used by the native outflow gate.
    pub node_value_milli: i32,
    pub base_outgoing_value_milli: i32,
    pub incoming_transfer_values_milli: Vec<i32>,
    pub additional_outgoing_multiplier_milli: i32,
    pub routes: Vec<VanillaTradeNodeRouteInput>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VanillaTradeNodePassResult {
    pub retention: VanillaTradeRetentionPassResult,
    pub route_weights: VanillaTradeRouteWeightPassResult,
    pub outflow: VanillaTradeOutflowResult,
    /// Route identifiers aligned by declaration order with `distribution.routes`.
    pub route_ids: Vec<i16>,
    pub distribution: VanillaTradeDistributionResult,
}

/// Node-pass inputs using row-level retention sources.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaTradeNodePassFromRetentionSourcesInput {
    pub source_value_milli: i32,
    pub retention_sources: VanillaTradeRetentionSourcesInput,
    pub route_weight_rows: VanillaTradeRouteWeightPassFromRowsInput,
    pub has_downstream_routes: bool,
    pub node_value_milli: i32,
    pub base_outgoing_value_milli: i32,
    pub incoming_transfer_values_milli: Vec<i32>,
    pub additional_outgoing_multiplier_milli: i32,
    pub routes: Vec<VanillaTradeNodeRouteInput>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VanillaTradeNodePassFromRetentionSourcesResult {
    pub retention: VanillaTradeRetentionSourcesResult,
    pub route_weights: VanillaTradeRouteWeightPassResult,
    pub outflow: VanillaTradeOutflowResult,
    pub route_ids: Vec<i16>,
    pub distribution: VanillaTradeDistributionResult,
}

struct VanillaTradeNodePassCalculation {
    source_value_milli: i32,
    total_effective_amount_milli: i32,
    retention_milli: i32,
    route_weights: VanillaTradeRouteWeightPassResult,
    has_downstream_routes: bool,
    node_value_milli: i32,
    base_outgoing_value_milli: i32,
    incoming_transfer_values_milli: Vec<i32>,
    additional_outgoing_multiplier_milli: i32,
    routes: Vec<VanillaTradeNodeRouteInput>,
}

struct VanillaTradeNodePassStages {
    route_weights: VanillaTradeRouteWeightPassResult,
    outflow: VanillaTradeOutflowResult,
    route_ids: Vec<i16>,
    distribution: VanillaTradeDistributionResult,
}

fn calculate_vanilla_trade_node_pass_stages(
    input: VanillaTradeNodePassCalculation,
) -> Result<VanillaTradeNodePassStages, Error> {
    if input.routes.len() != input.route_weights.weights_milli.len() {
        return Err(Error::Invalid(
            "trade node routes and route markers must have equal lengths".into(),
        ));
    }

    let route_weights = input.route_weights;
    let outflow = calculate_vanilla_trade_outflow(VanillaTradeOutflowInput {
        has_downstream_routes: input.has_downstream_routes,
        route_weights_milli: route_weights.weights_milli.clone(),
        total_effective_amount_milli: input.total_effective_amount_milli,
        node_value_milli: input.node_value_milli,
        base_outgoing_value_milli: input.base_outgoing_value_milli,
        incoming_transfer_values_milli: input.incoming_transfer_values_milli,
        retention_milli: input.retention_milli,
        additional_outgoing_multiplier_milli: input.additional_outgoing_multiplier_milli,
    });
    let route_ids = input.routes.iter().map(|route| route.route_id).collect();
    let routes = input
        .routes
        .into_iter()
        .zip(route_weights.weights_milli.iter().copied())
        .map(|(route, weight_milli)| VanillaTradeDistributionRouteInput {
            weight_milli,
            node_value_milli: outflow.distribution_amount_milli,
            candidates: select_vanilla_trade_distribution_candidates(
                route.route_id,
                route.candidates,
            ),
        })
        .collect();
    let distribution = calculate_vanilla_trade_distribution(VanillaTradeDistributionInput {
        source_value_milli: input.source_value_milli,
        source_debit_milli: outflow.outgoing_amount_milli,
        routes,
    });

    Ok(VanillaTradeNodePassStages {
        route_weights,
        outflow,
        route_ids,
        distribution,
    })
}

/// Runs the verified retention, route-weight, outflow, and distribution stages.
///
/// Record and route-state production stays in the compatibility layer. This
/// function preserves the native phase order and feeds each stage's result to
/// the next one without changing the input route declaration order.
pub fn calculate_vanilla_trade_node_pass(
    input: VanillaTradeNodePassInput,
) -> Result<VanillaTradeNodePassResult, Error> {
    let retention = calculate_vanilla_trade_retention_pass(input.retention);
    if input.routes.len() != input.route_weights.marked_routes.len() {
        return Err(Error::Invalid(
            "trade node routes and route markers must have equal lengths".into(),
        ));
    }
    let route_weights = calculate_vanilla_trade_route_weight_pass(input.route_weights)?;
    let stages = calculate_vanilla_trade_node_pass_stages(VanillaTradeNodePassCalculation {
        source_value_milli: input.source_value_milli,
        total_effective_amount_milli: retention.total_amount_milli,
        retention_milli: retention.retention.retention_milli,
        route_weights,
        has_downstream_routes: input.has_downstream_routes,
        node_value_milli: input.node_value_milli,
        base_outgoing_value_milli: input.base_outgoing_value_milli,
        incoming_transfer_values_milli: input.incoming_transfer_values_milli,
        additional_outgoing_multiplier_milli: input.additional_outgoing_multiplier_milli,
        routes: input.routes,
    })?;

    Ok(VanillaTradeNodePassResult {
        retention,
        route_weights: stages.route_weights,
        outflow: stages.outflow,
        route_ids: stages.route_ids,
        distribution: stages.distribution,
    })
}

/// Runs the trade-node stages with retention from normalized source rows.
pub fn calculate_vanilla_trade_node_pass_from_retention_sources(
    input: VanillaTradeNodePassFromRetentionSourcesInput,
) -> Result<VanillaTradeNodePassFromRetentionSourcesResult, Error> {
    let retention = calculate_vanilla_trade_retention_from_sources(input.retention_sources);
    if input.routes.len() != input.route_weight_rows.marked_routes.len() {
        return Err(Error::Invalid(
            "trade node routes and route markers must have equal lengths".into(),
        ));
    }
    let route_weights =
        calculate_vanilla_trade_route_weight_pass_from_rows(input.route_weight_rows)?;
    let stages = calculate_vanilla_trade_node_pass_stages(VanillaTradeNodePassCalculation {
        source_value_milli: input.source_value_milli,
        total_effective_amount_milli: retention.total_amount_milli,
        retention_milli: retention.retention.retention_milli,
        route_weights,
        has_downstream_routes: input.has_downstream_routes,
        node_value_milli: input.node_value_milli,
        base_outgoing_value_milli: input.base_outgoing_value_milli,
        incoming_transfer_values_milli: input.incoming_transfer_values_milli,
        additional_outgoing_multiplier_milli: input.additional_outgoing_multiplier_milli,
        routes: input.routes,
    })?;

    Ok(VanillaTradeNodePassFromRetentionSourcesResult {
        retention,
        route_weights: stages.route_weights,
        outflow: stages.outflow,
        route_ids: stages.route_ids,
        distribution: stages.distribution,
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

/// A normalized set of trade modifiers attached to one country.
///
/// The compatibility layer resolves each entry through the country's modifier
/// context before passing the values to the core.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaTradeAttachedModifierGroup {
    pub country_id: i16,
    pub modifiers_milli: Vec<i32>,
}

/// Inputs for the verified country and attached-source steering modifier.
///
/// When attached modifiers are active, only the first matching country group
/// contributes. The compatibility layer resolves source activation and the
/// modifier values; the core preserves the native accumulation order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaTradeSteeringModifierInput {
    pub country_id: i16,
    pub country_context_modifier_milli: i32,
    pub attached_modifiers_active: bool,
    pub attached_modifier_groups: Vec<VanillaTradeAttachedModifierGroup>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VanillaTradeSteeringModifierResult {
    pub country_context_modifier_milli: i32,
    pub attached_modifier_milli: i32,
    pub modifier_milli: i32,
}

/// Combines the verified country-context and attached-source modifiers.
///
/// Entries in the first matching country group are accumulated with 32-bit
/// wrapping. Disabled or unmatched attached sources contribute zero.
pub fn calculate_vanilla_trade_steering_modifier(
    input: VanillaTradeSteeringModifierInput,
) -> VanillaTradeSteeringModifierResult {
    let attached_modifier = if input.attached_modifiers_active {
        input
            .attached_modifier_groups
            .iter()
            .find(|group| group.country_id == input.country_id)
            .map(|group| {
                group
                    .modifiers_milli
                    .iter()
                    .copied()
                    .fold(0_i32, i32::wrapping_add)
            })
            .unwrap_or(0)
    } else {
        0
    };
    let modifier = input
        .country_context_modifier_milli
        .wrapping_add(attached_modifier);

    VanillaTradeSteeringModifierResult {
        country_context_modifier_milli: input.country_context_modifier_milli,
        attached_modifier_milli: attached_modifier,
        modifier_milli: modifier,
    }
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

/// Candidate state resolved for one outgoing trade route before eligibility filtering.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaTradeDistributionCandidateInput {
    pub country_id: i16,
    pub reference_valid: bool,
    pub country_flags: VanillaTradeCountryFlags,
    pub route_id: i16,
    pub adjusted_amount_milli: i32,
    pub unadjusted_amount_milli: i32,
    pub transfer_modifier_milli: i32,
    pub global_multiplier_milli: i32,
}

/// Selects the candidates eligible for one outgoing route.
///
/// The compatibility layer resolves record validity and country state. This
/// stage keeps valid `ae && ac` records for the requested route with a positive
/// global multiplier. Input order is retained so later stable sorting keeps
/// native tie order.
pub fn select_vanilla_trade_distribution_candidates(
    route_id: i16,
    candidates: Vec<VanillaTradeDistributionCandidateInput>,
) -> Vec<VanillaTradeDistributionCandidate> {
    candidates
        .into_iter()
        .filter(|candidate| {
            candidate.reference_valid
                && candidate.country_flags.ae
                && candidate.country_flags.ac
                && candidate.route_id == route_id
                && candidate.global_multiplier_milli > 0
        })
        .map(|candidate| VanillaTradeDistributionCandidate {
            country_id: candidate.country_id,
            adjusted_amount_milli: candidate.adjusted_amount_milli,
            unadjusted_amount_milli: candidate.unadjusted_amount_milli,
            transfer_modifier_milli: candidate.transfer_modifier_milli,
            global_multiplier_milli: candidate.global_multiplier_milli,
        })
        .collect()
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

#[cfg(test)]
mod tests {
    use super::{
        VanillaTradeIncomeInput, VanillaTradeIncomePassInput, calculate_vanilla_trade_income,
        calculate_vanilla_trade_income_pass,
    };

    #[test]
    fn trade_income_keeps_the_observed_two_stage_truncation() {
        // The capture gives the effective multiplier, not its source split.
        let input = VanillaTradeIncomeInput {
            node_value_milli: 4_748,
            country_record_value_milli: 306,
            base_context_modifier_milli: 70,
            selected_context_modifier_milli: 0,
            virtual_context_modifier_milli: None,
            minimum_context_modifier_milli: 0,
            maximum_context_modifier_milli: 2_000,
            post_context_modifier_milli: 0,
        };

        let result = calculate_vanilla_trade_income(input).unwrap();
        assert_eq!(result.stage_one_product, 1_452_888);
        assert_eq!(result.stage_one_milli, 1_452);
        assert_eq!(result.effective_multiplier_milli, 1_070);
        assert_eq!(result.stage_two_product, 1_553_640);
        assert_eq!(result.trade_income_milli, 1_553);

        let pass = calculate_vanilla_trade_income_pass(VanillaTradeIncomePassInput {
            eligible_records: vec![input],
        })
        .unwrap();
        assert_eq!(pass.total_income_milli, 1_553);
    }
}
