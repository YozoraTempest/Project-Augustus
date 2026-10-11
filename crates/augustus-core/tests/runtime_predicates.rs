// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    Action, Calendar, Condition, EntityId, EntityKind, EventCatalog, EventDefinition, EventOption,
    Schema, Target, World, WorldDate,
};

fn calendar() -> Calendar {
    Calendar {
        start: WorldDate {
            year: 1444,
            month: 12,
            day: 31,
        },
        month_lengths: [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31],
    }
}

fn event(id: &str, message: &str) -> EventDefinition {
    EventDefinition {
        id: id.into(),
        scope: EntityKind::Country,
        immediate: vec![Action::Emit(message.into())],
        options: vec![EventOption {
            id: "accept".into(),
            effects: Vec::new(),
        }],
        after: Vec::new(),
    }
}

#[test]
fn entity_predicates_read_the_selected_target_and_live_relationships() {
    let mut world = World::new(Schema::new(["value"]).unwrap());
    let first = world.add_entity("Union", EntityKind::Country);
    let second = world.add_entity("League", EntityKind::Country);
    let province = world.add_entity("Port", EntityKind::Province);
    world.set_owner(province, first).unwrap();
    let condition = Condition::EntityNamed {
        target: Target::Owner,
        name: "League".into(),
    };
    assert!(!condition.evaluate(&world, province).unwrap());
    world.set_owner(province, second).unwrap();
    assert!(condition.evaluate(&world, province).unwrap());
    assert!(condition.evaluate(&world, first).is_err());
    assert!(
        Condition::EntityNamed {
            target: Target::Current,
            name: "Union".into()
        }
        .evaluate(&world, first)
        .unwrap()
    );
}

#[test]
fn year_conditions_are_live_at_due_dates_and_across_saved_event_tasks() {
    let mut world = World::new(Schema::new(["value"]).unwrap());
    let country = world.add_entity("Union", EntityKind::Country);
    world.set_calendar(calendar()).unwrap();
    let before = Condition::Not(Box::new(Condition::YearAtLeast(1445)));
    world
        .set_event_catalog(
            EventCatalog::new([event("before", "before"), event("after", "after")])
                .unwrap()
                .with_conditions(
                    [
                        ("before".into(), before.clone()),
                        ("after".into(), Condition::YearAtLeast(1445)),
                    ]
                    .into(),
                )
                .unwrap(),
        )
        .unwrap();
    assert!(before.evaluate(&world, country).unwrap());
    world
        .execute(
            country,
            &[
                Action::TryEvent {
                    event: "after".into(),
                },
                Action::TryEvent {
                    event: "before".into(),
                },
            ],
        )
        .unwrap();
    for id in ["before", "after"] {
        world
            .schedule(country, 1, vec![Action::TryEvent { event: id.into() }])
            .unwrap();
    }
    let save = world.save("year:v1").unwrap();
    let mut resumed = World::restore_with_events(
        &save,
        "year:v1",
        world.schema(),
        Vec::new(),
        world.event_catalog().clone(),
    )
    .unwrap();
    for session in [&mut world, &mut resumed] {
        session.advance_to(1).unwrap();
    }
    assert_eq!(world, resumed);
    assert!(!before.evaluate(&world, country).unwrap());
    assert_eq!(
        world
            .events()
            .iter()
            .map(|e| (e.0, e.2.as_str()))
            .collect::<Vec<_>>(),
        [(0, "before"), (1, "after")]
    );
}

#[test]
fn any_and_all_short_circuit_in_order_without_hiding_missing_state() {
    let world = World::new(Schema::new(["value"]).unwrap());
    let year = Condition::YearAtLeast(1445);
    assert!(
        year.evaluate(&world, EntityId(0))
            .unwrap_err()
            .to_string()
            .contains("calendar")
    );
    assert!(
        Condition::Any(vec![Condition::Always(true), year.clone()])
            .evaluate(&world, EntityId(0))
            .unwrap()
    );
    assert!(
        !Condition::All(vec![Condition::Always(false), year.clone()])
            .evaluate(&world, EntityId(0))
            .unwrap()
    );
    assert!(
        Condition::Any(vec![Condition::Always(false), year.clone()])
            .evaluate(&world, EntityId(0))
            .is_err()
    );
    assert!(
        Condition::All(vec![Condition::Always(true), year])
            .evaluate(&world, EntityId(0))
            .is_err()
    );
    assert!(
        !Condition::Any(Vec::new())
            .evaluate(&world, EntityId(0))
            .unwrap()
    );
    assert!(
        Condition::All(Vec::new())
            .evaluate(&world, EntityId(0))
            .unwrap()
    );
}

#[test]
fn invalid_predicates_in_unselected_content_and_saved_tasks_are_rejected() {
    let mut world = World::new(Schema::new(["value"]).unwrap());
    let country = world.add_entity("Union", EntityKind::Country);
    for condition in [
        Condition::YearAtLeast(0),
        Condition::EntityNamed {
            target: Target::Current,
            name: String::new(),
        },
    ] {
        assert!(condition.evaluate(&world, country).is_err());
        let catalog = EventCatalog::new([event("test", "test")])
            .unwrap()
            .with_conditions(
                [(
                    "test".into(),
                    Condition::Any(vec![Condition::Always(true), condition.clone()]),
                )]
                .into(),
            )
            .unwrap();
        let before = world.clone();
        assert!(world.set_event_catalog(catalog).is_err());
        assert_eq!(world, before);
        assert!(
            world
                .schedule(
                    country,
                    1,
                    vec![Action::If {
                        condition,
                        then: Vec::new(),
                        otherwise: Vec::new()
                    }]
                )
                .is_err()
        );
        assert_eq!(world, before);
    }
    world
        .schedule(
            country,
            1,
            vec![Action::If {
                condition: Condition::YearAtLeast(1445),
                then: Vec::new(),
                otherwise: Vec::new(),
            }],
        )
        .unwrap();
    let mut saved: serde_json::Value =
        serde_json::from_str(&world.save("corrupt:v1").unwrap()).unwrap();
    saved["tasks"][0]["plan"][0]["If"]["condition"]["YearAtLeast"] = serde_json::json!(0);
    assert!(World::restore(&saved.to_string(), "corrupt:v1", world.schema()).is_err());
}
