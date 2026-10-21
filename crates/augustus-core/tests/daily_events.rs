// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    Action, Calendar, Condition, DailyEventCandidate, DailyEventCountry, DailyEventGroup,
    DailyEventRules, DailyEventSchedule, EntityId, EntityKind, EventCatalog, EventDefinition,
    EventOption, FieldId, ScheduledDailyEventCountry, ScheduledDailyEventGroup,
    ScheduledDailyEventRules, Schema, Target, World, WorldDate,
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
                    event: Some("left".into()),
                    weight: 1,
                },
                DailyEventCandidate {
                    event: Some("right".into()),
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
                event: Some("known".into()),
                weight: -1,
            }])],
        }],
    )
    .unwrap();
    assert!(world.select_daily_system(&negative).is_err());
    assert_eq!(world.daily_system_id(), None);
}

#[test]
fn weighted_group_can_select_an_empty_candidate_without_dispatching_an_event() {
    let (mut world, country) = world();
    world
        .set_event_catalog(
            EventCatalog::new([event("known", vec![Action::Emit("known".into())])]).unwrap(),
        )
        .unwrap();

    let rules = DailyEventRules::new(
        "daily-events:empty-weight:v1",
        vec![DailyEventCountry {
            country,
            groups: vec![DailyEventGroup::weighted(vec![DailyEventCandidate {
                event: None,
                weight: 5,
            }])],
        }],
    )
    .unwrap();
    world.select_daily_system(&rules).unwrap();
    world.advance_with_daily_system(1, &rules).unwrap();

    assert!(world.pending_events().is_empty());
    assert!(world.events().is_empty());
}

#[test]
fn weighted_group_keeps_empty_candidate_weight_separate_from_event_weight() {
    let (mut world, country) = world();
    world
        .set_event_catalog(
            EventCatalog::new([event("known", vec![Action::Emit("known".into())])]).unwrap(),
        )
        .unwrap();

    let rules = DailyEventRules::new(
        "daily-events:empty-mixed:v1",
        vec![DailyEventCountry {
            country,
            groups: vec![DailyEventGroup::weighted(vec![
                DailyEventCandidate {
                    event: None,
                    weight: 1,
                },
                DailyEventCandidate {
                    event: Some("known".into()),
                    weight: 1,
                },
            ])],
        }],
    )
    .unwrap();
    world.select_daily_system(&rules).unwrap();

    let mut saw_empty = false;
    let mut saw_event = false;
    for day in 1..=128 {
        let before = world.pending_events().len();
        world.advance_with_daily_system(day, &rules).unwrap();
        match world.pending_events().len() - before {
            0 => saw_empty = true,
            1 => saw_event = true,
            count => panic!("weighted group dispatched {count} events on day {day}"),
        }
        if saw_empty && saw_event {
            break;
        }
    }

    assert!(saw_empty, "empty candidate never occupied its weight");
    assert!(saw_event, "normal candidate never occupied its weight");
}

#[test]
fn all_group_rejects_empty_candidates() {
    let (mut world, country) = world();
    world
        .set_event_catalog(EventCatalog::new([event("known", Vec::new())]).unwrap())
        .unwrap();

    let rules = DailyEventRules::new(
        "daily-events:empty-all:v1",
        vec![DailyEventCountry {
            country,
            groups: vec![DailyEventGroup {
                mode: augustus_core::DailyEventGroupMode::All,
                candidates: vec![DailyEventCandidate {
                    event: None,
                    weight: 0,
                }],
            }],
        }],
    )
    .unwrap();

    assert!(world.select_daily_system(&rules).is_err());
    assert_eq!(world.daily_system_id(), None);
}

