// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use serde::{Deserialize, Serialize};

use augustus_core::{VanillaNativeMigrationTransferInput, VanillaNativeMigrationTransferResult};

use crate::{
    VanillaNativeMigrationStatePricing, VanillaNativeMigrationStateQuery,
    VanillaNativeMigrationStateQueryReport, VanillaWorld,
};

/// Caller-resolved transfer request using the current native state and costs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaNativeMigrationTransferRequest {
    pub country: String,
    pub destination: u32,
    pub pricing: VanillaNativeMigrationStatePricing,
    pub expected_current_capital: u32,
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
pub enum VanillaNativeMigrationTransferStatus {
    Applied,
    Rejected,
    Unresolved,
}

/// Transfer report. Rejected and unresolved results carry no core mutation.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct VanillaNativeMigrationTransferReport {
    pub status: VanillaNativeMigrationTransferStatus,
    pub query: VanillaNativeMigrationStateQueryReport,
    pub transfer: Option<VanillaNativeMigrationTransferResult>,
    pub transfer_error: Option<String>,
}

impl VanillaWorld {
    /// Rechecks the live gate, then applies the represented owner, capital,
    /// power and migration-step transaction. Controller, tribal-owner and raw
    /// native-list lifecycle remain outside this adapter's model.
    pub fn execute_native_migration_transfer_from_state(
        &mut self,
        request: &VanillaNativeMigrationTransferRequest,
    ) -> Result<VanillaNativeMigrationTransferReport, crate::Error> {
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
            return Ok(VanillaNativeMigrationTransferReport {
                status: VanillaNativeMigrationTransferStatus::Unresolved,
                query,
                transfer: None,
                transfer_error: None,
            });
        };
        match evaluation.resolved_gate {
            None => {
                return Ok(VanillaNativeMigrationTransferReport {
                    status: VanillaNativeMigrationTransferStatus::Unresolved,
                    query,
                    transfer: None,
                    transfer_error: None,
                });
            }
            Some(false) => {
                return Ok(VanillaNativeMigrationTransferReport {
                    status: VanillaNativeMigrationTransferStatus::Rejected,
                    query,
                    transfer: None,
                    transfer_error: None,
                });
            }
            Some(true) => {}
        }
        if query.query.current_capital != Some(request.expected_current_capital) {
            return Ok(VanillaNativeMigrationTransferReport {
                status: VanillaNativeMigrationTransferStatus::Rejected,
                query,
                transfer: None,
                transfer_error: Some(
                    "native migration current capital changed before transfer".into(),
                ),
            });
        }
        let cost = evaluation
            .cost
            .expect("resolved native migration gate has cost");
        let source_country = self
            .initial
            .country_entity(&request.country)
            .expect("validated native migration country");
        let Some(old_capital) = self
            .initial
            .province_entity(request.expected_current_capital)
        else {
            return Ok(VanillaNativeMigrationTransferReport {
                status: VanillaNativeMigrationTransferStatus::Rejected,
                query,
                transfer: None,
                transfer_error: Some("unknown native migration current capital".into()),
            });
        };
        let destination = self
            .initial
            .province_entity(request.destination)
            .expect("validated native migration destination");
        let transfer = self.initial.execute_vanilla_native_migration_transfer(
            VanillaNativeMigrationTransferInput {
                source_country,
                expected_current_capital: old_capital,
                destination_province: destination,
                expected_migration_steps: request.expected_migration_steps,
                next_migration_steps: request.next_migration_steps,
                adm_cost: cost.adm_cost,
                dip_cost: cost.dip_cost,
                mil_cost: cost.mil_cost,
            },
        );
        match transfer {
            Ok(transfer) => Ok(VanillaNativeMigrationTransferReport {
                status: VanillaNativeMigrationTransferStatus::Applied,
                query,
                transfer: Some(transfer),
                transfer_error: None,
            }),
            Err(error) => Ok(VanillaNativeMigrationTransferReport {
                status: VanillaNativeMigrationTransferStatus::Rejected,
                query,
                transfer: None,
                transfer_error: Some(error.to_string()),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, fs, path::Path};

    use super::{VanillaNativeMigrationTransferRequest, VanillaNativeMigrationTransferStatus};
    use crate::{
        ContentRoot, ContentSet, ContentSource, VanillaExternalNativeMigrationState,
        VanillaNativeMigrationStatePricing, VanillaWorld,
    };

    fn fixture(strip_controllers: bool, destination_controller: bool) -> tempfile::TempDir {
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
        if strip_controllers {
            for province in ["1 - North.txt", "2 - East.txt"] {
                let path = fixture.path().join("history/provinces").join(province);
                let contents = fs::read_to_string(&path).unwrap();
                fs::write(
                    path,
                    contents
                        .lines()
                        .filter(|line| !line.trim_start().starts_with("controller ="))
                        .collect::<Vec<_>>()
                        .join("\n"),
                )
                .unwrap();
            }
        }
        if destination_controller {
            let path = fixture.path().join("history/provinces/4 - Highland.txt");
            let mut contents = fs::read_to_string(&path).unwrap();
            contents.push_str("\ncontroller = AAA\n");
            fs::write(path, contents).unwrap();
        }
        fixture
    }

    fn load_world(strip_controllers: bool, destination_controller: bool) -> VanillaWorld {
        let fixture = fixture(strip_controllers, destination_controller);
        let content = ContentSet::resolve(vec![ContentSource::base(
            ContentRoot::open(fixture.path()).unwrap(),
        )])
        .unwrap();
        let mut world = VanillaWorld::load_from_content(&content, "1444.11.11").unwrap();
        world
            .set_country_metric_values(BTreeMap::from([(
                "AAA".into(),
                BTreeMap::from([
                    ("adm_power".into(), Some(0)),
                    ("dip_power".into(), Some(0)),
                    ("mil_power".into(), Some(150)),
                ]),
            )]))
            .unwrap();
        world
            .set_country_native_migration(BTreeMap::from([(
                "AAA".into(),
                VanillaExternalNativeMigrationState {
                    provinces: Some(vec![1]),
                    migration_steps: Some(2),
                },
            )]))
            .unwrap();
        world
    }

    fn request(destination: u32) -> VanillaNativeMigrationTransferRequest {
        VanillaNativeMigrationTransferRequest {
            country: "AAA".into(),
            destination,
            pricing: VanillaNativeMigrationStatePricing {
                all_power_cost_modifier_milli: 0,
                country_migration_cost_modifier_milli: 0,
                province_migration_cost_modifier_milli: 0,
            },
            expected_current_capital: 2,
            expected_migration_steps: 2,
            next_migration_steps: 3,
            native_mode_eligible: Some(true),
            owner_eligible: Some(true),
            controller_eligible: Some(true),
            tribal_owner_eligible: Some(true),
            additional_gate_eligible: Some(true),
        }
    }

    #[test]
    fn transfer_adapter_applies_core_transaction_and_restores() {
        let mut world = load_world(true, false);
        let report = world
            .execute_native_migration_transfer_from_state(&request(4))
            .unwrap();
        assert_eq!(report.status, VanillaNativeMigrationTransferStatus::Applied);
        assert_eq!(report.transfer.as_ref().unwrap().costs, [0, 0, 150]);
        let country = world.initial.country_entity("AAA").unwrap();
        let old = world.initial.province_entity(2).unwrap();
        let destination = world.initial.province_entity(4).unwrap();
        assert_eq!(
            world.initial.world().entity(country).unwrap().capital,
            Some(destination)
        );
        assert_eq!(world.initial.world().entity(old).unwrap().owner, None);
        assert_eq!(
            world.initial.world().entity(destination).unwrap().owner,
            Some(country)
        );
        assert_eq!(world.initial.province(2).unwrap().owner, None);
        assert_eq!(
            world.initial.province(4).unwrap().owner.as_deref(),
            Some("AAA")
        );
        assert_eq!(
            world.initial.world().native_migration_states().countries[&country].provinces,
            Some(vec![world.initial.province_entity(1).unwrap()])
        );

        let saved = world.save().unwrap();
        world.restore(&saved).unwrap();
        assert_eq!(world.initial.bound_capital("AAA"), Some(4));
        assert_eq!(world.initial.owned_provinces("AAA"), vec![4]);
        assert_eq!(world.initial.province(2).unwrap().owner, None);
        assert_eq!(
            world.initial.province(4).unwrap().owner.as_deref(),
            Some("AAA")
        );
    }

    #[test]
    fn transfer_adapter_rejects_same_capital_and_stale_capital_atomically() {
        let mut same_capital = load_world(true, false);
        let before_same_capital = same_capital.save().unwrap();
        let same_capital_report = same_capital
            .execute_native_migration_transfer_from_state(&request(2))
            .unwrap();
        assert_eq!(
            same_capital_report.status,
            VanillaNativeMigrationTransferStatus::Rejected
        );
        assert!(
            same_capital_report
                .transfer_error
                .unwrap()
                .contains("destination must differ")
        );
        assert_eq!(same_capital.save().unwrap(), before_same_capital);

        let mut stale = load_world(true, false);
        let before_stale = stale.save().unwrap();
        let mut stale_request = request(4);
        stale_request.expected_current_capital = 1;
        let stale_report = stale
            .execute_native_migration_transfer_from_state(&stale_request)
            .unwrap();
        assert_eq!(
            stale_report.status,
            VanillaNativeMigrationTransferStatus::Rejected
        );
        assert!(
            stale_report
                .transfer_error
                .unwrap()
                .contains("current capital changed")
        );
        assert_eq!(stale.save().unwrap(), before_stale);
    }

    #[test]
    fn transfer_adapter_rejects_unresolved_gate_and_known_controller_atomically() {
        let mut unresolved = load_world(true, false);
        let before_unresolved = unresolved.save().unwrap();
        let mut unresolved_request = request(4);
        unresolved_request.owner_eligible = None;
        let unresolved_report = unresolved
            .execute_native_migration_transfer_from_state(&unresolved_request)
            .unwrap();
        assert_eq!(
            unresolved_report.status,
            VanillaNativeMigrationTransferStatus::Unresolved
        );
        assert_eq!(unresolved.save().unwrap(), before_unresolved);

        let mut controller = load_world(true, true);
        let before_controller = controller.save().unwrap();
        let controller_report = controller
            .execute_native_migration_transfer_from_state(&request(4))
            .unwrap();
        assert_eq!(
            controller_report.status,
            VanillaNativeMigrationTransferStatus::Rejected
        );
        assert!(
            controller_report
                .transfer_error
                .unwrap()
                .contains("controller relation")
        );
        assert_eq!(controller.save().unwrap(), before_controller);
    }
}
