// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::BTreeMap;

use augustus_core::{
    AttributeBase, AttributeChannel, AttributeFormula, AttributeTerm, Attributes, EntityId,
    EntityKind, FieldId, ModifierDefinition, ModifierInstance, Modifiers, Schema, World,
};

fn world() -> World {
    let mut world = World::new(Schema::new(["base"]).unwrap());
    world.add_entity("A", EntityKind::Country);
    world.add_entity("B", EntityKind::Country);
    let p = world.add_entity("North", EntityKind::Province);
    let q = world.add_entity("East", EntityKind::Province);
    world.set_owner(p, EntityId(0)).unwrap();
    world.set_owner(q, EntityId(0)).unwrap();
    world.set(p, FieldId(0), 4000).unwrap();
    world.set(q, FieldId(0), 6000).unwrap();
    world
}

fn constant(source: &str, micros: i64) -> AttributeTerm {
    AttributeTerm::Constant {
        source: source.into(),
        micros,
    }
}

fn formula(base: AttributeBase) -> AttributeFormula {
    AttributeFormula {
        base,
        channels: Vec::new(),
        minimum_micros: None,
        maximum_micros: None,
        unresolved: Default::default(),
    }
}

fn attributes(formula: AttributeFormula) -> Attributes {
    Attributes {
        formulas: [(
            EntityId(2),
            [("yield".into(), formula)].into_iter().collect(),
        )]
        .into_iter()
        .collect(),
        ..Default::default()
    }
}

fn evaluate(world: &World) -> augustus_core::AttributeEvaluation {
    world
        .attributes()
        .evaluate(world, EntityId(2), "yield")
        .unwrap()
}

#[test]
fn channels_separate_additive_rates_from_successive_multiplication_and_clamp_last() {
    let mut world = world();
    let mut formula = formula(AttributeBase::Constant { micros: 10_000_000 });
    formula.channels = vec![
        AttributeChannel {
            flat: vec![constant("flat", 2_000_000)],
            rates: vec![constant("first", 200_000), constant("second", 300_000)],
        },
        AttributeChannel {
            flat: vec![constant("after_rate", 1_000_000)],
            rates: vec![constant("independent", -200_000)],
        },
    ];
    formula.maximum_micros = Some(15_000_000);
    world.set_attributes(attributes(formula)).unwrap();
    let result = evaluate(&world);
    assert_eq!(result.steps[0].output_micros, Some(18_000_000));
    assert_eq!(result.steps[0].rate_micros, 500_000);
    assert_eq!(result.steps[1].output_micros, Some(15_200_000));
    assert_eq!(result.value_micros, Some(15_000_000));
    assert_eq!(result.steps[0].contributions[2].source, "second");
    let saved = world.save("attrs").unwrap();
    assert_eq!(
        World::restore(&saved, "attrs", world.schema()).unwrap(),
        world
    );
    assert_eq!(world.save("attrs").unwrap(), saved);
}

#[test]
fn world_fields_owners_buildings_goods_and_attached_modifiers_are_read_live() {
    let mut world = world();
    world
        .set_economy(
            serde_json::from_str(
                r#"{
        "buildings":{"mill":{}},"trade_goods":{"grain":{},"cloth":{}},"trade_nodes":{},
        "provinces":{"2":{"buildings":["mill"],"trade_good":"grain"}}
    }"#,
            )
            .unwrap(),
        )
        .unwrap();
    let definition = |value| ModifierDefinition {
        values_micros: [("yield.rate".into(), value)].into_iter().collect(),
        flags: BTreeMap::new(),
    };
    world
        .set_modifiers(Modifiers {
            definitions: [("boost".into(), definition(100_000))]
                .into_iter()
                .collect(),
            active: [(
                EntityId(0),
                [("boost".into(), ModifierInstance::default())]
                    .into_iter()
                    .collect(),
            )]
            .into_iter()
            .collect(),
        })
        .unwrap();
    let mut formula = formula(AttributeBase::Field {
        field: FieldId(0),
        micros_per_unit: 1000,
    });
    formula.channels.push(AttributeChannel {
        flat: Vec::new(),
        rates: vec![
            AttributeTerm::Buildings {
                property: "yield.rate".into(),
            },
            AttributeTerm::TradeGood {
                property: "yield.rate".into(),
            },
            AttributeTerm::Modifiers {
                target: augustus_core::Target::Owner,
                property: "yield.rate".into(),
            },
        ],
    });
    let mut attributes = attributes(formula);
    attributes
        .building_modifiers
        .insert("mill".into(), definition(300_000));
    attributes
        .trade_good_modifiers
        .insert("grain".into(), definition(200_000));
    attributes
        .trade_good_modifiers
        .insert("cloth".into(), definition(-50_000));
    world.set_attributes(attributes).unwrap();
    assert_eq!(evaluate(&world).value_micros, Some(6_400_000));
    world.set(EntityId(2), FieldId(0), 5000).unwrap();
    assert_eq!(evaluate(&world).value_micros, Some(8_000_000));
    world.set_owner(EntityId(2), EntityId(1)).unwrap();
    assert_eq!(evaluate(&world).value_micros, Some(7_500_000));
    let mut economy = world.economy().clone();
    economy
        .provinces
        .get_mut(&EntityId(2))
        .unwrap()
        .buildings
        .clear();
    economy.provinces.get_mut(&EntityId(2)).unwrap().trade_good = Some("cloth".into());
    world.set_economy(economy).unwrap();
    assert_eq!(evaluate(&world).value_micros, Some(4_750_000));
    assert_eq!(
        world
            .attributes()
            .sum_owned(&world, EntityId(1), "yield")
            .unwrap()
            .value_micros,
        Some(4_750_000)
    );
    let aggregate = world
        .attributes()
        .sum_owned(&world, EntityId(0), "yield")
        .unwrap();
    assert_eq!(aggregate.province_count, 1);
    assert!(aggregate.value_micros.is_none());
    assert!(
        aggregate
            .unresolved
            .contains("province:East:missing_attribute:yield")
    );
}

