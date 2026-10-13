// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::*;

fn calendar(month: u8, day: u8) -> Calendar {
    Calendar {
        start: WorldDate {
            year: 1444,
            month,
            day,
        },
        month_lengths: [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31],
    }
}
fn world() -> World {
    let mut world = World::new(Schema::new(["unused"]).unwrap());
    for name in ["AAA", "BBB", "CCC"] {
        world.add_entity(name, EntityKind::Country);
    }
    world.add_entity("Port", EntityKind::Province);
    world.set_calendar(calendar(12, 1)).unwrap();
    world
}
fn monthly(count: u32) -> GrantTerm {
    GrantTerm::Months {
        count,
        monthly: true,
    }
}
fn named(name: &str) -> EntityRef {
    EntityRef::Selected(ScopeSelector::Named {
        kind: EntityKind::Country,
        name: name.into(),
    })
}
fn condition() -> Condition {
    Condition::TruceWith {
        first: named("AAA"),
        second: named("BBB"),
    }
}
fn observe() -> Action {
    Action::If {
        condition: condition(),
        then: vec![Action::Emit("present".into())],
        otherwise: vec![Action::Emit("absent".into())],
    }
}

#[test]
fn symmetry_self_and_known_absence_do_not_invent_a_complete_graph() {
    let mut world = world();
    assert!(world.truce_with(EntityId(0), EntityId(1)).is_err());
    world
        .grant_truce(EntityId(1), EntityId(0), GrantTerm::Permanent)
        .unwrap();
    assert!(world.truce_with(EntityId(0), EntityId(1)).unwrap());
    assert!(world.has_truce(EntityId(1)).unwrap());
    assert!(world.has_truce(EntityId(2)).is_err());
    world
        .grant_truce(EntityId(0), EntityId(0), GrantTerm::Permanent)
        .unwrap();
    world.clear_truce(EntityId(0), EntityId(1)).unwrap();
    assert!(!world.truce_with(EntityId(1), EntityId(0)).unwrap());
    assert!(world.has_truce(EntityId(0)).unwrap());
    assert!(world.has_truce(EntityId(1)).is_err());
    let mut state = world.truces().clone();
    state.known_countries.insert(EntityId(1));
    world.set_truces(state).unwrap();
    assert!(!world.has_truce(EntityId(1)).unwrap());
    assert!(!world.truce_with(EntityId(1), EntityId(2)).unwrap());
    assert!(world.truce_with(EntityId(0), EntityId(3)).is_err());
}

#[test]
fn equality_survives_and_due_plans_precede_monthly_retirement() {
    let mut world = world();
    world
        .grant_truce(EntityId(0), EntityId(1), monthly(12))
        .unwrap();
    for day in [365, 396, 397] {
        world.schedule(EntityId(0), day, vec![observe()]).unwrap();
    }
    world.advance_to(365).unwrap();
    assert!(world.truce_with(EntityId(1), EntityId(0)).unwrap());
    world.advance_to(396).unwrap();
    assert!(!world.truce_with(EntityId(0), EntityId(1)).unwrap());
    world.advance_to(397).unwrap();
    assert_eq!(
        world
            .events()
            .iter()
            .map(|e| (e.0, e.2.as_str()))
            .collect::<Vec<_>>(),
        [(365, "present"), (396, "present"), (397, "absent")]
    );
}

#[test]
fn renewal_cancels_the_old_retirement_and_survives_split_restore() {
    let mut whole = world();
    whole
        .grant_truce(EntityId(0), EntityId(1), monthly(12))
        .unwrap();
    whole.advance_to(10).unwrap();
    whole
        .grant_truce(EntityId(1), EntityId(0), monthly(24))
        .unwrap();
    let saved = whole.save("truces:1").unwrap();
    let mut resumed = World::restore(&saved, "truces:1", whole.schema()).unwrap();
    whole.advance_to(396).unwrap();
    assert!(whole.truce_with(EntityId(0), EntityId(1)).unwrap());
    whole.advance_to(761).unwrap();
    for day in [365, 396, 730, 761] {
        resumed.advance_to(day).unwrap();
    }
    assert!(!whole.truce_with(EntityId(0), EntityId(1)).unwrap());
    assert_eq!(
        whole.save("truces:1").unwrap(),
        resumed.save("truces:1").unwrap()
    );
}

