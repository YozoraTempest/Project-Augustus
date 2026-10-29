// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use serde::{Deserialize, Serialize};

use augustus_core::{VanillaNativeMigrationExecutionInput, VanillaNativeMigrationExecutionResult};

use crate::{
    VanillaNativeMigrationStatePricing, VanillaNativeMigrationStateQuery,
    VanillaNativeMigrationStateQueryReport, VanillaWorld,
};

/// Caller-resolved execution request using the current native state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaNativeMigrationExecutionRequest {
    pub country: String,
    pub destination: u32,
    pub pricing: VanillaNativeMigrationStatePricing,
    pub expected_migration_steps: u32,
    pub next_migration_steps: u32,
    pub native_mode_eligible: Option<bool>,
    pub owner_eligible: Option<bool>,
    pub controller_eligible: Option<bool>,
    pub tribal_owner_eligible: Option<bool>,
    pub additional_gate_eligible: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VanillaNativeMigrationExecutionStatus {
    Applied,
    Rejected,
    Unresolved,
}

/// Execution-stage report. Rejection and unresolved state are returned as a
/// report so callers can inspect why no mutation occurred.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct VanillaNativeMigrationExecutionReport {
    pub status: VanillaNativeMigrationExecutionStatus,
    pub query: VanillaNativeMigrationStateQueryReport,
    pub execution: Option<VanillaNativeMigrationExecutionResult>,
    pub execution_error: Option<String>,
}

