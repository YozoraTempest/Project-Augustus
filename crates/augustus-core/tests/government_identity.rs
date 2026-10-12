// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::*;

fn world() -> World {
    let mut world = World::new(Schema::new(["unused"]).unwrap());
    for name in ["AAA", "BBB", "CCC", "DDD"] {
        world.add_entity(name, EntityKind::Country);
    }
    let port = world.add_entity("Port", EntityKind::Province);
    world.set_owner(port, EntityId(0)).unwrap();
    world.set_capital(EntityId(0), port).unwrap();
    world.set_governments(serde_json::from_str(r#"{
        "reforms": {
            "council": {"features":{"assembly":true,"disputed":true},"modifier":{"values_micros":{},"flags":{}},"unresolved_properties":[]},
            "court": {"features":{"assembly":false,"disputed":false},"modifier":{"values_micros":{},"flags":{}},"unresolved_properties":[]},
            "uncertain": {"features":{"unsure":null},"modifier":{"values_micros":{},"flags":{}},"unresolved_properties":["opaque"]}
        },
        "metrics": {},
        "countries": {
            "0":{"kind":"civic","reforms":["council"],"metrics":null},
            "1":{"kind":"royal","reforms":["court"],"metrics":{}},
            "2":{"kind":null,"reforms":null,"metrics":null}
        }
    }"#).unwrap()).unwrap();
    world
}

fn queries(target: Target) -> Vec<Condition> {
    vec![
        Condition::GovernmentKind {
            target,
            kind: "civic".into(),
        },
        Condition::GovernmentReform {
            target,
            id: "council".into(),
        },
        Condition::GovernmentFeature {
            target,
            name: "assembly".into(),
        },
    ]
}

fn plan() -> Plan {
    queries(Target::Current)
        .into_iter()
        .enumerate()
        .map(|(i, condition)| Action::If {
            condition,
            then: vec![Action::Emit(format!("query{i}"))],
            otherwise: vec![],
        })
        .collect()
}

#[test]
fn queries_use_live_selection_and_owner_without_requiring_numeric_metrics() {
    let mut world = world();
    for condition in queries(Target::Current) {
        assert!(condition.evaluate(&world, EntityId(0)).unwrap());
        assert!(!condition.evaluate(&world, EntityId(1)).unwrap());
        assert!(condition.evaluate(&world, EntityId(4)).is_err());
    }
    for condition in queries(Target::Owner) {
        assert!(condition.evaluate(&world, EntityId(4)).unwrap());
    }
    let mut next = world.governments().clone();
    next.countries
        .insert(EntityId(0), next.countries[&EntityId(1)].clone());
    world.set_governments(next).unwrap();
    for condition in queries(Target::Current) {
        assert!(!condition.evaluate(&world, EntityId(0)).unwrap());
    }
}

#[test]
fn unknown_state_stays_unknown_even_under_negation_and_stops_an_event() {
    let mut world = world();
    for id in [EntityId(2), EntityId(3)] {
        for condition in queries(Target::Current) {
            assert!(
                Condition::Not(Box::new(condition))
                    .evaluate(&world, id)
                    .is_err()
            );
        }
    }
    assert!(
        world
            .government_reform_selected(EntityId(0), "absent")
            .is_err()
    );
    let mut actions = vec![Action::Emit("before".into())];
    actions.extend(plan());
    world
        .set_event_catalog(
            EventCatalog::new(vec![EventDefinition {
                id: "unknown".into(),
                scope: EntityKind::Country,
                immediate: actions,
                options: vec![EventOption {
                    id: "0".into(),
                    effects: vec![],
                }],
                after: vec![],
            }])
            .unwrap(),
        )
        .unwrap();
    assert!(world.trigger_event("unknown", EntityId(2)).is_err());
    assert!(world.pending_events().is_empty());
    assert!(world.save("identity").is_err());
}

#[test]
fn known_empty_selection_is_false_but_conflicting_and_unresolved_features_are_errors() {
    let mut world = world();
    let mut next = world.governments().clone();
    next.countries.get_mut(&EntityId(0)).unwrap().reforms = Some(Default::default());
    world.set_governments(next.clone()).unwrap();
    assert!(
        !world
            .government_reform_selected(EntityId(0), "council")
            .unwrap()
    );
    assert!(!world.government_feature(EntityId(0), "assembly").unwrap());
    next.countries.get_mut(&EntityId(0)).unwrap().reforms =
        Some(["council".into(), "court".into(), "uncertain".into()].into());
    world.set_governments(next).unwrap();
    for name in ["assembly", "disputed", "unsure", "opaque"] {
        assert!(
            world.government_feature(EntityId(0), name).is_err(),
            "{name}"
        );
    }
    assert!(!world.government_feature(EntityId(0), "absent").unwrap());
}

#[test]
fn saved_delayed_queries_read_replaced_state_and_preserve_required_definitions() {
    let mut world = world();
    world.schedule(EntityId(0), 1, plan()).unwrap();
    let saved = world.save("identity").unwrap();
    let mut resumed = World::restore(&saved, "identity", world.schema()).unwrap();
    for world in [&mut world, &mut resumed] {
        let mut next = world.governments().clone();
        next.countries
            .insert(EntityId(0), next.countries[&EntityId(1)].clone());
        world.set_governments(next).unwrap();
        let before = world.clone();
        let mut bad = world.governments().clone();
        bad.reforms.remove("council");
        assert!(world.set_governments(bad).is_err());
        assert_eq!(*world, before);
        world.advance_to(1).unwrap();
        assert!(world.events().is_empty());
    }
    assert_eq!(world, resumed);
    let mut tampered: serde_json::Value = serde_json::from_str(&saved).unwrap();
    tampered["governments"]["reforms"]
        .as_object_mut()
        .unwrap()
        .remove("council");
    tampered["governments"]["countries"]["0"]["reforms"] = serde_json::json!([]);
    assert!(World::restore(&tampered.to_string(), "identity", world.schema()).is_err());
}

#[test]
fn event_and_objective_dependencies_reject_removing_a_referenced_reform() {
    for objectives in [false, true] {
        let mut world = world();
        if objectives {
            world
                .set_completions(Completions {
                    identifiers: ["branch".into()].into(),
                    ..Default::default()
                })
                .unwrap();
            world
                .set_objectives(Objectives {
                    groups: vec![ObjectiveGroup {
                        id: "group".into(),
                        condition: queries(Target::Current)[1].clone(),
                        objectives: ["branch".into()].into(),
                        claims: Default::default(),
                        priority: 0,
                    }],
                    ..Default::default()
                })
                .unwrap();
        } else {
            world
                .set_event_catalog(
                    EventCatalog::new(vec![EventDefinition {
                        id: "identity".into(),
                        scope: EntityKind::Country,
                        immediate: plan(),
                        options: vec![EventOption {
                            id: "0".into(),
                            effects: vec![],
                        }],
                        after: vec![],
                    }])
                    .unwrap(),
                )
                .unwrap();
        }
        let before = world.clone();
        let mut next = world.governments().clone();
        next.reforms.remove("council");
        next.countries.get_mut(&EntityId(0)).unwrap().reforms = Some(Default::default());
        assert!(world.set_governments(next).is_err());
        assert_eq!(world, before);
    }
}
