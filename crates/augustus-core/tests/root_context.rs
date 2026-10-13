// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    Action, Condition, DiplomaticKind, DiplomaticRelation, EntityId, EntityKind, EntityRef,
    EventCatalog, EventChoice, EventDefinition, EventOption, PendingEventId, Politics, Schema,
    ScopeContext, ScopeSelector, Subjects, Target, World,
};

fn fixture() -> World {
    let mut world = World::new(Schema::new(["unused"]).unwrap());
    for name in ["Union", "Vassal", "Tributary", "Partner", "Other"] {
        world.add_entity(name, EntityKind::Country);
    }
    let port = world.add_entity("Port", EntityKind::Province);
    world.set_owner(port, EntityId(0)).unwrap();
    world.set_capital(EntityId(0), port).unwrap();
    world
        .set_politics(Politics {
            relations: vec![
                relation(0, 1, "vassal"),
                relation(0, 2, "tributary"),
                relation(1, 3, "union"),
            ],
            ..Default::default()
        })
        .unwrap();
    world
        .set_subjects(Subjects {
            types: ["vassal", "tributary", "union"].map(String::from).into(),
            known: true,
            ..Default::default()
        })
        .unwrap();
    world
}

fn relation(first: usize, second: usize, kind: &str) -> DiplomaticRelation {
    DiplomaticRelation {
        kind: DiplomaticKind::Subject,
        first: EntityId(first),
        second: EntityId(second),
        subject_type: Some(kind.into()),
    }
}

fn root_is(name: &str) -> Condition {
    Condition::InScope {
        selector: ScopeSelector::Root,
        kind: EntityKind::Country,
        condition: Box::new(Condition::EntityNamed {
            target: Target::Current,
            name: name.into(),
        }),
    }
}

fn root_emit(name: &str, kind: EntityKind) -> Action {
    Action::InScope {
        selector: ScopeSelector::Root,
        kind,
        plan: vec![Action::Emit(name.into())],
    }
}

#[test]
fn nested_queries_and_iteration_keep_the_entry_root() {
    let mut world = fixture();
    let query = Condition::AnySubjectCountry {
        target: Target::Current,
        condition: Box::new(Condition::All(vec![
            Condition::SubjectOf {
                subject: EntityRef::Relative(Target::Current),
                overlord: EntityRef::Selected(ScopeSelector::Root),
            },
            Condition::Any(vec![root_is("Other"), root_is("Union")]),
            Condition::Not(Box::new(root_is("Vassal"))),
        ])),
    };
    assert!(query.evaluate(&world, EntityId(0)).unwrap());
    assert!(
        world
            .any_subject_country(EntityId(0), &root_is("Union"))
            .unwrap()
    );
    let owned = Condition::AnyOwnedProvince {
        target: Target::Current,
        condition: Box::new(root_is("Union")),
    };
    assert!(owned.evaluate(&world, EntityId(0)).unwrap());
    assert!(
        world
            .any_owned_province(EntityId(0), &root_is("Union"))
            .unwrap()
    );
    let mut partial = world.clone();
    partial.add_entity("Unknown owner", EntityKind::Province);
    assert!(owned.evaluate(&partial, EntityId(0)).unwrap());
    world
        .execute(
            EntityId(0),
            &[
                Action::ForEachOwnedProvince {
                    country: Target::Current,
                    condition: root_is("Union"),
                    plan: vec![root_emit("owned", EntityKind::Country)],
                },
                Action::RandomOwnedProvince {
                    country: Target::Current,
                    condition: root_is("Union"),
                    plan: vec![root_emit("random", EntityKind::Country)],
                },
                Action::ForEach {
                    selectors: vec![
                        ScopeSelector::Named {
                            kind: EntityKind::Country,
                            name: "Vassal".into(),
                        },
                        ScopeSelector::Root,
                    ],
                    kind: EntityKind::Country,
                    condition: root_is("Union"),
                    plan: vec![root_emit("selected", EntityKind::Country)],
                },
            ],
        )
        .unwrap();
    assert_eq!(
        world
            .events()
            .iter()
            .map(|e| (e.1, e.2.as_str()))
            .collect::<Vec<_>>(),
        [
            (EntityId(0), "owned"),
            (EntityId(0), "random"),
            (EntityId(0), "selected"),
            (EntityId(0), "selected")
        ]
    );
}

