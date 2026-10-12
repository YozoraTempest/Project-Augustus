// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::{BTreeMap, BTreeSet};

use augustus_core::{EntityKind, InitialWorld, MetricDefinition, MetricEffect, Metrics, WorldDate};

use crate::attributes::AttributeDraft;

/// History operations are not event effects. Preserve unverified writes as unknown.
pub(super) fn history_mutations(
    reader: jomini::text::ObjectReader<'_, '_, crate::TextEncoding>,
    scripted_effects: &BTreeSet<String>,
    depth: usize,
) -> (bool, bool) {
    if depth > 64 {
        return (false, true);
    }
    let mut direct = false;
    let mut scoped = false;
    for (key, _, value) in reader.fields() {
        let name = key.read_string();
        if name.bytes().filter(|c| *c == b'.').count() == 2
            && jomini::common::Date::parse(&name).is_ok()
        {
            continue;
        }
        if matches!(name.as_str(), "stability" | "add_stability") {
            direct = true;
        } else if scripted_effects.contains(&name) {
            scoped = true;
        } else if let Ok(block) = value.read_object() {
            let (nested, deeper) = history_mutations(block, scripted_effects, depth + 1);
            scoped |= nested || deeper;
        }
    }
    (direct, scoped)
}

pub(super) fn bind(
    initial: &InitialWorld,
    attributes: &AttributeDraft,
    date: WorldDate,
    histories: &BTreeSet<String>,
    unknown_history: &BTreeSet<String>,
    unknown_scoped_history: bool,
) -> Metrics {
    let effect = |name: &str| {
        let modifier = attributes.static_modifier(name).cloned();
        let mut unresolved_properties = attributes.static_unresolved_properties(name);
        if modifier.is_none() {
            unresolved_properties.extend([
                "global_tax_modifier".into(),
                "global_manpower_modifier".into(),
            ]);
        }
        MetricEffect {
            modifier,
            unresolved_properties,
        }
    };
    let positive = effect("positive_stability");
    Metrics {
        definitions: [(
            "stability".into(),
            MetricDefinition {
                kind: EntityKind::Country,
                minimum: -3,
                maximum: 3,
                baseline: 0,
                divisor: 1,
                modifier: positive.modifier,
                unresolved_properties: positive.unresolved_properties,
                below_baseline: Some(effect("negative_stability")),
                growth_ceiling: false,
            },
        )]
        .into(),
        entities: initial
            .countries()
            .iter()
            .map(|country| {
                let value = (!unknown_scoped_history
                    && date
                        == (WorldDate {
                            year: 1444,
                            month: 11,
                            day: 11,
                        })
                    && histories.contains(&country.id)
                    && !unknown_history.contains(&country.id))
                .then_some(0);
                (
                    initial
                        .country_entity(&country.id)
                        .expect("registered country"),
                    BTreeMap::from([("stability".into(), value)]),
                )
            })
            .collect(),
        ceilings: BTreeMap::new(),
    }
}