impl VanillaWorld {
    /// Rechecks the live gate, then atomically debits power and writes the
    /// caller-resolved next migration step. Capital, owner, controller and
    /// tribal-owner effects are intentionally outside this stage.
    pub fn execute_native_migration_from_state(
        &mut self,
        request: &VanillaNativeMigrationExecutionRequest,
    ) -> Result<VanillaNativeMigrationExecutionReport, crate::Error> {
        let query = self.query_native_migration_from_state(&VanillaNativeMigrationStateQuery {
            country: request.country.clone(),
            destination: request.destination,
            pricing: request.pricing.clone(),
            native_mode_eligible: request.native_mode_eligible,
            owner_eligible: request.owner_eligible,
            controller_eligible: request.controller_eligible,
            tribal_owner_eligible: request.tribal_owner_eligible,
            additional_gate_eligible: request.additional_gate_eligible,
        })?;
        let Some(evaluation) = query.query.evaluation.as_ref() else {
            return Ok(VanillaNativeMigrationExecutionReport {
                status: VanillaNativeMigrationExecutionStatus::Unresolved,
                query,
                execution: None,
                execution_error: None,
            });
        };
        match evaluation.resolved_gate {
            None => {
                return Ok(VanillaNativeMigrationExecutionReport {
                    status: VanillaNativeMigrationExecutionStatus::Unresolved,
                    query,
                    execution: None,
                    execution_error: None,
                });
            }
            Some(false) => {
                return Ok(VanillaNativeMigrationExecutionReport {
                    status: VanillaNativeMigrationExecutionStatus::Rejected,
                    query,
                    execution: None,
                    execution_error: None,
                });
            }
            Some(true) => {}
        }
        let cost = evaluation
            .cost
            .expect("resolved native migration gate has cost");
        let source_country = self
            .initial
            .country_entity(&request.country)
            .expect("validated native migration country");
        let execution =
            self.initial
                .execute_vanilla_native_migration(VanillaNativeMigrationExecutionInput {
                    source_country,
                    expected_migration_steps: request.expected_migration_steps,
                    next_migration_steps: request.next_migration_steps,
                    adm_cost: cost.adm_cost,
                    dip_cost: cost.dip_cost,
                    mil_cost: cost.mil_cost,
                });
        match execution {
            Ok(execution) => Ok(VanillaNativeMigrationExecutionReport {
                status: VanillaNativeMigrationExecutionStatus::Applied,
                query,
                execution: Some(execution),
                execution_error: None,
            }),
            Err(error) => Ok(VanillaNativeMigrationExecutionReport {
                status: VanillaNativeMigrationExecutionStatus::Rejected,
                query,
                execution: None,
                execution_error: Some(error.to_string()),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, fs, path::Path};

    use super::{VanillaNativeMigrationExecutionRequest, VanillaNativeMigrationExecutionStatus};
    use crate::{
        ContentRoot, ContentSet, ContentSource, VanillaExternalNativeMigrationState,
        VanillaNativeMigrationStatePricing, VanillaNativeMigrationStateQuery, VanillaWorld,
    };

    fn fixture() -> tempfile::TempDir {
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world");
        let fixture = tempfile::tempdir().unwrap();
        for entry in walkdir::WalkDir::new(&source) {
            let entry = entry.unwrap();
            let target = fixture
                .path()
                .join(entry.path().strip_prefix(&source).unwrap());
            if entry.file_type().is_dir() {
                fs::create_dir_all(target).unwrap();
            } else {
                fs::copy(entry.path(), target).unwrap();
            }
        }
        let defines = fixture.path().join("common/defines.lua");
        let contents = fs::read_to_string(&defines).unwrap();
        fs::write(
            defines,
            format!(
                "{contents}\nNDefines.NCountry.DEBT_LIMIT_DIP=-999\nNDefines.NCountry.TRIBE_MIGRATION_ADM_COST=0\nNDefines.NCountry.TRIBE_MIGRATION_DIP_COST=0\nNDefines.NCountry.TRIBE_MIGRATION_MIL_COST=50\n"
            ),
        )
        .unwrap();
        fixture
    }

    fn load_world() -> (tempfile::TempDir, VanillaWorld) {
        let fixture = fixture();
        let content = ContentSet::resolve(vec![ContentSource::base(
            ContentRoot::open(fixture.path()).unwrap(),
        )])
        .unwrap();
        (
            fixture,
            VanillaWorld::load_from_content(&content, "1444.11.11").unwrap(),
        )
    }

    fn request(expected_steps: u32, next_steps: u32) -> VanillaNativeMigrationExecutionRequest {
        VanillaNativeMigrationExecutionRequest {
            country: "AAA".into(),
            destination: 4,
            pricing: VanillaNativeMigrationStatePricing {
                all_power_cost_modifier_milli: 0,
                country_migration_cost_modifier_milli: 0,
                province_migration_cost_modifier_milli: 0,
            },
            expected_migration_steps: expected_steps,
            next_migration_steps: next_steps,
            native_mode_eligible: Some(true),
            owner_eligible: Some(true),
            controller_eligible: Some(true),
            tribal_owner_eligible: Some(true),
            additional_gate_eligible: Some(true),
        }
    }

    fn prepare(
        world: &mut VanillaWorld,
        mil: Option<i64>,
        provinces: Option<Vec<u32>>,
        steps: Option<u32>,
    ) {
        world
            .set_country_metric_values(BTreeMap::from([(
                "AAA".into(),
                BTreeMap::from([
                    ("adm_power".into(), Some(0)),
                    ("dip_power".into(), Some(0)),
                    ("mil_power".into(), mil),
                ]),
            )]))
            .unwrap();
        world
            .set_country_native_migration(BTreeMap::from([(
                "AAA".into(),
                VanillaExternalNativeMigrationState {
                    provinces,
                    migration_steps: steps,
                },
            )]))
            .unwrap();
    }

    #[test]
    fn execution_rechecks_gate_then_applies_power_and_explicit_steps() {
        let (_fixture, mut world) = load_world();
        prepare(&mut world, Some(50), Some(vec![4]), Some(2));
        let report = world
            .execute_native_migration_from_state(&request(2, 3))
            .unwrap();
        assert_eq!(
            report.status,
            VanillaNativeMigrationExecutionStatus::Applied
        );
        assert_eq!(
            report
                .query
                .query
                .evaluation
                .as_ref()
                .unwrap()
                .resolved_gate,
            Some(true)
        );
        assert_eq!(report.execution.as_ref().unwrap().costs, [0, 0, 50]);
        assert_eq!(report.execution.as_ref().unwrap().after, [0, 0, 0]);
        assert_eq!(report.execution.as_ref().unwrap().migration_steps_after, 3);
        assert_eq!(
            world.initial.world().metrics().entities[&world.initial.country_entity("AAA").unwrap()]
                ["mil_power"],
            Some(0)
        );
        assert_eq!(report.query.context.migration_steps, Some(2));
    }

    #[test]
    fn execution_rejects_gate_or_stale_steps_without_mutation() {
        let (_fixture, mut world) = load_world();
        prepare(&mut world, Some(49), Some(vec![4]), Some(2));
        let before = world.save().unwrap();
        let rejected = world
            .execute_native_migration_from_state(&request(2, 3))
            .unwrap();
        assert_eq!(
            rejected.status,
            VanillaNativeMigrationExecutionStatus::Rejected
        );
        assert!(rejected.execution.is_none());
        assert_eq!(world.save().unwrap(), before);

        prepare(&mut world, Some(50), Some(vec![4]), Some(2));
        let before_stale = world.save().unwrap();
        let stale = world
            .execute_native_migration_from_state(&request(1, 3))
            .unwrap();
        assert_eq!(
            stale.status,
            VanillaNativeMigrationExecutionStatus::Rejected
        );
        assert!(stale.execution_error.unwrap().contains("steps changed"));
        assert_eq!(world.save().unwrap(), before_stale);
    }

    #[test]
    fn execution_unknown_state_or_power_is_unresolved_without_mutation() {
        let (_fixture, mut world) = load_world();
        prepare(&mut world, Some(50), None, Some(2));
        let before_unknown_list = world.save().unwrap();
        let unknown_list = world
            .execute_native_migration_from_state(&request(2, 3))
            .unwrap();
        assert_eq!(
            unknown_list.status,
            VanillaNativeMigrationExecutionStatus::Unresolved
        );
        assert_eq!(world.save().unwrap(), before_unknown_list);

        prepare(&mut world, None, Some(vec![4]), Some(2));
        let before_unknown_power = world.save().unwrap();
        let unknown_power = world
            .execute_native_migration_from_state(&request(2, 3))
            .unwrap();
        assert_eq!(
            unknown_power.status,
            VanillaNativeMigrationExecutionStatus::Unresolved
        );
        assert_eq!(world.save().unwrap(), before_unknown_power);
    }

    #[test]
    fn execution_unresolved_gate_is_unresolved_without_mutation() {
        let (_fixture, mut world) = load_world();
        prepare(&mut world, Some(50), Some(vec![4]), Some(2));
        let mut unresolved_request = request(2, 3);
        unresolved_request.owner_eligible = None;
        let before = world.save().unwrap();

        let report = world
            .execute_native_migration_from_state(&unresolved_request)
            .unwrap();

        assert_eq!(
            report.status,
            VanillaNativeMigrationExecutionStatus::Unresolved
        );
        assert_eq!(report.query.query.evaluation.unwrap().resolved_gate, None);
        assert!(report.execution.is_none());
        assert!(report.execution_error.is_none());
        assert_eq!(world.save().unwrap(), before);
    }

    #[test]
    fn migration_execution_and_state_query_requests_roundtrip_and_reject_unknown_fields() {
        let execution = request(2, 3);
        let encoded = serde_json::to_value(&execution).unwrap();
        let decoded: VanillaNativeMigrationExecutionRequest =
            serde_json::from_value(encoded.clone()).unwrap();
        assert_eq!(decoded, execution);

        let mut unknown_execution = encoded.as_object().unwrap().clone();
        unknown_execution.insert("unexpected".into(), serde_json::json!(true));
        assert!(
            serde_json::from_value::<VanillaNativeMigrationExecutionRequest>(
                serde_json::Value::Object(unknown_execution)
            )
            .is_err()
        );

        let mut nested_unknown_execution = encoded.as_object().unwrap().clone();
        let mut pricing = nested_unknown_execution["pricing"]
            .as_object()
            .unwrap()
            .clone();
        pricing.insert("unexpected".into(), serde_json::json!(true));
        nested_unknown_execution.insert("pricing".into(), serde_json::Value::Object(pricing));
        assert!(
            serde_json::from_value::<VanillaNativeMigrationExecutionRequest>(
                serde_json::Value::Object(nested_unknown_execution)
            )
            .is_err()
        );

        let state_query = VanillaNativeMigrationStateQuery {
            country: execution.country,
            destination: execution.destination,
            pricing: execution.pricing,
            native_mode_eligible: execution.native_mode_eligible,
            owner_eligible: execution.owner_eligible,
            controller_eligible: execution.controller_eligible,
            tribal_owner_eligible: execution.tribal_owner_eligible,
            additional_gate_eligible: execution.additional_gate_eligible,
        };
        let encoded = serde_json::to_value(&state_query).unwrap();
        let decoded: VanillaNativeMigrationStateQuery =
            serde_json::from_value(encoded.clone()).unwrap();
        assert_eq!(decoded, state_query);

        let mut unknown_state_query = encoded.as_object().unwrap().clone();
        unknown_state_query.insert("unexpected".into(), serde_json::json!(true));
        assert!(
            serde_json::from_value::<VanillaNativeMigrationStateQuery>(serde_json::Value::Object(
                unknown_state_query
            ))
            .is_err()
        );

        let mut nested_unknown_state_query = encoded.as_object().unwrap().clone();
        let mut pricing = nested_unknown_state_query["pricing"]
            .as_object()
            .unwrap()
            .clone();
        pricing.insert("unexpected".into(), serde_json::json!(true));
        nested_unknown_state_query.insert("pricing".into(), serde_json::Value::Object(pricing));
        assert!(
            serde_json::from_value::<VanillaNativeMigrationStateQuery>(serde_json::Value::Object(
                nested_unknown_state_query
            ))
            .is_err()
        );
    }
}
