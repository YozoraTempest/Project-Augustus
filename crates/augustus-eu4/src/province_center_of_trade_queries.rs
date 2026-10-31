// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

//! Read-only queries for province center-of-trade levels.

use serde::{Deserialize, Serialize};

/// Select one province whose current center-of-trade level should be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaProvinceCenterOfTradeQuery {
    pub province: u32,
}

/// Current center-of-trade level for one EU4 province.
///
/// `None` means that the level is unknown or no runtime value has been bound;
/// `Some(0)` is an explicitly known province without a center of trade.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct VanillaProvinceCenterOfTradeQueryReport {
    pub province: u32,
    pub level: Option<u8>,
}

impl crate::VanillaWorld {
    /// Reads the live core level without applying effects or mutating state.
    pub fn query_vanilla_province_center_of_trade(
        &self,
        query: &VanillaProvinceCenterOfTradeQuery,
    ) -> Result<VanillaProvinceCenterOfTradeQueryReport, crate::Error> {
        let province = self
            .initial
            .province_entity(query.province)
            .ok_or_else(|| {
                crate::Error::message(
                    "province center-of-trade query",
                    format!("unknown EU4 center-of-trade province {}", query.province),
                )
            })?;
        let level = self
            .initial
            .world()
            .province_center_of_trade_level(province)
            .map_err(|error| crate::Error::caused_by("province center-of-trade query", error))?;
        Ok(VanillaProvinceCenterOfTradeQueryReport {
            province: query.province,
            level,
        })
    }
}