#[test]
fn unknown_inputs_and_unresolved_sources_do_not_turn_partial_totals_into_complete_values() {
    let mut world = world();
    let mut known = formula(AttributeBase::Constant { micros: 3_000_000 });
    known.unresolved.insert("autonomy".into());
    let mut attributes = attributes(known);
    attributes.formulas.insert(
        EntityId(3),
        [(
            "yield".into(),
            formula(AttributeBase::Unknown {
                reason: "missing_base".into(),
            }),
        )]
        .into_iter()
        .collect(),
    );
    world.set_attributes(attributes).unwrap();
    let before = world.clone();
    assert_eq!(evaluate(&world).known_subtotal_micros, Some(3_000_000));
    assert_eq!(evaluate(&world).value_micros, None);
    let unknown = world
        .attributes()
        .evaluate(&world, EntityId(3), "yield")
        .unwrap();
    assert_eq!(unknown.known_subtotal_micros, None);
    let aggregate = world
        .attributes()
        .sum_owned(&world, EntityId(0), "yield")
        .unwrap();
    assert_eq!(aggregate.known_subtotal_micros, 3_000_000);
    assert!(aggregate.unresolved.contains("province:East:unknown_base"));
    assert_eq!(aggregate.value_micros, None);
    assert_eq!(world, before);
}

#[test]
fn invalid_formulas_and_aggregate_overflow_are_atomic_and_invalid_saves_are_rejected() {
    let mut world = world();
    let old = world.save("attrs").unwrap();
    assert!(!old.contains("attributes"));
    assert_eq!(
        World::restore(&old, "attrs", world.schema()).unwrap(),
        world
    );
    for base in [
        AttributeBase::Field {
            field: FieldId(1),
            micros_per_unit: 1,
        },
        AttributeBase::Field {
            field: FieldId(0),
            micros_per_unit: i64::MAX,
        },
    ] {
        assert!(world.set_attributes(attributes(formula(base))).is_err());
    }
    let mut invalid = attributes(formula(AttributeBase::Constant { micros: 1 }));
    invalid
        .formulas
        .get_mut(&EntityId(2))
        .unwrap()
        .get_mut("yield")
        .unwrap()
        .channels
        .push(AttributeChannel {
            flat: vec![AttributeTerm::Static {
                definition: "missing".into(),
                property: "rate".into(),
            }],
            rates: vec![],
        });
    assert!(world.set_attributes(invalid).is_err());
    let mut overflow = attributes(formula(AttributeBase::Constant { micros: i64::MAX }));
    overflow.formulas.insert(
        EntityId(3),
        [(
            "yield".into(),
            formula(AttributeBase::Constant { micros: 1 }),
        )]
        .into_iter()
        .collect(),
    );
    assert!(
        world
            .set_attributes(overflow)
            .unwrap_err()
            .to_string()
            .contains("overflow")
    );
    assert_eq!(world.save("attrs").unwrap(), old);
    world
        .set_attributes(attributes(formula(AttributeBase::Constant { micros: 1 })))
        .unwrap();
    let mut save: serde_json::Value = serde_json::from_str(&world.save("attrs").unwrap()).unwrap();
    save["attributes"]["formulas"]["2"]["yield"]["minimum_micros"] = 10.into();
    save["attributes"]["formulas"]["2"]["yield"]["maximum_micros"] = 1.into();
    assert!(World::restore(&save.to_string(), "attrs", world.schema()).is_err());
}
