// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    Action, Calendar, EntityId, EntityKind, EntityRef, NavalConstructionId, NavalConstructionInput,
    Schema, ScopeContext, ScopeSelector, World, WorldDate,
};

fn fixture() -> (World, EntityId, EntityId) {
    let mut world = World::new(Schema::new(std::iter::empty::<String>()).unwrap());
    let country = world.add_entity("SWE", EntityKind::Country);
    let port = world.add_entity("1", EntityKind::Province);
    world.set_province_has_port(port, true).unwrap();
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
    (world, country, port)
}

fn input(
    owner: EntityId,
    location: EntityId,
    unit_type: &str,
    completion_day: u64,
) -> NavalConstructionInput {
    NavalConstructionInput {
        owner,
        location,
        unit_type: unit_type.into(),
        completion_day,
    }
}

#[test]
fn construction_uses_explicit_day_and_round_trips_without_duplicate_completion() {
    let (mut continuous, owner, port) = fixture();
    assert_eq!(
        continuous
            .enqueue_naval_constructions(&[input(owner, port, "construction_hull", 3)])
            .unwrap(),
        [NavalConstructionId(0)]
    );

    continuous.advance_to(2).unwrap();
    assert!(continuous.naval_units().is_empty());
    assert_eq!(continuous.naval_constructions().len(), 1);
    let saved = continuous.save("naval-construction").unwrap();
    let mut resumed = World::restore(&saved, "naval-construction", continuous.schema()).unwrap();

    continuous.advance_to(3).unwrap();
    resumed.advance_to(3).unwrap();
    assert_eq!(resumed, continuous);
    assert!(continuous.naval_constructions().is_empty());
    assert_eq!(continuous.naval_units().len(), 1);
    assert_eq!(continuous.naval_units()[0].home_province, Some(port));
    assert_eq!(continuous.naval_units()[0].name, None);
    assert_eq!(continuous.naval_units()[0].morale_milli, None);
    assert_eq!(continuous.fleets().len(), 1);
    assert_eq!(continuous.fleets()[0].ships.len(), 1);

    continuous.advance_to(6).unwrap();
    assert_eq!(continuous.naval_units().len(), 1);
    let completed = continuous.save("naval-construction").unwrap();
    let saved_value: serde_json::Value = serde_json::from_str(&completed).unwrap();
    assert_eq!(saved_value["naval_constructions"]["next_id"], 1);
    assert_eq!(
        saved_value["naval_constructions"]["orders"]
            .as_array()
            .unwrap()
            .len(),
        0
    );

    let mut after_restore =
        World::restore(&completed, "naval-construction", continuous.schema()).unwrap();
    assert_eq!(
        after_restore
            .enqueue_naval_constructions(&[input(owner, port, "next_hull", 7)])
            .unwrap(),
        [NavalConstructionId(1)]
    );
}

#[test]
fn due_construction_runs_before_same_day_scheduled_tasks() {
    let (mut world, owner, port) = fixture();
    world
        .enqueue_naval_constructions(&[input(owner, port, "construction_hull", 2)])
        .unwrap();
    world
        .schedule_in_context(
            ScopeContext {
                current: port,
                root: owner,
            },
            2,
            vec![Action::CreateNavalUnit {
                owner: EntityRef::Selected(ScopeSelector::Root),
                unit_type: "task_hull".into(),
            }],
        )
        .unwrap();

    world.advance_to(2).unwrap();

    assert_eq!(world.naval_units().len(), 2);
    assert_eq!(world.naval_units()[0].unit_type, "construction_hull");
    assert_eq!(world.naval_units()[1].unit_type, "task_hull");
}

