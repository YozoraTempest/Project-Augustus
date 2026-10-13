// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::BTreeMap;

use augustus_core::{EntityKind, InitialWorld, MetricDefinition, Metrics};

use crate::{ContentView, Error, FileEntry, SetupReport};

/// Declare stocks without inventing initialization or unresolved country caps.
pub(super) fn bind(
    root: &impl ContentView,
    files: &[FileEntry],
    initial: &InitialWorld,
    report: &mut SetupReport,
) -> Result<Metrics, Error> {
    let mut minimums = BTreeMap::from([("adm_power", -100), ("mil_power", -100)]);
    if files
        .iter()
        .any(|f| f.relative_path == std::path::Path::new("common/defines.lua"))
    {
        if let Some((value, source)) =
            crate::defines::optional_signed_literal(root, files, "NCountry", "DEBT_LIMIT_DIP")?
        {
            let minimum = value / 1_000_000;
            if value % 1_000_000 != 0 || minimum > 0 || i32::try_from(minimum).is_err() {
                return Err(Error::message(
                    &source.path,
                    "DEBT_LIMIT_DIP requires a nonpositive integer",
                ));
            }
            minimums.insert("dip_power", minimum);
            report
                .definition_sources
                .insert("monarch_power:dip_minimum".into(), source);
        }
        if let Some((value, source)) =
            crate::defines::optional_literal(root, files, "NCountry", "POWER_MAX")?
        {
            if value % 1_000_000 != 0 || i32::try_from(value / 1_000_000).is_err() {
                return Err(Error::message(
                    &source.path,
                    "POWER_MAX requires a nonnegative integer",
                ));
            }
            report.monarch_power_base_maximum = Some(value / 1_000_000);
            report
                .definition_sources
                .insert("monarch_power:base_maximum".into(), source);
        }
    }
    let definitions = minimums
        .iter()
        .map(|(&name, &minimum)| {
            (
                name.into(),
                MetricDefinition {
                    kind: EntityKind::Country,
                    minimum,
                    maximum: i64::MAX,
                    baseline: 0,
                    divisor: 1,
                    modifier: None,
                    unresolved_properties: Default::default(),
                    below_baseline: None,
                    growth_ceiling: true,
                },
            )
        })
        .collect();
    let entities = initial
        .countries()
        .iter()
        .map(|country| {
            report
                .unknown_monarch_power_countries
                .push(country.id.clone());
            (
                initial
                    .country_entity(&country.id)
                    .expect("registered country"),
                minimums.keys().map(|&name| (name.into(), None)).collect(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    Ok(Metrics {
        definitions,
        ceilings: entities.clone(),
        entities,
    })
}
