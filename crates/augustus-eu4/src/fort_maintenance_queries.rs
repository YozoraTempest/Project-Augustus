// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

//! Caller-resolved and content-backed fort-maintenance queries.

use augustus_core::{
    VanillaFortMaintenanceInput, VanillaFortMaintenanceResult, calculate_vanilla_fort_maintenance,
};
use serde::{Deserialize, Serialize};

use crate::VanillaFortMaintenanceContextInput;

const EU4_1_37_5_FORT_BASE_COST_MILLI: i32 = 500;
const EU4_1_37_5_CURRENCY_CONVERSION_FACTOR_MICROS: i32 = 1_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaFortMaintenanceDirectQuery {
    pub province: u32,
    pub input: VanillaFortMaintenanceInput,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaFortMaintenanceBuildingsQuery {
    pub province: u32,
    pub context: VanillaFortMaintenanceContextInput,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VanillaFortMaintenanceQueryMode {
    Direct,
    FromBuildings,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "mode",
    content = "query",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum VanillaFortMaintenanceQuery {
    Direct(VanillaFortMaintenanceDirectQuery),
    FromBuildings(VanillaFortMaintenanceBuildingsQuery),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VanillaFortMaintenanceQueryReport {
    pub mode: VanillaFortMaintenanceQueryMode,
    pub province: u32,
    pub owner: Option<String>,
    pub fort_level_milli: Option<i64>,
    pub mothball_status: Option<u8>,
    pub evaluation: Option<VanillaFortMaintenanceResult>,
    pub unresolved: Vec<String>,
}

pub(crate) struct VanillaFortMaintenanceBuildingResolution {
    pub owner: Option<String>,
    pub fort_level_milli: Option<i64>,
    pub mothball_status: Option<u8>,
    pub evaluation: Option<VanillaFortMaintenanceResult>,
    pub input: Option<VanillaFortMaintenanceInput>,
    pub unresolved: Vec<String>,
}

impl crate::VanillaWorld {
    pub(crate) fn resolve_vanilla_fort_maintenance_from_building(
        &self,
        province: u32,
        context: Option<VanillaFortMaintenanceContextInput>,
    ) -> Result<VanillaFortMaintenanceBuildingResolution, augustus_core::Error> {
        let entity = self.initial.province_entity(province).ok_or_else(|| {
            augustus_core::Error::Invalid(format!(
                "unknown EU4 fort-maintenance province {province}"
            ))
        })?;
        let owner = self
            .initial
            .province(province)
            .and_then(|province| province.owner.clone());
        if self.initial.world().entity(entity)?.owner.is_none() {
            return Ok(VanillaFortMaintenanceBuildingResolution {
                owner,
                fort_level_milli: None,
                mothball_status: self.province_mothball_status(province),
                evaluation: None,
                input: None,
                unresolved: vec!["fort_maintenance:unowned".into()],
            });
        }

        let state = self
            .initial
            .world()
            .economy()
            .provinces
            .get(&entity)
            .ok_or_else(|| {
                augustus_core::Error::Invalid(format!(
                    "fort-maintenance building state is unknown for owned province {province}"
                ))
            })?;
        let mut fort_level_milli = None;
        for building_name in &state.buildings {
            let definition = self
                .initial
                .world()
                .economy()
                .buildings
                .get(building_name)
                .ok_or_else(|| {
                    augustus_core::Error::Invalid(format!(
                        "unknown building {building_name} in province {province}"
                    ))
                })?;
            if definition.role != augustus_core::BuildingRole::Fortification {
                continue;
            }
            let level = definition.fort_level_milli.ok_or_else(|| {
                augustus_core::Error::Invalid(format!(
                    "fortification building {building_name} has no loaded fort level"
                ))
            })?;
            if level <= 0 {
                return Err(augustus_core::Error::Invalid(format!(
                    "fortification building {building_name} has a nonpositive fort level"
                )));
            }
            if fort_level_milli.replace(level).is_some() {
                return Err(augustus_core::Error::Invalid(format!(
                    "province {province} has multiple fortification buildings"
                )));
            }
        }

        let Some(fort_level_milli) = fort_level_milli else {
            return Ok(VanillaFortMaintenanceBuildingResolution {
                owner,
                fort_level_milli: None,
                mothball_status: self.province_mothball_status(province),
                evaluation: None,
                input: None,
                unresolved: vec!["fort_maintenance:no_fortification_building".into()],
            });
        };
        let context = context.ok_or_else(|| {
            augustus_core::Error::Invalid(format!(
                "missing fort-maintenance context for province {province}"
            ))
        })?;
        let mothball_status = self.province_mothball_status(province);
        let input = VanillaFortMaintenanceInput {
            base_cost_milli: EU4_1_37_5_FORT_BASE_COST_MILLI,
            country_modifier_milli: context.country_modifier_milli,
            province_modifier_milli: context.province_modifier_milli,
            conversion_factor_micros: EU4_1_37_5_CURRENCY_CONVERSION_FACTOR_MICROS,
            local_multiplier_milli: i32::try_from(fort_level_milli)
                .map_err(|_| augustus_core::Error::Overflow)?,
            relationship_modifier_milli: context.relationship_modifier_milli,
            apply_relationship_modifier: context.apply_relationship_modifier,
            halve_for_special_case: context.halve_for_special_case || mothball_status == Some(2),
        };
        let evaluation = calculate_vanilla_fort_maintenance(input)?;
        if evaluation.amount_milli < 0 {
            return Err(augustus_core::Error::Invalid(
                "fort-maintenance expense must be nonnegative".into(),
            ));
        }
        Ok(VanillaFortMaintenanceBuildingResolution {
            owner,
            fort_level_milli: Some(fort_level_milli),
            mothball_status,
            evaluation: Some(evaluation),
            input: Some(input),
            unresolved: Vec::new(),
        })
    }

    pub fn query_vanilla_fort_maintenance(
        &self,
        query: &VanillaFortMaintenanceQuery,
    ) -> Result<VanillaFortMaintenanceQueryReport, crate::Error> {
        match query {
            VanillaFortMaintenanceQuery::Direct(query) => {
                if self.initial.province(query.province).is_none() {
                    return Err(crate::Error::message(
                        "fort maintenance query",
                        format!("unknown EU4 fort-maintenance province {}", query.province),
                    ));
                }
                let owner = self
                    .initial
                    .province(query.province)
                    .and_then(|province| province.owner.clone());
                let evaluation = calculate_vanilla_fort_maintenance(query.input)
                    .map_err(|error| crate::Error::caused_by("fort maintenance query", error))?;
                if evaluation.amount_milli < 0 {
                    return Err(crate::Error::message(
                        "fort maintenance query",
                        "fort-maintenance expense must be nonnegative",
                    ));
                }
                Ok(VanillaFortMaintenanceQueryReport {
                    mode: VanillaFortMaintenanceQueryMode::Direct,
                    province: query.province,
                    owner,
                    fort_level_milli: None,
                    mothball_status: self.province_mothball_status(query.province),
                    evaluation: Some(evaluation),
                    unresolved: Vec::new(),
                })
            }
            VanillaFortMaintenanceQuery::FromBuildings(query) => {
                let resolution = self
                    .resolve_vanilla_fort_maintenance_from_building(
                        query.province,
                        Some(query.context),
                    )
                    .map_err(|error| crate::Error::caused_by("fort maintenance query", error))?;
                Ok(VanillaFortMaintenanceQueryReport {
                    mode: VanillaFortMaintenanceQueryMode::FromBuildings,
                    province: query.province,
                    owner: resolution.owner,
                    fort_level_milli: resolution.fort_level_milli,
                    mothball_status: resolution.mothball_status,
                    evaluation: resolution.evaluation,
                    unresolved: resolution.unresolved,
                })
            }
        }
    }
}
