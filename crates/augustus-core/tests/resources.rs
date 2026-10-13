// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::*;

fn world() -> World {
    let mut world = World::new(Schema::new(["unused"]).unwrap());
    world.add_entity("AAA", EntityKind::Country);
    world.add_entity("BBB", EntityKind::Country);
    world.add_entity("Port", EntityKind::Province);
    world
        .set_country_resources(
            [(
                EntityId(0),
                CountryResources {
                    treasury_milli: Some(5000),
                    manpower_milli: Some(1500000),
                    mercantilism_milli: None,
                },
            )]
            .into(),
        )
        .unwrap();
    world
}
fn slot() -> NumericSlot {
    NumericSlot::Resource(CountryResource::Manpower)
}

#[test]
fn selected_stock_writes_preserve_other_values_and_do_not_guess_unknowns() {
    let mut world = world();
    world
        .update_numeric(EntityId(0), &slot(), NumericOperation::Add, 100000000)
        .unwrap();
    assert_eq!(
        world
            .resource_value(EntityId(0), CountryResource::Manpower)
            .unwrap(),
        101500000
    );
    assert_eq!(
        world.country_resources()[&EntityId(0)].treasury_milli,
        Some(5000)
    );
    assert_eq!(
        world.country_resources()[&EntityId(0)].mercantilism_milli,
        None
    );
    let before = world.clone();
    assert!(
        world
            .update_numeric(EntityId(1), &slot(), NumericOperation::Add, 1000)
            .is_err()
    );
    assert_eq!(world, before);
    world
        .update_numeric(EntityId(1), &slot(), NumericOperation::Set, 1000)
        .unwrap();
    assert_eq!(
        world.country_resources()[&EntityId(1)].manpower_milli,
        Some(1000)
    );
    assert_eq!(world.country_resources()[&EntityId(1)].treasury_milli, None);
}

#[test]
fn established_resource_bounds_and_scope_checks_precede_writes() {
    let mut world = world();
    world
        .update_numeric(EntityId(0), &slot(), NumericOperation::Add, -2000000)
        .unwrap();
    assert_eq!(
        world
            .resource_value(EntityId(0), CountryResource::Manpower)
            .unwrap(),
        0
    );
    world
        .update_numeric(
            EntityId(0),
            &NumericSlot::Resource(CountryResource::Treasury),
            NumericOperation::Subtract,
            10000,
        )
        .unwrap();
    assert_eq!(
        world.country_resources()[&EntityId(0)].treasury_milli,
        Some(-5000)
    );
    let before = world.clone();
    assert!(
        world
            .update_numeric(EntityId(2), &slot(), NumericOperation::Set, 1000)
            .is_err()
    );
    assert_eq!(world, before);
    world
        .update_numeric(EntityId(0), &slot(), NumericOperation::Set, i64::MAX)
        .unwrap();
    let before = world.clone();
    assert!(
        world
            .update_numeric(EntityId(0), &slot(), NumericOperation::Add, 1)
            .is_err()
    );
    assert_eq!(world, before);
}

#[test]
fn queued_queries_and_changes_share_the_saved_resource_state() {
    let mut whole = world();
    whole
        .schedule(
            EntityId(0),
            2,
            vec![
                Action::UpdateNumeric {
                    target: Target::Current,
                    slot: slot(),
                    operation: NumericOperation::Add,
                    value: 1000,
                },
                Action::If {
                    condition: Condition::NumericCompare {
                        target: Target::Current,
                        slot: slot(),
                        comparison: NumericComparison::Equal,
                        value: 1501000,
                    },
                    then: vec![Action::Emit("one_stock".into())],
                    otherwise: vec![],
                },
            ],
        )
        .unwrap();
    whole.advance_to(1).unwrap();
    let saved = whole.save("resources:1").unwrap();
    let mut resumed = World::restore(&saved, "resources:1", whole.schema()).unwrap();
    whole.advance_to(3).unwrap();
    resumed.advance_to(2).unwrap();
    resumed.advance_to(3).unwrap();
    assert_eq!(
        whole.save("resources:1").unwrap(),
        resumed.save("resources:1").unwrap()
    );
    assert_eq!(whole.events()[0].2, "one_stock");
}

#[test]
fn failure_retains_prior_writes_and_prevents_saving() {
    let mut world = world();
    assert!(
        world
            .execute(
                EntityId(0),
                &[
                    Action::UpdateNumeric {
                        target: Target::Current,
                        slot: slot(),
                        operation: NumericOperation::Set,
                        value: i64::MAX
                    },
                    Action::UpdateNumeric {
                        target: Target::Current,
                        slot: slot(),
                        operation: NumericOperation::Add,
                        value: 1
                    }
                ]
            )
            .is_err()
    );
    assert_eq!(
        world.country_resources()[&EntityId(0)].manpower_milli,
        Some(i64::MAX)
    );
    assert!(world.save("resources:1").is_err());
}

#[test]
fn corrupt_resource_states_cannot_be_restored() {
    let world = world();
    let mut saved: serde_json::Value =
        serde_json::from_str(&world.save("resources:1").unwrap()).unwrap();
    saved["country_resources"]["0"]["manpower_milli"] = serde_json::json!(-1);
    assert!(World::restore(&saved.to_string(), "resources:1", world.schema()).is_err());
}
