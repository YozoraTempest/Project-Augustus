// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

//! Caller-resolved province production value-stage queries.

use augustus_core::{
    VanillaProductionBaseValueResult, VanillaProductionInput, VanillaProductionInputState,
    VanillaTradeGoodsSizeResult, calculate_vanilla_trade_goods_size,
};
use serde::{Deserialize, Serialize};

use crate::{VanillaContentProductionGoodsSizeInput, VanillaProvinceProductionBaseValueInput};

/// Explicit inputs for one province's ordinary production value stage.
///
/// The loaded EU4 content supplies the current trade-good base price. The
/// caller supplies the already-resolved goods size and modifier stages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaProvinceProductionBaseValueQuery {
    pub province: u32,
    pub input: VanillaProvinceProductionBaseValueInput,
}

/// Read-only result of one province production value-stage query.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VanillaProvinceProductionBaseValueQueryReport {
    pub province: u32,
    pub evaluation: Option<VanillaProductionBaseValueResult>,
    pub unresolved: Vec<String>,
}

/// Explicit inputs for the full content-backed ordinary production binding
/// chain through goods-size, loaded trade-good price, and country efficiency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaProvinceContentProductionGoodsSizeQuery {
    pub province: u32,
    pub input: VanillaContentProductionGoodsSizeInput,
}

/// Read-only report for the content-backed ordinary production binding chain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VanillaProvinceContentProductionGoodsSizeQueryReport {
    pub province: u32,
    pub goods_size: VanillaTradeGoodsSizeResult,
    pub value: Option<VanillaProductionBaseValueResult>,
    pub production_input: VanillaProductionInputState,
    pub unresolved: Vec<String>,
}

pub(crate) struct VanillaContentProductionGoodsSizeResolution {
    pub goods_size: VanillaTradeGoodsSizeResult,
    pub value: Option<VanillaProductionBaseValueResult>,
    pub production_input: VanillaProductionInputState,
}

impl crate::VanillaWorld {
    /// Evaluates the ordinary production value stage using the loaded
    /// trade-good base price without changing production inputs or world state.
    ///
    /// A missing or unresolved trade-good price remains a null evaluation.
    /// The query does not calculate goods size, country efficiency, complete
    /// province modifiers, monthly settlement, or cashflow.
    pub fn query_vanilla_production_base_value(
        &self,
        query: &VanillaProvinceProductionBaseValueQuery,
    ) -> Result<VanillaProvinceProductionBaseValueQueryReport, crate::Error> {
        let evaluation = self
            .calculate_vanilla_province_production_base_value(query.province, query.input)
            .map_err(|error| crate::Error::caused_by("production value query", error))?;
        let unresolved = if evaluation.is_none() {
            vec!["production_value:missing_trade_good_price".into()]
        } else {
            Vec::new()
        };
        Ok(VanillaProvinceProductionBaseValueQueryReport {
            province: query.province,
            evaluation,
            unresolved,
        })
    }

    /// Resolves one content-backed production input without installing it in
    /// the monthly executor. The setter and preview query share this exact
    /// owner, technology, price, and unresolved-state boundary.
    pub(crate) fn resolve_vanilla_content_production_goods_size(
        &self,
        province: u32,
        source: VanillaContentProductionGoodsSizeInput,
    ) -> Result<VanillaContentProductionGoodsSizeResolution, augustus_core::Error> {
        let entity = self.initial.province_entity(province).ok_or_else(|| {
            augustus_core::Error::Invalid(format!("unknown EU4 production province {province}"))
        })?;
        let (country_modifier_milli, unresolved) =
            self.country_production_efficiency_milli(entity, province)?;
        let goods_size = calculate_vanilla_trade_goods_size(source.goods_size_input);
        let value = self.calculate_vanilla_province_production_base_value(
            province,
            VanillaProvinceProductionBaseValueInput {
                goods_size_milli: goods_size.quantity_milli,
                flat_modifier_milli: source.flat_modifier_milli,
                external_flat_modifier_milli: source.external_flat_modifier_milli,
                rate_modifier_milli: source.rate_modifier_milli,
                external_rate_modifier_milli: source.external_rate_modifier_milli,
            },
        )?;
        let production_input = match value {
            Some(value) => VanillaProductionInputState {
                input: Some(VanillaProductionInput {
                    base_value_milli: value.value_milli,
                    country_modifier_milli,
                    legacy_province_modifier_milli: source.legacy_province_modifier_milli,
                    queried_province_modifier_milli: source.queried_province_modifier_milli,
                }),
                unresolved,
            },
            None => {
                let mut unresolved = unresolved;
                unresolved.insert("trade_good_price:unknown".into());
                VanillaProductionInputState {
                    input: None,
                    unresolved,
                }
            }
        };
        Ok(VanillaContentProductionGoodsSizeResolution {
            goods_size,
            value,
            production_input,
        })
    }

    /// Previews the full content-backed ordinary production input chain. The
    /// query never installs monthly inputs or changes the world.
    pub fn query_vanilla_content_production_goods_size(
        &self,
        query: &VanillaProvinceContentProductionGoodsSizeQuery,
    ) -> Result<VanillaProvinceContentProductionGoodsSizeQueryReport, crate::Error> {
        let resolution = self
            .resolve_vanilla_content_production_goods_size(query.province, query.input)
            .map_err(|error| crate::Error::caused_by("content production query", error))?;
        let unresolved = resolution
            .production_input
            .unresolved
            .iter()
            .cloned()
            .collect();
        Ok(VanillaProvinceContentProductionGoodsSizeQueryReport {
            province: query.province,
            goods_size: resolution.goods_size,
            value: resolution.value,
            production_input: resolution.production_input,
            unresolved,
        })
    }
}
