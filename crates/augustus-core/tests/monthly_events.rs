// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    Action, Calendar, DailyEventCountry, DailyEventGroup, EntityId, EntityKind, EventCatalog,
    EventDefinition, EventOption, MonthlyEventRules, Schema, World, WorldDate,
};

fn world() -> (World, EntityId) {
    let mut world = World::new(Schema::new([] as [&str; 0]).unwrap());
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
fn monthly_event_rules_dispatch_translated_candidates_and_restore() {
    let (mut world, country) = world();
    let catalog = EventCatalog::new([EventDefinition {
        id: "monthly".into(),
        scope: EntityKind::Country,
        immediate: vec![Action::Emit("monthly".into())],
        options: vec![EventOption {
            id: "accept".into(),
            effects: Vec::new(),
        }],
        after: Vec::new(),
    }])
    .unwrap();
    world.set_event_catalog(catalog.clone()).unwrap();

    let rules = MonthlyEventRules::new(
        "monthly-events:test:v1",
        vec![DailyEventCountry {
            country,
            groups: vec![DailyEventGroup::all(["monthly"])],
        }],
    )
    .unwrap();
    world.select_monthly_system(&rules).unwrap();
    world.advance_with_monthly_system(21, &rules).unwrap();

    assert_eq!(world.pending_events().len(), 1);
    assert_eq!(world.pending_events()[0].event, "monthly");
    assert_eq!(
        world.events().last().map(|event| event.2.as_str()),
        Some("monthly")
    );

    let save = world.save("monthly-events").unwrap();
    let restored =
        World::restore_with_events(&save, "monthly-events", world.schema(), Vec::new(), catalog)
            .unwrap();
    assert_eq!(restored.monthly_system_id(), Some("monthly-events:test:v1"));
}

#[test]
fn monthly_event_rules_reject_empty_ids() {
    assert!(MonthlyEventRules::new(" ", Vec::new()).is_err());
}
