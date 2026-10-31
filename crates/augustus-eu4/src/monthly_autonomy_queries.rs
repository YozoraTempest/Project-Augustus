// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

//! Read-only previews for the shared monthly province-autonomy phase.

use augustus_core::MonthlyAutonomyPreview;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaMonthlyAutonomyQuery {
    pub province: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VanillaMonthlyAutonomyQueryReport {
    pub province: u32,
    pub evaluation: MonthlyAutonomyPreview,
}

impl crate::VanillaWorld {
    /// Previews one province's next raw-autonomy value without replacing the
    /// administration state or advancing the monthly system.
    pub fn query_vanilla_monthly_autonomy(
        &self,
        query: &VanillaMonthlyAutonomyQuery,
    ) -> Result<VanillaMonthlyAutonomyQueryReport, crate::Error> {
        let entity = self
            .initial
            .province_entity(query.province)
            .ok_or_else(|| crate::Error::message("monthly autonomy query", "unknown province"))?;
        let evaluation = self
            .initial
            .world()
            .preview_monthly_autonomy(entity)
            .map_err(|error| crate::Error::caused_by("monthly autonomy query", error))?;
        Ok(VanillaMonthlyAutonomyQueryReport {
            province: query.province,
            evaluation,
        })
    }
}