#[test]
fn queued_plans_capture_root_identity_but_resolve_live_relationships_after_restore() {
    let mut world = fixture();
    world.bind_global_scope("holder", EntityId(0)).unwrap();
    world
        .execute(
            EntityId(0),
            &[Action::With {
                target: Target::Capital,
                plan: vec![Action::Schedule {
                    delay: 3,
                    plan: vec![
                        root_emit("captured", EntityKind::Country),
                        Action::InScope {
                            selector: ScopeSelector::Global("holder".into()),
                            kind: EntityKind::Country,
                            plan: vec![Action::Emit("live".into())],
                        },
                    ],
                }],
            }],
        )
        .unwrap();
    let saved = world.save("context").unwrap();
    let mut restored = World::restore(&saved, "context", world.schema()).unwrap();
    for w in [&mut world, &mut restored] {
        w.bind_global_scope("holder", EntityId(4)).unwrap();
        w.set_owner(EntityId(5), EntityId(4)).unwrap();
        w.advance_to(3).unwrap();
    }
    assert_eq!(world, restored);
    assert_eq!(
        world.events().iter().map(|e| e.1).collect::<Vec<_>>(),
        [EntityId(0), EntityId(4)]
    );
    let mut invalid: serde_json::Value = serde_json::from_str(&saved).unwrap();
    invalid["tasks"][0]["root"] = 999.into();
    assert!(World::restore(&invalid.to_string(), "context", world.schema()).is_err());
    let mut legacy = fixture();
    legacy
        .schedule(
            EntityId(0),
            1,
            vec![root_emit("legacy", EntityKind::Country)],
        )
        .unwrap();
    let saved = legacy.save("legacy").unwrap();
    assert!(
        !serde_json::from_str::<serde_json::Value>(&saved).unwrap()["tasks"][0]
            .as_object()
            .unwrap()
            .contains_key("root")
    );
    let mut loaded = World::restore(&saved, "legacy", legacy.schema()).unwrap();
    assert_eq!(saved, loaded.save("legacy").unwrap());
    loaded.advance_to(1).unwrap();
    assert_eq!(loaded.events()[0].1, EntityId(0));
}

#[test]
fn subject_endpoints_query_direct_live_relations_and_preserve_unknown_state_errors() {
    let mut world = fixture();
    assert!(world.is_subject_of(EntityId(1), EntityId(0)).unwrap());
    assert!(world.is_subject_of(EntityId(2), EntityId(0)).unwrap());
    assert!(world.is_subject_of(EntityId(3), EntityId(1)).unwrap());
    assert!(!world.is_subject_of(EntityId(3), EntityId(0)).unwrap());
    assert!(!world.is_subject_of(EntityId(0), EntityId(0)).unwrap());
    assert!(!world.is_subject_of(EntityId(0), EntityId(1)).unwrap());
    let mut politics = world.politics().clone();
    politics.relations.retain(|r| r.second != EntityId(1));
    world.set_politics(politics).unwrap();
    assert!(!world.is_subject_of(EntityId(1), EntityId(0)).unwrap());
    world.bind_global_scope("place", EntityId(5)).unwrap();
    let subject = |selector| Condition::SubjectOf {
        subject: EntityRef::Relative(Target::Current),
        overlord: EntityRef::Selected(selector),
    };
    assert!(
        !subject(ScopeSelector::Global("place".into()))
            .evaluate(&world, EntityId(1))
            .unwrap()
    );
    assert!(
        !subject(ScopeSelector::Global("missing".into()))
            .evaluate(&world, EntityId(1))
            .unwrap()
    );
    let mut subjects = world.subjects().clone();
    subjects.known = false;
    world.set_subjects(subjects).unwrap();
    assert!(
        subject(ScopeSelector::Root)
            .evaluate(&world, EntityId(1))
            .is_err()
    );
    assert!(
        subject(ScopeSelector::Named {
            kind: EntityKind::Country,
            name: "Unregistered".into()
        })
        .evaluate(&world, EntityId(1))
        .is_err()
    );
}

