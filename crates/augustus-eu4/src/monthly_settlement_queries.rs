// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

//! Read-only previews for the final state of a standard monthly settlement.

use augustus_core::MonthlySettlementAccount;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaMonthlySettlementQuery {
    pub country: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VanillaMonthlySettlementQueryReport {
    pub country: String,
    pub day: u64,
    pub system_id: String,
    pub evaluation: Option<MonthlySettlementAccount>,
}

impl crate::VanillaWorld {
    /// Previews the final treasury and ledger state produced by the selected
    /// standard monthly executor without changing the live world.
    pub fn query_vanilla_monthly_settlement(
        &self,
        query: &VanillaMonthlySettlementQuery,
    ) -> Result<VanillaMonthlySettlementQueryReport, crate::Error> {
        let entity = self.initial.country_entity(&query.country).ok_or_else(|| {
            crate::Error::message("monthly settlement query", "unknown settlement country")
        })?;
        let preview = self
            .initial
            .world()
            .preview_standard_monthly_settlement()
            .map_err(|error| crate::Error::caused_by("monthly settlement query", error))?;
        Ok(VanillaMonthlySettlementQueryReport {
            country: query.country.clone(),
            day: preview.day,
            system_id: preview.system_id,
            evaluation: preview.accounts.get(&entity).cloned(),
        })
    }
}
