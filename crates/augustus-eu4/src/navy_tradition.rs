// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use crate::attributes::AttributeDraft;
use augustus_core::{
    AccrualRule, AttributeBase, AttributeChannel, AttributeFormula, AttributeTerm, Attributes,
    EntityKind, InitialWorld, MetricDefinition, Metrics, MonthlyAccruals, NumericSlot, Target,
};

pub(super) fn metrics(initial: &InitialWorld, attributes: &AttributeDraft) -> Metrics {
    Metrics {
        definitions: [(
            "navy_tradition".into(),
            MetricDefinition {
                kind: EntityKind::Country,
                minimum: 0,
                maximum: 100_000,
                baseline: 0,
                divisor: 100_000,
                modifier: attributes.static_modifier("navy_tradition").cloned(),
                unresolved_properties: attributes.static_unresolved_properties("navy_tradition"),
                below_baseline: None,
                growth_ceiling: false,
            },
        )]
        .into(),
        entities: initial
            .countries()
            .iter()
            .map(|country| {
                (
                    initial
                        .country_entity(&country.id)
                        .expect("registered country"),
                    [("navy_tradition".into(), None)].into(),
                )
            })
            .collect(),
        ceilings: Default::default(),
    }
}

fn current(property: &str) -> Vec<AttributeTerm> {
    vec![
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
        AttributeTerm::AffiliationModifiers {
            dimension: "religion".into(),
            target: Target::Current,
            scope: "country".into(),
            property: property.into(),
        },
    ]
}

pub(super) fn bind(
    initial: &InitialWorld,
    attributes: &mut Attributes,
    accruals: &mut MonthlyAccruals,
) {
    let base_decay = attributes
        .static_modifiers
        .get("base_values")
        .and_then(|definition| definition.values_micros.get("navy_tradition_decay"))
        .copied();
    let mut decay_rate = vec![
        base_decay
            .map(|micros| AttributeTerm::Constant {
                source: "base_values:navy_tradition_decay".into(),
                micros,
            })
            .unwrap_or_else(|| AttributeTerm::Unknown {
                reason: "navy_tradition:missing_base_decay".into(),
            }),
    ];
    decay_rate.extend(current("navy_tradition_decay"));

    let negative_stock = AttributeTerm::Scale {
        factor_micros: -1_000_000,
        term: Box::new(AttributeTerm::Numeric {
            source: "current_navy_tradition".into(),
            target: Target::Current,
            slot: NumericSlot::Metric("navy_tradition".into()),
            micros_per_unit: 1000,
        }),
    };
    let nonnegative_decay_rate = AttributeTerm::ClampMinimum {
        source: "navy_tradition_decay_floor".into(),
        minimum_micros: 0,
        term: Box::new(AttributeTerm::TruncatedSum {
            source: "navy_tradition_decay_rate".into(),
            quantum_micros: 1000,
            terms: decay_rate,
        }),
    };
    let formula = AttributeFormula {
        base: AttributeBase::Constant { micros: 0 },
        channels: vec![
            AttributeChannel {
                flat: vec![negative_stock],
                rates: vec![
                    AttributeTerm::Constant {
                        source: "navy_tradition_decay_response".into(),
                        micros: -1_000_000,
                    },
                    nonnegative_decay_rate,
                ],
            },
            AttributeChannel {
                flat: current("navy_tradition"),
                rates: vec![],
            },
        ],
        minimum_micros: None,
        maximum_micros: None,
        result_quantum_micros: Some(1000),
        unresolved: Default::default(),
    };

    for country in initial.countries() {
        let entity = initial
            .country_entity(&country.id)
            .expect("registered country");
        attributes
            .formulas
            .entry(entity)
            .or_default()
            .insert("yearly_navy_tradition_change".into(), formula.clone());
        accruals.rules.entry(entity).or_default().insert(
            "navy_tradition".into(),
            AccrualRule {
                attribute: "yearly_navy_tradition_change".into(),
                divisor: 12_000,
            },
        );
    }
}
