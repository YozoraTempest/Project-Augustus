// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    Action, Condition, EntityId, EntityKind, EventCatalog, EventChoice, EventDefinition,
    EventOption, FieldId, PendingEventId, Plan, RuleId, Schema, Target, World,
};

fn event(id: &str, scope: EntityKind, immediate: Plan) -> EventDefinition {
    EventDefinition {
        id: id.into(),
        scope,
        immediate,
        options: vec![EventOption {
            id: "accept".into(),
            effects: Vec::new(),
        }],
        after: Vec::new(),
    }
}

fn call(id: &str) -> Action {
    Action::TriggerEvent { event: id.into() }
}

fn world(definitions: Vec<EventDefinition>) -> World {
    let mut world = World::new(Schema::new(["value"]).unwrap());
    let country = world.add_entity("Union", EntityKind::Country);
    let province = world.add_entity("Port", EntityKind::Province);
    world.set_owner(province, country).unwrap();
    world.set_capital(country, province).unwrap();
    world
        .set_event_catalog(EventCatalog::new(definitions).unwrap())
        .unwrap();
    world
}

#[test]
fn nested_immediate_calls_finish_before_caller_and_allocate_distinct_records() {
    let child = event(
        "child",
        EntityKind::Country,
        vec![Action::Emit("child".into())],
    );
    let mut parent = event(
        "parent",
        EntityKind::Country,
        vec![
            Action::Emit("before".into()),
            call("child"),
            Action::Emit("continued".into()),
        ],
    );
    parent.options[0].effects = vec![call("child"), Action::Emit("option".into())];
    parent.after = vec![call("child"), Action::Emit("after".into())];
    let mut world = world(vec![parent, child]);
    let parent_id = world.trigger_event("parent", EntityId(0)).unwrap();
    assert_eq!(parent_id, PendingEventId(1));
    assert_eq!(
        world
            .pending_events()
            .iter()
            .map(|pending| (pending.id.0, pending.event.as_str()))
            .collect::<Vec<_>>(),
        [(0, "child"), (1, "parent")]
    );
    world
        .choose_event(&EventChoice {
            pending: parent_id,
            option: "accept".into(),
        })
        .unwrap();
    assert_eq!(
        world
            .pending_events()
            .iter()
            .map(|pending| pending.id.0)
            .collect::<Vec<_>>(),
        [0, 2, 3]
    );
    assert_eq!(
        world
            .events()
            .iter()
            .map(|entry| entry.2.as_str())
            .collect::<Vec<_>>(),
        [
            "before",
            "child",
            "continued",
            "child",
            "option",
            "child",
            "after"
        ]
    );
    let restored = World::restore_with_events(
        &world.save("nested:v1").unwrap(),
        "nested:v1",
        world.schema(),
        Vec::new(),
        world.event_catalog().clone(),
    )
    .unwrap();
    assert_eq!(world, restored);
}

#[test]
fn delayed_calls_keep_current_identity_and_resume_with_the_existing_task_queue() {
    let province_event = event(
        "port",
        EntityKind::Province,
        vec![Action::With {
            target: Target::Owner,
            plan: vec![Action::Add {
                target: Target::Current,
                field: FieldId(0),
                value: 5,
            }],
        }],
    );
    let mut continuous = world(vec![province_event]);
    continuous
        .execute(
            EntityId(0),
            &[Action::With {
                target: Target::Capital,
                plan: vec![Action::Schedule {
                    delay: 3,
                    plan: vec![call("port")],
                }],
            }],
        )
        .unwrap();
    let buyer = continuous.add_entity("Buyer", EntityKind::Country);
    continuous.set_owner(EntityId(1), buyer).unwrap();
    // Keep the saved world consistent after transferring its former capital.
    let replacement = continuous.add_entity("Replacement", EntityKind::Province);
    continuous.set_owner(replacement, EntityId(0)).unwrap();
    continuous.set_capital(EntityId(0), replacement).unwrap();
    continuous.advance_to(2).unwrap();
    assert!(continuous.pending_events().is_empty());
    assert_eq!(continuous.pending_tasks(), 1);
    let mut resumed = World::restore_with_events(
        &continuous.save("delay:v1").unwrap(),
        "delay:v1",
        continuous.schema(),
        Vec::new(),
        continuous.event_catalog().clone(),
    )
    .unwrap();
    for world in [&mut continuous, &mut resumed] {
        world.advance_to(3).unwrap();
        assert_eq!(world.pending_events()[0].current, EntityId(1));
        assert_eq!(world.pending_events()[0].day, 3);
        assert_eq!(world.value(buyer, FieldId(0)).unwrap(), 5);
        assert_eq!(world.value(EntityId(0), FieldId(0)).unwrap(), 0);
        world
            .choose_event(&EventChoice {
                pending: PendingEventId(0),
                option: "accept".into(),
            })
            .unwrap();
        world.advance_to(5).unwrap();
    }
    assert_eq!(continuous, resumed);
    assert_eq!(continuous.pending_tasks(), 0);
}

