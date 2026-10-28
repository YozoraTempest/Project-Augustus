// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::BTreeMap;

use augustus_core::{EntityKind, MetricDefinition, Metrics};

pub(crate) const FIELDS: [(&str, &str); 3] = [
    ("adm_dev", "native_adm_development"),
    ("dip_dev", "native_dip_development"),
    ("mil_dev", "native_mil_development"),
];

pub(crate) fn is_metric(name: &str) -> bool {
    FIELDS.iter().any(|(_, metric)| *metric == name)
}

pub(crate) fn register_metrics(
    metrics: &mut Metrics,
    values: Option<&BTreeMap<String, BTreeMap<String, Option<i64>>>>,
) {
    let has_native_value = values.is_some_and(|countries| {
        countries
            .values()
            .any(|country| country.keys().any(|name| is_metric(name)))
    });
    if !has_native_value {
        return;
    }

    for (_, name) in FIELDS {
        metrics
            .definitions
            .entry(name.into())
            .or_insert(MetricDefinition {
                kind: EntityKind::Country,
                minimum: i64::from(i32::MIN),
                maximum: i64::from(i32::MAX),
                baseline: 0,
                divisor: 1_000,
                modifier: None,
                unresolved_properties: Default::default(),
                below_baseline: None,
                growth_ceiling: false,
            });
    }
}