fn event(id: &str, scope: EntityKind, immediate: Vec<Action>) -> EventDefinition {
    EventDefinition {
        id: id.into(),
        scope,
        immediate,
        options: vec![EventOption {
            id: "accept".into(),
            effects: vec![root_emit("option", scope)],
        }],
        after: vec![root_emit("after", scope)],
    }
}

#[test]
fn event_entry_resets_root_and_caller_options_retain_their_receiver_after_restore() {
    let mut world = fixture();
    let child = event(
        "child",
        EntityKind::Country,
        vec![root_emit("child", EntityKind::Country)],
    );
    let province = event(
        "province",
        EntityKind::Province,
        vec![Action::With {
            target: Target::Owner,
            plan: vec![root_emit("province", EntityKind::Province)],
        }],
    );
    let parent = event(
        "parent",
        EntityKind::Country,
        vec![
            Action::InScope {
                selector: ScopeSelector::Named {
                    kind: EntityKind::Country,
                    name: "Other".into(),
                },
                kind: EntityKind::Country,
                plan: vec![
                    Action::TriggerEvent {
                        event: "child".into(),
                    },
                    root_emit("caller", EntityKind::Country),
                    Action::Schedule {
                        delay: 2,
                        plan: vec![Action::TriggerEvent {
                            event: "child".into(),
                        }],
                    },
                ],
            },
            Action::With {
                target: Target::Capital,
                plan: vec![Action::TriggerEvent {
                    event: "province".into(),
                }],
            },
        ],
    );
    world
        .set_event_catalog(EventCatalog::new(vec![parent, child, province]).unwrap())
        .unwrap();
    let parent = world.trigger_event("parent", EntityId(0)).unwrap();
    assert_eq!(parent, PendingEventId(2));
    assert_eq!(
        world
            .events()
            .iter()
            .map(|e| (e.1, e.2.as_str()))
            .collect::<Vec<_>>(),
        [
            (EntityId(4), "child"),
            (EntityId(0), "caller"),
            (EntityId(5), "province")
        ]
    );
    let saved = world.save("events").unwrap();
    let mut restored = World::restore_with_events(
        &saved,
        "events",
        world.schema(),
        vec![],
        world.event_catalog().clone(),
    )
    .unwrap();
    for w in [&mut world, &mut restored] {
        w.set_owner(EntityId(5), EntityId(4)).unwrap();
        for pending in [0, 1, 2] {
            w.choose_event(&EventChoice {
                pending: PendingEventId(pending),
                option: "accept".into(),
            })
            .unwrap();
        }
        w.advance_to(2).unwrap();
    }
    assert_eq!(world, restored);
    assert_eq!(
        world
            .events()
            .iter()
            .skip(3)
            .map(|e| e.1)
            .collect::<Vec<_>>(),
        [
            EntityId(4),
            EntityId(4),
            EntityId(5),
            EntityId(5),
            EntityId(0),
            EntityId(0),
            EntityId(4)
        ]
    );
}

#[test]
fn root_selection_requires_valid_context_and_obeys_scope_type_checks() {
    let mut world = fixture();
    assert!(world.resolve_scope(&ScopeSelector::Root).is_err());
    assert!(
        root_is("Union")
            .evaluate_in_context(
                &world,
                ScopeContext {
                    current: EntityId(1),
                    root: EntityId(99)
                }
            )
            .is_err()
    );
    assert!(
        world
            .schedule_in_context(
                ScopeContext {
                    current: EntityId(1),
                    root: EntityId(99)
                },
                2,
                vec![]
            )
            .is_err()
    );
    assert!(
        world
            .execute_in_context(
                ScopeContext {
                    current: EntityId(1),
                    root: EntityId(5)
                },
                &[root_emit("wrong", EntityKind::Country)]
            )
            .is_err()
    );
    assert!(world.events().is_empty());
}
