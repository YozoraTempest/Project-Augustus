// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    Action, EntityId, EntityKind, EntityRef, Schema, ScopeSelector, Target, World, WorldDate,
};

struct Scenario {
    world: World,
    country: EntityId,
    capital: EntityId,
    owned_province: EntityId,
    rival_province: EntityId,
}

fn scenario() -> Scenario {
    let mut world = World::new(Schema::new([] as [&str; 0]).unwrap());
    let country = world.add_entity("AAA", EntityKind::Country);
    let rival = world.add_entity("BBB", EntityKind::Country);
    let capital = world.add_entity("1", EntityKind::Province);
    let owned_province = world.add_entity("2", EntityKind::Province);
    let rival_province = world.add_entity("3", EntityKind::Province);
    world.set_owner(capital, country).unwrap();
    world.set_owner(owned_province, country).unwrap();
    world.set_owner(rival_province, rival).unwrap();
    world.set_capital(country, capital).unwrap();
    Scenario {
        world,
        country,
        capital,
        owned_province,
        rival_province,
    }
}

fn named(kind: EntityKind, name: &str) -> EntityRef {
    EntityRef::Selected(ScopeSelector::Named {
        kind,
        name: name.into(),
    })
}

fn set_capital(target: Target, province: EntityRef) -> Action {
    Action::SetCapital { target, province }
}

fn assert_current_capital(world: &World, country: EntityId, expected: EntityId) {
    assert_eq!(world.entity(country).unwrap().capital, Some(expected));
    assert_eq!(world.target(country, Target::Capital).unwrap(), expected);
}

#[test]
fn set_capital_resolves_live_scopes_and_preserves_independent_history() {
    let mut scenario = scenario();
    let original_capital = scenario.capital;
    let dated_history = vec![
        (
            WorldDate {
                year: 1444,
                month: 11,
                day: 11,
            },
            Some(original_capital),
        ),
        (
            WorldDate {
                year: 1500,
                month: 1,
                day: 1,
            },
            None,
        ),
    ];
    scenario
        .world
        .set_country_original_capital(scenario.country, Some(original_capital))
        .unwrap();
    scenario
        .world
        .set_country_capital_history(scenario.country, dated_history.clone())
        .unwrap();

    scenario
        .world
        .execute(
            scenario.country,
            &[set_capital(
                Target::Current,
                named(EntityKind::Province, "2"),
            )],
        )
        .unwrap();
    assert_current_capital(&scenario.world, scenario.country, scenario.owned_province);
    assert_eq!(
        scenario
            .world
            .country_original_capital_state(scenario.country)
            .unwrap(),
        Some(Some(original_capital))
    );
    assert_eq!(
        scenario
            .world
            .country_capital_history(scenario.country)
            .unwrap(),
        Some(&dated_history)
    );

    let after_move = scenario.world.clone();
    scenario
        .world
        .execute(
            scenario.country,
            &[set_capital(
                Target::Current,
                named(EntityKind::Province, "2"),
            )],
        )
        .unwrap();
    assert_eq!(scenario.world, after_move);

    let save = scenario.world.save("capital-lifecycle").unwrap();
    let mut resumed = World::restore(&save, "capital-lifecycle", scenario.world.schema()).unwrap();
    assert_eq!(resumed, scenario.world);

    resumed
        .execute(
            scenario.capital,
            &[Action::With {
                target: Target::Owner,
                plan: vec![set_capital(Target::Current, EntityRef::Previous)],
            }],
        )
        .unwrap();
    assert_current_capital(&resumed, scenario.country, scenario.capital);
    assert_eq!(
        resumed
            .country_original_capital_state(scenario.country)
            .unwrap(),
        Some(Some(original_capital))
    );
    assert_eq!(
        resumed.country_capital_history(scenario.country).unwrap(),
        Some(&dated_history)
    );
}

#[test]
fn scheduled_capital_change_survives_save_restore() {
    let mut scenario = scenario();
    scenario
        .world
        .execute(
            scenario.country,
            &[Action::Schedule {
                delay: 5,
                plan: vec![set_capital(
                    Target::Current,
                    named(EntityKind::Province, "2"),
                )],
            }],
        )
        .unwrap();

    let save = scenario.world.save("capital-lifecycle").unwrap();
    let mut resumed = World::restore(&save, "capital-lifecycle", scenario.world.schema()).unwrap();
    assert_current_capital(&resumed, scenario.country, scenario.capital);
    resumed.advance_to(5).unwrap();
    assert_current_capital(&resumed, scenario.country, scenario.owned_province);
}

fn assert_failed_change_is_terminal(
    mut world: World,
    execution_scope: EntityId,
    country: EntityId,
    capital: EntityId,
    target: Target,
    invalid_province: EntityRef,
) {
    assert!(
        world
            .execute(execution_scope, &[set_capital(target, invalid_province)])
            .is_err()
    );
    assert_current_capital(&world, country, capital);
    assert!(world.save("capital-lifecycle").is_err());
    assert!(
        world
            .execute(
                country,
                &[set_capital(
                    Target::Current,
                    named(EntityKind::Province, "2"),
                )],
            )
            .is_err()
    );
    assert_current_capital(&world, country, capital);
}

#[test]
fn invalid_capital_changes_fail_without_changing_capital_and_stop_execution() {
    let scenario = scenario();
    assert_failed_change_is_terminal(
        scenario.world.clone(),
        scenario.country,
        scenario.country,
        scenario.capital,
        Target::Current,
        named(EntityKind::Province, "3"),
    );

    let mut unowned = scenario.world.clone();
    unowned.clear_owner(scenario.rival_province).unwrap();
    assert_failed_change_is_terminal(
        unowned,
        scenario.country,
        scenario.country,
        scenario.capital,
        Target::Current,
        named(EntityKind::Province, "3"),
    );

    assert_failed_change_is_terminal(
        scenario.world.clone(),
        scenario.country,
        scenario.country,
        scenario.capital,
        Target::Current,
        named(EntityKind::Country, "BBB"),
    );

    assert_failed_change_is_terminal(
        scenario.world.clone(),
        scenario.country,
        scenario.country,
        scenario.capital,
        Target::Current,
        named(EntityKind::Province, "missing"),
    );

    assert_failed_change_is_terminal(
        scenario.world.clone(),
        scenario.capital,
        scenario.country,
        scenario.capital,
        Target::Current,
        named(EntityKind::Province, "2"),
    );
}
