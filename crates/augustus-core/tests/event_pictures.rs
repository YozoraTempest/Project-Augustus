// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::BTreeMap;

use augustus_core::{
    Action, Completions, Condition, EntityId, EntityKind, EventCatalog, EventChoice,
    EventDefinition, EventOption, EventPictures, FieldId, PendingEventId, PictureVariant, Schema,
    ScopeSelector, Target, World,
};

fn variant(key: &str, condition: Condition) -> PictureVariant {
    PictureVariant {
        key: key.into(),
        condition,
    }
}

fn definition(id: &str, immediate: Vec<Action>) -> EventDefinition {
    EventDefinition {
        id: id.into(),
        scope: EntityKind::Country,
        immediate,
        options: vec![EventOption {
            id: "0".into(),
            effects: vec![],
        }],
        after: vec![],
    }
}

fn catalog(pictures: EventPictures, immediate: Vec<Action>) -> EventCatalog {
    EventCatalog::new([definition("test", immediate)])
        .unwrap()
        .with_pictures(BTreeMap::from([("test".into(), pictures)]))
        .unwrap()
}

fn world() -> World {
    let mut world = World::new(Schema::new(["gate"]).unwrap());
    world.add_entity("AAA", EntityKind::Country);
    world.add_entity("BBB", EntityKind::Country);
    world
}

fn restore(world: &World, save: &str) -> Result<World, augustus_core::Error> {
    World::restore_with_events(
        save,
        "test",
        world.schema(),
        vec![],
        world.event_catalog().clone(),
    )
}

#[test]
fn ordered_selection_has_only_the_fallback_declared_by_content() {
    let world = world();
    let mut pictures = EventPictures {
        variants: vec![
            variant("first", Condition::Always(true)),
            variant("second", Condition::Always(true)),
        ],
        fallback: Some("unmatched".into()),
    };
    let before = world.save("test").unwrap();
    assert_eq!(pictures.select(&world, EntityId(0)).unwrap(), Some("first"));
    pictures.variants[0].condition = Condition::Always(false);
    assert_eq!(
        pictures.select(&world, EntityId(0)).unwrap(),
        Some("second")
    );
    pictures.variants[1].condition = Condition::Always(false);
    assert_eq!(
        pictures.select(&world, EntityId(0)).unwrap(),
        Some("unmatched")
    );
    pictures.fallback = None;
    assert_eq!(pictures.select(&world, EntityId(0)).unwrap(), None);
    assert!(pictures.select(&world, EntityId(99)).is_err());
    assert_eq!(world.save("test").unwrap(), before);
}

#[test]
fn selection_reads_immediate_writes_then_survives_live_rebinding_and_restore() {
    let mut world = world();
    let pictures = EventPictures {
        variants: vec![variant(
            "holder",
            Condition::ScopeIsCurrent(ScopeSelector::Global("holder".into())),
        )],
        fallback: Some("other".into()),
    };
    world
        .set_event_catalog(catalog(
            pictures,
            vec![Action::BindGlobalScope {
                name: "holder".into(),
            }],
        ))
        .unwrap();
    let id = world.trigger_event("test", EntityId(0)).unwrap();
    assert_eq!(world.pending_event_picture(id).unwrap(), Some("holder"));
    world.bind_global_scope("holder", EntityId(1)).unwrap();
    world.advance_to(3).unwrap();
    assert_eq!(
        world.event_catalog().pictures()["test"]
            .select(&world, EntityId(0))
            .unwrap(),
        Some("other")
    );
    let save = world.save("test").unwrap();
    let mut resumed = restore(&world, &save).unwrap();
    assert_eq!(resumed.pending_event_picture(id).unwrap(), Some("holder"));
    assert_eq!(resumed.save("test").unwrap(), save);
    assert_eq!(world.save("test").unwrap(), save);
    for state in [&mut world, &mut resumed] {
        let before = state.clone();
        assert!(
            state
                .choose_event(&EventChoice {
                    pending: id,
                    option: "missing".into()
                })
                .is_err()
        );
        assert_eq!(*state, before);
        state
            .choose_event(&EventChoice {
                pending: id,
                option: "0".into(),
            })
            .unwrap();
        assert!(state.pending_event_picture(id).is_err());
        assert!(state.pending_events().is_empty());
        assert!(!serde_json::from_str::<serde_json::Value>(&state.save("test").unwrap()).unwrap()["event_queue"].as_object().unwrap().contains_key("pictures"));
    }
    assert_eq!(world.save("test").unwrap(), resumed.save("test").unwrap());
}

#[test]
fn nested_dispatches_capture_separate_pictures_after_their_own_immediate_effects() {
    let mut world = world();
    let gate = Condition::AtLeast {
        target: Target::Current,
        field: FieldId(0),
        value: 1,
    };
    let pictures = EventPictures {
        variants: vec![variant("set", gate)],
        fallback: Some("unset".into()),
    };
    let catalog = EventCatalog::new([
        definition(
            "outer",
            vec![
                Action::TriggerEvent {
                    event: "inner".into(),
                },
                Action::Set {
                    target: Target::Current,
                    field: FieldId(0),
                    value: 0,
                },
            ],
        ),
        definition(
            "inner",
            vec![Action::Set {
                target: Target::Current,
                field: FieldId(0),
                value: 1,
            }],
        ),
    ])
    .unwrap()
    .with_pictures(BTreeMap::from([
        ("outer".into(), pictures.clone()),
        ("inner".into(), pictures),
    ]))
    .unwrap();
    world.set_event_catalog(catalog).unwrap();
    assert_eq!(
        world.trigger_event("outer", EntityId(0)).unwrap(),
        PendingEventId(1)
    );
    assert_eq!(
        world.pending_event_picture(PendingEventId(0)).unwrap(),
        Some("set")
    );
    assert_eq!(
        world.pending_event_picture(PendingEventId(1)).unwrap(),
        Some("unset")
    );
    assert_eq!(
        restore(&world, &world.save("test").unwrap()).unwrap(),
        world
    );
}

