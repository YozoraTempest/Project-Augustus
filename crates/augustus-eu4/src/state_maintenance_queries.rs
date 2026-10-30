// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

//! Caller-resolved province state-maintenance queries.

use augustus_core::VanillaStateMaintenanceResult;
use serde::{Deserialize, Serialize};

use crate::{VanillaContentStateMaintenanceInput, VanillaProvinceStateMaintenanceInput};

/// Direct state-maintenance query with an explicit development factor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaStateMaintenanceDirectQuery {
    pub province: u32,
    pub input: VanillaProvinceStateMaintenanceInput,
}

/// State-maintenance query that reads the development factor from loaded EU4 content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaStateMaintenanceContentQuery {
    pub province: u32,
    pub input: VanillaContentStateMaintenanceInput,
}

/// Selects whether a query supplies or loads the development factor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VanillaStateMaintenanceQueryMode {
    Direct,
    Content,
}

/// Strict JSON request for a caller-resolved province state-maintenance query.
///
/// The adjacent `mode` and `query` fields keep the two input shapes explicit
/// while allowing both variants in one CLI request file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "mode",
    content = "query",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum VanillaStateMaintenanceQuery {
    Direct(VanillaStateMaintenanceDirectQuery),
    Content(VanillaStateMaintenanceContentQuery),
}

/// Read-only result for one province state-maintenance query.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VanillaStateMaintenanceQueryReport {
    pub mode: VanillaStateMaintenanceQueryMode,
    pub province: u32,
    pub evaluation: Option<VanillaStateMaintenanceResult>,
    pub unresolved: Vec<String>,
}

impl crate::VanillaWorld {
    /// Evaluates a caller-resolved province state-maintenance contribution.
    ///
    /// Unknown provinces and unresolved base/content inputs remain visible as
    /// null evaluations. The query never installs monthly inputs or mutates
    /// the world.
    pub fn query_vanilla_state_maintenance(
        &self,
        query: &VanillaStateMaintenanceQuery,
    ) -> Result<VanillaStateMaintenanceQueryReport, crate::Error> {
        match query {
            VanillaStateMaintenanceQuery::Direct(query) => {
                let unresolved = self.province_unresolved(query.province);
                if !unresolved.is_empty() {
                    return Ok(query_report(
                        VanillaStateMaintenanceQueryMode::Direct,
                        query.province,
                        None,
                        unresolved,
                    ));
                }
                let evaluation = self
                    .calculate_vanilla_province_state_maintenance(query.province, query.input)
                    .map_err(|error| crate::Error::caused_by("state maintenance query", error))?;
                Ok(query_report(
                    VanillaStateMaintenanceQueryMode::Direct,
                    query.province,
                    evaluation,
                    Vec::new(),
                ))
            }
            VanillaStateMaintenanceQuery::Content(query) => {
                let mut unresolved = self.province_unresolved(query.province);
                if !unresolved.is_empty() {
                    return Ok(query_report(
                        VanillaStateMaintenanceQueryMode::Content,
                        query.province,
                        None,
                        unresolved,
                    ));
                }
                let evaluation = self
                    .calculate_vanilla_province_state_maintenance_from_content(
                        query.province,
                        query.input,
                    )
                    .map_err(|error| crate::Error::caused_by("state maintenance query", error))?;
                if evaluation.is_none() {
                    unresolved.push(
                        "state_maintenance:missing_define:STATE_MAINTENANCE_DEV_FACTOR".into(),
                    );
                }
                Ok(query_report(
                    VanillaStateMaintenanceQueryMode::Content,
                    query.province,
                    evaluation,
                    unresolved,
                ))
            }
        }
    }

    fn province_unresolved(&self, province: u32) -> Vec<String> {
        let Some(state) = self.initial.province(province) else {
            return vec![format!("state_maintenance:unknown_province:{province}")];
        };
        let mut unresolved = Vec::new();
        for (name, value) in [
            ("base_tax_milli", state.tax_milli),
            ("base_production_milli", state.production_milli),
            ("base_manpower_milli", state.manpower_milli),
        ] {
            if value.is_none() {
                unresolved.push(format!("state_maintenance:missing_input:{name}"));
            }
        }
        unresolved
    }
}

fn query_report(
    mode: VanillaStateMaintenanceQueryMode,
    province: u32,
    evaluation: Option<VanillaStateMaintenanceResult>,
    unresolved: Vec<String>,
) -> VanillaStateMaintenanceQueryReport {
    VanillaStateMaintenanceQueryReport {
        mode,
        province,
        evaluation,
        unresolved,
    }
}
