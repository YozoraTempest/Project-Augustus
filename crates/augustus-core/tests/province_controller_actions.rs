// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    Action, EntityId, EntityKind, EntityRef, Schema, ScopeSelector, Target, World,
};

struct Scenario {
    world: World,
    aaa: EntityId,
    bbb: EntityId,
    province_aaa: EntityId,
    province_bbb: EntityId,
}

fn scenario() -> Scenario {
    let mut world = World::new(Schema::new([] as [&str; 0]).unwrap());
    let aaa = world.add_entity("AAA", EntityKind::Country);
    let bbb = world.add_entity("BBB", EntityKind::Country);
    let province_aaa = world.add_entity("1", EntityKind::Province);
    let province_bbb = world.add_entity("2", EntityKind::Province);
    world.set_owner(province_aaa, aaa).unwrap();
    world.set_owner(province_bbb, bbb).unwrap();
    world.set_capital(aaa, province_aaa).unwrap();
    world.set_capital(bbb, province_bbb).unwrap();
    world
        .set_province_controller(province_aaa, Some(aaa))
        .unwrap();
    world
        .set_province_controller(province_bbb, Some(bbb))
        .unwrap();
    world
        .set_province_previous_controller(province_aaa, Some(aaa))
        .unwrap();
    world
        .set_province_previous_controller(province_bbb, Some(bbb))
        .unwrap();
    Scenario {
        world,
        aaa,
        bbb,
        province_aaa,
        province_bbb,
    }
}

fn named(kind: EntityKind, name: &str) -> EntityRef {
    EntityRef::Selected(ScopeSelector::Named {
        kind,
        name: name.into(),
    })
}

fn set_controller(target: Target, controller: Option<EntityRef>) -> Action {
    Action::SetProvinceController { target, controller }
}

fn bbb_capital_controller_action(controller: EntityRef) -> Action {
    Action::InScope {
        selector: ScopeSelector::Named {
            kind: EntityKind::Country,
            name: "BBB".into(),
        },
        kind: EntityKind::Country,
        plan: vec![Action::With {
            target: Target::Capital,
            plan: vec![set_controller(Target::Current, Some(controller))],
        }],
    }
}

#[test]
fn controller_action_resolves_named_root_and_previous_without_changing_owner_or_history() {
    let mut scenario = scenario();

    scenario
        .world
        .execute(
            scenario.province_aaa,
            &[set_controller(
                Target::Current,
                Some(named(EntityKind::Country, "BBB")),
            )],
        )
        .unwrap();
    assert_eq!(
        scenario
            .world
            .province_controller_state(scenario.province_aaa)
            .unwrap(),
        Some(Some(scenario.bbb))
    );
    assert_eq!(
        scenario.world.entity(scenario.province_aaa).unwrap().owner,
        Some(scenario.aaa)
    );
    assert_eq!(
        scenario
            .world
            .province_previous_controller_state(scenario.province_aaa)
            .unwrap(),
        Some(Some(scenario.aaa))
    );

    scenario
        .world
        .execute(
            scenario.aaa,
            &[bbb_capital_controller_action(EntityRef::Selected(
                ScopeSelector::Root,
            ))],
        )
        .unwrap();
    assert_eq!(
        scenario
            .world
            .province_controller_state(scenario.province_bbb)
            .unwrap(),
        Some(Some(scenario.aaa))
    );
    assert_eq!(
        scenario.world.entity(scenario.province_bbb).unwrap().owner,
        Some(scenario.bbb)
    );
    assert_eq!(
        scenario
            .world
            .province_previous_controller_state(scenario.province_bbb)
            .unwrap(),
        Some(Some(scenario.bbb))
    );

    scenario
        .world
        .execute(
            scenario.aaa,
            &[bbb_capital_controller_action(EntityRef::Previous)],
        )
        .unwrap();
    assert_eq!(
        scenario
            .world
            .province_controller_state(scenario.province_bbb)
            .unwrap(),
        Some(Some(scenario.bbb))
    );
    assert_eq!(
        scenario.world.entity(scenario.province_bbb).unwrap().owner,
        Some(scenario.bbb)
    );
    assert_eq!(
        scenario
            .world
            .province_previous_controller_state(scenario.province_bbb)
            .unwrap(),
        Some(Some(scenario.bbb))
    );
}

