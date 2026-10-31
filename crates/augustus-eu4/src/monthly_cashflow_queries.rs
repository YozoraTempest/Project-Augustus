// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

//! Read-only previews for the next shared monthly cashflow settlement.

use augustus_core::CashflowAccount;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaMonthlyCashflowQuery {
    pub country: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VanillaMonthlyCashflowQueryReport {
    pub country: String,
    pub day: u64,
    pub evaluation: Option<CashflowAccount>,
}

impl crate::VanillaWorld {
    /// Previews the next monthly cashflow for one country without committing
    /// treasury, ledger, refreshed inputs, or settlement receipts.
    pub fn query_vanilla_monthly_cashflow(
        &self,
        query: &VanillaMonthlyCashflowQuery,
    ) -> Result<VanillaMonthlyCashflowQueryReport, crate::Error> {
        let entity = self.initial.country_entity(&query.country).ok_or_else(|| {
            crate::Error::message("monthly cashflow query", "unknown cashflow country")
        })?;
        let settlement = self
            .initial
            .world()
            .preview_monthly_cashflows()
            .map_err(|error| crate::Error::caused_by("monthly cashflow query", error))?;
        Ok(VanillaMonthlyCashflowQueryReport {
            country: query.country.clone(),
            day: settlement.day,
            evaluation: settlement.accounts.get(&entity).cloned(),
        })
    }
}