#[test]
fn unknown_conditions_cannot_be_silenced_by_a_fallback() {
    let mut world = world();
    let pictures = EventPictures {
        variants: vec![variant(
            "human",
            Condition::CountryControlled {
                target: Target::Current,
                controller: augustus_core::CountryController::Human,
            },
        )],
        fallback: Some("other".into()),
    };
    assert!(
        pictures
            .select(&world, EntityId(0))
            .unwrap_err()
            .to_string()
            .contains("unknown country controller")
    );
    world.set_event_catalog(catalog(pictures, vec![])).unwrap();
    assert!(world.trigger_event("test", EntityId(0)).is_err());
    assert!(world.pending_events().is_empty());
    assert!(world.advance_to(1).is_err());
}

#[test]
fn content_dependencies_are_validated_even_in_unreachable_picture_variants() {
    let mut world = world();
    world
        .set_completions(Completions {
            identifiers: ["milestone".into()].into(),
            entities: BTreeMap::from([(EntityId(0), Default::default())]),
        })
        .unwrap();
    let pictures = EventPictures {
        variants: vec![
            variant("first", Condition::Always(true)),
            variant(
                "later",
                Condition::CompletionRecorded {
                    target: Target::Current,
                    id: "milestone".into(),
                },
            ),
        ],
        fallback: None,
    };
    world.set_event_catalog(catalog(pictures, vec![])).unwrap();
    let before = world.clone();
    assert!(world.set_completions(Completions::default()).is_err());
    assert_eq!(world, before);
    assert_eq!(
        world.trigger_event("test", EntityId(0)).unwrap(),
        PendingEventId(0)
    );
}

#[test]
fn saved_selection_requires_a_pending_record_and_a_declared_resource_key() {
    let mut world = world();
    world
        .set_event_catalog(catalog(
            EventPictures {
                variants: vec![variant("first", Condition::Always(true))],
                fallback: Some("other".into()),
            },
            vec![],
        ))
        .unwrap();
    world.trigger_event("test", EntityId(0)).unwrap();
    let saved: serde_json::Value = serde_json::from_str(&world.save("test").unwrap()).unwrap();
    for selections in [
        serde_json::json!({}),
        serde_json::json!({"0":"invented"}),
        serde_json::json!({"0":null}),
        serde_json::json!({"0":"first", "99":"first"}),
    ] {
        let mut bad = saved.clone();
        bad["event_queue"]["pictures"] = selections;
        assert!(restore(&world, &bad.to_string()).is_err());
    }
}

#[test]
fn no_picture_selection_is_saved_explicitly_and_content_without_pictures_stays_unchanged() {
    let mut world = world();
    world
        .set_event_catalog(catalog(
            EventPictures {
                variants: vec![variant("unused", Condition::Always(false))],
                fallback: None,
            },
            vec![],
        ))
        .unwrap();
    let id = world.trigger_event("test", EntityId(0)).unwrap();
    assert_eq!(world.pending_event_picture(id).unwrap(), None);
    let save = world.save("test").unwrap();
    assert!(
        serde_json::from_str::<serde_json::Value>(&save).unwrap()["event_queue"]["pictures"]["0"]
            .is_null()
    );
    assert_eq!(restore(&world, &save).unwrap().save("test").unwrap(), save);
    let mut plain = World::new(Schema::new(["gate"]).unwrap());
    plain.add_entity("AAA", EntityKind::Country);
    plain
        .set_event_catalog(EventCatalog::new([definition("test", vec![])]).unwrap())
        .unwrap();
    plain.trigger_event("test", EntityId(0)).unwrap();
    let mut saved: serde_json::Value = serde_json::from_str(&plain.save("test").unwrap()).unwrap();
    assert!(
        !saved["event_queue"]
            .as_object()
            .unwrap()
            .contains_key("pictures")
    );
    saved["event_queue"]["pictures"] = serde_json::json!({"0":"unused"});
    assert!(restore(&plain, &saved.to_string()).is_err());
}

#[test]
fn invalid_picture_content_is_rejected_before_installation() {
    let base = EventCatalog::new([definition("test", vec![])]).unwrap();
    assert!(
        base.clone()
            .with_pictures(BTreeMap::from([(
                "absent".into(),
                EventPictures {
                    variants: vec![],
                    fallback: Some("key".into())
                }
            )]))
            .is_err()
    );
    for pictures in [
        EventPictures::default(),
        EventPictures {
            variants: vec![variant(" ", Condition::Always(true))],
            fallback: None,
        },
        EventPictures {
            variants: vec![],
            fallback: Some("".into()),
        },
    ] {
        assert!(
            base.clone()
                .with_pictures(BTreeMap::from([("test".into(), pictures)]))
                .is_err()
        );
    }
    let mut world = world();
    let before = world.clone();
    let invalid = catalog(
        EventPictures {
            variants: vec![variant(
                "key",
                Condition::AtLeast {
                    target: Target::Current,
                    field: FieldId(99),
                    value: 1,
                },
            )],
            fallback: None,
        },
        vec![],
    );
    assert!(world.set_event_catalog(invalid).is_err());
    assert_eq!(world, before);
}
