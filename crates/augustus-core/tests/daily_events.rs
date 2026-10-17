// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    Action, Calendar, Condition, DailyEventCandidate, DailyEventCountry, DailyEventGroup,
    DailyEventRules, EntityId, EntityKind, EventCatalog, EventDefinition, EventOption, FieldId,
    Schema, Target, World, WorldDate,
};

fn event(id: &str, immediate: Vec<Action>) -> EventDefinition {
    EventDefinition {
        id: id.into(),
        scope: EntityKind::Country,
        immediate,
        options: vec![EventOption {
            id: "accept".into(),
            effects: Vec::new(),
        }],
        after: Vec::new(),
    }
}

fn world() -> (World, EntityId) {
    let mut world = World::new(Schema::new(["phase"]).unwrap());
    let country = world.add_entity("AAA", EntityKind::Country);
    world
        .set_calendar(Calendar {
            start: WorldDate {
                year: 1444,
                month: 11,
                day: 11,
            },
            month_lengths: [31; 12],
        })
        .unwrap();
    (world, country)
}

#[test]
fn all_groups_freeze_eligibility_before_immediate_effects_and_keep_order() {
    let (mut world, country) = world();
    let catalog = EventCatalog::new([
        event(
            "first",
            vec![Action::Set {
                target: Target::Current,
                field: FieldId(0),
                value: 1,
            }],
        ),
        event("second", vec![Action::Emit("second".into())]),
    ])
    .unwrap()
    .with_conditions(
        [(
            "second".into(),
            Condition::AtLeast {
                target: Target::Current,
                field: FieldId(0),
                value: 1,
            },
        )]
        .into(),
    )
    .unwrap()
    .with_once_events(["first".into()].into())
    .unwrap();
    world.set_event_catalog(catalog.clone()).unwrap();

    let rules = DailyEventRules::new(
        "daily-events:test:v1",
        vec![DailyEventCountry {
            country,
            groups: vec![DailyEventGroup::all(["first", "second"])],
        }],
    )
    .unwrap();
    world.select_daily_system(&rules).unwrap();

    world.advance_with_daily_system(1, &rules).unwrap();
    assert_eq!(
        world
            .pending_events()
            .iter()
            .map(|pending| pending.event.as_str())
            .collect::<Vec<_>>(),
        ["first"]
    );
    assert_eq!(world.value(country, FieldId(0)).unwrap(), 1);
    assert_eq!(
        world
            .fired_events()
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["first"]
    );

    world.advance_with_daily_system(2, &rules).unwrap();
    assert_eq!(
        world
            .pending_events()
            .iter()
            .map(|pending| pending.event.as_str())
            .collect::<Vec<_>>(),
        ["first", "second"]
    );
    assert_eq!(
        world.events().last().map(|event| event.2.as_str()),
        Some("second")
    );

    let save = world.save("daily-events").unwrap();
    let restored =
        World::restore_with_events(&save, "daily-events", world.schema(), Vec::new(), catalog)
            .unwrap();
    assert_eq!(restored.daily_system_id(), Some("daily-events:test:v1"));
}

#[test]
fn weighted_groups_choose_one_positive_candidate_and_ignore_zero_weight_entries() {
    let (mut world, country) = world();
    world
        .set_event_catalog(
            EventCatalog::new([
                event("left", vec![Action::Emit("left".into())]),
                event("right", vec![Action::Emit("right".into())]),
            ])
            .unwrap(),
        )
        .unwrap();
    let rules = DailyEventRules::new(
        "daily-events:weighted:v1",
        vec![DailyEventCountry {
            country,
            groups: vec![DailyEventGroup::weighted(vec![
                DailyEventCandidate {
                    event: "left".into(),
                    weight: 1,
                },
                DailyEventCandidate {
                    event: "right".into(),
                    weight: 0,
                },
            ])],
        }],
    )
    .unwrap();
    world.select_daily_system(&rules).unwrap();
    world.advance_with_daily_system(1, &rules).unwrap();

    assert_eq!(world.pending_events().len(), 1);
    assert_eq!(world.pending_events()[0].event, "left");
    assert_eq!(
        world.events().last().map(|event| event.2.as_str()),
        Some("left")
    );
}

#[test]
fn daily_event_rules_reject_duplicate_countries_unknown_events_and_negative_weights() {
    let (mut world, country) = world();
    world
        .set_event_catalog(EventCatalog::new([event("known", Vec::new())]).unwrap())
        .unwrap();

    let duplicate = DailyEventRules::new(
        "daily-events:invalid:v1",
        vec![
            DailyEventCountry {
                country,
                groups: Vec::new(),
            },
            DailyEventCountry {
                country,
                groups: Vec::new(),
            },
        ],
    )
    .unwrap();
    assert!(world.select_daily_system(&duplicate).is_err());

    let unknown = DailyEventRules::new(
        "daily-events:unknown:v1",
        vec![DailyEventCountry {
            country,
            groups: vec![DailyEventGroup::all(["missing"])],
        }],
    )
    .unwrap();
    assert!(world.select_daily_system(&unknown).is_err());

    let negative = DailyEventRules::new(
        "daily-events:negative:v1",
        vec![DailyEventCountry {
            country,
            groups: vec![DailyEventGroup::weighted(vec![DailyEventCandidate {
                event: "known".into(),
                weight: -1,
            }])],
        }],
    )
    .unwrap();
    assert!(world.select_daily_system(&negative).is_err());
    assert_eq!(world.daily_system_id(), None);
}
