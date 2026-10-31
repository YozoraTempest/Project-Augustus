// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

//! Read-only previews for the next shared monthly accrual settlement.

use augustus_core::AccrualAccount;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaMonthlyAccrualQuery {
    pub country: String,
    #[serde(default)]
    pub metric: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VanillaMonthlyAccrualQueryReport {
    pub country: String,
    pub day: u64,
    pub accounts: BTreeMap<String, AccrualAccount>,
}

impl crate::VanillaWorld {
    /// Previews the next monthly accrual settlement after the shared
    /// monthly-autonomy phase, without changing the live world or snapshot.
    pub fn query_vanilla_monthly_accrual(
        &self,
        query: &VanillaMonthlyAccrualQuery,
    ) -> Result<VanillaMonthlyAccrualQueryReport, crate::Error> {
        let entity = self.initial.country_entity(&query.country).ok_or_else(|| {
            crate::Error::message("monthly accrual query", "unknown accrual country")
        })?;
        let settlement = self
            .initial
            .world()
            .preview_monthly_accruals()
            .map_err(|error| crate::Error::caused_by("monthly accrual query", error))?;
        let mut accounts = settlement.accounts.get(&entity).cloned().ok_or_else(|| {
            crate::Error::message(
                "monthly accrual query",
                format!("country {} has no accrual rules", query.country),
            )
        })?;
        if let Some(metric) = &query.metric {
            let account = accounts.remove(metric).ok_or_else(|| {
                crate::Error::message(
                    "monthly accrual query",
                    format!("unknown accrual metric {metric}"),
                )
            })?;
            accounts = BTreeMap::from([(metric.clone(), account)]);
        }
        Ok(VanillaMonthlyAccrualQueryReport {
            country: query.country.clone(),
            day: settlement.day,
            accounts,
        })
    }
}
