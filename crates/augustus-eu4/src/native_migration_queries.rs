// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::BTreeMap;

use augustus_core::{
    VanillaNativeMigrationCostInput, VanillaNativeMigrationGateInput,
    VanillaNativeMigrationGateResult,
};
use serde::{Deserialize, Serialize};

const POWER_NAMES: [&str; 3] = ["adm_power", "dip_power", "mil_power"];

/// EU4 values needed to price native migration, excluding live power balances.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaNativeMigrationPricing {
    pub define_adm_power_milli: i32,
    pub define_dip_power_milli: i32,
    pub define_mil_power_milli: i32,
    pub migration_steps: u32,
    pub destination_in_native_membership: bool,
    pub all_power_cost_modifier_milli: i32,
    pub country_migration_cost_modifier_milli: i32,
    pub province_migration_cost_modifier_milli: i32,
}

/// Caller-resolved native migration context paired with live world balances.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaNativeMigrationQuery {
    pub country: String,
    pub destination: u32,
    pub pricing: VanillaNativeMigrationPricing,
    pub native_membership_known: bool,
    pub native_mode_eligible: Option<bool>,
    pub owner_eligible: Option<bool>,
    pub controller_eligible: Option<bool>,
    pub tribal_owner_eligible: Option<bool>,
    pub additional_gate_eligible: Option<bool>,
}

/// Read-only result of revalidating a migration query against the current world.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct VanillaNativeMigrationQueryReport {
    pub country: String,
    pub destination: u32,
    pub current_capital: Option<u32>,
    pub available_power: BTreeMap<String, Option<i64>>,
    pub evaluation: Option<VanillaNativeMigrationGateResult>,
    pub unresolved: Vec<String>,
}

