// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::BTreeMap;

use augustus_core::{
    VanillaGoldProductionCountryResult, VanillaGoldProductionInputState,
    VanillaProductionCountryResult, VanillaProductionInputState,
};
use serde::{Deserialize, Serialize};

/// Explicit inputs for an ordinary production aggregation query.
///
/// The compatibility provider owns production-stage resolution. The query
/// only re-aggregates the supplied province inputs over the country's current
/// owned provinces.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaProductionCountryQuery {
    pub country: String,
    pub inputs: BTreeMap<u32, VanillaProductionInputState>,
}

/// Read-only ordinary production aggregation result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VanillaProductionCountryQueryReport {
    pub country: String,
    pub evaluation: VanillaProductionCountryResult,
}

/// Explicit inputs for a gold production aggregation query.
///
/// Gold provinces are selected from the loaded trade-good definitions. The
/// query re-aggregates the supplied inputs over the country's current owned
/// gold provinces and leaves missing or unresolved inputs visible in the
/// returned partial result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaGoldProductionCountryQuery {
    pub country: String,
    pub inputs: BTreeMap<u32, VanillaGoldProductionInputState>,
}

/// Read-only gold production aggregation result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VanillaGoldProductionCountryQueryReport {
    pub country: String,
    pub evaluation: VanillaGoldProductionCountryResult,
}

impl crate::VanillaWorld {
    /// Aggregates explicit ordinary production inputs over the country's live
    /// owned provinces without changing world state.
    pub fn query_vanilla_production(
        &self,
        query: &VanillaProductionCountryQuery,
    ) -> Result<VanillaProductionCountryQueryReport, crate::Error> {
        let evaluation = self
            .country_vanilla_production(&query.country, &query.inputs)
            .map_err(|error| crate::Error::caused_by("vanilla production query", error))?;
        Ok(VanillaProductionCountryQueryReport {
            country: query.country.clone(),
            evaluation,
        })
    }

    /// Aggregates explicit gold production inputs over the country's live
    /// owned gold provinces without changing world state.
    pub fn query_vanilla_gold_production(
        &self,
        query: &VanillaGoldProductionCountryQuery,
    ) -> Result<VanillaGoldProductionCountryQueryReport, crate::Error> {
        let evaluation = self
            .country_vanilla_gold_production(&query.country, &query.inputs)
            .map_err(|error| crate::Error::caused_by("vanilla gold production query", error))?;
        Ok(VanillaGoldProductionCountryQueryReport {
            country: query.country.clone(),
            evaluation,
        })
    }
}
