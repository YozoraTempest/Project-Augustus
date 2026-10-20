// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{Action, EntityKind, Schema, World};

#[test]
fn repeat_executes_body_with_shared_scope() {
    let schema = Schema::new([] as [&str; 0]).unwrap();
    let mut world = World::new(schema);
    let country = world.add_entity("AAA", EntityKind::Country);

    world
        .execute(
            country,
            &[Action::Repeat {
                count: 3,
                plan: vec![Action::Emit("created".into())],
            }],
        )
        .unwrap();

    assert_eq!(world.events().len(), 3);
    assert!(world.events().iter().all(|event| event.2 == "created"));
}

#[test]
fn repeat_with_zero_count_does_not_execute_its_body() {
    let schema = Schema::new([] as [&str; 0]).unwrap();
    let mut world = World::new(schema);
    let country = world.add_entity("AAA", EntityKind::Country);

    world
        .execute(
            country,
            &[Action::Repeat {
                count: 0,
                plan: vec![Action::Emit("unreachable".into())],
            }],
        )
        .unwrap();

    assert!(world.events().is_empty());
}

#[test]
fn empty_repeat_is_bounded_by_the_plan_instruction_budget() {
    let schema = Schema::new([] as [&str; 0]).unwrap();
    let mut world = World::new(schema);
    let country = world.add_entity("AAA", EntityKind::Country);

    let error = world
        .execute(
            country,
            &[Action::Repeat {
                count: u32::MAX,
                plan: Vec::new(),
            }],
        )
        .unwrap_err();

    assert!(error.to_string().contains("instruction limit exceeded"));
}
