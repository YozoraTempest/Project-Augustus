// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

//! Read-only previews for the next shared monthly tax settlement.

use augustus_core::MonthlyTaxAccount;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaMonthlyTaxQuery {
    pub country: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VanillaMonthlyTaxQueryReport {
    pub country: String,
    pub day: u64,
    pub evaluation: Option<MonthlyTaxAccount>,
}

impl crate::VanillaWorld {
    /// Previews the next monthly tax account without committing treasury,
    /// ledger, receipt, or settlement-count changes.
    pub fn query_vanilla_monthly_tax(
        &self,
        query: &VanillaMonthlyTaxQuery,
    ) -> Result<VanillaMonthlyTaxQueryReport, crate::Error> {
        let entity = self
            .initial
            .country_entity(&query.country)
            .ok_or_else(|| crate::Error::message("monthly tax query", "unknown tax country"))?;
        let settlement = self
            .initial
            .world()
            .preview_monthly_tax()
            .map_err(|error| crate::Error::caused_by("monthly tax query", error))?;
        Ok(VanillaMonthlyTaxQueryReport {
            country: query.country.clone(),
            day: settlement.day,
            evaluation: settlement.accounts.get(&entity).cloned(),
        })
    }
}
