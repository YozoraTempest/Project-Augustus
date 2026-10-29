// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use serde::{Deserialize, Serialize};

use crate::{
    Error, VanillaNativeMigrationContentPricing, VanillaNativeMigrationContentQuery,
    VanillaNativeMigrationQueryReport, VanillaWorld,
};

/// Effective migration modifiers supplied by the caller.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaNativeMigrationStatePricing {
    pub all_power_cost_modifier_milli: i32,
    pub country_migration_cost_modifier_milli: i32,
    pub province_migration_cost_modifier_milli: i32,
}

/// Query using persisted native membership and steps plus loaded cost defines.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaNativeMigrationStateQuery {
    pub country: String,
    pub destination: u32,
    pub pricing: VanillaNativeMigrationStatePricing,
    pub native_mode_eligible: Option<bool>,
    pub owner_eligible: Option<bool>,
    pub controller_eligible: Option<bool>,
    pub tribal_owner_eligible: Option<bool>,
    pub additional_gate_eligible: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VanillaNativeMigrationContext {
    pub provinces: Option<Vec<u32>>,
    pub migration_steps: Option<u32>,
    pub destination_in_native_membership: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct VanillaNativeMigrationStateQueryReport {
    pub context: VanillaNativeMigrationContext,
    pub query: VanillaNativeMigrationQueryReport,
}

impl VanillaWorld {
    /// Reads membership and steps from current state; never substitutes ownership.
    /// The remaining qualifications and effective modifiers are caller assertions.
    pub fn query_native_migration_from_state(
        &self,
        request: &VanillaNativeMigrationStateQuery,
    ) -> Result<VanillaNativeMigrationStateQueryReport, Error> {
        let prepared =
            self.prepare_native_migration_query(&request.country, request.destination)?;
        let country = self
            .initial
            .country_entity(&request.country)
            .expect("validated country");
        let state = self
            .initial
            .world()
            .native_migration_states()
            .countries
            .get(&country);
        let mut unresolved = Vec::new();
        let provinces = state
            .and_then(|state| state.provinces.as_ref())
            .and_then(|members| {
                members
                    .iter()
                    .map(|member| {
                        self.initial
                            .provinces()
                            .iter()
                            .find(|province| {
                                self.initial.province_entity(province.id) == Some(*member)
                            })
                            .map(|province| province.id)
                    })
                    .collect::<Option<Vec<_>>>()
            });
        let steps = state.and_then(|state| state.migration_steps);
        if provinces.is_none() {
            unresolved.push("native_migration:provinces_unknown".to_string());
        }
        if steps.is_none() {
            unresolved.push("native_migration:steps_unknown".to_string());
        }
        let membership = provinces
            .as_ref()
            .map(|members| members.contains(&request.destination));
        let context = VanillaNativeMigrationContext {
            provinces,
            migration_steps: steps,
            destination_in_native_membership: membership,
        };
        let (Some(migration_steps), Some(destination_in_native_membership)) = (steps, membership)
        else {
            return Ok(VanillaNativeMigrationStateQueryReport {
                context,
                query: prepared.report(&request.country, request.destination, unresolved),
            });
        };
        let query =
            self.query_native_migration_from_content(&VanillaNativeMigrationContentQuery {
                country: request.country.clone(),
                destination: request.destination,
                pricing: VanillaNativeMigrationContentPricing {
                    migration_steps,
                    destination_in_native_membership,
                    all_power_cost_modifier_milli: request.pricing.all_power_cost_modifier_milli,
                    country_migration_cost_modifier_milli: request
                        .pricing
                        .country_migration_cost_modifier_milli,
                    province_migration_cost_modifier_milli: request
                        .pricing
                        .province_migration_cost_modifier_milli,
                },
                native_membership_known: true,
                native_mode_eligible: request.native_mode_eligible,
                owner_eligible: request.owner_eligible,
                controller_eligible: request.controller_eligible,
                tribal_owner_eligible: request.tribal_owner_eligible,
                additional_gate_eligible: request.additional_gate_eligible,
            })?;
        Ok(VanillaNativeMigrationStateQueryReport { context, query })
    }
}
