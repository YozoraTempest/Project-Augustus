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

/// Caller-resolved pricing inputs when migration costs come from EU4 defines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaNativeMigrationContentPricing {
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

/// Caller-resolved context priced with definitions from the loaded EU4 content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaNativeMigrationContentQuery {
    pub country: String,
    pub destination: u32,
    pub pricing: VanillaNativeMigrationContentPricing,
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

pub(super) struct PreparedNativeMigrationQuery {
    country_entity: augustus_core::EntityId,
    current_capital: Option<u32>,
    available_power: BTreeMap<String, Option<i64>>,
    converted_power: [Option<i32>; 3],
    unresolved: Vec<String>,
}

impl PreparedNativeMigrationQuery {
    pub(super) fn report(
        &self,
        country: &str,
        destination: u32,
        additional_unresolved: impl IntoIterator<Item = String>,
    ) -> VanillaNativeMigrationQueryReport {
        let mut unresolved = self.unresolved.clone();
        unresolved.extend(additional_unresolved);
        VanillaNativeMigrationQueryReport {
            country: country.into(),
            destination,
            current_capital: self.current_capital,
            available_power: self.available_power.clone(),
            evaluation: None,
            unresolved,
        }
    }
}

impl crate::VanillaWorld {
    /// Re-evaluates native migration using the world's current power and capital.
    pub fn query_native_migration(
        &self,
        query: &VanillaNativeMigrationQuery,
    ) -> Result<VanillaNativeMigrationQueryReport, crate::Error> {
        let prepared = self.prepare_native_migration_query(&query.country, query.destination)?;
        let [
            Some(available_adm_power),
            Some(available_dip_power),
            Some(available_mil_power),
        ] = prepared.converted_power
        else {
            return Ok(prepared.report(&query.country, query.destination, []));
        };

        let pricing = &query.pricing;
        let input = VanillaNativeMigrationGateInput {
            source_country: Some(prepared.country_entity),
            current_capital: prepared.current_capital,
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
        Ok(prepared
            .report(
                &query.country,
                query.destination,
                evaluation.unresolved.iter().cloned(),
            )
            .with_evaluation(evaluation))
    }

    /// Re-evaluates migration with the current world and the loaded content's costs.
    pub fn query_native_migration_from_content(
        &self,
        query: &VanillaNativeMigrationContentQuery,
    ) -> Result<VanillaNativeMigrationQueryReport, crate::Error> {
        let prepared = self.prepare_native_migration_query(&query.country, query.destination)?;
        if let Some((name, detail)) = self
            .report
            .setup
            .native_migration_cost_define_errors
            .iter()
            .next()
        {
            let source = self
                .report
                .setup
                .definition_sources
                .get(&format!("native_migration_cost:{name}"));
            return Err(match source {
                Some(source) => crate::Error::message(&source.path, detail.clone()).with_location(
                    crate::SourceLocation {
                        line: source.line,
                        column: source.column,
                    },
                ),
                None => crate::Error::message("native-migration content query", detail.clone()),
            });
        }

        const COST_NAMES: [&str; 3] = [
            "TRIBE_MIGRATION_ADM_COST",
            "TRIBE_MIGRATION_DIP_COST",
            "TRIBE_MIGRATION_MIL_COST",
        ];
        let defines = &self.report.setup.native_migration_cost_defines_milli;
        let define_values = COST_NAMES.map(|name| defines.get(name).copied());
        let [
            Some(define_adm_power_milli),
            Some(define_dip_power_milli),
            Some(define_mil_power_milli),
        ] = define_values
        else {
            let missing = COST_NAMES
                .into_iter()
                .zip(define_values)
                .filter(|(_, value)| value.is_none())
                .map(|(name, _)| format!("native_migration_cost:missing_define:{name}"))
                .collect::<Vec<_>>();
            return Ok(prepared.report(&query.country, query.destination, missing));
        };

        let pricing = query.pricing;
        self.query_native_migration(&VanillaNativeMigrationQuery {
            country: query.country.clone(),
            destination: query.destination,
            pricing: VanillaNativeMigrationPricing {
                define_adm_power_milli,
                define_dip_power_milli,
                define_mil_power_milli,
                migration_steps: pricing.migration_steps,
                destination_in_native_membership: pricing.destination_in_native_membership,
                all_power_cost_modifier_milli: pricing.all_power_cost_modifier_milli,
                country_migration_cost_modifier_milli: pricing
                    .country_migration_cost_modifier_milli,
                province_migration_cost_modifier_milli: pricing
                    .province_migration_cost_modifier_milli,
            },
            native_membership_known: query.native_membership_known,
            native_mode_eligible: query.native_mode_eligible,
            owner_eligible: query.owner_eligible,
            controller_eligible: query.controller_eligible,
            tribal_owner_eligible: query.tribal_owner_eligible,
            additional_gate_eligible: query.additional_gate_eligible,
        })
    }

    pub(super) fn prepare_native_migration_query(
        &self,
        country: &str,
        destination: u32,
    ) -> Result<PreparedNativeMigrationQuery, crate::Error> {
        let country_entity = self.initial.country_entity(country).ok_or_else(|| {
            crate::Error::message(
                "native-migration query",
                format!("unknown EU4 country {country}"),
            )
        })?;
        if self.initial.province_entity(destination).is_none() {
            return Err(crate::Error::message(
                "native-migration query",
                format!("unknown EU4 destination province {destination}"),
            ));
        }

        let current_capital = self.initial.bound_capital(country);
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

        Ok(PreparedNativeMigrationQuery {
            country_entity,
            current_capital,
            available_power,
            converted_power,
            unresolved,
        })
    }
}

impl VanillaNativeMigrationQueryReport {
    fn with_evaluation(mut self, evaluation: VanillaNativeMigrationGateResult) -> Self {
        self.evaluation = Some(evaluation);
        self
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, fs, path::Path};

