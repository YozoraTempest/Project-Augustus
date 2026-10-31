// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{EntityId, EntityKind, Error, InitialWorld, World};

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildingDefinition {
    pub cost_milli: Option<i64>,
    pub construction_months: Option<u32>,
    #[serde(default, skip_serializing_if = "BuildingRole::is_standard")]
    pub role: BuildingRole,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fort_level_milli: Option<i64>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuildingRole {
    #[default]
    Standard,
    Fortification,
}

impl BuildingRole {
    fn is_standard(&self) -> bool {
        *self == Self::Standard
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TradeGoodDefinition {
    /// Runtime table index supplied by the compatibility layer.
    ///
    /// `None` means that the source does not expose a stable numeric index.
    /// The shared core keeps the value opaque so other rulesets can use their
    /// own tables without importing EU4-specific numbering.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_index: Option<u32>,
    pub base_price_milli: Option<i64>,
    pub gold_type: Option<bool>,
}

/// Runtime price inputs reconstructed by the EU4 compatibility layer.
///
/// EU4 stores price impacts as thousandths around a 1000 baseline. The
/// definition price remains separate from the dynamic impact list so a save
/// loader can rebuild the effective price table without changing content data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaTradeGoodPriceInput {
    pub base_price_milli: i32,
    pub price_impact_milli: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VanillaTradeGoodPriceResult {
    pub factor_milli: i32,
    pub effective_price_milli: i32,
}

/// Inputs for rebuilding one runtime trade-good price from ordered price
/// impact entries. The compatibility layer resolves the entries; the core
/// preserves their 32-bit accumulation and the native price stage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaTradeGoodPriceTableInput {
    pub base_price_milli: i32,
    pub price_impacts_milli: Vec<i32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct VanillaTradeGoodPriceTableResult {
    pub combined_impact_milli: i32,
    pub factor_milli: i32,
    pub effective_price_milli: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TradeProductionInput {
    pub province_base_quantity_milli: i32,
    pub country_base_quantity_milli: i32,
    pub external_base_quantity_milli: i32,
    pub province_quantity_modifier_milli: i32,
    pub country_quantity_modifier_milli: i32,
    pub external_quantity_modifier_milli: i32,
    pub trade_good_price_milli: i32,
    pub province_fixed_value_milli: i32,
    pub external_fixed_value_milli: i32,
    pub province_value_modifier_milli: i32,
    pub external_value_modifier_milli: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TradeProductionResult {
    pub quantity_milli: i32,
    pub value_milli: i32,
}

/// Inputs for the vanilla trade-goods-size calculation used by ordinary
/// production. The compatibility layer resolves the six source values; this
/// operator owns their merge, lower bound, and truncation order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaTradeGoodsSizeInput {
    pub province_base_milli: i32,
    pub trade_good_base_milli: i32,
    pub external_base_milli: i32,
    pub province_modifier_milli: i32,
    pub trade_good_modifier_milli: i32,
    pub external_modifier_milli: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct VanillaTradeGoodsSizeResult {
    pub combined_base_milli: i32,
    pub combined_modifier_milli: i32,
    pub quantity_milli: i32,
}

/// Inputs for the verified EU4 gold-province base stage (`A11B80`). The
/// compatibility layer resolves the three province fields and the selected
/// global base; the core owns the max/min and fixed-point order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaGoldBaseInput {
    pub province_field_2df0: i32,
    pub province_field_428: i32,
    pub province_field_2df4: i32,
    pub global_base_milli: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct VanillaGoldBaseResult {
    pub selected_province_field_milli: i32,
    pub bounded_province_field_milli: i32,
    pub reduction_milli: i32,
    pub base_value_milli: i32,
}

/// Inputs for the native `a13fd0`-equivalent value stage. The first stage's
/// output is supplied by `calculate_vanilla_trade_goods_size`; flat and rate
/// contributions are kept separate because the original code adds the former
/// before applying the latter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaProductionBaseValueInput {
    pub goods_size_milli: i32,
    pub trade_good_price_milli: i32,
    pub flat_modifier_milli: i32,
    pub external_flat_modifier_milli: i32,
    pub rate_modifier_milli: i32,
    pub external_rate_modifier_milli: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct VanillaProductionBaseValueResult {
    pub priced_value_milli: i32,
    pub flat_value_milli: i32,
    pub rate_factor_milli: i32,
    pub value_milli: i32,
}

/// Inputs for the verified gold-production value stage. The compatibility
/// layer resolves the native gold quantity, mine base and any additional gold
/// quantity; the core preserves the observed multiply-and-divide order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaGoldProductionInput {
    pub gold_size_milli: i32,
    pub mine_base_milli: i32,
    pub external_gold_size_milli: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct VanillaGoldProductionResult {
    pub combined_size_milli: i32,
    pub value_milli: i32,
    pub monthly_value_milli: i32,
}

/// Explicit gold-production input state for one EU4 province. The
/// compatibility layer owns the upstream quantity and mine-base producers;
/// unresolved reasons prevent the country total from being treated as final.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaGoldProductionInputState {
    pub input: Option<VanillaGoldProductionInput>,
    pub unresolved: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaGoldProductionProvinceResult {
    pub province_id: u32,
    pub value_milli: Option<i64>,
    pub monthly_value_milli: Option<i64>,
    pub unresolved: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaGoldProductionCountryResult {
    pub known_value_milli: i64,
    pub known_monthly_value_milli: i64,
    pub value_milli: Option<i64>,
    pub monthly_value_milli: Option<i64>,
    pub provinces: Vec<VanillaGoldProductionProvinceResult>,
    pub unresolved: BTreeSet<String>,
}

/// Rebuilds the effective EU4 trade-good price from its content price and the
/// summed runtime price-impact entries. The native writer uses signed 32-bit
/// accumulation, a lower factor bound of 10, and truncation toward zero.
pub fn calculate_vanilla_trade_good_price(
    input: VanillaTradeGoodPriceInput,
) -> VanillaTradeGoodPriceResult {
    let factor_milli = 1_000_i32.wrapping_add(input.price_impact_milli).max(10);
    let effective_price_milli =
        ((i64::from(factor_milli) * i64::from(input.base_price_milli)) / 1_000) as i32;

    VanillaTradeGoodPriceResult {
        factor_milli,
        effective_price_milli,
    }
}

/// Rebuilds a runtime trade-good price after aggregating all resolved impact
/// entries. The impact sum uses signed 32-bit wrapping before the factor lower
/// bound and price multiplication are applied.
pub fn calculate_vanilla_trade_good_price_table(
    input: VanillaTradeGoodPriceTableInput,
) -> VanillaTradeGoodPriceTableResult {
    let combined_impact = input
        .price_impacts_milli
        .into_iter()
        .fold(0_i32, i32::wrapping_add);
    let price = calculate_vanilla_trade_good_price(VanillaTradeGoodPriceInput {
        base_price_milli: input.base_price_milli,
        price_impact_milli: combined_impact,
    });
    VanillaTradeGoodPriceTableResult {
        combined_impact_milli: combined_impact,
        factor_milli: price.factor_milli,
        effective_price_milli: price.effective_price_milli,
    }
}

/// Inputs for the observed country-modifier refresh stage. The compatibility
/// layer resolves the two source values and the scale supplied by the source
/// definition; the core owns the effective-value clamp and fixed-point order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaDerivedModifierInput {
    pub base_value_milli: i32,
    pub context_modifier_milli: i32,
    pub scale_milli: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VanillaDerivedModifierResult {
    pub effective_value_milli: i32,
    pub derived_value_milli: i32,
}

/// Explicit contributions to the observed country-modifier aggregation stage.
/// Source selection stays in the compatibility layer; the core only performs
/// the ordered 32-bit accumulation of contributions that have already been
/// resolved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaModifierAggregationInput {
    pub initial_value_milli: i32,
    pub contributions_milli: Vec<i32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VanillaModifierAggregationResult {
    pub value_milli: i32,
}

/// Reproduces the ordinary vanilla trade-goods-size stage.
///
/// The native implementation combines the three base terms and three
/// modifier terms in 32-bit values, clamps the combined modifier at -1000,
/// multiplies by the resulting factor, and truncates toward zero. The
/// quotient is then cast to a signed 32-bit value before its sign is checked;
/// negative produced quantities are replaced with zero after that cast.
pub fn calculate_vanilla_trade_goods_size(
    input: VanillaTradeGoodsSizeInput,
) -> VanillaTradeGoodsSizeResult {
    let combined_base = input
        .province_base_milli
        .wrapping_add(input.trade_good_base_milli)
        .wrapping_add(input.external_base_milli);
    let combined_modifier = input
        .province_modifier_milli
        .wrapping_add(input.trade_good_modifier_milli)
        .wrapping_add(input.external_modifier_milli);
    let bounded_modifier = combined_modifier.max(-1000);
    let raw_quantity = ((i64::from(bounded_modifier) + 1000) * i64::from(combined_base)) / 1000;
    let quantity = (raw_quantity as i32).max(0);

    VanillaTradeGoodsSizeResult {
        combined_base_milli: combined_base,
        combined_modifier_milli: combined_modifier,
        quantity_milli: quantity,
    }
}

/// Reproduces the verified gold-province base stage used before the gold
/// quantity is multiplied into the monthly ledger value. Division truncates
/// toward zero, matching the native signed integer operations.
pub fn calculate_vanilla_gold_base(input: VanillaGoldBaseInput) -> VanillaGoldBaseResult {
    let selected = input.province_field_2df0.max(input.province_field_428);
    let bounded = selected.min(input.province_field_2df4);
    let reduction = ((i64::from(bounded) * 1_000) / 100_000) as i32;
    let base =
        (((1_000_i64 - i64::from(reduction)) * i64::from(input.global_base_milli)) / 1_000) as i32;

    VanillaGoldBaseResult {
        selected_province_field_milli: selected,
        bounded_province_field_milli: bounded,
        reduction_milli: reduction,
        base_value_milli: base,
    }
}

/// Reproduces the native ordinary production value stage after quantity
/// calculation. Each division truncates toward zero. The price product is
/// cast to a signed 32-bit value and negative results are replaced with zero
/// before fixed modifiers are added; the final rate factor is clamped to the
/// observed `[10, 2000]` range, while the final value itself is not clamped.
pub fn calculate_vanilla_production_base_value(
    input: VanillaProductionBaseValueInput,
) -> VanillaProductionBaseValueResult {
    let priced =
        (i64::from(input.goods_size_milli) * i64::from(input.trade_good_price_milli)) / 1000;
    let priced_value = (priced as i32).max(0);
    let flat_value = priced_value
        .wrapping_add(input.flat_modifier_milli)
        .wrapping_add(input.external_flat_modifier_milli);
    let rate_factor = 1000_i32
        .wrapping_add(input.rate_modifier_milli)
        .wrapping_add(input.external_rate_modifier_milli)
        .clamp(10, 2000);
    let value = ((i64::from(flat_value) * i64::from(rate_factor)) / 1000) as i32;

    VanillaProductionBaseValueResult {
        priced_value_milli: priced_value,
        flat_value_milli: flat_value,
        rate_factor_milli: rate_factor,
        value_milli: value,
    }
}

/// Reproduces the confirmed gold path after its upstream values have been
/// resolved. The monthly ledger uses truncation toward zero after division by
/// twelve; no ordinary trade-good price is involved.
pub fn calculate_vanilla_gold_production(
    input: VanillaGoldProductionInput,
) -> VanillaGoldProductionResult {
    let combined_size = input
        .gold_size_milli
        .wrapping_add(input.external_gold_size_milli);
    let value = ((i64::from(combined_size) * i64::from(input.mine_base_milli)) / 1_000) as i32;
    let monthly_value = (i64::from(value) / 12) as i32;
    VanillaGoldProductionResult {
        combined_size_milli: combined_size,
        value_milli: value,
        monthly_value_milli: monthly_value,
    }
}

/// Aggregates the confirmed gold-production path over a country's currently
/// owned provinces whose loaded trade-good definition explicitly has
/// `gold_type = yes`. Ordinary provinces are ignored; a gold province with a
/// missing or unresolved input keeps the country total partial and marked
/// unresolved.
pub fn calculate_vanilla_gold_production_for_country(
    initial: &InitialWorld,
    country: EntityId,
    inputs: &BTreeMap<u32, VanillaGoldProductionInputState>,
) -> Result<VanillaGoldProductionCountryResult, Error> {
    let owned = initial.world().owned_provinces(country)?;
    let entity_to_province: BTreeMap<_, _> = initial
        .provinces()
        .iter()
        .filter_map(|province| {
            initial
                .province_entity(province.id)
                .map(|entity| (entity, province.id))
        })
        .collect();
    let economy = initial.world().economy();
    let mut known_value = 0_i64;
    let mut known_monthly = 0_i64;
    let mut unresolved = BTreeSet::new();
    let mut provinces = Vec::new();

    for &entity in owned {
        let province_id = entity_to_province
            .get(&entity)
            .copied()
            .ok_or_else(|| Error::Invalid("owned entity is not a province".into()))?;
        let Some(province_economy) = economy.provinces.get(&entity) else {
            continue;
        };
        let Some(trade_good) = province_economy.trade_good.as_deref() else {
            continue;
        };
        let Some(definition) = economy.trade_goods.get(trade_good) else {
            let reason = "unknown_gold_trade_good_definition".to_string();
            unresolved.insert(format!("province:{province_id}:{reason}"));
            provinces.push(VanillaGoldProductionProvinceResult {
                province_id,
                value_milli: None,
                monthly_value_milli: None,
                unresolved: [reason].into_iter().collect(),
            });
            continue;
        };
        if definition.gold_type != Some(true) {
            continue;
        }

        let mut province_unresolved = BTreeSet::new();
        let result = match inputs.get(&province_id) {
            Some(state) if state.unresolved.is_empty() => state
                .input
                .map(calculate_vanilla_gold_production)
                .ok_or_else(|| "missing_gold_production_input".to_string())
                .map(Some)
                .unwrap_or_else(|reason| {
                    province_unresolved.insert(reason);
                    None
                }),
            Some(state) => {
                province_unresolved.extend(
                    state
                        .unresolved
                        .iter()
                        .filter(|reason| !reason.is_empty())
                        .cloned(),
                );
                if province_unresolved.is_empty() {
                    province_unresolved.insert("unknown_gold_production_input".into());
                }
                None
            }
            None => {
                province_unresolved.insert("missing_gold_production_input".into());
                None
            }
        };
        let (value, monthly) = result
            .map(|value| {
                (
                    Some(i64::from(value.value_milli)),
                    Some(i64::from(value.monthly_value_milli)),
                )
            })
            .unwrap_or((None, None));
        if let Some(value) = value {
            known_value = known_value.checked_add(value).ok_or(Error::Overflow)?;
        }
        if let Some(monthly) = monthly {
            known_monthly = known_monthly.checked_add(monthly).ok_or(Error::Overflow)?;
        }
        for reason in &province_unresolved {
            unresolved.insert(format!("province:{province_id}:{reason}"));
        }
        provinces.push(VanillaGoldProductionProvinceResult {
            province_id,
            value_milli: value,
            monthly_value_milli: monthly,
            unresolved: province_unresolved,
        });
    }

    Ok(VanillaGoldProductionCountryResult {
        known_value_milli: known_value,
        known_monthly_value_milli: known_monthly,
        value_milli: unresolved.is_empty().then_some(known_value),
        monthly_value_milli: unresolved.is_empty().then_some(known_monthly),
        provinces,
        unresolved,
    })
}

/// Calculates the derived country-modifier value observed after the source
/// context has been aggregated. The effective value is clamped before the
/// scale is applied, and the final division truncates toward zero.
pub fn calculate_vanilla_derived_modifier(
    input: VanillaDerivedModifierInput,
) -> VanillaDerivedModifierResult {
    let effective_value = (i64::from(input.base_value_milli)
        + i64::from(input.context_modifier_milli))
    .clamp(10, 2_000);
    let derived_value = (effective_value * i64::from(input.scale_milli)) / 1_000;

    VanillaDerivedModifierResult {
        effective_value_milli: effective_value as i32,
        derived_value_milli: derived_value as i32,
    }
}

/// Aggregates already-resolved modifier contributions in declaration order.
/// Each addition follows the native 32-bit signed storage behavior.
pub fn aggregate_vanilla_modifier_contributions(
    input: VanillaModifierAggregationInput,
) -> VanillaModifierAggregationResult {
    let value = input
        .contributions_milli
        .into_iter()
        .fold(input.initial_value_milli, i32::wrapping_add);
    VanillaModifierAggregationResult { value_milli: value }
}

/// Inputs for the verified ordinary EU4 production-income path.
///
/// The inputs are deliberately explicit: their upstream producers (technology,
/// country modifiers and province state) are resolved by the compatibility layer.
/// `base_value_milli` is the output of the native value stage above; this
/// function only owns the later country/province arithmetic and truncation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaProductionInput {
    /// The price-adjusted province value before country and province modifiers.
    pub base_value_milli: i32,
    /// Country-side production efficiency in thousandths.
    pub country_modifier_milli: i32,
    /// Legacy province production slot in thousandths.
    pub legacy_province_modifier_milli: i32,
    /// Province modifier queried from the live modifier container.
    pub queried_province_modifier_milli: i32,
}

/// Explicit inputs supplied by a compatibility layer for one province.
/// Native container keys are intentionally not interpreted here because the
/// same numeric key can have different meanings in different contexts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaProductionInputState {
    pub input: Option<VanillaProductionInput>,
    pub unresolved: BTreeSet<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VanillaProductionResult {
    pub pre_query_value_milli: i32,
    pub annual_value_milli: i32,
    pub monthly_value_milli: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaProductionProvinceResult {
    pub province_id: u32,
    pub pre_query_value_milli: Option<i64>,
    pub annual_value_milli: Option<i64>,
    pub monthly_value_milli: Option<i64>,
    pub unresolved: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaProductionCountryResult {
    pub known_annual_value_milli: i64,
    pub known_monthly_value_milli: i64,
    pub annual_value_milli: Option<i64>,
    pub monthly_value_milli: Option<i64>,
    pub provinces: Vec<VanillaProductionProvinceResult>,
    pub unresolved: BTreeSet<String>,
}

/// Calculates ordinary production income using EU4's observed order.
///
/// Each division truncates toward zero. The first stage combines the country
/// modifier and legacy province slot; the second applies the queried province
/// modifier; the annual result is then divided by twelve.
pub fn calculate_vanilla_production(input: VanillaProductionInput) -> VanillaProductionResult {
    let first_factor = 1_000_i64
        + i64::from(input.country_modifier_milli)
        + i64::from(input.legacy_province_modifier_milli);
    let pre_query = (i64::from(input.base_value_milli) * first_factor) / 1_000;
    let query_factor = 1_000_i64 + i64::from(input.queried_province_modifier_milli);
    let annual = (pre_query * query_factor) / 1_000;

    VanillaProductionResult {
        pre_query_value_milli: pre_query as i32,
        annual_value_milli: annual as i32,
        monthly_value_milli: (annual / 12) as i32,
    }
}

/// Aggregates the verified ordinary-production formula over a country's live
/// owned provinces. Inputs remain explicit because their native producers and
/// refresh timing are compatibility-layer concerns.
pub fn calculate_vanilla_production_for_country(
    initial: &InitialWorld,
    country: EntityId,
    inputs: &BTreeMap<u32, VanillaProductionInputState>,
) -> Result<VanillaProductionCountryResult, Error> {
    let owned = initial.world().owned_provinces(country)?;
    let entity_to_province: BTreeMap<_, _> = initial
        .provinces()
        .iter()
        .filter_map(|province| {
            initial
                .province_entity(province.id)
                .map(|entity| (entity, province.id))
        })
        .collect();
    let mut known_annual = 0_i64;
    let mut known_monthly = 0_i64;
    let mut unresolved = BTreeSet::new();
    let mut provinces = Vec::with_capacity(owned.len());

    for &entity in owned {
        let province_id = entity_to_province
            .get(&entity)
            .copied()
            .ok_or_else(|| Error::Invalid("owned entity is not a province".into()))?;
        let mut province_unresolved = BTreeSet::new();
        let result = match inputs.get(&province_id) {
            Some(state) if state.unresolved.is_empty() => state
                .input
                .map(calculate_vanilla_production)
                .ok_or_else(|| "missing_production_input".to_string())
                .map(Some)
                .unwrap_or_else(|reason| {
                    province_unresolved.insert(reason);
                    None
                }),
            Some(state) => {
                province_unresolved.extend(
                    state
                        .unresolved
                        .iter()
                        .filter(|reason| !reason.is_empty())
                        .cloned(),
                );
                if province_unresolved.is_empty() {
                    province_unresolved.insert("unknown_production_input".into());
                }
                None
            }
            None => {
                province_unresolved.insert("missing_production_input".into());
                None
            }
        };
        let (pre_query, annual, monthly) = result
            .map(|value| {
                (
                    Some(i64::from(value.pre_query_value_milli)),
                    Some(i64::from(value.annual_value_milli)),
                    Some(i64::from(value.monthly_value_milli)),
                )
            })
            .unwrap_or((None, None, None));
        if let Some(value) = annual {
            known_annual = known_annual.checked_add(value).ok_or(Error::Overflow)?;
        }
        if let Some(value) = monthly {
            known_monthly = known_monthly.checked_add(value).ok_or(Error::Overflow)?;
        }
        for reason in &province_unresolved {
            unresolved.insert(format!("province:{province_id}:{reason}"));
        }
        provinces.push(VanillaProductionProvinceResult {
            province_id,
            pre_query_value_milli: pre_query,
            annual_value_milli: annual,
            monthly_value_milli: monthly,
            unresolved: province_unresolved,
        });
    }

    Ok(VanillaProductionCountryResult {
        known_annual_value_milli: known_annual,
        known_monthly_value_milli: known_monthly,
        annual_value_milli: unresolved.is_empty().then_some(known_annual),
        monthly_value_milli: unresolved.is_empty().then_some(known_monthly),
        provinces,
        unresolved,
    })
}

/// Calculates produced quantity and its trade value from already-resolved inputs.
/// Modifier and base-value sums wrap at 32 bits; each scaling step truncates toward zero.
pub fn calculate_trade_production(input: TradeProductionInput) -> TradeProductionResult {
    let base_quantity = input
        .province_base_quantity_milli
        .wrapping_add(input.country_base_quantity_milli)
        .wrapping_add(input.external_base_quantity_milli);
    let quantity_modifier = input
        .province_quantity_modifier_milli
        .wrapping_add(input.country_quantity_modifier_milli)
        .wrapping_add(input.external_quantity_modifier_milli)
        .max(-1000);
    let quantity_factor = i64::from(1000_i32.wrapping_add(quantity_modifier));
    let quantity = ((i64::from(base_quantity) * quantity_factor) / 1000) as i32;
    let quantity = quantity.max(0);

    let produced_value =
        ((i64::from(quantity) * i64::from(input.trade_good_price_milli)) / 1000) as i32;
    let base_value = produced_value
        .max(0)
        .wrapping_add(input.province_fixed_value_milli)
        .wrapping_add(input.external_fixed_value_milli);
    let value_factor = 1000_i32
        .wrapping_add(input.province_value_modifier_milli)
        .wrapping_add(input.external_value_modifier_milli)
        .clamp(10, 2000);
    let value = ((i64::from(base_value) * i64::from(value_factor)) / 1000) as i32;

    TradeProductionResult {
        quantity_milli: quantity,
        value_milli: value,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TradeNode {
    pub location: EntityId,
    pub members: BTreeSet<EntityId>,
    pub outgoing: BTreeSet<String>,
    pub inland: Option<bool>,
    pub end: Option<bool>,
}

/// Runtime values saved for one trade node. Missing operands remain unknown.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TradeNodeRuntimeState {
    pub current_milli: Option<i64>,
    pub local_value_milli: Option<i64>,
    pub outgoing_milli: Option<i64>,
    pub value_added_outgoing_milli: Option<i64>,
    pub retention_milli: Option<i64>,
    pub p_pow_milli: Option<i64>,
    pub max_milli: Option<i64>,
    pub collector_power_milli: Option<i64>,
    pub collector_power_including_pirates_milli: Option<i64>,
    pub pull_power_milli: Option<i64>,
    pub retain_power_milli: Option<i64>,
    pub highest_power_milli: Option<i64>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProvinceEconomy {
    pub buildings: BTreeSet<String>,
    pub trade_good: Option<String>,
}

/// Shared economic inputs and current province state, without input syntax or paths.
/// Prices are definition values; no production or trade settlement is implied.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Economy {
    pub buildings: BTreeMap<String, BuildingDefinition>,
    pub trade_goods: BTreeMap<String, TradeGoodDefinition>,
    /// Runtime market prices keyed by the content trade-good identifier.
    /// `None` is an explicitly unresolved price; an absent entry means that
    /// only the content-defined base price is available.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub current_trade_good_prices_milli: BTreeMap<String, Option<i64>>,
    /// Unkeyed price-impact chains supplied by the compatibility layer.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub external_trade_good_price_impacts_milli: BTreeMap<String, Vec<i32>>,
    /// Persistent script price impacts, grouped by trade good and effect key.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub keyed_trade_good_price_impacts_milli: BTreeMap<String, BTreeMap<String, i32>>,
    /// Unsupported saved impacts prevent monthly refresh from replacing the
    /// saved current price with a partial impact total.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub unresolved_trade_good_price_impacts_milli: BTreeMap<String, BTreeSet<String>>,
    pub trade_nodes: BTreeMap<String, TradeNode>,
    pub provinces: BTreeMap<EntityId, ProvinceEconomy>,
}

impl Economy {
    pub fn is_empty(&self) -> bool {
        self.buildings.is_empty()
            && self.trade_goods.is_empty()
            && self.current_trade_good_prices_milli.is_empty()
            && self.external_trade_good_price_impacts_milli.is_empty()
            && self.keyed_trade_good_price_impacts_milli.is_empty()
            && self.unresolved_trade_good_price_impacts_milli.is_empty()
            && self.trade_nodes.is_empty()
            && self.provinces.is_empty()
    }

    pub fn province_node(&self, province: EntityId) -> Option<&str> {
        self.trade_nodes
            .iter()
            .find_map(|(name, node)| node.members.contains(&province).then_some(name.as_str()))
    }

    /// Finds a trade good by a compatibility-layer runtime index.
    pub fn trade_good_by_runtime_index(&self, index: u32) -> Option<(&str, &TradeGoodDefinition)> {
        self.trade_goods.iter().find_map(|(name, definition)| {
            (definition.runtime_index == Some(index)).then_some((name.as_str(), definition))
        })
    }

    /// Returns the current price state for a known trade good. The outer
    /// `None` means the identifier is unknown; the inner `None` means the
    /// identifier is known but its runtime price is unresolved.
    pub fn current_trade_good_price_milli(&self, name: &str) -> Option<Option<i64>> {
        let definition = self.trade_goods.get(name)?;
        Some(
            self.current_trade_good_prices_milli
                .get(name)
                .copied()
                .unwrap_or(definition.base_price_milli),
        )
    }

    pub(crate) fn refreshed_trade_good_prices(
        &self,
    ) -> Result<BTreeMap<String, Option<i64>>, Error> {
        let mut refreshed = BTreeMap::new();
        for (name, external_impacts) in &self.external_trade_good_price_impacts_milli {
            if self
                .unresolved_trade_good_price_impacts_milli
                .contains_key(name)
            {
                continue;
            }
            let Some(definition) = self.trade_goods.get(name) else {
                return Err(Error::Invalid("unknown trade good in price impacts".into()));
            };
            let Some(base_price) = definition.base_price_milli else {
                continue;
            };
            let base_price = i32::try_from(base_price).map_err(|_| {
                Error::Invalid("trade-good base price exceeds runtime range".into())
            })?;
            let mut impacts = external_impacts.clone();
            if let Some(keyed) = self.keyed_trade_good_price_impacts_milli.get(name) {
                impacts.extend(keyed.values().copied());
            }
            if !impacts.is_empty() {
                let price =
                    calculate_vanilla_trade_good_price_table(VanillaTradeGoodPriceTableInput {
                        base_price_milli: base_price,
                        price_impacts_milli: impacts,
                    });
                refreshed.insert(name.clone(), Some(i64::from(price.effective_price_milli)));
            }
        }
        for (name, keyed) in &self.keyed_trade_good_price_impacts_milli {
            if self
                .unresolved_trade_good_price_impacts_milli
                .contains_key(name)
            {
                continue;
            }
            if self
                .external_trade_good_price_impacts_milli
                .contains_key(name)
            {
                continue;
            }
            let Some(definition) = self.trade_goods.get(name) else {
                return Err(Error::Invalid("unknown trade good in price impacts".into()));
            };
            let Some(base_price) = definition.base_price_milli else {
                continue;
            };
            let base_price = i32::try_from(base_price).map_err(|_| {
                Error::Invalid("trade-good base price exceeds runtime range".into())
            })?;
            let price = calculate_vanilla_trade_good_price_table(VanillaTradeGoodPriceTableInput {
                base_price_milli: base_price,
                price_impacts_milli: keyed.values().copied().collect(),
            });
            refreshed.insert(name.clone(), Some(i64::from(price.effective_price_milli)));
        }
        Ok(refreshed)
    }

    pub(crate) fn validate(&self, world: &World) -> Result<(), Error> {
        let fail = |message: &str| Error::Invalid(message.into());
        let province = |id| -> Result<(), Error> {
            if world.entity(id)?.kind != EntityKind::Province {
                return Err(fail("economy requires a province entity"));
            }
            Ok(())
        };
        if self
            .buildings
            .keys()
            .chain(self.trade_goods.keys())
            .chain(self.trade_nodes.keys())
            .any(|name| name.is_empty())
        {
            return Err(fail("empty economic definition identifier"));
        }
        if self
            .current_trade_good_prices_milli
            .iter()
            .any(|(name, price)| {
                !self.trade_goods.contains_key(name) || price.is_some_and(|value| value < 0)
            })
        {
            return Err(fail("invalid current trade-good price"));
        }
        if self
            .external_trade_good_price_impacts_milli
            .keys()
            .any(|name| !self.trade_goods.contains_key(name))
            || self
                .keyed_trade_good_price_impacts_milli
                .iter()
                .any(|(name, keyed)| {
                    !self.trade_goods.contains_key(name) || keyed.keys().any(String::is_empty)
                })
            || self
                .unresolved_trade_good_price_impacts_milli
                .iter()
                .any(|(name, keys)| {
                    !self.trade_goods.contains_key(name) || keys.iter().any(String::is_empty)
                })
        {
            return Err(fail("invalid trade-good price impacts"));
        }
        let mut runtime_indices = BTreeSet::new();
        for definition in self.trade_goods.values() {
            if let Some(index) = definition.runtime_index
                && !runtime_indices.insert(index)
            {
                return Err(fail("duplicate trade-good runtime index"));
            }
        }
        if self.buildings.values().any(|b| {
            b.cost_milli.is_some_and(|v| v < 0)
                || b.construction_months == Some(0)
                || b.fort_level_milli.is_some_and(|v| v < 0)
        }) || self
            .trade_goods
            .values()
            .any(|g| g.base_price_milli.is_some_and(|v| v < 0))
        {
            return Err(fail("invalid economic definition value"));
        }
        for (&id, state) in &self.provinces {
            province(id)?;
            if state
                .buildings
                .iter()
                .any(|b| !self.buildings.contains_key(b))
                || state
                    .trade_good
                    .as_ref()
                    .is_some_and(|g| !self.trade_goods.contains_key(g))
            {
                return Err(fail("unknown province economic definition"));
            }
        }
        let mut members = BTreeSet::new();
        for node in self.trade_nodes.values() {
            province(node.location)?;
            if node.end == Some(true) && !node.outgoing.is_empty() {
                return Err(fail("terminal trade node has outgoing routes"));
            }
            for &id in &node.members {
                province(id)?;
                if !members.insert(id) {
                    return Err(fail("province belongs to multiple trade nodes"));
                }
            }
            for target in &node.outgoing {
                if !self.trade_nodes.contains_key(target) {
                    return Err(fail("unknown trade route destination"));
                }
            }
        }
        Ok(())
    }
}

impl World {
    pub(crate) fn set_trade_good_price_impact(
        &mut self,
        trade_good: &str,
        key: &str,
        value_milli: i32,
    ) -> Result<(), Error> {
        self.require_running()?;
        if key.is_empty() || !self.economy().trade_goods.contains_key(trade_good) {
            return Err(Error::Invalid("invalid trade-good price impact".into()));
        }
        let mut economy = self.economy().clone();
        economy
            .keyed_trade_good_price_impacts_milli
            .entry(trade_good.into())
            .or_default()
            .insert(key.into(), value_milli);
        self.set_economy(economy)
    }

    pub(crate) fn set_current_trade_good_prices(
        &mut self,
        prices: &BTreeMap<String, Option<i64>>,
    ) -> Result<(), Error> {
        if prices.is_empty() {
            return Ok(());
        }
        let mut economy = self.economy().clone();
        economy
            .current_trade_good_prices_milli
            .extend(prices.clone());
        self.set_economy(economy)
    }
}
