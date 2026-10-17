// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use crate::{Error, FileEntry, SetupReport};
use augustus_core::{
    AccrualRule, AttributeBase, AttributeChannel, AttributeFormula, AttributeTerm, Attributes,
    InitialWorld, MonthlyAccruals, RulerSkill, Target,
};

/// Bind verified sources while keeping unimplemented producers explicit.
pub(super) fn bind(
    files: &[FileEntry],
    defines: &crate::Defines,
    initial: &InitialWorld,
    attributes: &mut Attributes,
    report: &mut SetupReport,
) -> Result<MonthlyAccruals, Error> {
    let mut values = std::collections::BTreeMap::new();
    if files
        .iter()
        .any(|file| file.relative_path == std::path::Path::new("common/defines.lua"))
    {
        for name in [
            "BASE_POWER_INCREASE",
            "NAT_FOCUS_INCREASE",
            "NAT_FOCUS_DECREASE",
        ] {
            if let Some((value, source)) = defines.optional_signed("NCountry", name)? {
                if value % 1_000_000 != 0 || i32::try_from(value / 1_000_000).is_err() {
                    return Err(Error::message(
                        &source.path,
                        format!("{name} requires a signed integer"),
                    ));
                }
                report
                    .definition_sources
                    .insert(format!("power_income:{name}"), source);
                values.insert(name, value);
            }
        }
    }
    report.monarch_power_income_defines = values
        .iter()
        .map(|(k, v)| (k.to_string(), v / 1_000_000))
        .collect();
    let mut accruals = MonthlyAccruals::default();
    for country in initial.countries() {
        let entity = initial
            .country_entity(&country.id)
            .expect("registered country");
        for (branch, skill, property) in [
            ("adm", RulerSkill::Administration, "country_admin_power"),
            ("dip", RulerSkill::Diplomacy, "country_diplomatic_power"),
            ("mil", RulerSkill::Military, "country_military_power"),
        ] {
            let name = format!("monthly_{branch}");
            let mut unresolved = std::collections::BTreeSet::from([
                "power_income:advisors_not_initialized".into(),
                "power_income:remaining_sources_not_bound".into(),
            ]);
            for define in ["NAT_FOCUS_INCREASE", "NAT_FOCUS_DECREASE"] {
                if !values.contains_key(define) {
                    unresolved.insert(format!("power_income:missing_define:{define}"));
                }
            }
            let base = values
                .get("BASE_POWER_INCREASE")
                .map(|&micros| AttributeBase::Constant { micros })
                .unwrap_or_else(|| AttributeBase::Unknown {
                    reason: "power_income:missing_base_define".into(),
                });
            let formula = AttributeFormula {
                base,
                channels: vec![AttributeChannel {
                    flat: vec![
                        AttributeTerm::RulerSkill {
                            target: Target::Current,
                            skill,
                            micros_per_unit: 1_000_000,
                        },
                        AttributeTerm::NationalFocus {
                            target: Target::Current,
                            skill,
                            increase_micros: values.get("NAT_FOCUS_INCREASE").copied().unwrap_or(0),
                            decrease_micros: values.get("NAT_FOCUS_DECREASE").copied().unwrap_or(0),
                        },
                        AttributeTerm::TruncatedSum {
                            source: "country_power_sources".into(),
                            quantum_micros: 1_000_000,
                            terms: vec![
                                AttributeTerm::Modifiers {
                                    target: Target::Current,
                                    property: property.into(),
                                },
                                AttributeTerm::Government {
                                    target: Target::Current,
                                    property: property.into(),
                                },
                                AttributeTerm::Ideas {
                                    target: Target::Current,
                                    property: property.into(),
                                },
                                AttributeTerm::Estates {
                                    target: Target::Current,
                                    property: property.into(),
                                },
                                AttributeTerm::Metrics {
                                    target: Target::Current,
                                    property: property.into(),
                                },
                            ],
                        },
                    ],
                    rates: vec![],
                }],
                minimum_micros: None,
                maximum_micros: None,
                result_quantum_micros: None,
                unresolved,
            };
            attributes
                .formulas
                .entry(entity)
                .or_default()
                .insert(name.clone(), formula);
            let metric = format!("{branch}_power");
            if initial.world().metrics().definitions.contains_key(&metric) {
                accruals.rules.entry(entity).or_default().insert(
                    metric,
                    AccrualRule {
                        attribute: name,
                        divisor: 1_000_000,
                    },
                );
            }
        }
    }
    Ok(accruals)
}
