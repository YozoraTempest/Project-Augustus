// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    Action, EntityId, EntityKind, EventCatalog, EventChoice, EventDefinition, EventLocations,
    EventOption, FieldId, PendingEventId, Schema, Target, World,
};
use std::collections::BTreeMap;

fn catalog() -> EventCatalog {
    EventCatalog::new([EventDefinition {
        id: "visit".into(),
        scope: EntityKind::Country,
        immediate: vec![],
        options: vec![
            EventOption {
                id: "0".into(),
                effects: vec![Action::Add {
                    target: Target::Current,
                    field: FieldId(0),
                    value: 1,
                }],
            },
            EventOption {
                id: "1".into(),
                effects: vec![],
            },
        ],
        after: vec![],
    }])
    .unwrap()
}
fn locations(event: Option<&str>, options: &[(&str, &str)]) -> BTreeMap<String, EventLocations> {
    BTreeMap::from([(
        "visit".into(),
        EventLocations {
            event: event.map(str::to_owned),
            options: options
                .iter()
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect(),
        },
    )])
}
fn world() -> World {
    let mut world = World::new(Schema::new(["treasury"]).unwrap());
    world.add_entity("AAA", EntityKind::Country);
    let province = world.add_entity("7", EntityKind::Province);
    world.add_entity("8", EntityKind::Province);
    world.set_owner(province, EntityId(0)).unwrap();
    world.set_capital(EntityId(0), province).unwrap();
    world
}

#[test]
fn event_and_option_locations_are_pure_independent_content_queries_across_restore() {
    let mut world = world();
    world
        .set_event_catalog(
            catalog()
                .with_locations(locations(Some("8"), &[("0", "7")]))
                .unwrap(),
        )
        .unwrap();
    let id = world.trigger_event("visit", EntityId(0)).unwrap();
    world.advance_to(4).unwrap();
    let save = world.save("locations").unwrap();
    let mut restored = World::restore_with_events(
        &save,
        "locations",
        world.schema(),
        vec![],
        world.event_catalog().clone(),
    )
    .unwrap();
    for current in [&mut world, &mut restored] {
        assert_eq!(
            current.pending_event_location(id).unwrap(),
            Some(EntityId(2))
        );
        assert_eq!(
            current.pending_event_option_location(id, "0").unwrap(),
            Some(EntityId(1))
        );
        assert_eq!(
            current.pending_event_option_location(id, "1").unwrap(),
            None
        );
        assert!(
            current
                .pending_event_option_location(id, "missing")
                .is_err()
        );
        assert!(current.pending_event_location(PendingEventId(99)).is_err());
        assert_eq!(current.save("locations").unwrap(), save);
        current
            .choose_event(&EventChoice {
                pending: id,
                option: "0".into(),
            })
            .unwrap();
        assert!(current.pending_event_location(id).is_err());
        assert!(current.pending_event_option_location(id, "0").is_err());
    }
    assert_eq!(
        world.save("locations").unwrap(),
        restored.save("locations").unwrap()
    );
}

#[test]
fn option_only_and_plain_events_do_not_invent_an_event_location() {
    for values in [BTreeMap::new(), locations(None, &[("1", "8")])] {
        let mut world = world();
        world
            .set_event_catalog(catalog().with_locations(values).unwrap())
            .unwrap();
        let id = world.trigger_event("visit", EntityId(0)).unwrap();
        assert_eq!(world.pending_event_location(id).unwrap(), None);
        assert_eq!(world.pending_event_option_location(id, "0").unwrap(), None);
        let json: serde_json::Value =
            serde_json::from_str(&world.save("locations").unwrap()).unwrap();
        assert!(
            !json["event_queue"]
                .as_object()
                .unwrap()
                .contains_key("locations")
        );
    }
}

#[test]
fn invalid_content_and_missing_wrong_kind_or_ambiguous_provinces_are_rejected_atomically() {
    for values in [
        locations(None, &[]),
        locations(Some(" "), &[]),
        locations(None, &[("missing", "7")]),
    ] {
        assert!(catalog().with_locations(values).is_err());
    }
    let mut unknown = locations(Some("7"), &[]);
    unknown.insert("missing".into(), unknown["visit"].clone());
    assert!(catalog().with_locations(unknown).is_err());
    for name in ["missing", "AAA"] {
        let mut world = world();
        let before = world.clone();
        assert!(
            world
                .set_event_catalog(
                    catalog()
                        .with_locations(locations(Some(name), &[]))
                        .unwrap()
                )
                .is_err()
        );
        assert_eq!(world, before);
    }
    let mut world = world();
    world.add_entity("7", EntityKind::Province);
    let before = world.clone();
    assert!(
        world
            .set_event_catalog(catalog().with_locations(locations(Some("7"), &[])).unwrap())
            .is_err()
    );
    assert_eq!(world, before);
}