#[test]
fn daily_and_monthly_grants_at_the_same_boundary_have_distinct_order() {
    let mut world = world();
    world
        .grant_truce(EntityId(0), EntityId(1), GrantTerm::Days(31))
        .unwrap();
    world
        .grant_truce(EntityId(0), EntityId(2), monthly(0))
        .unwrap();
    world
        .schedule(
            EntityId(0),
            31,
            vec![
                observe(),
                Action::If {
                    condition: Condition::TruceWith {
                        first: named("AAA"),
                        second: named("CCC"),
                    },
                    then: vec![Action::Emit("monthly".into())],
                    otherwise: vec![],
                },
            ],
        )
        .unwrap();
    world.advance_to(31).unwrap();
    assert_eq!(
        world
            .events()
            .iter()
            .map(|e| e.2.as_str())
            .collect::<Vec<_>>(),
        ["absent", "monthly"]
    );
    assert!(!world.truce_with(EntityId(0), EntityId(2)).unwrap());
}

#[test]
fn invalid_saved_graphs_are_rejected_without_mutating_live_state() {
    let mut world = world();
    world
        .grant_truce(EntityId(0), EntityId(1), monthly(12))
        .unwrap();
    let before = world.clone();
    for (first, second, record) in [
        (
            1,
            0,
            Some(Truce {
                expires_on: Some(365),
                monthly: true,
            }),
        ),
        (0, 3, None),
        (
            0,
            1,
            Some(Truce {
                expires_on: None,
                monthly: true,
            }),
        ),
        (
            0,
            1,
            Some(Truce {
                expires_on: Some(0),
                monthly: false,
            }),
        ),
        (
            0,
            1,
            Some(Truce {
                expires_on: Some(u64::MAX),
                monthly: true,
            }),
        ),
    ] {
        let invalid = Truces {
            records: [(EntityId(first), [(EntityId(second), record)].into())].into(),
            ..Default::default()
        };
        assert!(world.set_truces(invalid).is_err());
        assert_eq!(world, before);
    }
    let saved = world.save("truces:1").unwrap();
    let mut corrupt: serde_json::Value = serde_json::from_str(&saved).unwrap();
    corrupt["truces"]["records"]["0"]["1"]["expires_on"] = serde_json::json!(u64::MAX);
    assert!(World::restore(&corrupt.to_string(), "truces:1", world.schema()).is_err());
}

#[test]
fn calendar_changes_rebuild_the_index_and_reject_overflow_atomically() {
    let mut world = world();
    world
        .grant_truce(EntityId(0), EntityId(1), monthly(12))
        .unwrap();
    let before = world.clone();
    let mut invalid = calendar(12, 1);
    invalid.start.year = i16::MAX;
    assert!(world.set_calendar(invalid).is_err());
    assert_eq!(world, before);
    world.set_calendar(calendar(11, 11)).unwrap();
    world.advance_to(384).unwrap();
    assert!(world.truce_with(EntityId(0), EntityId(1)).unwrap());
    world.advance_to(385).unwrap();
    assert!(!world.truce_with(EntityId(0), EntityId(1)).unwrap());
}

#[test]
fn overflow_keeps_the_existing_deadline_and_execution_keeps_prior_writes() {
    let mut world = world();
    world
        .grant_truce(EntityId(0), EntityId(1), monthly(12))
        .unwrap();
    let before = world.clone();
    assert!(
        world
            .grant_truce(EntityId(0), EntityId(1), monthly(u32::MAX))
            .is_err()
    );
    assert_eq!(world, before);
    assert!(
        world
            .execute(
                EntityId(0),
                &[
                    Action::ClearTruce {
                        first: named("AAA"),
                        second: named("BBB")
                    },
                    Action::GrantTruce {
                        first: named("AAA"),
                        second: named("BBB"),
                        term: monthly(u32::MAX)
                    },
                ]
            )
            .is_err()
    );
    assert!(!world.truce_with(EntityId(0), EntityId(1)).unwrap());
    assert!(world.save("truces:1").is_err());
}

#[test]
fn absent_references_skip_effects_and_do_not_require_graph_coverage() {
    let mut world = world();
    let missing = EntityRef::Selected(ScopeSelector::Global("missing".into()));
    world
        .execute(
            EntityId(0),
            &[
                Action::GrantTruce {
                    first: named("AAA"),
                    second: missing.clone(),
                    term: monthly(12),
                },
                Action::If {
                    condition: Condition::TruceWith {
                        first: named("AAA"),
                        second: missing,
                    },
                    then: vec![],
                    otherwise: vec![Action::Emit("missing".into())],
                },
            ],
        )
        .unwrap();
    assert!(world.truces().is_empty());
    assert_eq!(world.events()[0].2, "missing");
}
