// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::{BTreeMap, BTreeSet};

use augustus_core::{EntityKind, InitialWorld, MetricDefinition, Metrics, WorldDate};

use crate::attributes::AttributeDraft;

pub(super) fn history_mutations(
    reader: jomini::text::ObjectReader<'_, '_, crate::TextEncoding>,
    depth: usize,
) -> (bool, bool) {
    if depth > 64 {
        return (false, true);
    }
    let mut direct = false;
    let mut scoped = false;
    for (key, _, value) in reader.fields() {
        let name = key.read_string();
        // Bootstrap visits dated blocks separately, only when the date applies.
        if name.bytes().filter(|c| *c == b'.').count() == 2
            && jomini::common::Date::parse(&name).is_ok()
        {
            continue;
        }
        if matches!(name.as_str(), "devastation" | "add_devastation") {
            direct = true;
        } else if let Ok(block) = value.read_object() {
            let (nested, deeper) = history_mutations(block, depth + 1);
            scoped |= nested || deeper;
        }
    }
    (direct, scoped)
}

pub(super) fn bind(
    initial: &InitialWorld,
    attributes: &AttributeDraft,
    date: WorldDate,
    unknown_history: &BTreeSet<u32>,
    unknown_scoped_history: bool,
) -> Metrics {
    let name = "devastation".to_owned();
    let modifier = attributes.static_modifier(&name).cloned();
    let mut unresolved_properties = attributes.static_unresolved_properties(&name);
    if modifier.is_none() {
        unresolved_properties.extend([
            "local_tax_modifier".into(),
            "local_manpower_modifier".into(),
        ]);
    }
    let prosperity_name = "prosperity".to_owned();
    let mut prosperity_unresolved_properties =
        attributes.static_unresolved_properties(&prosperity_name);
    if let Some(modifier) = attributes.static_modifier(&prosperity_name) {
        prosperity_unresolved_properties.extend(modifier.values_micros.keys().cloned());
        prosperity_unresolved_properties.extend(modifier.flags.keys().cloned());
    } else {
        prosperity_unresolved_properties.insert("trade_goods_size_modifier".into());
    }
    // Keep Prosperity's static effects unresolved until their runtime scaling is verified.
    Metrics {
        definitions: [
            (
                name.clone(),
                MetricDefinition {
                    kind: EntityKind::Province,
                    minimum: 0,
                    maximum: 100_000,
                    baseline: 0,
                    divisor: 100_000,
                    modifier,
                    unresolved_properties,
                    below_baseline: None,
                    growth_ceiling: false,
                },
            ),
            (
                prosperity_name.clone(),
                MetricDefinition {
                    kind: EntityKind::Province,
                    minimum: 0,
                    maximum: 100_000,
                    baseline: 0,
                    divisor: 100_000,
                    modifier: None,
                    unresolved_properties: prosperity_unresolved_properties,
                    below_baseline: None,
                    growth_ceiling: false,
                },
            ),
        ]
        .into_iter()
        .collect(),
        entities: initial
            .provinces()
            .iter()
            .map(|province| {
                let value = (!unknown_scoped_history
                    && date
                        == (WorldDate {
                            year: 1444,
                            month: 11,
                            day: 11,
                        })
                    && !unknown_history.contains(&province.id))
                .then_some(0);
                (
                    initial
                        .province_entity(province.id)
                        .expect("registered province"),
                    BTreeMap::from([(name.clone(), value), (prosperity_name.clone(), None)]),
                )
            })
            .collect(),
        ceilings: BTreeMap::new(),
    }
}