#[test]
fn daily_schedule_uses_year_day_and_signed_country_index() {
    let calendar = Calendar {
        start: WorldDate {
            year: 1444,
            month: 1,
            day: 1,
        },
        month_lengths: [31; 12],
    };
    let schedule = DailyEventSchedule {
        country_index: 2,
        interval: 5,
    };

    assert!(
        schedule
            .is_due_on(
                WorldDate {
                    year: 1444,
                    month: 1,
                    day: 5,
                },
                &calendar,
            )
            .unwrap()
    );
    assert!(
        !schedule
            .is_due_on(
                WorldDate {
                    year: 1444,
                    month: 1,
                    day: 4,
                },
                &calendar,
            )
            .unwrap()
    );
    assert!(
        !schedule
            .is_due_on(
                WorldDate {
                    year: 1444,
                    month: 2,
                    day: 1,
                },
                &calendar,
            )
            .unwrap()
    );

    let negative_index = DailyEventSchedule {
        country_index: -4,
        interval: 5,
    };
    assert!(
        negative_index
            .is_due_on(
                WorldDate {
                    year: 1444,
                    month: 1,
                    day: 1,
                },
                &calendar,
            )
            .unwrap()
    );

    let year_boundary = DailyEventSchedule {
        country_index: 0,
        interval: 5,
    };
    assert!(
        year_boundary
            .is_due_on(
                WorldDate {
                    year: 1445,
                    month: 1,
                    day: 1,
                },
                &calendar,
            )
            .unwrap()
    );
    assert!(
        !year_boundary
            .is_due_on(
                WorldDate {
                    year: 1446,
                    month: 1,
                    day: 1,
                },
                &calendar,
            )
            .unwrap()
    );
}

#[test]
fn scheduled_groups_do_not_consume_random_values_before_their_due_day() {
    let country = EntityId(0);
    let mut world = World::eu4_seeded_from_saved_count(
        Schema::new([] as [&str; 0]).unwrap(),
        Vec::new(),
        8351375,
        0,
    );
    world.add_entity("AAA", EntityKind::Country);
    world
        .set_calendar(Calendar {
            start: WorldDate {
                year: 1444,
                month: 1,
                day: 1,
            },
            month_lengths: [31; 12],
        })
        .unwrap();

    let rules = ScheduledDailyEventRules::new(
        "scheduled-daily-events:test:v1",
        vec![ScheduledDailyEventCountry {
            country,
            country_index: 0,
            groups: vec![ScheduledDailyEventGroup {
                interval: 2,
                group: DailyEventGroup::weighted(vec![DailyEventCandidate {
                    event: None,
                    weight: 5,
                }]),
            }],
        }],
    )
    .unwrap();
    world.select_daily_system(&rules).unwrap();

    world.advance_with_daily_system(1, &rules).unwrap();
    assert_eq!(world.eu4_random_state().unwrap().1, 0);

    let save = world.save("scheduled-daily-test").unwrap();
    let mut resumed = World::restore(&save, "scheduled-daily-test", world.schema()).unwrap();
    assert_eq!(
        resumed.daily_system_id(),
        Some("scheduled-daily-events:test:v1")
    );
    resumed.advance_with_daily_system(2, &rules).unwrap();
    assert!(resumed.eu4_random_state().unwrap().1 > 0);
}

#[test]
fn scheduled_daily_rules_reject_zero_intervals_and_invalid_dates() {
    let (mut world, country) = world();
    let zero_interval = ScheduledDailyEventRules::new(
        "scheduled-daily-events:zero:v1",
        vec![ScheduledDailyEventCountry {
            country,
            country_index: 0,
            groups: vec![ScheduledDailyEventGroup {
                interval: 0,
                group: DailyEventGroup::all([] as [&str; 0]),
            }],
        }],
    )
    .unwrap();
    assert!(world.select_daily_system(&zero_interval).is_err());
    assert_eq!(world.daily_system_id(), None);

    let schedule = DailyEventSchedule {
        country_index: 0,
        interval: 1,
    };
    assert!(
        schedule
            .is_due_on(
                WorldDate {
                    year: 1444,
                    month: 1,
                    day: 0,
                },
                world.calendar().unwrap(),
            )
            .is_err()
    );
}
