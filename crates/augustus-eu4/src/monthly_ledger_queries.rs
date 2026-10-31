// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

//! Read-only previews for the next monthly country-ledger updates.

use augustus_core::CountryLedger;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaMonthlyLedgerQuery {
    pub country: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VanillaMonthlyLedgerQueryReport {
    pub country: String,
    pub evaluation: Option<CountryLedger>,
}

impl crate::VanillaWorld {
    /// Previews the ledger state that the next monthly settlement would
    /// append, without consolidating or flushing the live ledger.
    pub fn query_vanilla_monthly_ledger(
        &self,
        query: &VanillaMonthlyLedgerQuery,
    ) -> Result<VanillaMonthlyLedgerQueryReport, crate::Error> {
        let entity = self.initial.country_entity(&query.country).ok_or_else(|| {
            crate::Error::message("monthly ledger query", "unknown ledger country")
        })?;
        let updates = self
            .initial
            .world()
            .preview_monthly_ledger_updates()
            .map_err(|error| crate::Error::caused_by("monthly ledger query", error))?;
        Ok(VanillaMonthlyLedgerQueryReport {
            country: query.country.clone(),
            evaluation: updates.get(&entity).cloned(),
        })
    }
}
