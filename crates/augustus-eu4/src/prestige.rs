// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use crate::attributes::AttributeDraft;
use augustus_core::{
    AccrualRule, AttributeBase, AttributeChannel, AttributeFormula, AttributeTerm, Attributes,
    Condition, EntityKind, InitialWorld, MetricDefinition, Metrics, MonthlyAccruals,
    NumericComparison, NumericSlot, Target,
};

pub(super) fn metrics(initial: &InitialWorld, attributes: &AttributeDraft) -> Metrics {
    Metrics {
        definitions: [(
            "prestige".into(),
            MetricDefinition {
                kind: EntityKind::Country,
                minimum: -100000,
                maximum: 100000,
                baseline: 0,
                divisor: 100000,
                modifier: attributes.static_modifier("prestige").cloned(),
                unresolved_properties: attributes.static_unresolved_properties("prestige"),
                below_baseline: None,
                growth_ceiling: false,
            },
        )]
        .into(),
        // Initial prestige has its own production rules. A missing value is not zero.
        entities: initial
            .countries()
            .iter()
            .map(|country| {
                (
                    initial
                        .country_entity(&country.id)
                        .expect("registered country"),
                    [("prestige".into(), None)].into(),
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

fn owned(then: Vec<AttributeTerm>) -> AttributeTerm {
    AttributeTerm::Conditional {
        condition: Box::new(Condition::HasOwnedProvince {
            target: Target::Current,
        }),
        then,
        otherwise: vec![],
    }
}

/// Translate the response and known source categories; unbound producers remain explicit.
pub(super) fn bind(
    initial: &InitialWorld,
    attributes: &mut Attributes,
    accruals: &mut MonthlyAccruals,
) {
    let base = attributes
        .static_modifiers
        .get("base_values")
        .and_then(|definition| definition.values_micros.get("prestige_decay"))
        .copied();
    let mut rates = vec![AttributeTerm::Constant {
        source: "prestige_response".into(),
        micros: -1_000_000,
    }];
    rates.push(
        base.map(|micros| AttributeTerm::Constant {
            source: "base_values:prestige_decay".into(),
            micros,
        })
        .unwrap_or_else(|| AttributeTerm::Unknown {
            reason: "prestige:missing_base_decay".into(),
        }),
    );
    let decay = current("prestige_decay");
    rates.push(AttributeTerm::Conditional {
        condition: Box::new(Condition::NumericCompare {
            target: Target::Current,
            slot: NumericSlot::Metric("prestige".into()),
            comparison: NumericComparison::AtLeast,
            value: 0,
        }),
        then: decay.clone(),
        otherwise: vec![AttributeTerm::Scale {
            factor_micros: -1_000_000,
            term: Box::new(AttributeTerm::TruncatedSum {
                source: "prestige_local_decay".into(),
                quantum_micros: 1,
                terms: decay,
            }),
        }],
    });
    rates.push(AttributeTerm::Unknown {
        reason: "prestige:remaining_decay_sources_not_bound".into(),
    });
    let mut income = current("prestige");
    income.push(AttributeTerm::Unknown {
        reason: "prestige:remaining_income_sources_not_bound".into(),
    });
    let formula = AttributeFormula {
        base: AttributeBase::Constant { micros: 0 },
        channels: vec![
            AttributeChannel {
                flat: vec![owned(vec![AttributeTerm::Numeric {
                    source: "current_prestige".into(),
                    target: Target::Current,
                    slot: NumericSlot::Metric("prestige".into()),
                    micros_per_unit: -1000,
                }])],
                rates: vec![owned(rates)],
            },
            AttributeChannel {
                flat: vec![owned(income)],
                rates: vec![],
            },
        ],
        minimum_micros: None,
        maximum_micros: None,
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
            .insert("yearly_prestige_change".into(), formula.clone());
        accruals.rules.entry(entity).or_default().insert(
            "prestige".into(),
            AccrualRule {
                attribute: "yearly_prestige_change".into(),
                divisor: 12000,
            },
        );
    }
}
