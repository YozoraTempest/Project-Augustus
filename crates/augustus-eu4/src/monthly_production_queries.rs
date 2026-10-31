// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

//! Read-only previews for the next shared monthly production settlement.

use augustus_core::ProductionCountryAccount;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaMonthlyProductionQuery {
    pub country: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VanillaMonthlyProductionQueryReport {
    pub country: String,
    pub day: u64,
    pub evaluation: Option<ProductionCountryAccount>,
}

impl crate::VanillaWorld {
    /// Previews the next ordinary-production settlement for one country
    /// without committing refreshed prices, inputs, treasury, or receipts.
    pub fn query_vanilla_monthly_production(
        &self,
        query: &VanillaMonthlyProductionQuery,
    ) -> Result<VanillaMonthlyProductionQueryReport, crate::Error> {
        let entity = self.initial.country_entity(&query.country).ok_or_else(|| {
            crate::Error::message("monthly production query", "unknown production country")
        })?;
        let settlement = self
            .initial
            .world()
            .preview_monthly_production()
            .map_err(|error| crate::Error::caused_by("monthly production query", error))?;
        Ok(VanillaMonthlyProductionQueryReport {
            country: query.country.clone(),
            day: settlement.day,
            evaluation: settlement.accounts.get(&entity).cloned(),
        })
    }
}
