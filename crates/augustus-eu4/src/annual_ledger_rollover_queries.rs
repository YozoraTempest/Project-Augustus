// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

//! Read-only previews for the explicit annual country-ledger rollover stage.

use augustus_core::CountryLedger;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaAnnualLedgerRolloverQuery {
    pub country: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VanillaAnnualLedgerRolloverQueryReport {
    pub country: String,
    pub evaluation: Option<CountryLedger>,
}

impl crate::VanillaWorld {
    /// Previews the explicit annual ledger rollover stage without advancing
    /// the calendar or mutating the live ledger.
    pub fn query_vanilla_annual_ledger_rollover(
        &self,
        query: &VanillaAnnualLedgerRolloverQuery,
    ) -> Result<VanillaAnnualLedgerRolloverQueryReport, crate::Error> {
        let entity = self.initial.country_entity(&query.country).ok_or_else(|| {
            crate::Error::message("annual ledger rollover query", "unknown ledger country")
        })?;
        let ledgers = self
            .initial
            .world()
            .preview_country_ledgers_annual_rollover()
            .map_err(|error| crate::Error::caused_by("annual ledger rollover query", error))?;
        Ok(VanillaAnnualLedgerRolloverQueryReport {
            country: query.country.clone(),
            evaluation: ledgers.get(&entity).cloned(),
        })
    }
}
