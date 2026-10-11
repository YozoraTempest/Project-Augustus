// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    Action, EntityId, EntityKind, FieldId, Plan, Politics, RuleId, Schema, ScopeSelector, World,
};

fn world() -> World {
    let mut world = World::new(Schema::new(["value"]).unwrap());
    let first = world.add_entity("Union", EntityKind::Country);
    world.add_entity("Federation", EntityKind::Country);
    let port = world.add_entity("Port", EntityKind::Province);
    world.set_owner(port, first).unwrap();
    world.set_capital(first, port).unwrap();
    world
        .set_politics(Politics {
            offices: [("council".into(), first)].into(),
            ..Default::default()
        })
        .unwrap();
    world
}

fn selected(selector: ScopeSelector, kind: EntityKind, plan: Plan) -> Action {
    Action::InScope {
        selector,
        kind,
        plan,
    }
}

fn global(name: &str) -> ScopeSelector {
    ScopeSelector::Global(name.into())
}

#[test]
fn live_offices_and_bound_identities_have_distinct_lifetimes() {
    let mut world = world();
    world
        .execute(
            EntityId(1),
            &[selected(
                ScopeSelector::Office("council".into()),
                EntityKind::Country,
                vec![Action::BindGlobalScope {
                    name: "original".into(),
                }],
            )],
        )
        .unwrap();
    let mut politics = world.politics().clone();
    politics.offices.insert("council".into(), EntityId(1));
    world.set_politics(politics).unwrap();
    world.set(EntityId(0), FieldId(0), 7).unwrap();
    world
        .execute(
            EntityId(1),
            &[
                selected(
                    ScopeSelector::Office("council".into()),
                    EntityKind::Country,
                    vec![Action::Emit("current holder".into())],
                ),
                selected(
                    global("original"),
                    EntityKind::Country,
                    vec![Action::Add {
                        target: augustus_core::Target::Current,
                        field: FieldId(0),
                        value: 3,
                    }],
                ),
            ],
        )
        .unwrap();
    assert_eq!(world.events()[0].1, EntityId(1));
    assert_eq!(world.value(EntityId(0), FieldId(0)).unwrap(), 10);
    assert_eq!(world.global_scopes()["original"], EntityId(0));
}

#[test]
fn delayed_scope_resolution_reads_rebinding_and_survives_restore() {
    let mut continuous = world();
    continuous
        .bind_global_scope("destination", EntityId(0))
        .unwrap();
    continuous
        .schedule(
            EntityId(0),
            2,
            vec![selected(
                global("destination"),
                EntityKind::Country,
                vec![Action::Emit("arrived".into())],
            )],
        )
        .unwrap();
    continuous
        .bind_global_scope("destination", EntityId(1))
        .unwrap();
    continuous.bind_global_scope("port", EntityId(2)).unwrap();
    continuous.advance_to(1).unwrap();
    let mut restored = World::restore(
        &continuous.save("scope:v1").unwrap(),
        "scope:v1",
        continuous.schema(),
    )
    .unwrap();
    for world in [&mut continuous, &mut restored] {
        world.set_owner(EntityId(2), EntityId(1)).unwrap();
        // A transferred capital must be removed from the former owner's relation.
        let replacement = world.add_entity("Replacement", EntityKind::Province);
        world.set_owner(replacement, EntityId(0)).unwrap();
        world.set_capital(EntityId(0), replacement).unwrap();
        world.advance_to(2).unwrap();
        world
            .execute(
                EntityId(0),
                &[selected(
                    global("port"),
                    EntityKind::Province,
                    vec![Action::With {
                        target: augustus_core::Target::Owner,
                        plan: vec![Action::Emit("new owner".into())],
                    }],
                )],
            )
            .unwrap();
    }
    assert_eq!(continuous, restored);
    assert_eq!(
        continuous.events().iter().map(|e| e.1).collect::<Vec<_>>(),
        [EntityId(1), EntityId(1)]
    );
}

#[test]
fn absent_scopes_skip_bodies_and_clear_removes_persistent_state() {
    let mut world = world();
    let unused = selected(
        global("missing"),
        EntityKind::Country,
        vec![Action::Emit("incorrect".into())],
    );
    world
        .execute(
            EntityId(0),
            &[
                unused.clone(),
                Action::BindGlobalScope {
                    name: "missing".into(),
                },
                selected(
                    global("missing"),
                    EntityKind::Country,
                    vec![Action::Emit("present".into())],
                ),
                Action::ClearGlobalScope {
                    name: "missing".into(),
                },
                unused,
            ],
        )
        .unwrap();
    assert_eq!(world.events().len(), 1);
    assert!(world.global_scopes().is_empty());
    assert!(!world.save("empty:v1").unwrap().contains("global_scopes"));
    assert_eq!(
        world
            .resolve_scope(&ScopeSelector::Office("vacant".into()))
            .unwrap(),
        None
    );
}

#[test]
fn invalid_saved_references_and_runtime_types_are_rejected() {
    let mut world = world();
    let before = world.clone();
    assert!(world.bind_global_scope("", EntityId(0)).is_err());
    assert!(world.bind_global_scope("bad", EntityId(999)).is_err());
    assert_eq!(world, before);
    world.bind_global_scope("destination", EntityId(2)).unwrap();
    let save = world.save("bad:v1").unwrap();
    for (name, entity) in [("destination", 999), ("", 0)] {
        let mut json: serde_json::Value = serde_json::from_str(&save).unwrap();
        json["global_scopes"] = serde_json::json!({name:entity});
        assert!(World::restore(&json.to_string(), "bad:v1", world.schema()).is_err());
    }
    assert!(
        world
            .execute(
                EntityId(0),
                &[
                    Action::Emit("prefix".into()),
                    selected(
                        global("destination"),
                        EntityKind::Country,
                        vec![Action::Emit("incorrect".into())]
                    )
                ]
            )
            .is_err()
    );
    assert_eq!(world.events().len(), 1);
    assert!(world.save("failed:v1").is_err());
}

#[test]
fn named_scopes_share_depth_limits_and_validate_unselected_bodies() {
    let loop_plan = vec![selected(
        global("loop"),
        EntityKind::Country,
        vec![Action::Run(RuleId(0))],
    )];
    let mut world = World::with_rules(Schema::new(["value"]).unwrap(), vec![loop_plan]);
    let country = world.add_entity("Union", EntityKind::Country);
    world.bind_global_scope("loop", country).unwrap();
    assert!(
        world
            .execute(country, &[Action::Run(RuleId(0))])
            .unwrap_err()
            .to_string()
            .contains("call depth")
    );
    let mut world = self::world();
    let before = world.clone();
    assert!(
        world
            .schedule(
                EntityId(0),
                1,
                vec![selected(
                    global("missing"),
                    EntityKind::Country,
                    vec![Action::Run(RuleId(99))]
                )]
            )
            .is_err()
    );
    assert_eq!(world, before);
}
