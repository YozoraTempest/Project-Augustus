// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    VanillaNavalConstructionTimeInput, VanillaNavalConstructionTimeResult,
    calculate_vanilla_naval_construction_time,
};
use serde::{Deserialize, Serialize};

const GALLEY_TIME: &str = "GALLEY_TIME";
const MIN_RECRUIT_TIME_MODIFIER: &str = "MIN_RECRUIT_TIME_MODIFIER";
const NAVAL_CATEGORIES: [&str; 4] = ["heavy_ship", "light_ship", "galley", "transport"];

/// Caller-resolved modifier totals for an ordinary galley duration query.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaNavalConstructionTimeQuery {
    pub country: String,
    pub province_id: u32,
    pub unit_type: String,
    pub country_modifier_milli: Option<i32>,
    pub province_modifier_milli: Option<i32>,
    pub additional_province_modifier_milli: Option<i32>,
}

/// Read-only duration result for a supported naval construction query.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VanillaNavalConstructionTimeQueryReport {
    pub country: String,
    pub province_id: u32,
    pub unit_type: String,
    pub evaluation: Option<VanillaNavalConstructionTimeResult>,
    pub unresolved: Vec<String>,
}

impl crate::VanillaWorld {
    /// Calculates ordinary galley duration from loaded defines and caller-resolved modifiers.
    ///
    /// Only the normal flags-zero branch with matching country, province owner,
    /// and controller is supported. No missing modifier is inferred.
    pub fn query_naval_construction_time(
        &self,
        query: &VanillaNavalConstructionTimeQuery,
    ) -> Result<VanillaNavalConstructionTimeQueryReport, crate::Error> {
        let invalid =
            |message: String| crate::Error::message("naval construction time query", message);
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
                ["naval_construction_time:unsupported_category".into()],
            ));
        }

        for name in [GALLEY_TIME, MIN_RECRUIT_TIME_MODIFIER] {
            if let Some(error) = self.naval_construction_time_define_error(name) {
                return Err(error);
            }
        }

        let mut unresolved = Vec::new();
        let world = self.initial.world();
        let owner = world
            .entity(province)
            .map_err(|error| crate::Error::caused_by("naval construction time query", error))?
            .owner;
        match owner {
            Some(owner) if owner == country => {}
            Some(_) => unresolved.push("naval_construction_time:owner_mismatch".into()),
            None => unresolved.push("naval_construction_time:owner_unknown_or_absent".into()),
        }
        match world
            .province_controller_state(province)
            .map_err(|error| crate::Error::caused_by("naval construction time query", error))?
        {
            Some(Some(controller)) if controller == country => {}
            Some(Some(_)) | Some(None) => {
                unresolved.push("naval_construction_time:controller_mismatch".into());
            }
            None => unresolved.push("naval_construction_time:controller_unknown".into()),
        }

        let setup = &self.report.setup;
        let base_days = setup
            .naval_construction_time_defines
            .get(GALLEY_TIME)
            .copied();
        if base_days.is_none() {
            unresolved.push(format!(
                "naval_construction_time:missing_define:{GALLEY_TIME}"
            ));
        }
        let minimum_modifier_milli = setup
            .naval_construction_time_defines
            .get(MIN_RECRUIT_TIME_MODIFIER)
            .copied();
        if minimum_modifier_milli.is_none() {
            unresolved.push(format!(
                "naval_construction_time:missing_define:{MIN_RECRUIT_TIME_MODIFIER}"
            ));
        }

        for (name, value) in [
            ("country_modifier_milli", query.country_modifier_milli),
            ("province_modifier_milli", query.province_modifier_milli),
            (
                "additional_province_modifier_milli",
                query.additional_province_modifier_milli,
            ),
        ] {
            if value.is_none() {
                unresolved.push(format!("naval_construction_time:missing_input:{name}"));
            }
        }

        let [Some(base_days), Some(minimum_modifier_milli)] = [base_days, minimum_modifier_milli]
        else {
            return Ok(query_report(query, None, unresolved));
        };
        let [
            Some(country_modifier_milli),
            Some(province_modifier_milli),
            Some(additional_province_modifier_milli),
        ] = [
            query.country_modifier_milli,
            query.province_modifier_milli,
            query.additional_province_modifier_milli,
        ]
        else {
            return Ok(query_report(query, None, unresolved));
        };

        if !unresolved.is_empty() {
            return Ok(query_report(query, None, unresolved));
        }

        let evaluation =
            calculate_vanilla_naval_construction_time(VanillaNavalConstructionTimeInput {
                base_days,
                minimum_modifier_milli,
                country_modifier_milli,
                province_modifier_milli,
                additional_province_modifier_milli,
            })
            .map_err(|error| crate::Error::caused_by("naval construction time query", error))?;

        Ok(query_report(query, Some(evaluation), unresolved))
    }

    fn naval_construction_time_define_error(&self, name: &str) -> Option<crate::Error> {
        let detail = self
            .report
            .setup
            .naval_construction_time_define_errors
            .get(name)?
            .clone();
        let source = self
            .report
            .setup
            .definition_sources
            .get(&format!("naval_construction_time:{name}"));
        Some(match source {
            Some(source) => {
                crate::Error::message(&source.path, detail).with_location(crate::SourceLocation {
                    line: source.line,
                    column: source.column,
                })
            }
            None => crate::Error::message("naval construction time query", detail),
        })
    }
}

fn query_report(
    query: &VanillaNavalConstructionTimeQuery,
    evaluation: Option<VanillaNavalConstructionTimeResult>,
    unresolved: impl IntoIterator<Item = String>,
) -> VanillaNavalConstructionTimeQueryReport {
    VanillaNavalConstructionTimeQueryReport {
        country: query.country.clone(),
        province_id: query.province_id,
        unit_type: query.unit_type.clone(),
        evaluation,
        unresolved: unresolved.into_iter().collect(),
    }
}
