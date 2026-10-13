// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::{BTreeMap, BTreeSet};

use augustus_core::{
    AttributeBase, AttributeChannel, AttributeFormula, AttributeTerm, Attributes, Condition,
    ConvergenceRule, EntityKind, InitialWorld, MetricDefinition, Metrics, ModifierDefinition,
    MonthlyConvergence, Target, WorldDate,
};

use crate::{ContentView, Error, FileEntry, SetupReport};

pub(super) fn metrics(
    initial: &InitialWorld,
    date: WorldDate,
    histories: &BTreeSet<String>,
    unknown: &BTreeSet<String>,
    scoped_unknown: bool,
) -> Metrics {
    // Preserve temporary quantities outside the limits of the final calculation.
    // These bounds keep their contribution representable in attribute micro-units.
    Metrics {
        definitions: [(
            "temporary_liberty_desire".into(),
            MetricDefinition {
                kind: EntityKind::Country,
                minimum: i64::MIN / 1000,
                maximum: i64::MAX / 1000,
                baseline: 0,
                divisor: 1000,
                modifier: Some(ModifierDefinition {
                    values_micros: [("liberty_desire".into(), 1_000_000)].into(),
                    ..Default::default()
                }),
                below_baseline: None,
                unresolved_properties: Default::default(),
                growth_ceiling: false,
            },
        )]
        .into(),
        entities: initial
            .countries()
            .iter()
            .map(|country| {
                let value = (!scoped_unknown
                    && date
                        == (WorldDate {
                            year: 1444,
                            month: 11,
                            day: 11,
                        })
                    && histories.contains(&country.id)
                    && !unknown.contains(&country.id))
                .then_some(0);
                (
                    initial
                        .country_entity(&country.id)
                        .expect("registered country"),
                    [("temporary_liberty_desire".into(), value)].into(),
                )
            })
            .collect(),
        ceilings: Default::default(),
    }
}

pub(super) fn bind(
    root: &impl ContentView,
    files: &[FileEntry],
    initial: &InitialWorld,
    attributes: &mut Attributes,
    report: &mut SetupReport,
) -> Result<MonthlyConvergence, Error> {
    let mut values = BTreeMap::new();
    if files
        .iter()
        .any(|file| file.relative_path == std::path::Path::new("common/defines.lua"))
    {
        for name in [
            "MONTHLY_LIBERTY_DECREASE",
            "MIN_LIBERTY_DESIRE",
            "MAX_LIBERTY_DESIRE",
        ] {
            if let Some((value, source)) =
                crate::defines::optional_literal(root, files, "NCountry", name)?
            {
                if value % 1000 != 0 {
                    return Err(Error::message(
                        &source.path,
                        format!("{name} requires thousandth precision"),
                    ));
                }
                report
                    .definition_sources
                    .insert(format!("liberty_desire:{name}"), source);
                values.insert(name, value);
            }
        }
    }
    let mut rules = BTreeMap::new();
    for country in initial.countries() {
        let entity = initial
            .country_entity(&country.id)
            .expect("registered country");
        attributes.formulas.entry(entity).or_default().insert(
            "liberty_desire".into(),
            AttributeFormula {
                base: AttributeBase::Unknown {
                    reason: "liberty_desire:subject_type_and_dynamic_contributions".into(),
                },
                channels: vec![AttributeChannel {
                    flat: vec![AttributeTerm::Metrics {
                        target: Target::Current,
                        property: "liberty_desire".into(),
                    }],
                    rates: vec![],
                }],
                minimum_micros: values.get("MIN_LIBERTY_DESIRE").copied(),
                maximum_micros: values.get("MAX_LIBERTY_DESIRE").copied(),
                unresolved: Default::default(),
            },
        );
        rules.insert(
            entity,
            [(
                "temporary_liberty_desire".into(),
                ConvergenceRule {
                    target_value: 0,
                    step: values
                        .get("MONTHLY_LIBERTY_DECREASE")
                        .map(|value| value / 1000),
                    condition: Some(Condition::SubjectPresent {
                        target: Target::Current,
                    }),
                    reset_when_inactive: true,
                },
            )]
            .into(),
        );
    }
    Ok(MonthlyConvergence {
        rules,
        ..Default::default()
    })
}
