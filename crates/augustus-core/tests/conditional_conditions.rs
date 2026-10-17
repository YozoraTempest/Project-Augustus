// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::*;

fn choose(condition: Condition, then: Condition, otherwise: Condition) -> Condition {
    Condition::If {
        condition: Box::new(condition),
        then: Box::new(then),
        otherwise: Box::new(otherwise),
    }
}

#[test]
fn conditional_predicates_evaluate_only_the_selected_branch_and_keep_unknown_selectors() {
    let mut world = World::new(Schema::new(["signal"]).unwrap());
    let country = world.add_entity("Country", EntityKind::Country);
    let selector = Condition::AtLeast {
        target: Target::Current,
        field: FieldId(0),
        value: 1,
    };
    let predicate = choose(
        selector,
        Condition::Always(true),
        Condition::Unavailable("missing source".into()),
    );
    assert!(predicate.evaluate(&world, country).is_err());
    world.set(country, FieldId(0), 1).unwrap();
    assert!(predicate.evaluate(&world, country).unwrap());
    let unknown = choose(
        Condition::Unavailable("missing selector".into()),
        Condition::Always(true),
        Condition::Always(true),
    );
    assert!(
        unknown
            .evaluate(&world, country)
            .unwrap_err()
            .to_string()
            .contains("missing selector")
    );
    assert!(
        !choose(Condition::Always(true), Condition::Always(false), predicate)
            .evaluate(&world, country)
            .unwrap()
    );
}

#[test]
fn conditional_predicate_dependencies_are_validated_in_dormant_branches_and_saved_tasks() {
    let schema = Schema::new(["signal"]).unwrap();
    let mut world = World::new(schema.clone());
    let country = world.add_entity("Country", EntityKind::Country);
    let invalid = choose(
        Condition::Always(false),
        Condition::AtLeast {
            target: Target::Current,
            field: FieldId(1),
            value: 0,
        },
        Condition::Always(true),
    );
    let before = world.clone();
    assert!(
        world
            .schedule(
                country,
                1,
                vec![Action::If {
                    condition: invalid,
                    then: vec![],
                    otherwise: vec![]
                }]
            )
            .is_err()
    );
    assert_eq!(world, before);
    let valid = choose(
        Condition::Always(false),
        Condition::AtLeast {
            target: Target::Current,
            field: FieldId(0),
            value: 0,
        },
        Condition::Always(true),
    );
    world
        .schedule(
            country,
            1,
            vec![Action::If {
                condition: valid,
                then: vec![Action::Emit("selected".into())],
                otherwise: vec![],
            }],
        )
        .unwrap();
    let saved = world.save("conditional-predicates").unwrap();
    let mut corrupt: serde_json::Value = serde_json::from_str(&saved).unwrap();
    corrupt["tasks"][0]["plan"][0]["If"]["condition"]["If"]["then"]["AtLeast"]["field"] = 99.into();
    assert!(World::restore(&corrupt.to_string(), "conditional-predicates", &schema).is_err());
    let mut restored = World::restore(&saved, "conditional-predicates", &schema).unwrap();
    world.advance_to(1).unwrap();
    restored.advance_to(1).unwrap();
    assert_eq!(world, restored);
    assert_eq!(world.events().last().unwrap().2, "selected");
}

#[test]
fn conditional_predicates_cannot_hide_attribute_feedback_dependencies() {
    let mut world = World::new(Schema::new([] as [&str; 0]).unwrap());
    let country = world.add_entity("Country", EntityKind::Country);
    let condition = choose(
        Condition::Always(false),
        Condition::NumericCompare {
            target: Target::Current,
            slot: NumericSlot::Attribute {
                name: "response".into(),
                divisor: 1,
            },
            comparison: NumericComparison::AtLeast,
            value: 0,
        },
        Condition::Always(true),
    );
    let attributes = Attributes {
        formulas: [(
            country,
            [(
                "response".into(),
                AttributeFormula {
                    base: AttributeBase::Constant { micros: 0 },
                    channels: vec![AttributeChannel {
                        flat: vec![AttributeTerm::Conditional {
                            condition: Box::new(condition),
                            then: vec![],
                            otherwise: vec![],
                        }],
                        rates: vec![],
                    }],
                    minimum_micros: None,
                    maximum_micros: None,
                    result_quantum_micros: None,
                    unresolved: Default::default(),
                },
            )]
            .into(),
        )]
        .into(),
        ..Default::default()
    };
    let before = world.clone();
    assert!(
        world
            .set_attributes(attributes)
            .unwrap_err()
            .to_string()
            .contains("cannot depend on another attribute")
    );
    assert_eq!(world, before);
}

#[test]
fn objective_eligibility_applies_partial_logic_only_inside_the_selected_branch() {
    let mut world = World::new(Schema::new(["signal"]).unwrap());
    let country = world.add_entity("Country", EntityKind::Country);
    world
        .set_completions(Completions {
            identifiers: ["task".into()].into(),
            entities: [(country, Default::default())].into(),
        })
        .unwrap();
    world
        .set_objectives(Objectives {
            groups: vec![ObjectiveGroup {
                id: "series".into(),
                condition: choose(
                    Condition::AtLeast {
                        target: Target::Current,
                        field: FieldId(0),
                        value: 1,
                    },
                    Condition::All(vec![
                        Condition::Unavailable("unbound source".into()),
                        Condition::Always(false),
                    ]),
                    Condition::Always(true),
                ),
                objectives: ["task".into()].into(),
                claims: ["row".into()].into(),
                priority: 1,
            }],
            entities: Default::default(),
        })
        .unwrap();
    world.refresh_objectives(country).unwrap();
    assert!(world.objective_available(country, "task").unwrap());
    world.set(country, FieldId(0), 1).unwrap();
    world.refresh_objectives(country).unwrap();
    assert!(!world.objective_available(country, "task").unwrap());
}
