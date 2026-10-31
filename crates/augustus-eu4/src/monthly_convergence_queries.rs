// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

//! Read-only previews for the next shared monthly convergence settlement.

use augustus_core::ConvergenceAccount;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaMonthlyConvergenceQuery {
    pub country: String,
    #[serde(default)]
    pub metric: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VanillaMonthlyConvergenceQueryReport {
    pub country: String,
    pub day: u64,
    pub accounts: BTreeMap<String, ConvergenceAccount>,
}

impl crate::VanillaWorld {
    /// Previews the next monthly convergence settlement for one country
    /// without committing metric values, receipts, or settlement counters.
    pub fn query_vanilla_monthly_convergence(
        &self,
        query: &VanillaMonthlyConvergenceQuery,
    ) -> Result<VanillaMonthlyConvergenceQueryReport, crate::Error> {
        let entity = self.initial.country_entity(&query.country).ok_or_else(|| {
            crate::Error::message("monthly convergence query", "unknown convergence country")
        })?;
        let settlement = self
            .initial
            .world()
            .preview_monthly_convergence()
            .map_err(|error| crate::Error::caused_by("monthly convergence query", error))?;
        let mut accounts = settlement.accounts.get(&entity).cloned().ok_or_else(|| {
            crate::Error::message(
                "monthly convergence query",
                format!("country {} has no convergence rules", query.country),
            )
        })?;
        if let Some(metric) = &query.metric {
            let account = accounts.remove(metric).ok_or_else(|| {
                crate::Error::message(
                    "monthly convergence query",
                    format!("unknown convergence metric {metric}"),
                )
            })?;
            accounts = BTreeMap::from([(metric.clone(), account)]);
        }
        Ok(VanillaMonthlyConvergenceQueryReport {
            country: query.country.clone(),
            day: settlement.day,
            accounts,
        })
    }
}