#[test]
fn event_calls_share_the_callers_budget_instead_of_starting_new_execution() {
    let increment = Action::Add {
        target: Target::Current,
        field: FieldId(0),
        value: 1,
    };
    let child = event("child", EntityKind::Country, vec![increment; 60_000]);
    let parent = event(
        "parent",
        EntityKind::Country,
        vec![call("child"), call("child")],
    );
    let mut world = world(vec![parent, child]);
    let error = world.trigger_event("parent", EntityId(0)).unwrap_err();
    assert!(error.to_string().contains("instruction limit"));
    assert_eq!(world.value(EntityId(0), FieldId(0)).unwrap(), 99_998);
    assert_eq!(world.pending_events().len(), 1);
    assert_eq!(world.pending_events()[0].event, "child");
    assert!(world.save("budget:v1").is_err());
}

#[test]
fn cyclic_events_and_rule_calls_share_a_bounded_stack() {
    let first = event("first", EntityKind::Country, vec![call("second")]);
    let second = event("second", EntityKind::Country, vec![call("first")]);
    let mut cycle = world(vec![first, second]);
    assert!(
        cycle
            .trigger_event("first", EntityId(0))
            .unwrap_err()
            .to_string()
            .contains("call depth")
    );
    assert!(cycle.pending_events().is_empty());
    assert!(cycle.advance_to(1).is_err());

    let mut world = World::with_rules(
        Schema::new(["value"]).unwrap(),
        vec![vec![call("recursive")]],
    );
    let country = world.add_entity("Union", EntityKind::Country);
    world
        .set_event_catalog(
            EventCatalog::new([event(
                "recursive",
                EntityKind::Country,
                vec![Action::Run(RuleId(0))],
            )])
            .unwrap(),
        )
        .unwrap();
    assert!(
        world
            .execute(country, &[call("recursive")])
            .unwrap_err()
            .to_string()
            .contains("call depth")
    );
    assert!(world.save("cycle:v1").is_err());
}

#[test]
fn missing_references_in_unselected_branches_are_rejected_when_binding_content() {
    let mut world = world(vec![event("known", EntityKind::Country, Vec::new())]);
    let invalid = event(
        "invalid",
        EntityKind::Country,
        vec![Action::If {
            condition: Condition::Always(false),
            then: vec![call("missing")],
            otherwise: Vec::new(),
        }],
    );
    let before = world.clone();
    assert!(
        world
            .set_event_catalog(EventCatalog::new([invalid]).unwrap())
            .is_err()
    );
    assert_eq!(world, before);
    assert!(
        world
            .schedule(EntityId(0), 1, vec![call("missing")])
            .is_err()
    );
    assert_eq!(world, before);
}

#[test]
fn event_content_replacement_and_restore_validate_rule_and_task_references() {
    let known = event("known", EntityKind::Country, Vec::new());
    let mut world = World::with_rules(Schema::new(["value"]).unwrap(), vec![vec![call("known")]]);
    let country = world.add_entity("Union", EntityKind::Country);
    world
        .set_event_catalog(EventCatalog::new([known]).unwrap())
        .unwrap();
    world.schedule(country, 1, vec![call("known")]).unwrap();
    let before = world.clone();
    assert!(world.set_event_catalog(EventCatalog::default()).is_err());
    assert_eq!(world, before);
    let save = world.save("reference:v1").unwrap();
    assert!(
        World::restore_with_rules(
            &save,
            "reference:v1",
            world.schema(),
            vec![vec![call("known")]]
        )
        .is_err()
    );
    let mut resumed = World::restore_with_events(
        &save,
        "reference:v1",
        world.schema(),
        vec![vec![call("known")]],
        world.event_catalog().clone(),
    )
    .unwrap();
    resumed.advance_to(1).unwrap();
    assert_eq!(resumed.pending_events().len(), 1);
}

#[test]
fn runtime_scope_errors_keep_the_executed_prefix_and_stop_the_session() {
    let mut world = world(vec![event("port", EntityKind::Province, Vec::new())]);
    assert!(
        world
            .execute(
                EntityId(0),
                &[
                    Action::Emit("prefix".into()),
                    call("port"),
                    Action::Emit("incorrect".into())
                ]
            )
            .is_err()
    );
    assert_eq!(world.events().len(), 1);
    assert!(world.pending_events().is_empty());
    assert!(world.save("scope:v1").is_err());
}
