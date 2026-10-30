// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

//! Caller-resolved province production value-stage queries.

use augustus_core::VanillaProductionBaseValueResult;
use serde::{Deserialize, Serialize};

use crate::VanillaProvinceProductionBaseValueInput;

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
}
