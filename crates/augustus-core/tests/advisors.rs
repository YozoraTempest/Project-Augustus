// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    AdvisorState, Advisors, AttributeBase, AttributeChannel, AttributeFormula, AttributeTerm,
    Attributes, EntityId, EntityKind, RulerSkill, Schema, World,
};
use std::collections::BTreeMap;

fn formula() -> AttributeFormula {
    AttributeFormula {
        base: AttributeBase::Constant { micros: 0 },
        channels: vec![AttributeChannel {
            flat: vec![AttributeTerm::AdvisorSkill {
                target: augustus_core::Target::Current,
                skill: RulerSkill::Administration,
                micros_per_unit: 1_000_000,
            }],
            rates: Vec::new(),
        }],
        minimum_micros: None,
        maximum_micros: None,
        result_quantum_micros: None,
        unresolved: Default::default(),
    }
}

fn fixture() -> (World, EntityId) {
    let mut world = World::new(Schema::new(std::iter::empty::<String>()).unwrap());
    let country = world.add_entity("AAA", EntityKind::Country);
    world
        .set_advisors(Advisors {
            countries: BTreeMap::from([(
                country,
                Some(vec![
                    AdvisorState {
                        type_name: "treasurer".into(),
                        category: RulerSkill::Administration,
                        skill: 2,
                    },
                    AdvisorState {
                        type_name: "master_of_mint".into(),
                        category: RulerSkill::Administration,
                        skill: 3,
                    },
                ]),
            )]),
        })
        .unwrap();
    world
        .set_attributes(Attributes {
            formulas: BTreeMap::from([(
                country,
                BTreeMap::from([("monthly_adm".into(), formula())]),
            )]),
            ..Default::default()
        })
        .unwrap();
    (world, country)
}

#[test]
fn advisor_skill_uses_the_first_matching_slot() {
    let (world, country) = fixture();
    let evaluation = world
        .attributes()
        .evaluate(&world, country, "monthly_adm")
        .unwrap();
    assert_eq!(evaluation.value_micros, Some(2_000_000));
    assert!(evaluation.unresolved.is_empty());
    assert_eq!(
        evaluation.steps[0].contributions[0].source,
        "advisor:treasurer"
    );
}

#[test]
fn unknown_advisors_keep_the_power_formula_unresolved() {
    let mut world = World::new(Schema::new(std::iter::empty::<String>()).unwrap());
    let country = world.add_entity("AAA", EntityKind::Country);
    world
        .set_attributes(Attributes {
            formulas: BTreeMap::from([(
                country,
                BTreeMap::from([("monthly_adm".into(), formula())]),
            )]),
            ..Default::default()
        })
        .unwrap();
    let evaluation = world
        .attributes()
        .evaluate(&world, country, "monthly_adm")
        .unwrap();
    assert!(evaluation.value_micros.is_none());
    assert!(
        evaluation
            .unresolved
            .contains("power_income:advisors_not_initialized")
    );
}

#[test]
fn advisors_round_trip_through_core_saves() {
    let (world, _) = fixture();
    let saved = world.save("advisors:1").unwrap();
    let restored = World::restore(&saved, "advisors:1", world.schema()).unwrap();
    assert_eq!(restored.advisors(), world.advisors());
}
