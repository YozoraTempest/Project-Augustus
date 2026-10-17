// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    Calendar, EntityKind, EstateDefinition, EstateLoyaltyDecay, EstateRules, EstateState, Estates,
    World, WorldDate,
};
use std::collections::BTreeMap;

fn estates(aaa: augustus_core::EntityId, bbb: augustus_core::EntityId) -> Estates {
    Estates {
        definitions: [("estate_church".into(), EstateDefinition::default())].into(),
        rules: EstateRules {
            loyalty_decay: Some(EstateLoyaltyDecay {
                base_min_annual_milli: 2_000,
                base_max_annual_milli: 5_000,
            }),
            ..Default::default()
        },
        countries: [
            (
                aaa,
                [(
                    "estate_church".into(),
                    EstateState {
                        active: Some(true),
                        loyalty_milli: Some(49_571),
                        loyalty_target_milli: Some(40_300),
                        ..Default::default()
                    },
                )]
                .into(),
            ),
            (
                bbb,
                [(
                    "estate_church".into(),
                    EstateState {
                        active: Some(true),
                        loyalty_milli: Some(49_615),
                        loyalty_target_milli: Some(44_600),
                        ..Default::default()
                    },
                )]
                .into(),
            ),
        ]
        .into(),
        ..Default::default()
    }
}

#[test]
fn scheduled_loyalty_updates_follow_country_order_and_research_formula() {
    let mut world = World::new(augustus_core::Schema::new(["unused"]).unwrap());
    let aaa = world.add_entity("AAA", EntityKind::Country);
    let bbb = world.add_entity("BBB", EntityKind::Country);
    world.set_subject_country_order(vec![aaa, bbb]).unwrap();
    world.set_estates(estates(aaa, bbb)).unwrap();
    world
        .set_calendar(Calendar {
            start: WorldDate {
                year: 1445,
                month: 1,
                day: 1,
            },
            month_lengths: [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31],
        })
        .unwrap();

    world.advance_to(2).unwrap();
    assert_eq!(
        world.estates().countries[&bbb]["estate_church"].loyalty_milli,
        Some(49_424)
    );

    world.advance_to(31).unwrap();
    assert_eq!(
        world.estates().countries[&aaa]["estate_church"].loyalty_milli,
        Some(49_358)
    );
}

#[test]
fn unknown_targets_do_not_become_default_loyalty_updates() {
    let mut world = World::new(augustus_core::Schema::new(["unused"]).unwrap());
    let aaa = world.add_entity("AAA", EntityKind::Country);
    world.set_subject_country_order(vec![aaa]).unwrap();
    let mut state = EstateState {
        active: Some(true),
        loyalty_milli: Some(49_571),
        ..Default::default()
    };
    state.loyalty_target_milli = None;
    world
        .set_estates(Estates {
            definitions: [("estate_church".into(), EstateDefinition::default())].into(),
            rules: EstateRules {
                loyalty_decay: Some(EstateLoyaltyDecay {
                    base_min_annual_milli: 2_000,
                    base_max_annual_milli: 5_000,
                }),
                ..Default::default()
            },
            countries: [(aaa, BTreeMap::from([("estate_church".into(), state)]))].into(),
            ..Default::default()
        })
        .unwrap();
    world
        .set_calendar(Calendar {
            start: WorldDate {
                year: 1445,
                month: 1,
                day: 1,
            },
            month_lengths: [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31],
        })
        .unwrap();

    world.advance_to(1).unwrap();
    assert_eq!(
        world.estates().countries[&aaa]["estate_church"].loyalty_milli,
        Some(49_571)
    );
}