impl crate::VanillaWorld {
    /// Re-evaluates native migration using the world's current power and capital.
    pub fn query_native_migration(
        &self,
        query: &VanillaNativeMigrationQuery,
    ) -> Result<VanillaNativeMigrationQueryReport, crate::Error> {
        let country_entity = self.initial.country_entity(&query.country).ok_or_else(|| {
            crate::Error::message(
                "native-migration query",
                format!("unknown EU4 country {}", query.country),
            )
        })?;
        if self.initial.province_entity(query.destination).is_none() {
            return Err(crate::Error::message(
                "native-migration query",
                format!("unknown EU4 destination province {}", query.destination),
            ));
        }

        let current_capital = self.initial.bound_capital(&query.country);
        let metrics = self.initial.world().metrics().entities.get(&country_entity);
        let mut available_power = BTreeMap::new();
        let mut converted_power = [None; 3];
        let mut unresolved = Vec::new();

        for (index, name) in POWER_NAMES.iter().enumerate() {
            let value = metrics
                .and_then(|values| values.get(*name))
                .copied()
                .flatten();
            available_power.insert((*name).to_string(), value);
            match value {
                None => unresolved.push(format!("{name}_unknown")),
                Some(value) if value < 0 => {
                    unresolved.push(format!("{name}_unsupported_negative"));
                }
                Some(value) => match i32::try_from(value) {
                    Ok(value) => converted_power[index] = Some(value),
                    Err(_) => unresolved.push(format!("{name}_out_of_range")),
                },
            }
        }

        let [
            Some(available_adm_power),
            Some(available_dip_power),
            Some(available_mil_power),
        ] = converted_power
        else {
            return Ok(VanillaNativeMigrationQueryReport {
                country: query.country.clone(),
                destination: query.destination,
                current_capital,
                available_power,
                evaluation: None,
                unresolved,
            });
        };

        let pricing = &query.pricing;
        let input = VanillaNativeMigrationGateInput {
            source_country: Some(country_entity),
            current_capital,
            destination_province: Some(query.destination),
            cost: VanillaNativeMigrationCostInput {
                define_adm_power_milli: pricing.define_adm_power_milli,
                define_dip_power_milli: pricing.define_dip_power_milli,
                define_mil_power_milli: pricing.define_mil_power_milli,
                migration_steps: pricing.migration_steps,
                destination_in_native_membership: pricing.destination_in_native_membership,
                all_power_cost_modifier_milli: pricing.all_power_cost_modifier_milli,
                country_migration_cost_modifier_milli: pricing
                    .country_migration_cost_modifier_milli,
                province_migration_cost_modifier_milli: pricing
                    .province_migration_cost_modifier_milli,
                available_adm_power,
                available_dip_power,
                available_mil_power,
            },
            native_membership_known: query.native_membership_known,
            native_mode_eligible: query.native_mode_eligible,
            owner_eligible: query.owner_eligible,
            controller_eligible: query.controller_eligible,
            tribal_owner_eligible: query.tribal_owner_eligible,
            additional_gate_eligible: query.additional_gate_eligible,
        };
        let evaluation = self
            .calculate_vanilla_native_migration_gate(query.destination, input)
            .map_err(|error| crate::Error::caused_by("native-migration query", error))?;
        unresolved.extend(evaluation.unresolved.iter().cloned());

        Ok(VanillaNativeMigrationQueryReport {
            country: query.country.clone(),
            destination: query.destination,
            current_capital,
            available_power,
            evaluation: Some(evaluation),
            unresolved,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, fs, path::Path};

    use crate::VanillaWorld;

    use super::{VanillaNativeMigrationPricing, VanillaNativeMigrationQuery};

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
        let contents = contents.replace(
            "HRE_RANK = 3,",
            "HRE_RANK = 3,\n        DEBT_LIMIT_DIP = -999,",
        );
        fs::write(defines, contents).unwrap();
        fixture
    }

    fn load_world() -> (tempfile::TempDir, VanillaWorld) {
        load_world_at("1444.11.11")
    }

    fn load_world_at(date: &str) -> (tempfile::TempDir, VanillaWorld) {
        let fixture = fixture();
        let world = VanillaWorld::load(fixture.path(), date).unwrap();
        (fixture, world)
    }

    fn pricing() -> VanillaNativeMigrationPricing {
        VanillaNativeMigrationPricing {
            define_adm_power_milli: 0,
            define_dip_power_milli: 0,
            define_mil_power_milli: 50_000,
            migration_steps: 2,
            destination_in_native_membership: false,
            all_power_cost_modifier_milli: 0,
            country_migration_cost_modifier_milli: 0,
            province_migration_cost_modifier_milli: 0,
        }
    }

    fn query() -> VanillaNativeMigrationQuery {
        VanillaNativeMigrationQuery {
            country: "AAA".into(),
            destination: 4,
            pricing: pricing(),
            native_membership_known: true,
            native_mode_eligible: Some(true),
            owner_eligible: Some(true),
            controller_eligible: Some(true),
            tribal_owner_eligible: Some(true),
            additional_gate_eligible: Some(true),
        }
    }

    fn set_power(world: &mut VanillaWorld, adm: Option<i64>, dip: Option<i64>, mil: Option<i64>) {
        world
            .set_country_metric_values(BTreeMap::from([(
                "AAA".into(),
                BTreeMap::from([
                    ("adm_power".into(), adm),
                    ("dip_power".into(), dip),
                    ("mil_power".into(), mil),
                ]),
            )]))
            .unwrap();
    }

    #[test]
    fn query_rechecks_current_power_and_restored_power() {
        let (_fixture, mut world) = load_world();
        set_power(&mut world, Some(140), Some(126), Some(150));
        let request = query();

        let passed = world.query_native_migration(&request).unwrap();
        assert_eq!(passed.available_power["mil_power"], Some(150));
        assert_eq!(
            passed.evaluation.as_ref().unwrap().cost.unwrap().mil_cost,
            150
        );
        assert_eq!(
            passed.evaluation.as_ref().unwrap().resolved_gate,
            Some(true)
        );

        set_power(&mut world, Some(140), Some(126), Some(149));
        let failed = world.query_native_migration(&request).unwrap();
        assert_eq!(failed.available_power["mil_power"], Some(149));
        assert_eq!(
            failed.evaluation.as_ref().unwrap().resolved_gate,
            Some(false)
        );

        let saved_at_149 = world.save().unwrap();
        set_power(&mut world, Some(140), Some(126), Some(150));
        assert_eq!(
            world
                .query_native_migration(&request)
                .unwrap()
                .evaluation
                .unwrap()
                .resolved_gate,
            Some(true)
        );
        world.restore(&saved_at_149).unwrap();
        let restored = world.query_native_migration(&request).unwrap();
        assert_eq!(restored.available_power["mil_power"], Some(149));
        assert_eq!(
            restored.evaluation.as_ref().unwrap().resolved_gate,
            Some(false)
        );
    }

    #[test]
    fn unknown_power_stays_unknown_without_mutating_world() {
        let (_fixture, mut world) = load_world();
        set_power(&mut world, Some(140), Some(126), None);
        let before = world.save().unwrap();

        let report = world.query_native_migration(&query()).unwrap();

        assert_eq!(report.available_power["mil_power"], None);
        assert_eq!(report.evaluation, None);
        assert!(report.unresolved.contains(&"mil_power_unknown".into()));
        assert_eq!(world.save().unwrap(), before);
    }

    #[test]
    fn negative_and_out_of_range_power_are_reported_without_calculation() {
        let (_fixture, mut world) = load_world();
        set_power(&mut world, Some(140), Some(126), Some(-1));
        let negative = world.query_native_migration(&query()).unwrap();
        assert_eq!(negative.available_power["mil_power"], Some(-1));
        assert_eq!(negative.evaluation, None);
        assert!(
            negative
                .unresolved
                .contains(&"mil_power_unsupported_negative".into())
        );

        let above_i32 = i64::from(i32::MAX) + 1;
        set_power(&mut world, Some(140), Some(126), Some(above_i32));
        let overflow = world.query_native_migration(&query()).unwrap();
        assert_eq!(overflow.available_power["mil_power"], Some(above_i32));
        assert_eq!(overflow.evaluation, None);
        assert!(
            overflow
                .unresolved
                .contains(&"mil_power_out_of_range".into())
        );
    }

    #[test]
    fn unknown_membership_does_not_assume_a_zero_cost() {
        let (_fixture, mut world) = load_world();
        set_power(&mut world, Some(140), Some(126), Some(150));
        let mut request = query();
        request.native_membership_known = false;

        let report = world.query_native_migration(&request).unwrap();
        let evaluation = report.evaluation.unwrap();
        assert_eq!(evaluation.cost, None);
        assert_eq!(evaluation.cost_eligible, None);
        assert_eq!(evaluation.resolved_gate, None);
        assert!(
            report
                .unresolved
                .contains(&"native_membership_unknown".into())
        );
    }

    #[test]
    fn query_uses_capital_from_the_restored_live_world() {
        let (_fixture, mut world) = load_world_at("1430.1.1");
        set_power(&mut world, Some(140), Some(126), Some(150));
        assert_eq!(world.initial.bound_capital("AAA"), Some(1));
        let country = world.initial.country_entity("AAA").unwrap();
        let new_capital = world.initial.province_entity(2).unwrap();
        let mut snapshot: serde_json::Value = serde_json::from_str(&world.save().unwrap()).unwrap();
        snapshot["entities"][country.0]["capital"] = serde_json::json!(new_capital.0);
        world
            .restore(&serde_json::to_string(&snapshot).unwrap())
            .unwrap();

        let report = world.query_native_migration(&query()).unwrap();
        assert_eq!(report.current_capital, Some(2));
        assert_eq!(report.evaluation.unwrap().current_capital, Some(2));
    }

    #[test]
    fn missing_live_capital_remains_unresolved() {
        let (_fixture, mut world) = load_world();
        set_power(&mut world, Some(140), Some(126), Some(150));
        let country = world.initial.country_entity("AAA").unwrap();
        let mut snapshot: serde_json::Value = serde_json::from_str(&world.save().unwrap()).unwrap();
        snapshot["entities"][country.0]["capital"] = serde_json::Value::Null;
        world
            .restore(&serde_json::to_string(&snapshot).unwrap())
            .unwrap();

        let report = world.query_native_migration(&query()).unwrap();
        assert_eq!(report.current_capital, None);
        let evaluation = report.evaluation.unwrap();
        assert_eq!(evaluation.current_capital, None);
        assert_eq!(evaluation.resolved_gate, None);
        assert!(
            report
                .unresolved
                .contains(&"current_capital_unknown".into())
        );
    }

    #[test]
    fn unknown_country_and_destination_are_rejected() {
        let (_fixture, world) = load_world();
        let mut request = query();
        request.country = "ZZZ".into();
        assert!(world.query_native_migration(&request).is_err());

        request.country = "AAA".into();
        request.destination = 999;
        assert!(world.query_native_migration(&request).is_err());
    }

    #[test]
    fn query_input_rejects_live_resource_and_capital_overrides() {
        let request = serde_json::to_value(query()).unwrap();
        let mut with_power_override = request.clone();
        with_power_override["pricing"]["available_mil_power"] = serde_json::json!(150);
        assert!(
            serde_json::from_value::<VanillaNativeMigrationQuery>(with_power_override).is_err()
        );

        let mut with_capital_override = request;
        with_capital_override["capital"] = serde_json::json!(1);
        assert!(
            serde_json::from_value::<VanillaNativeMigrationQuery>(with_capital_override).is_err()
        );
    }
}
