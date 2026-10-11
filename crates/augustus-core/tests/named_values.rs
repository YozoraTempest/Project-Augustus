// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    Action, Condition, EntityId, EntityKind, NamedValues, NumericComparison, NumericOperation,
    NumericSlot, Schema, Target, World,
};

fn world() -> World {
    let mut world = World::new(Schema::new(["balance"]).unwrap());
    let country = world.add_entity("AAA", EntityKind::Country);
    let other = world.add_entity("BBB", EntityKind::Country);
    let province = world.add_entity("Port", EntityKind::Province);
    world.set_owner(province, country).unwrap();
    world.set_capital(country, province).unwrap();
    world
        .set_named_values(NamedValues {
            entities: [
                (country, Default::default()),
                (other, Default::default()),
                (province, Default::default()),
            ]
            .into(),
            ..Default::default()
        })
        .unwrap();
    world
}

fn slot() -> NumericSlot {
    NumericSlot::Named("count".into())
}

#[test]
fn sparse_values_are_independent_and_arithmetic_uses_checked_scaled_integers() {
    let mut world = world();
    let country = EntityId(0);
    let province = EntityId(2);
    assert_eq!(world.numeric_value(country, &slot()).unwrap(), 0);
    assert!(world.named_values().entities[&country].is_empty());
    for (operation, value, expected) in [
        (NumericOperation::Add, 2125, 2125),
        (NumericOperation::Subtract, 3375, -1250),
        (NumericOperation::Multiply { scale: 1000 }, -2000, 2500),
        (NumericOperation::Divide { scale: 1000 }, 4000, 625),
    ] {
        world
            .update_numeric(country, &slot(), operation, value)
            .unwrap();
        assert_eq!(world.numeric_value(country, &slot()).unwrap(), expected);
    }
    assert_eq!(world.numeric_value(province, &slot()).unwrap(), 0);
    assert_eq!(world.numeric_value(EntityId(1), &slot()).unwrap(), 0);
    for value in [-1, 1] {
        world
            .update_numeric(country, &slot(), NumericOperation::Set, value)
            .unwrap();
        world
            .update_numeric(
                country,
                &slot(),
                NumericOperation::Multiply { scale: 1000 },
                500,
            )
            .unwrap();
        assert_eq!(world.numeric_value(country, &slot()).unwrap(), 0);
    }
    let field = NumericSlot::Field(augustus_core::FieldId(0));
    world
        .update_numeric(country, &field, NumericOperation::Set, 2500)
        .unwrap();
    world
        .update_numeric(
            country,
            &field,
            NumericOperation::Divide { scale: 1000 },
            4000,
        )
        .unwrap();
    assert_eq!(
        world.value(country, augustus_core::FieldId(0)).unwrap(),
        625
    );
}

#[test]
fn arithmetic_errors_precede_writes_and_executor_preserves_the_prior_prefix() {
    let mut world = world();
    let country = EntityId(0);
    world
        .update_numeric(country, &slot(), NumericOperation::Set, i64::MAX)
        .unwrap();
    let before = world.clone();
    for (operation, value) in [
        (NumericOperation::Add, 1),
        (NumericOperation::Multiply { scale: 1 }, 2),
        (NumericOperation::Divide { scale: 1000 }, 0),
        (NumericOperation::Multiply { scale: 0 }, 1),
    ] {
        assert!(
            world
                .update_numeric(country, &slot(), operation, value)
                .is_err()
        );
        assert_eq!(world, before);
    }
    assert!(
        world
            .execute(
                country,
                &[
                    Action::Emit("prefix".into()),
                    Action::UpdateNumeric {
                        target: Target::Current,
                        slot: slot(),
                        operation: NumericOperation::Add,
                        value: 1
                    },
                    Action::Emit("unreached".into())
                ]
            )
            .is_err()
    );
    assert_eq!(world.events(), &[(0, country, "prefix".into())]);
    assert!(world.save("numeric:v1").is_err());
}

#[test]
fn unknown_namespaces_allow_individual_known_writes_and_validate_saved_state() {
    let mut world = world();
    let country = EntityId(0);
    let mut values = world.named_values().clone();
    values.entities.remove(&country);
    world.set_named_values(values).unwrap();
    assert!(world.numeric_value(country, &slot()).is_err());
    world
        .update_numeric(country, &slot(), NumericOperation::Set, 1)
        .unwrap();
    assert_eq!(world.numeric_value(country, &slot()).unwrap(), 1);
    assert!(
        world
            .numeric_value(country, &NumericSlot::Named("other".into()))
            .is_err()
    );
    let saved = world.save("numeric:v1").unwrap();
    let restored = World::restore(&saved, "numeric:v1", world.schema()).unwrap();
    assert_eq!(world, restored);
    for (entity, name) in [(EntityId(99), "count"), (country, "")] {
        assert!(
            world
                .update_numeric(
                    entity,
                    &NumericSlot::Named(name.into()),
                    NumericOperation::Set,
                    1
                )
                .is_err()
        );
    }
    let mut corrupt: serde_json::Value = serde_json::from_str(&saved).unwrap();
    corrupt["named_values"]["entities"]["99"] = serde_json::json!({});
    assert!(World::restore(&corrupt.to_string(), "numeric:v1", world.schema()).is_err());
    let mut corrupt: serde_json::Value = serde_json::from_str(&saved).unwrap();
    corrupt["named_values"]["entities"]["0"] = serde_json::json!({"":1});
    assert!(World::restore(&corrupt.to_string(), "numeric:v1", world.schema()).is_err());
}

#[test]
fn delayed_comparisons_use_live_scopes_and_saved_values_and_validate_all_branches() {
    let mut world = world();
    let country = EntityId(0);
    let condition = Condition::NumericCompare {
        target: Target::Capital,
        slot: slot(),
        comparison: NumericComparison::Equal,
        value: 1250,
    };
    world
        .schedule(
            country,
            1,
            vec![Action::If {
                condition: condition.clone(),
                then: vec![Action::Emit("done".into())],
                otherwise: vec![Action::Emit("missing".into())],
            }],
        )
        .unwrap();
    world
        .execute(
            country,
            &[Action::UpdateNumeric {
                target: Target::Capital,
                slot: slot(),
                operation: NumericOperation::Set,
                value: 1250,
            }],
        )
        .unwrap();
    let mut restored = World::restore(
        &world.save("numeric:v1").unwrap(),
        "numeric:v1",
        world.schema(),
    )
    .unwrap();
    world.advance_to(1).unwrap();
    restored.advance_to(1).unwrap();
    assert_eq!(world, restored);
    assert_eq!(world.events(), &[(1, country, "done".into())]);
    let before = world.clone();
    assert!(
        world
            .schedule(
                country,
                1,
                vec![Action::If {
                    condition: Condition::Any(vec![
                        Condition::Always(true),
                        Condition::NumericCompare {
                            target: Target::Current,
                            slot: NumericSlot::Named("".into()),
                            comparison: NumericComparison::AtLeast,
                            value: 0
                        }
                    ]),
                    then: vec![],
                    otherwise: vec![]
                }]
            )
            .is_err()
    );
    assert_eq!(world, before);
    assert!(
        world
            .schedule(
                country,
                1,
                vec![Action::UpdateNumeric {
                    target: Target::Current,
                    slot: NumericSlot::Field(augustus_core::FieldId(9)),
                    operation: NumericOperation::Set,
                    value: 0
                }]
            )
            .is_err()
    );
    assert_eq!(world, before);
}
