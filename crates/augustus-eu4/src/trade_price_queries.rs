// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

//! Caller-resolved ordered trade-good price-table previews.

use augustus_core::{
    VanillaTradeGoodPriceTableInput, VanillaTradeGoodPriceTableResult,
    calculate_vanilla_trade_good_price_table,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaTradeGoodPriceTableQuery {
    pub trade_good: String,
    pub price_impacts_milli: Vec<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VanillaTradeGoodPriceTableQueryReport {
    pub trade_good: String,
    pub base_price_milli: Option<i64>,
    pub evaluation: Option<VanillaTradeGoodPriceTableResult>,
    pub unresolved: Vec<String>,
}

impl crate::VanillaWorld {
    /// Rebuilds one explicitly ordered price-impact chain from the loaded
    /// content base price. Persisted keyed impacts and unresolved runtime maps
    /// remain outside this caller-resolved query contract.
    pub fn query_vanilla_trade_good_price_table(
        &self,
        query: &VanillaTradeGoodPriceTableQuery,
    ) -> Result<VanillaTradeGoodPriceTableQueryReport, crate::Error> {
        let definition = self
            .initial
            .world()
            .economy()
            .trade_goods
            .get(&query.trade_good)
            .ok_or_else(|| {
                crate::Error::message(
                    "trade-good price table query",
                    format!("unknown trade good {}", query.trade_good),
                )
            })?;
        let Some(base_price_milli) = definition.base_price_milli else {
            return Ok(VanillaTradeGoodPriceTableQueryReport {
                trade_good: query.trade_good.clone(),
                base_price_milli: None,
                evaluation: None,
                unresolved: vec!["trade_good_price:missing_base_price".into()],
            });
        };
        let base_price = i32::try_from(base_price_milli).map_err(|_| {
            crate::Error::message(
                "trade-good price table query",
                format!(
                    "trade-good base price for {} is outside the price range",
                    query.trade_good
                ),
            )
        })?;
        Ok(VanillaTradeGoodPriceTableQueryReport {
            trade_good: query.trade_good.clone(),
            base_price_milli: Some(base_price_milli),
            evaluation: Some(calculate_vanilla_trade_good_price_table(
                VanillaTradeGoodPriceTableInput {
                    base_price_milli: base_price,
                    price_impacts_milli: query.price_impacts_milli.clone(),
                },
            )),
            unresolved: Vec::new(),
        })
    }
}