    use crate::{ContentRoot, ContentSet, ContentSource, TextEncoding, VanillaWorld};

    use super::{
        VanillaNativeMigrationContentPricing, VanillaNativeMigrationContentQuery,
        VanillaNativeMigrationPricing, VanillaNativeMigrationQuery,
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

    fn add_migration_cost_defines(root: &Path, adm: &str, dip: &str, mil: &str) {
        let path = root.join("common/defines.lua");
        let contents = fs::read_to_string(&path).unwrap();
        let replacement = format!(
            "DEBT_LIMIT_DIP = -999,\n        TRIBE_MIGRATION_ADM_COST = {adm},\n        TRIBE_MIGRATION_DIP_COST = {dip},\n        TRIBE_MIGRATION_MIL_COST = {mil},"
        );
        let updated = contents.replace("DEBT_LIMIT_DIP = -999,", &replacement);
        assert_ne!(updated, contents, "fixture define insertion anchor changed");
        fs::write(path, updated).unwrap();
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

    fn content_query() -> VanillaNativeMigrationContentQuery {
        VanillaNativeMigrationContentQuery {
            country: "AAA".into(),
            destination: 4,
            pricing: VanillaNativeMigrationContentPricing {
                migration_steps: 2,
                destination_in_native_membership: true,
                all_power_cost_modifier_milli: 0,
                country_migration_cost_modifier_milli: 0,
                province_migration_cost_modifier_milli: 0,
            },
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
    fn loaded_defines_supply_content_query_costs_and_mod_sources() {
        let base = fixture();
        add_migration_cost_defines(base.path(), "0", "0", "50");
        let mut base_world = VanillaWorld::load(base.path(), "1444.11.11").unwrap();
        set_power(&mut base_world, Some(0), Some(0), Some(50));
        let base_report = base_world
            .query_native_migration_from_content(&content_query())
            .unwrap();
        assert_eq!(
            base_world.report.setup.native_migration_cost_defines_milli["TRIBE_MIGRATION_MIL_COST"],
            50_000
        );
        assert_eq!(
            base_report
                .evaluation
                .as_ref()
                .unwrap()
                .cost
                .unwrap()
                .mil_cost,
            50
        );

        let mod_root = tempfile::tempdir().unwrap();
        fs::write(
            mod_root.path().join("descriptor.mod"),
            "name=MigrationCostMod",
        )
        .unwrap();
        let mod_defines = mod_root.path().join("common/defines");
        fs::create_dir_all(&mod_defines).unwrap();
        fs::write(
            mod_defines.join("migration_cost.lua"),
            "NDefines.NCountry.TRIBE_MIGRATION_MIL_COST = 60",
        )
        .unwrap();
        let content = ContentSet::resolve(vec![
            ContentSource::base(ContentRoot::open(base.path()).unwrap()),
            ContentSource::mod_directory(mod_root.path(), TextEncoding::Windows1252).unwrap(),
        ])
        .unwrap();
        let mut mod_world = VanillaWorld::load_from_content(&content, "1444.11.11").unwrap();
        set_power(&mut mod_world, Some(0), Some(0), Some(60));
        let mod_report = mod_world
            .query_native_migration_from_content(&content_query())
            .unwrap();

        assert_eq!(
            mod_world.report.setup.native_migration_cost_defines_milli["TRIBE_MIGRATION_MIL_COST"],
            60_000
        );
        assert_eq!(
            mod_world.report.setup.definition_sources
                ["native_migration_cost:TRIBE_MIGRATION_MIL_COST"]
                .path,
            mod_root
                .path()
                .canonicalize()
                .unwrap()
                .join("common/defines/migration_cost.lua")
        );
        assert_eq!(
            mod_report
                .evaluation
                .as_ref()
                .unwrap()
                .cost
                .unwrap()
                .mil_cost,
            60
        );
    }

    #[test]
    fn missing_and_partial_content_defines_never_produce_a_zero_fee() {
        let (_fixture, mut missing_world) = load_world();
        set_power(&mut missing_world, Some(140), Some(126), Some(50));
        assert!(
            missing_world
                .report
                .setup
                .native_migration_cost_defines_milli
                .is_empty()
        );
        let setup_json = serde_json::to_value(&missing_world.report.setup).unwrap();
        assert!(
            setup_json
                .get("native_migration_cost_defines_milli")
                .is_none()
        );
        let mut request = content_query();
        request.native_membership_known = false;
        let missing = missing_world
            .query_native_migration_from_content(&request)
            .unwrap();
        assert_eq!(missing.evaluation, None);
        assert_eq!(missing.available_power["mil_power"], Some(50));
        assert_eq!(missing.current_capital, Some(2));
        for name in [
            "TRIBE_MIGRATION_ADM_COST",
            "TRIBE_MIGRATION_DIP_COST",
            "TRIBE_MIGRATION_MIL_COST",
        ] {
            assert!(
                missing
                    .unresolved
                    .contains(&format!("native_migration_cost:missing_define:{name}"))
            );
        }
        assert!(
            !missing
                .unresolved
                .contains(&"native_membership_unknown".into())
        );

        let partial_fixture = fixture();
        let defines = partial_fixture.path().join("common/defines.lua");
        let contents = fs::read_to_string(&defines).unwrap();
        let updated = contents.replace(
            "DEBT_LIMIT_DIP = -999,",
            "DEBT_LIMIT_DIP = -999,\n        TRIBE_MIGRATION_ADM_COST = 0,\n        TRIBE_MIGRATION_MIL_COST = 50,",
        );
        fs::write(defines, updated).unwrap();
        let mut partial_world = VanillaWorld::load(partial_fixture.path(), "1444.11.11").unwrap();
        set_power(&mut partial_world, Some(140), Some(126), Some(50));
        let partial = partial_world
            .query_native_migration_from_content(&content_query())
            .unwrap();
        assert_eq!(partial.evaluation, None);
        assert!(
            partial
                .unresolved
                .contains(&"native_migration_cost:missing_define:TRIBE_MIGRATION_DIP_COST".into())
        );
        assert!(
            !partial
                .unresolved
                .contains(&"native_migration_cost:missing_define:TRIBE_MIGRATION_ADM_COST".into())
        );
        assert!(
            !partial
                .unresolved
                .contains(&"native_migration_cost:missing_define:TRIBE_MIGRATION_MIL_COST".into())
        );
    }

    #[test]
    fn content_define_precision_is_preserved_and_invalid_costs_are_query_only_errors() {
        let precise_fixture = fixture();
        add_migration_cost_defines(precise_fixture.path(), "0", "0", "50.125");
        let mut precise_world = VanillaWorld::load(precise_fixture.path(), "1444.11.11").unwrap();
        set_power(&mut precise_world, Some(0), Some(0), Some(51));
        assert_eq!(
            precise_world
                .report
                .setup
                .native_migration_cost_defines_milli["TRIBE_MIGRATION_MIL_COST"],
            50_125
        );
        let precise = precise_world
            .query_native_migration_from_content(&content_query())
            .unwrap();
        assert_eq!(
            precise.evaluation.as_ref().unwrap().cost.unwrap().mil_cost,
            51
        );

        for (mil, expected) in [
            ("-0.0000001", "nonnegative"),
            ("50.0005", "must be an exact thousandth"),
            ("2147483.648", "exceeds the i32 milli range"),
        ] {
            let invalid_fixture = fixture();
            add_migration_cost_defines(invalid_fixture.path(), "0", "0", mil);
            let mut world = VanillaWorld::load(invalid_fixture.path(), "1444.11.11").unwrap();
            assert!(
                world
                    .report
                    .setup
                    .native_migration_cost_define_errors
                    .contains_key("TRIBE_MIGRATION_MIL_COST")
            );
            set_power(&mut world, Some(0), Some(0), Some(51));
            let error = world
                .query_native_migration_from_content(&content_query())
                .unwrap_err()
                .to_string();
            assert!(error.contains("TRIBE_MIGRATION_MIL_COST"), "{error}");
            assert!(error.contains(expected), "{error}");
            let explicit = world.query_native_migration(&query()).unwrap();
            assert_eq!(
                explicit.evaluation.as_ref().unwrap().cost.unwrap().mil_cost,
                150
            );
        }
    }

    #[test]
    fn content_query_keeps_live_unknowns_and_checks_identity_before_define_errors() {
        let valid_fixture = fixture();
        add_migration_cost_defines(valid_fixture.path(), "0", "0", "50");
        let mut world = VanillaWorld::load(valid_fixture.path(), "1444.11.11").unwrap();
        set_power(&mut world, Some(140), Some(126), None);
        let report = world
            .query_native_migration_from_content(&content_query())
            .unwrap();
        assert_eq!(report.evaluation, None);
        assert_eq!(report.available_power["mil_power"], None);
        assert!(report.unresolved.contains(&"mil_power_unknown".into()));
        assert!(
            !report
                .unresolved
                .iter()
                .any(|reason| reason.contains("missing_define"))
        );

        let invalid_fixture = fixture();
        add_migration_cost_defines(invalid_fixture.path(), "0", "0", "50.0005");
        let invalid_world = VanillaWorld::load(invalid_fixture.path(), "1444.11.11").unwrap();
        let mut invalid_query = content_query();
        invalid_query.country = "ZZZ".into();
        let error = invalid_world
            .query_native_migration_from_content(&invalid_query)
            .unwrap_err()
            .to_string();
        assert!(error.contains("unknown EU4 country"));
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

    fn state_query() -> crate::VanillaNativeMigrationStateQuery {
        crate::VanillaNativeMigrationStateQuery {
            country: "AAA".into(),
            destination: 4,
            pricing: crate::VanillaNativeMigrationStatePricing {
                all_power_cost_modifier_milli: 0,
                country_migration_cost_modifier_milli: 0,
                province_migration_cost_modifier_milli: 0,
            },
            native_mode_eligible: Some(true),
            owner_eligible: Some(true),
            controller_eligible: Some(true),
            tribal_owner_eligible: Some(true),
            additional_gate_eligible: Some(true),
        }
    }

    #[test]
    fn state_query_rechecks_current_membership_steps_and_snapshot_values() {
        let fixture = fixture();
        add_migration_cost_defines(fixture.path(), "0", "0", "50");
        let mut world = VanillaWorld::load(fixture.path(), "1444.11.11").unwrap();
        set_power(&mut world, Some(140), Some(126), Some(150));
        let request = state_query();
        let set_state = |world: &mut VanillaWorld, provinces, migration_steps| {
            world
                .set_country_native_migration(BTreeMap::from([(
                    "AAA".into(),
                    crate::VanillaExternalNativeMigrationState {
                        provinces,
                        migration_steps,
                    },
                )]))
                .unwrap();
        };
        set_state(&mut world, Some(vec![4, 2]), Some(2));
        let snapshot = world.save().unwrap();
        let member = world.query_native_migration_from_state(&request).unwrap();
        assert_eq!(member.context.provinces, Some(vec![4, 2]));
        assert_eq!(member.context.destination_in_native_membership, Some(true));
        assert_eq!(member.query.evaluation.unwrap().cost.unwrap().mil_cost, 50);
        assert_eq!(world.save().unwrap(), snapshot);

        set_state(&mut world, Some(vec![]), Some(2));
        let nonmember = world.query_native_migration_from_state(&request).unwrap();
        assert_eq!(
            nonmember.context.destination_in_native_membership,
            Some(false)
        );
        assert_eq!(
            nonmember.query.evaluation.unwrap().cost.unwrap().mil_cost,
            150
        );
        set_state(&mut world, Some(vec![]), Some(3));
        let more_steps = world.query_native_migration_from_state(&request).unwrap();
        let evaluation = more_steps.query.evaluation.unwrap();
        assert_eq!(evaluation.cost.unwrap().mil_cost, 200);
        assert_eq!(evaluation.resolved_gate, Some(false));

        world.restore(&snapshot).unwrap();
        let restored = world.query_native_migration_from_state(&request).unwrap();
        assert_eq!(restored.context.provinces, Some(vec![4, 2]));
        assert_eq!(
            restored.query.evaluation.unwrap().cost.unwrap().mil_cost,
            50
        );
    }

    #[test]
    fn state_query_preserves_unknown_fields_and_rejects_caller_overrides() {
        let fixture = fixture();
        add_migration_cost_defines(fixture.path(), "0", "0", "50");
        let mut world = VanillaWorld::load(fixture.path(), "1444.11.11").unwrap();
        set_power(&mut world, Some(140), Some(126), Some(150));
        let request = state_query();
        for (provinces, steps, expected_missing) in [
            (None, Some(0), "native_migration:provinces_unknown"),
            (Some(vec![]), None, "native_migration:steps_unknown"),
            (Some(vec![4]), None, "native_migration:steps_unknown"),
        ] {
            world
                .set_country_native_migration(BTreeMap::from([(
                    "AAA".into(),
                    crate::VanillaExternalNativeMigrationState {
                        provinces: provinces.clone(),
                        migration_steps: steps,
                    },
                )]))
                .unwrap();
            let report = world.query_native_migration_from_state(&request).unwrap();
            assert_eq!(report.context.provinces, provinces);
            assert_eq!(report.context.migration_steps, steps);
            assert!(report.query.evaluation.is_none());
            assert!(report.query.unresolved.contains(&expected_missing.into()));
            assert_eq!(report.query.available_power["mil_power"], Some(150));
        }
        let json = serde_json::to_value(&request).unwrap();
        for field in [
            "migration_steps",
            "destination_in_native_membership",
            "define_mil_power_milli",
            "available_mil_power",
        ] {
            let mut overridden = json.clone();
            overridden["pricing"][field] = serde_json::json!(0);
            assert!(
                serde_json::from_value::<crate::VanillaNativeMigrationStateQuery>(overridden)
                    .is_err()
            );
        }
        let mut invalid = request.clone();
        invalid.country = "ZZZ".into();
        assert!(world.query_native_migration_from_state(&invalid).is_err());
        invalid.country = "AAA".into();
        invalid.destination = 999;
        assert!(world.query_native_migration_from_state(&invalid).is_err());
    }
}
