// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

//! Caller-resolved naval force-limit queries.

use augustus_core::{
    VanillaForceLimitInput, VanillaForceLimitResult, calculate_vanilla_force_limit,
};
use serde::{Deserialize, Serialize};

/// Complete caller-resolved inputs for the shared naval force-limit stage.
///
/// The EU4 adapter does not infer these values from partial attribute
/// subtotals. `None` keeps the query unresolved, while explicit zero remains a
/// valid resolved input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaNavalForceLimitQuery {
    pub country: String,
    pub base_milli: Option<i32>,
    pub flat_modifier_milli: Option<i32>,
    pub percentage_modifier_milli: Option<i32>,
}

/// Read-only result for a caller-resolved naval force-limit query.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VanillaNavalForceLimitQueryReport {
    pub country: String,
    pub evaluation: Option<VanillaForceLimitResult>,
    pub unresolved: Vec<String>,
}

impl crate::VanillaWorld {
    /// Calculates a naval force limit from complete caller-resolved inputs.
    ///
    /// This method deliberately does not promote `country_attributes` known
    /// subtotals into a complete limit. The compatibility provider owns the
    /// source resolution and rounding inputs.
    pub fn query_naval_force_limit(
        &self,
        query: &VanillaNavalForceLimitQuery,
    ) -> Result<VanillaNavalForceLimitQueryReport, crate::Error> {
        if self.initial.country_entity(&query.country).is_none() {
            return Err(crate::Error::message(
                "naval force-limit query",
                format!("unknown country {}", query.country),
            ));
        }

        let mut unresolved = Vec::new();
        for (name, value) in [
            ("base_milli", query.base_milli),
            ("flat_modifier_milli", query.flat_modifier_milli),
            ("percentage_modifier_milli", query.percentage_modifier_milli),
        ] {
            if value.is_none() {
                unresolved.push(format!("naval_force_limit:missing_input:{name}"));
            }
        }

        let evaluation = match (
            query.base_milli,
            query.flat_modifier_milli,
            query.percentage_modifier_milli,
        ) {
            (Some(base_milli), Some(flat_modifier_milli), Some(percentage_modifier_milli))
                if unresolved.is_empty() =>
            {
                Some(calculate_vanilla_force_limit(VanillaForceLimitInput {
                    base_milli,
                    flat_modifier_milli,
                    percentage_modifier_milli,
                }))
            }
            _ => None,
        };

        Ok(VanillaNavalForceLimitQueryReport {
            country: query.country.clone(),
            evaluation,
            unresolved,
        })
    }
}
