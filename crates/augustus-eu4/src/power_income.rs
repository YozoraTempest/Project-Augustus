// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use crate::{Error, FileEntry, SetupReport};
use augustus_core::{
    AccrualRule, AttributeBase, AttributeChannel, AttributeFormula, AttributeTerm, Attributes,
    InitialWorld, MonthlyAccruals, RulerSkill, Target,
};
use std::collections::{BTreeMap, BTreeSet};

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
                        AttributeTerm::Threshold {
                            target: Target::Current,
                            metric: "power_projection".into(),
                            threshold: 50_000,
                            micros_if_at_least: 1_000_000,
                        },
                        AttributeTerm::AdvisorSkill {
                            target: Target::Current,
                            skill,
                            micros_per_unit: 1_000_000,
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
                                AttributeTerm::AffiliationModifiers {
                                    dimension: "religion".into(),
                                    target: Target::Current,
                                    scope: "country".into(),
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

/// Replace selected monthly income formulas with values supplied by a
/// compatibility provider. Omitted countries and metrics keep the native
/// formula, while an explicit `None` remains unresolved.
pub(super) fn apply_external(
    initial: &InitialWorld,
    attributes: &mut Attributes,
    values: &BTreeMap<String, BTreeMap<String, Option<i64>>>,
) -> Result<(), Error> {
    for (country, metrics) in values {
        let entity = initial.country_entity(country).ok_or_else(|| {
            Error::message(
                "external monthly power income",
                format!("unknown EU4 income country {country}"),
            )
        })?;
        for (metric, value) in metrics {
            let attribute = match metric.as_str() {
                "adm_power" => "monthly_adm",
                "dip_power" => "monthly_dip",
                "mil_power" => "monthly_mil",
                other => {
                    return Err(Error::message(
                        "external monthly power income",
                        format!("unsupported monthly power metric {other}"),
                    ));
                }
            };
            let (base, unresolved) = match value {
                Some(value) => (
                    AttributeBase::Constant {
                        micros: value.checked_mul(1_000_000).ok_or_else(|| {
                            Error::message(
                                "external monthly power income",
                                format!("income value overflows for {country}:{metric}"),
                            )
                        })?,
                    },
                    BTreeSet::new(),
                ),
                None => (
                    AttributeBase::Unknown {
                        reason: format!(
                            "external monthly power income unknown for {country}:{metric}"
                        ),
                    },
                    BTreeSet::from([format!("external_monthly_power_income:{country}:{metric}")]),
                ),
            };
            attributes.formulas.entry(entity).or_default().insert(
                attribute.into(),
                AttributeFormula {
                    base,
                    channels: Vec::new(),
                    minimum_micros: None,
                    maximum_micros: None,
                    result_quantum_micros: None,
                    unresolved,
                },
            );
        }
    }
    Ok(())
}

/// Mark advisor state as authoritative for the countries supplied by a
/// compatibility provider. The advisor term itself reads the live core state;
/// this only removes the legacy unresolved marker left by the base formula.
pub(super) fn apply_external_advisors(
    initial: &InitialWorld,
    attributes: &mut Attributes,
    values: &BTreeMap<String, Vec<crate::VanillaExternalAdvisor>>,
) -> Result<(), Error> {
    for country in values.keys() {
        let entity = initial.country_entity(country).ok_or_else(|| {
            Error::message(
                "external advisors",
                format!("unknown advisor country {country}"),
            )
        })?;
        if let Some(formulas) = attributes.formulas.get_mut(&entity) {
            for branch in ["monthly_adm", "monthly_dip", "monthly_mil"] {
                if let Some(formula) = formulas.get_mut(branch) {
                    formula
                        .unresolved
                        .remove("power_income:advisors_not_initialized");
                }
            }
        }
    }
    Ok(())
}
