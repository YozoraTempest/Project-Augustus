// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    VanillaNavalConstructionCostInput, VanillaNavalConstructionCostResult,
    calculate_vanilla_naval_construction_cost,
};
use serde::{Deserialize, Serialize};

const GALLEY_COST: &str = "GALLEY_COST";
const NAVAL_CATEGORIES: [&str; 4] = ["heavy_ship", "light_ship", "galley", "transport"];

/// Caller-resolved inputs for an ordinary, in-limit galley cost query.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaNavalConstructionCostQuery {
    pub country: String,
    pub province_id: u32,
    pub unit_type: String,
    pub country_modifier_milli: Option<i32>,
    pub additional_country_modifier_milli: Option<i32>,
    pub province_modifier_milli: Option<i32>,
    pub naval_quantity_milli: Option<i32>,
    pub naval_limit_milli: Option<i32>,
}

/// Read-only cost result for a supported naval construction query.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VanillaNavalConstructionCostQueryReport {
    pub country: String,
    pub province_id: u32,
    pub unit_type: String,
    pub evaluation: Option<VanillaNavalConstructionCostResult>,
    pub unresolved: Vec<String>,
}

impl crate::VanillaWorld {
    /// Calculates ordinary galley cost from loaded defines and caller-resolved inputs.
    ///
    /// Only the normal flags-zero, in-limit path with matching country, province
    /// owner, and controller is supported. No missing input is inferred.
    pub fn query_naval_construction_cost(
        &self,
        query: &VanillaNavalConstructionCostQuery,
    ) -> Result<VanillaNavalConstructionCostQueryReport, crate::Error> {
        let invalid =
            |message: String| crate::Error::message("naval construction cost query", message);
        let country = self
            .initial
            .country_entity(&query.country)
            .ok_or_else(|| invalid(format!("unknown country {}", query.country)))?;
        let province = self
            .initial
            .province_entity(query.province_id)
            .ok_or_else(|| invalid(format!("unknown province {}", query.province_id)))?;
        let template = self
            .initial
            .world()
            .unit_templates()
            .definitions
            .get(&query.unit_type)
            .ok_or_else(|| invalid(format!("unknown unit template {}", query.unit_type)))?;
        if !NAVAL_CATEGORIES.contains(&template.category.as_str()) {
            return Err(invalid(format!(
                "unit template {} is not naval",
                query.unit_type
            )));
        }
        if template.category != "galley" {
            return Ok(query_report(
                query,
                None,
                ["naval_construction_cost:unsupported_category".into()],
            ));
        }

        if query.naval_quantity_milli.is_some_and(|value| value < 0) {
            return Err(invalid("naval quantity must be nonnegative".into()));
        }
        if query.naval_limit_milli.is_some_and(|value| value < 0) {
            return Err(invalid("naval limit must be nonnegative".into()));
        }

        let mut unresolved = Vec::new();
        let world = self.initial.world();
        let owner = world
            .entity(province)
            .map_err(|error| crate::Error::caused_by("naval construction cost query", error))?
            .owner;
        match owner {
            Some(owner) if owner == country => {}
            Some(_) => unresolved.push("naval_construction_cost:owner_mismatch".into()),
            None => unresolved.push("naval_construction_cost:owner_unknown_or_absent".into()),
        }
        match world
            .province_controller_state(province)
            .map_err(|error| crate::Error::caused_by("naval construction cost query", error))?
        {
            Some(Some(controller)) if controller == country => {}
            Some(Some(_)) | Some(None) => {
                unresolved.push("naval_construction_cost:controller_mismatch".into());
            }
            None => unresolved.push("naval_construction_cost:controller_unknown".into()),
        }
        if !unresolved.is_empty() {
            return Ok(query_report(query, None, unresolved));
        }

        if let Some(error) = self.naval_construction_cost_define_error(GALLEY_COST) {
            return Err(error);
        }

        let base_cost_milli = self
            .report
            .setup
            .naval_construction_cost_defines_milli
            .get(GALLEY_COST)
            .copied();
        if base_cost_milli.is_none() {
            unresolved.push(format!(
                "naval_construction_cost:missing_define:{GALLEY_COST}"
            ));
        }

        for (name, value) in [
            ("country_modifier_milli", query.country_modifier_milli),
            (
                "additional_country_modifier_milli",
                query.additional_country_modifier_milli,
            ),
            ("province_modifier_milli", query.province_modifier_milli),
            ("naval_quantity_milli", query.naval_quantity_milli),
            ("naval_limit_milli", query.naval_limit_milli),
        ] {
            if value.is_none() {
                unresolved.push(format!("naval_construction_cost:missing_input:{name}"));
            }
        }

        if let Some(limit) = query.naval_limit_milli {
            if limit == 0 {
                unresolved.push("naval_construction_cost:zero_naval_limit".into());
            } else if let Some(quantity) = query.naval_quantity_milli
                && quantity > limit
            {
                unresolved.push("naval_construction_cost:over_limit".into());
            }
        }

        let [
            Some(base_cost_milli),
            Some(country_modifier_milli),
            Some(additional_country_modifier_milli),
            Some(province_modifier_milli),
            Some(naval_quantity_milli),
            Some(naval_limit_milli),
        ] = [
            base_cost_milli,
            query.country_modifier_milli,
            query.additional_country_modifier_milli,
            query.province_modifier_milli,
            query.naval_quantity_milli,
            query.naval_limit_milli,
        ]
        else {
            return Ok(query_report(query, None, unresolved));
        };

        if !unresolved.is_empty() {
            return Ok(query_report(query, None, unresolved));
        }

        let evaluation =
            calculate_vanilla_naval_construction_cost(VanillaNavalConstructionCostInput {
                base_cost_milli,
                country_modifier_milli,
                additional_country_modifier_milli,
                province_modifier_milli,
                naval_quantity_milli,
                naval_limit_milli,
            })
            .map_err(|error| crate::Error::caused_by("naval construction cost query", error))?;

        Ok(query_report(query, Some(evaluation), unresolved))
    }

    fn naval_construction_cost_define_error(&self, name: &str) -> Option<crate::Error> {
        let detail = self
            .report
            .setup
            .naval_construction_cost_define_errors
            .get(name)?
            .clone();
        let source = self
            .report
            .setup
            .definition_sources
            .get(&format!("naval_construction_cost:{name}"));
        Some(match source {
            Some(source) => {
                crate::Error::message(&source.path, detail).with_location(crate::SourceLocation {
                    line: source.line,
                    column: source.column,
                })
            }
            None => crate::Error::message("naval construction cost query", detail),
        })
    }
}

fn query_report(
    query: &VanillaNavalConstructionCostQuery,
    evaluation: Option<VanillaNavalConstructionCostResult>,
    unresolved: impl IntoIterator<Item = String>,
) -> VanillaNavalConstructionCostQueryReport {
    VanillaNavalConstructionCostQueryReport {
        country: query.country.clone(),
        province_id: query.province_id,
        unit_type: query.unit_type.clone(),
        evaluation,
        unresolved: unresolved.into_iter().collect(),
    }
}
