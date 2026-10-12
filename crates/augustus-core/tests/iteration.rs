// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::*;

fn fixture() -> World {
    let mut world = World::new(Schema::new(["unused"]).unwrap());
    let country = world.add_entity("AAA", EntityKind::Country);
    for name in ["1", "2", "3"] {
        let province = world.add_entity(name, EntityKind::Province);
        world.set_owner(province, country).unwrap();
    }
    world
        .set_markers(Markers {
            entities: [(country, Default::default())].into(),
            ..Default::default()
        })
        .unwrap();
    world
}
fn selected(name: &str) -> ScopeSelector {
    ScopeSelector::Named {
        kind: EntityKind::Province,
        name: name.into(),
    }
}
fn iteration(selectors: Vec<ScopeSelector>, condition: Condition, plan: Plan) -> Action {
    Action::ForEach {
        selectors,
        kind: EntityKind::Province,
        condition,
        plan,
    }
}

#[test]
fn declared_order_duplicates_empty_members_and_outer_scope_are_preserved() {
    let mut world = fixture();
    world
        .execute(
            EntityId(0),
            &[
                iteration(
                    vec![
                        selected("3"),
                        selected("1"),
                        selected("3"),
                        ScopeSelector::Global("missing".into()),
                    ],
                    Condition::Always(true),
                    vec![Action::Emit("member".into())],
                ),
                iteration(
                    vec![],
                    Condition::Always(true),
                    vec![Action::Emit("unreachable".into())],
                ),
                Action::Emit("outer".into()),
            ],
        )
        .unwrap();
    assert_eq!(
        world.events().iter().map(|e| e.1).collect::<Vec<_>>(),
        [EntityId(3), EntityId(1), EntityId(3), EntityId(0)]
    );
}

#[test]
fn each_filter_reads_preceding_body_changes_instead_of_an_eager_selected_batch() {
    let mut world = fixture();
    world.set_marker(EntityId(0), "ready", true).unwrap();
    world
        .execute(
            EntityId(0),
            &[iteration(
                vec![selected("3"), selected("1"), selected("2")],
                Condition::HasMarker {
                    target: Target::Owner,
                    name: "ready".into(),
                },
                vec![
                    Action::Emit("first".into()),
                    Action::With {
                        target: Target::Owner,
                        plan: vec![Action::SetMarker {
                            target: Target::Current,
                            name: "ready".into(),
                            present: false,
                        }],
                    },
                ],
            )],
        )
        .unwrap();
    assert_eq!(
        world.events().iter().map(|e| e.1).collect::<Vec<_>>(),
        [EntityId(3)]
    );
}

#[test]
fn nested_iteration_and_scheduled_restore_keep_independent_ordered_frames() {
    let mut world = fixture();
    let inner = iteration(
        vec![selected("2"), selected("1")],
        Condition::Always(true),
        vec![Action::Emit("inner".into())],
    );
    world
        .schedule(
            EntityId(0),
            2,
            vec![iteration(
                vec![selected("3"), selected("1")],
                Condition::Always(true),
                vec![inner, Action::Emit("outer_member".into())],
            )],
        )
        .unwrap();
    let mut restored = World::restore(
        &world.save("iteration").unwrap(),
        "iteration",
        world.schema(),
    )
    .unwrap();
    world.advance_to(2).unwrap();
    restored.advance_to(2).unwrap();
    assert_eq!(world, restored);
    assert_eq!(
        world.events().iter().map(|e| e.1).collect::<Vec<_>>(),
        [
            EntityId(2),
            EntityId(1),
            EntityId(3),
            EntityId(2),
            EntityId(1),
            EntityId(1)
        ]
    );
}

#[test]
fn selection_is_captured_but_global_rebinding_is_visible_to_nested_actions() {
    let mut world = fixture();
    world.bind_global_scope("next", EntityId(3)).unwrap();
    let plan = vec![iteration(
        vec![selected("1"), ScopeSelector::Global("next".into())],
        Condition::Always(true),
        vec![
            Action::BindGlobalScope {
                name: "next".into(),
            },
            Action::Emit("captured".into()),
        ],
    )];
    world.execute(EntityId(0), &plan).unwrap();
    assert_eq!(
        world.events().iter().map(|e| e.1).collect::<Vec<_>>(),
        [EntityId(1), EntityId(3)]
    );
    assert_eq!(world.global_scopes()["next"], EntityId(3));
}

#[test]
fn skipped_iterations_share_the_instruction_limit_and_invalid_types_are_rejected() {
    let mut world = fixture();
    assert!(
        world
            .execute(
                EntityId(0),
                &[iteration(
                    vec![ScopeSelector::Named {
                        kind: EntityKind::Country,
                        name: "AAA".into()
                    }],
                    Condition::Always(false),
                    vec![]
                )]
            )
            .is_err()
    );
    let mut world = fixture();
    assert!(
        world
            .execute(
                EntityId(0),
                &[iteration(
                    vec![selected("1"); 100_000],
                    Condition::Always(false),
                    vec![]
                )]
            )
            .unwrap_err()
            .to_string()
            .contains("instruction limit")
    );
    let mut world = fixture();
    let mut plan = vec![];
    for _ in 0..66 {
        plan = vec![iteration(
            vec![selected("1")],
            Condition::Always(true),
            plan,
        )];
    }
    assert!(world.execute(EntityId(0), &plan).is_err());
}
