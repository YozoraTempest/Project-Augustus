// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::*;
use std::collections::{BTreeMap, BTreeSet};

struct Scenario {
    world: World,
    aaa: EntityId,
    bbb: EntityId,
    province: EntityId,
    unknown_controller_province: EntityId,
    unknown_owner_province: EntityId,
}

fn scenario() -> Scenario {
    let mut world = World::new(Schema::new([] as [&str; 0]).unwrap());
    let aaa = world.add_entity("AAA", EntityKind::Country);
    let bbb = world.add_entity("BBB", EntityKind::Country);
    let province = world.add_entity("1", EntityKind::Province);
    let unknown_controller_province = world.add_entity("2", EntityKind::Province);
    let unknown_owner_province = world.add_entity("3", EntityKind::Province);
    world.set_owner(province, aaa).unwrap();
    world.set_province_controller(province, Some(aaa)).unwrap();
    world.set_owner(unknown_controller_province, aaa).unwrap();
    world
        .set_province_controller(unknown_owner_province, Some(aaa))
        .unwrap();
    world
        .set_global_markers(GlobalMarkers {
            known: true,
            values: [("active".into(), true)].into(),
        })
        .unwrap();
    Scenario {
        world,
        aaa,
        bbb,
        province,
        unknown_controller_province,
        unknown_owner_province,
    }
}

fn controlled_by(condition: Condition) -> Condition {
    Condition::ProvinceController {
        condition: Box::new(condition),
    }
}

fn current_is(name: &str) -> Condition {
    Condition::EntityNamed {
        target: Target::Current,
        name: name.into(),
    }
}

fn set_controller(country: &str) -> Action {
    Action::SetProvinceController {
        target: Target::Current,
        controller: Some(EntityRef::Selected(ScopeSelector::Named {
            kind: EntityKind::Country,
            name: country.into(),
        })),
    }
}

fn controller_is_owner() -> Condition {
    Condition::ProvinceControllerIsOwner
}

#[test]
fn condition_uses_controller_as_current_and_preserves_the_callers_root() {
    let mut scenario = scenario();
    let province = scenario.province;
    let bbb = scenario.bbb;
    scenario
        .world
        .set_province_controller(province, Some(bbb))
        .unwrap();
    assert_eq!(
        scenario.world.entity(province).unwrap().owner,
        Some(scenario.aaa)
    );
    let condition = controlled_by(Condition::All(vec![
        current_is("BBB"),
        Condition::Not(Box::new(Condition::ScopeIsCurrent(ScopeSelector::Root))),
        Condition::InScope {
            selector: ScopeSelector::Root,
            kind: EntityKind::Province,
            condition: Box::new(current_is("1")),
        },
    ]));
    let before = scenario
        .world
        .save("province-controller-condition")
        .unwrap();

    assert!(condition.evaluate(&scenario.world, province).unwrap());
    assert!(scenario.world.validate_condition(&condition).is_ok());
    assert_eq!(
        scenario
            .world
            .save("province-controller-condition")
            .unwrap(),
        before
    );
}

#[test]
fn known_absence_is_false_but_missing_controller_state_stays_unknown_under_not() {
    let mut scenario = scenario();
    let true_child = controlled_by(Condition::Always(true));
    scenario
        .world
        .set_province_controller(scenario.province, None)
        .unwrap();
    let after_known_absence = scenario
        .world
        .save("province-controller-condition")
        .unwrap();
    assert!(
        !true_child
            .evaluate(&scenario.world, scenario.province)
            .unwrap()
    );
    assert!(
        Condition::Not(Box::new(true_child))
            .evaluate(&scenario.world, scenario.province)
            .unwrap()
    );

    let by_name = controlled_by(current_is("BBB"));
    assert!(
        by_name
            .evaluate(&scenario.world, scenario.unknown_controller_province)
            .is_err()
    );
    assert!(
        Condition::Not(Box::new(by_name))
            .evaluate(&scenario.world, scenario.unknown_controller_province)
            .is_err()
    );
    assert!(
        controlled_by(Condition::Always(true))
            .evaluate(&scenario.world, scenario.aaa)
            .is_err()
    );
    assert_eq!(
        scenario
            .world
            .save("province-controller-condition")
            .unwrap(),
        after_known_absence
    );
}

#[test]
fn controller_is_owner_distinguishes_equal_different_absent_and_unknown_relations() {
    let mut scenario = scenario();
    let condition = controller_is_owner();
    assert!(scenario.world.validate_condition(&condition).is_ok());
    assert!(
        condition
            .evaluate(&scenario.world, scenario.province)
            .unwrap()
    );

    scenario
        .world
        .set_province_controller(scenario.province, Some(scenario.bbb))
        .unwrap();
    assert!(
        !condition
            .evaluate(&scenario.world, scenario.province)
            .unwrap()
    );

    scenario
        .world
        .set_province_controller(scenario.province, None)
        .unwrap();
    assert!(
        !condition
            .evaluate(&scenario.world, scenario.province)
            .unwrap()
    );
    assert!(
        Condition::Not(Box::new(condition.clone()))
            .evaluate(&scenario.world, scenario.province)
            .unwrap()
    );

    assert!(
        condition
            .evaluate(&scenario.world, scenario.unknown_controller_province)
            .is_err()
    );
    assert!(
        Condition::Not(Box::new(condition.clone()))
            .evaluate(&scenario.world, scenario.unknown_controller_province)
            .is_err()
    );
    assert!(
        condition
            .evaluate(&scenario.world, scenario.unknown_owner_province)
            .is_err()
    );
    assert!(
        Condition::Not(Box::new(condition))
            .evaluate(&scenario.world, scenario.unknown_owner_province)
            .is_err()
    );
}

