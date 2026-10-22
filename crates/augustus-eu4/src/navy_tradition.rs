// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use crate::attributes::AttributeDraft;
use augustus_core::{
    AccrualRule, AttributeBase, AttributeChannel, AttributeFormula, AttributeTerm, Attributes,
    EntityKind, InitialWorld, MetricDefinition, Metrics, MonthlyAccruals, NumericSlot,
    ProvinceClass, Target,
};

pub(super) fn metrics(initial: &InitialWorld, attributes: &AttributeDraft) -> Metrics {
    let provinces_by_entity: std::collections::BTreeMap<_, _> = initial
        .provinces()
        .iter()
        .filter_map(|province| {
            initial
                .province_entity(province.id)
                .map(|entity| (entity, province))
        })
        .collect();

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
                let owned_count = initial
                    .country_entity(&country.id)
                    .and_then(|entity| initial.world().owned_provinces(entity).ok())
                    .map(|provinces| provinces.len());
                let capital_area_ports = initial.bound_capital(&country.id).and_then(|capital| {
                    let capital = initial.province_entity(capital)?;
                    let owner_area = initial.world().province_owner_area(capital).ok()??;
                    let mut count = 0;
                    for entity in owner_area {
                        let province = provinces_by_entity.get(&entity)?;
                        if province.class != ProvinceClass::Land {
                            continue;
                        }
                        match province.has_port {
                            Some(true) => count += 1,
                            Some(false) => {}
                            None => return None,
                        }
                    }
                    Some(count)
                });
                // These two inputs remain unknown until their original sources are confirmed.
                let value = initial_value(owned_count, capital_area_ports, None, None);
                (
                    initial
                        .country_entity(&country.id)
                        .expect("registered country"),
                    [("navy_tradition".into(), value)].into(),
                )
            })
            .collect(),
        ceilings: Default::default(),
    }
}

fn initial_value(
    owned_count: Option<usize>,
    capital_area_ports: Option<usize>,
    auxiliary: Option<u32>,
    multiplier_branch: Option<bool>,
) -> Option<i64> {
    let owned_count = owned_count?;
    if owned_count == 0 {
        return None;
    }
    let capital_area_ports = i64::try_from(capital_area_ports?).ok()?;
    let owned_count = i64::try_from(owned_count).ok()?;
    let auxiliary = i64::from(auxiliary?);
    let multiplier_branch = multiplier_branch?;

    let port_ratio = capital_area_ports.checked_mul(1_000)? / owned_count;
    let ratio = port_ratio.checked_mul(3)?.min(1_000);
    let base = 50_i64.checked_add(auxiliary.checked_mul(20)?)?;
    let mut value = ratio.checked_mul(base)? / 1_000;
    if multiplier_branch {
        value = value.checked_mul(1_250)? / 1_000;
    }
    Some(value.checked_mul(100)?.clamp(0, 100_000))
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

#[cfg(test)]
mod tests {
    use super::initial_value;

    #[test]
    fn initial_formula_matches_the_eight_observed_countries() {
        let observations = [
            (37, 18, 2, 9_000),
            (12, 11, 2, 9_000),
            (26, 12, 3, 11_000),
            (40, 21, 0, 5_000),
            (25, 8, 4, 12_400),
            (16, 4, 4, 9_700),
            (20, 2, 2, 2_700),
            (41, 17, 4, 13_000),
        ];

        for (owned, capital_area_ports, auxiliary, expected) in observations {
            assert_eq!(
                initial_value(
                    Some(owned),
                    Some(capital_area_ports),
                    Some(auxiliary),
                    Some(false),
                ),
                Some(expected),
                "owned={owned}, ports={capital_area_ports}, auxiliary={auxiliary}"
            );
        }
    }

    #[test]
    fn initial_formula_preserves_unknown_inputs_and_applies_known_multiplier() {
        assert_eq!(initial_value(Some(20), Some(2), None, Some(false)), None);
        assert_eq!(initial_value(Some(20), Some(2), Some(2), None), None);
        assert_eq!(initial_value(Some(20), None, Some(2), Some(false)), None);
        assert_eq!(initial_value(None, Some(2), Some(2), Some(false)), None);
        assert_eq!(initial_value(Some(0), Some(0), Some(2), Some(false)), None);
        assert_eq!(
            initial_value(Some(100), Some(100), Some(0), Some(true)),
            Some(6_200)
        );
    }
}