#[test]
fn none_sets_known_controller_absence_and_round_trips_without_changing_history() {
    let mut scenario = scenario();

    scenario
        .world
        .execute(
            scenario.province_aaa,
            &[set_controller(Target::Current, None)],
        )
        .unwrap();
    assert_eq!(
        scenario
            .world
            .province_controller_state(scenario.province_aaa)
            .unwrap(),
        Some(None)
    );
    assert_eq!(
        scenario.world.entity(scenario.province_aaa).unwrap().owner,
        Some(scenario.aaa)
    );
    assert_eq!(
        scenario
            .world
            .province_previous_controller_state(scenario.province_aaa)
            .unwrap(),
        Some(Some(scenario.aaa))
    );

    let snapshot = scenario.world.save("province-controller-actions").unwrap();
    let restored = World::restore(
        &snapshot,
        "province-controller-actions",
        scenario.world.schema(),
    )
    .unwrap();
    assert_eq!(
        restored
            .province_controller_state(scenario.province_aaa)
            .unwrap(),
        Some(None)
    );
    assert_eq!(
        restored
            .province_previous_controller_state(scenario.province_aaa)
            .unwrap(),
        Some(Some(scenario.aaa))
    );
    assert_eq!(
        restored.entity(scenario.province_aaa).unwrap().owner,
        Some(scenario.aaa)
    );
}

#[test]
fn scheduled_controller_action_survives_save_restore_and_updates_only_current_state() {
    let mut scenario = scenario();
    scenario
        .world
        .schedule(
            scenario.province_aaa,
            5,
            vec![set_controller(
                Target::Current,
                Some(named(EntityKind::Country, "BBB")),
            )],
        )
        .unwrap();

    let snapshot = scenario.world.save("province-controller-actions").unwrap();
    let mut resumed = World::restore(
        &snapshot,
        "province-controller-actions",
        scenario.world.schema(),
    )
    .unwrap();
    assert_eq!(
        resumed
            .province_controller_state(scenario.province_aaa)
            .unwrap(),
        Some(Some(scenario.aaa))
    );

    scenario.world.advance_to(5).unwrap();
    resumed.advance_to(5).unwrap();
    assert_eq!(scenario.world, resumed);
    assert_eq!(
        resumed
            .province_controller_state(scenario.province_aaa)
            .unwrap(),
        Some(Some(scenario.bbb))
    );
    assert_eq!(
        resumed.entity(scenario.province_aaa).unwrap().owner,
        Some(scenario.aaa)
    );
    assert_eq!(
        resumed
            .province_previous_controller_state(scenario.province_aaa)
            .unwrap(),
        Some(Some(scenario.aaa))
    );
    assert_eq!(
        scenario.world.save("province-controller-actions").unwrap(),
        resumed.save("province-controller-actions").unwrap()
    );
}

fn assert_failed_action_is_terminal_without_controller_write(
    mut world: World,
    execution_scope: EntityId,
    province: EntityId,
    owner: EntityId,
    previous_controller: EntityId,
    action: Action,
) {
    assert!(world.execute(execution_scope, &[action]).is_err());
    assert_eq!(
        world.province_controller_state(province).unwrap(),
        Some(Some(owner))
    );
    assert_eq!(world.entity(province).unwrap().owner, Some(owner));
    assert_eq!(
        world.province_previous_controller_state(province).unwrap(),
        Some(Some(previous_controller))
    );
    assert!(world.save("province-controller-actions").is_err());
    assert!(world.execute(execution_scope, &[]).is_err());
}

#[test]
fn invalid_target_controller_type_and_absent_reference_fail_before_writing() {
    let scenario = scenario();
    assert_failed_action_is_terminal_without_controller_write(
        scenario.world.clone(),
        scenario.aaa,
        scenario.province_aaa,
        scenario.aaa,
        scenario.aaa,
        set_controller(Target::Current, Some(named(EntityKind::Country, "BBB"))),
    );
    assert_failed_action_is_terminal_without_controller_write(
        scenario.world.clone(),
        scenario.province_aaa,
        scenario.province_aaa,
        scenario.aaa,
        scenario.aaa,
        set_controller(Target::Current, Some(named(EntityKind::Province, "2"))),
    );
    assert_failed_action_is_terminal_without_controller_write(
        scenario.world,
        scenario.province_aaa,
        scenario.province_aaa,
        scenario.aaa,
        scenario.aaa,
        set_controller(
            Target::Current,
            Some(EntityRef::Selected(ScopeSelector::Global(
                "missing_controller".into(),
            ))),
        ),
    );
}

#[test]
fn schedule_rejects_an_empty_global_controller_reference_without_changing_the_snapshot() {
    let mut scenario = scenario();
    let before = scenario.world.save("province-controller-actions").unwrap();

    assert!(
        scenario
            .world
            .schedule(
                scenario.province_aaa,
                1,
                vec![set_controller(
                    Target::Current,
                    Some(EntityRef::Selected(ScopeSelector::Global(String::new()))),
                )],
            )
            .is_err()
    );
    assert_eq!(
        scenario.world.save("province-controller-actions").unwrap(),
        before
    );
}
