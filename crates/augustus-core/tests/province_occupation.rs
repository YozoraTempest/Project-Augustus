// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::BTreeSet;

use augustus_core::{
    AttributeBase, AttributeChannel, AttributeFormula, AttributeTerm, Attributes, Condition,
    EntityId, EntityKind, ModifierDefinition, Schema, World,
};

fn world() -> (World, EntityId, EntityId, EntityId) {
    let mut world = World::new(Schema::new([] as [&str; 0]).unwrap());
    let owner = world.add_entity("AAA", EntityKind::Country);
    let other = world.add_entity("BBB", EntityKind::Country);
    let province = world.add_entity("1", EntityKind::Province);
    world.set_owner(province, owner).unwrap();
    (world, owner, other, province)
}

fn assert_occupation(world: &World, province: EntityId, expected: Option<bool>) {
    let before = world.save("occupation").unwrap();
    let direct = world.province_is_occupied(province);
    let condition = Condition::ProvinceOccupied.evaluate(world, province);
    match expected {
        Some(expected) => {
            assert_eq!(direct.unwrap(), expected);
            assert_eq!(condition.unwrap(), expected);
        }
        None => {
            assert!(direct.is_err());
            assert!(condition.is_err());
        }
    }
    assert_eq!(world.save("occupation").unwrap(), before);
}

#[test]
fn occupation_reads_current_relations_and_distinguishes_missing_from_absent() {
    let (mut world, owner, other, province) = world();
    assert_occupation(&world, province, None);

    world
        .set_province_controller(province, Some(owner))
        .unwrap();
    assert_occupation(&world, province, Some(false));
    world
        .set_province_controller(province, Some(other))
        .unwrap();
    assert_occupation(&world, province, Some(true));
    world.set_owner(province, other).unwrap();
    assert_occupation(&world, province, Some(false));

    world.set_province_controller(province, None).unwrap();
    assert_occupation(&world, province, Some(false));
    world.clear_province_controller_state(province).unwrap();
    assert_occupation(&world, province, None);
    world.clear_owner(province).unwrap();
    assert_occupation(&world, province, Some(false));

    let unknown_owner = world.add_entity("2", EntityKind::Province);
    world
        .set_province_controller(unknown_owner, Some(other))
        .unwrap();
    assert_occupation(&world, unknown_owner, None);
    world.set_province_controller(unknown_owner, None).unwrap();
    assert_occupation(&world, unknown_owner, Some(false));
    assert!(world.province_is_occupied(owner).is_err());
    assert!(Condition::ProvinceOccupied.evaluate(&world, owner).is_err());
}

#[test]
fn conditional_attribute_reacts_without_rebinding_and_restores_its_live_inputs() {
    let (mut world, owner, other, province) = world();
    world
        .set_attributes(Attributes {
            static_modifiers: [(
                "occupied".into(),
                ModifierDefinition {
                    values_micros: [("local_tax_modifier".into(), -500_000)].into(),
                    ..Default::default()
                },
            )]
            .into(),
            formulas: [(
                province,
                [(
                    "annual_tax".into(),
                    AttributeFormula {
                        base: AttributeBase::Constant { micros: 12_000_000 },
                        channels: vec![AttributeChannel {
                            flat: vec![],
                            rates: vec![AttributeTerm::Conditional {
                                condition: Box::new(Condition::ProvinceOccupied),
                                then: vec![AttributeTerm::Static {
                                    definition: "occupied".into(),
                                    property: "local_tax_modifier".into(),
                                }],
                                otherwise: vec![],
                            }],
                        }],
                        minimum_micros: None,
                        maximum_micros: None,
                        result_quantum_micros: None,
                        unresolved: BTreeSet::new(),
                    },
                )]
                .into(),
            )]
            .into(),
            ..Default::default()
        })
        .unwrap();
    let formulas = world.attributes().clone();
    let evaluate = |world: &World| {
        world
            .attributes()
            .evaluate(world, province, "annual_tax")
            .unwrap()
    };
    let unknown = evaluate(&world);
    assert_eq!(unknown.value_micros, None);
    assert!(
        unknown
            .unresolved
            .iter()
            .any(|reason| reason.contains("occupation controller is unknown"))
    );

    world
        .set_province_controller(province, Some(owner))
        .unwrap();
    assert_eq!(evaluate(&world).value_micros, Some(12_000_000));
    world
        .set_province_controller(province, Some(other))
        .unwrap();
    let occupied = evaluate(&world);
    assert_eq!(occupied.value_micros, Some(6_000_000));
    assert!(occupied.unresolved.is_empty());
    assert!(occupied.steps[0].contributions.iter().any(|term| {
        term.source == "static:occupied" && term.micros == -500_000 && term.is_rate
    }));
    assert_eq!(world.attributes(), &formulas);

    let snapshot = world.save("occupation").unwrap();
    let mut restored = World::restore(&snapshot, "occupation", world.schema()).unwrap();
    assert_eq!(restored.save("occupation").unwrap(), snapshot);
    assert_eq!(evaluate(&restored).value_micros, Some(6_000_000));
    restored.set_province_controller(province, None).unwrap();
    assert_eq!(evaluate(&restored).value_micros, Some(12_000_000));
    restored.clear_province_controller_state(province).unwrap();
    assert_eq!(evaluate(&restored).value_micros, None);
    restored
        .set_province_controller(province, Some(owner))
        .unwrap();
    assert_eq!(evaluate(&restored).value_micros, Some(12_000_000));
    assert!(evaluate(&restored).unresolved.is_empty());
    assert_eq!(restored.attributes(), &formulas);
}