#[test]
fn controller_is_owner_condition_is_valid_and_round_trips_in_a_scheduled_plan() {
    let mut scenario = scenario();
    let condition = controller_is_owner();
    assert!(scenario.world.validate_condition(&condition).is_ok());
    scenario
        .world
        .schedule(
            scenario.province,
            2,
            vec![Action::If {
                condition,
                then: vec![Action::Emit("controller_matches_owner".into())],
                otherwise: vec![Action::Emit("controller_differs_from_owner".into())],
            }],
        )
        .unwrap();

    let saved = scenario.world.save("province-controller-is-owner").unwrap();
    let restored = World::restore(
        &saved,
        "province-controller-is-owner",
        scenario.world.schema(),
    )
    .unwrap();
    assert_eq!(
        restored.save("province-controller-is-owner").unwrap(),
        saved
    );
}

#[test]
fn nested_global_marker_references_and_invalid_children_are_validated_recursively() {
    let mut scenario = scenario();
    let condition = controlled_by(Condition::All(vec![
        Condition::HasGlobalMarker {
            name: "active".into(),
        },
        Condition::Not(Box::new(Condition::HasGlobalMarker {
            name: "inactive".into(),
        })),
    ]));
    assert_eq!(
        condition.global_marker_references(),
        BTreeSet::from(["active".into(), "inactive".into()])
    );
    assert!(
        condition
            .evaluate(&scenario.world, scenario.province)
            .unwrap()
    );

    let invalid = controlled_by(Condition::HasGlobalMarker {
        name: String::new(),
    });
    assert!(scenario.world.validate_condition(&invalid).is_err());
    let before = scenario
        .world
        .save("province-controller-condition")
        .unwrap();
    assert!(
        scenario
            .world
            .schedule(
                scenario.province,
                1,
                vec![Action::If {
                    condition: invalid,
                    then: vec![],
                    otherwise: vec![],
                }],
            )
            .is_err()
    );
    assert_eq!(
        scenario
            .world
            .save("province-controller-condition")
            .unwrap(),
        before
    );
}

#[test]
fn attribute_conditions_cannot_hide_derived_dependencies_under_controller_scope() {
    let mut scenario = scenario();
    let condition = controlled_by(Condition::NumericCompare {
        target: Target::Current,
        slot: NumericSlot::Attribute {
            name: "yield".into(),
            divisor: 1,
        },
        comparison: NumericComparison::AtLeast,
        value: 0,
    });
    let formula = AttributeFormula {
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
        unresolved: BTreeSet::new(),
    };
    let attributes = Attributes {
        formulas: BTreeMap::from([(scenario.aaa, BTreeMap::from([("yield".into(), formula)]))]),
        ..Default::default()
    };

    assert!(scenario.world.set_attributes(attributes).is_err());
}

#[test]
fn scheduled_condition_reads_the_controller_written_after_restore() {
    let mut scenario = scenario();
    let scheduled_condition = controlled_by(current_is("BBB"));
    assert!(
        !scheduled_condition
            .evaluate(&scenario.world, scenario.province)
            .unwrap()
    );
    scenario
        .world
        .schedule(
            scenario.province,
            3,
            vec![Action::If {
                condition: scheduled_condition,
                then: vec![Action::Emit("controlled".into())],
                otherwise: vec![Action::Emit("uncontrolled".into())],
            }],
        )
        .unwrap();
    let saved = scenario
        .world
        .save("province-controller-condition-schedule")
        .unwrap();
    let mut restored = World::restore(
        &saved,
        "province-controller-condition-schedule",
        scenario.world.schema(),
    )
    .unwrap();
    let province = scenario.province;
    let bbb = scenario.bbb;

    for world in [&mut scenario.world, &mut restored] {
        world.execute(province, &[set_controller("BBB")]).unwrap();
        assert_eq!(
            world.province_controller_state(province).unwrap(),
            Some(Some(bbb))
        );
        assert!(
            controlled_by(current_is("BBB"))
                .evaluate(world, province)
                .unwrap()
        );
        world.advance_to(3).unwrap();
        assert_eq!(world.events(), &[(3, province, "controlled".into())]);
    }
    assert_eq!(scenario.world, restored);
    assert_eq!(
        scenario
            .world
            .save("province-controller-condition-schedule")
            .unwrap(),
        restored
            .save("province-controller-condition-schedule")
            .unwrap()
    );
}