#[test]
fn invalid_batch_input_and_id_overflow_do_not_mutate_orders() {
    let (mut world, owner, port) = fixture();
    let before = world.save("naval-construction").unwrap();
    assert!(
        world
            .enqueue_naval_constructions(&[
                input(owner, port, "valid_hull", 3),
                input(port, port, "invalid_owner", 4),
            ])
            .is_err()
    );
    assert_eq!(world.save("naval-construction").unwrap(), before);
    assert!(
        world
            .enqueue_naval_constructions(&[input(owner, port, "too_late", u64::MAX)])
            .is_err()
    );
    assert_eq!(world.save("naval-construction").unwrap(), before);
    assert_eq!(
        world
            .enqueue_naval_constructions(&[input(owner, port, "valid_hull", 3)])
            .unwrap(),
        [NavalConstructionId(0)]
    );

    let mut exhausted: serde_json::Value = before.parse().unwrap();
    exhausted["naval_constructions"] = serde_json::json!({
        "next_id": u64::MAX,
        "orders": []
    });
    let mut exhausted =
        World::restore(&exhausted.to_string(), "naval-construction", world.schema()).unwrap();
    let exhausted_before = exhausted.save("naval-construction").unwrap();
    assert!(
        exhausted
            .enqueue_naval_constructions(&[input(owner, port, "cannot_allocate", 3)])
            .is_err()
    );
    assert_eq!(
        exhausted.save("naval-construction").unwrap(),
        exhausted_before
    );
}

#[test]
fn invalid_due_order_leaves_the_whole_completion_batch_unchanged() {
    let (mut world, owner, first_port) = fixture();
    let second_port = world.add_entity("2", EntityKind::Province);
    world.set_province_has_port(second_port, true).unwrap();
    world
        .enqueue_naval_constructions(&[
            input(owner, first_port, "first_hull", 2),
            input(owner, second_port, "second_hull", 2),
        ])
        .unwrap();
    world.set_province_has_port(second_port, false).unwrap();

    assert!(world.advance_to(2).is_err());
    assert!(world.naval_units().is_empty());
    assert!(world.fleets().is_empty());
    assert_eq!(world.naval_constructions().len(), 2);
}

#[test]
fn completion_identifier_overflow_does_not_partially_create_ships() {
    let (mut world, owner, port) = fixture();
    world
        .enqueue_naval_constructions(&[
            input(owner, port, "first_hull", 2),
            input(owner, port, "second_hull", 2),
        ])
        .unwrap();
    let mut saved: serde_json::Value = world.save("naval-construction").unwrap().parse().unwrap();
    saved["naval_units"] = serde_json::json!({
        "next_id": u64::MAX - 1,
        "units": []
    });
    let mut world =
        World::restore(&saved.to_string(), "naval-construction", world.schema()).unwrap();

    assert!(world.advance_to(2).is_err());
    assert!(world.naval_units().is_empty());
    assert!(world.fleets().is_empty());
    assert_eq!(world.naval_constructions().len(), 2);
}

#[test]
fn old_empty_snapshot_shape_and_restore_validation_are_preserved() {
    let (empty, _, _) = fixture();
    let empty_saved = empty.save("naval-construction").unwrap();
    assert!(!empty_saved.contains("naval_constructions"));
    let mut legacy: serde_json::Value = serde_json::from_str(&empty_saved).unwrap();
    legacy
        .as_object_mut()
        .unwrap()
        .remove("naval_constructions");
    assert_eq!(
        World::restore(&legacy.to_string(), "naval-construction", empty.schema()).unwrap(),
        empty
    );

    let (mut world, owner, port) = fixture();
    world
        .enqueue_naval_constructions(&[input(owner, port, "valid_hull", 3)])
        .unwrap();
    let saved: serde_json::Value =
        serde_json::from_str(&world.save("naval-construction").unwrap()).unwrap();

    let mut corrupt = saved.clone();
    corrupt["naval_constructions"]["orders"][0]["owner"] = serde_json::json!(port.0);
    assert!(World::restore(&corrupt.to_string(), "naval-construction", world.schema()).is_err());

    let mut corrupt = saved.clone();
    corrupt["naval_constructions"]["orders"][0]["location"] = serde_json::json!(owner.0);
    assert!(World::restore(&corrupt.to_string(), "naval-construction", world.schema()).is_err());

    let mut corrupt = saved.clone();
    corrupt["entities"][port.0]["province_has_port"] = serde_json::json!(false);
    assert!(World::restore(&corrupt.to_string(), "naval-construction", world.schema()).is_err());

    let mut corrupt = saved.clone();
    corrupt["naval_constructions"]["orders"][0]["unit_type"] = serde_json::json!("invalid type");
    assert!(World::restore(&corrupt.to_string(), "naval-construction", world.schema()).is_err());

    let mut corrupt = saved;
    corrupt["naval_constructions"]["orders"][0]["completion_day"] = serde_json::json!(0);
    assert!(World::restore(&corrupt.to_string(), "naval-construction", world.schema()).is_err());
}
