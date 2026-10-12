// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::*;

fn world() -> World {
    let mut world = World::new(Schema::new(["unused"]).unwrap());
    for name in ["AAA", "BBB", "CCC"] {
        world.add_entity(name, EntityKind::Country);
    }
    for (index, name) in ["West", "East"].into_iter().enumerate() {
        let province = world.add_entity(name, EntityKind::Province);
        world.set_owner(province, EntityId(index)).unwrap();
    }
    world
        .set_calendar(Calendar {
            start: WorldDate {
                year: 1444,
                month: 11,
                day: 11,
            },
            month_lengths: [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31],
        })
        .unwrap();
    world
        .set_justifications(Justifications {
            definitions: ["restoration", "honour"]
                .into_iter()
                .map(|name| {
                    (
                        name.into(),
                        JustificationDefinition {
                            issuer: Some(Condition::HasOwnedProvince {
                                target: Target::Current,
                            }),
                            target: Some(Condition::HasOwnedProvince {
                                target: Target::Current,
                            }),
                        },
                    )
                })
                .collect(),
            ..Default::default()
        })
        .unwrap();
    world
}

fn monthly(count: u32) -> GrantTerm {
    GrantTerm::Months {
        count,
        monthly: true,
    }
}
fn current() -> EntityRef {
    EntityRef::Relative(Target::Current)
}
fn named(name: &str) -> EntityRef {
    EntityRef::Selected(ScopeSelector::Named {
        kind: EntityKind::Country,
        name: name.into(),
    })
}
fn condition(name: &str) -> Condition {
    Condition::HasJustification {
        issuer: current(),
        target: named("BBB"),
        name: Some(name.into()),
    }
}

#[test]
fn directed_grants_replace_only_the_matching_type_and_target() {
    let mut world = world();
    for (issuer, target, name) in [(0, 1, "restoration"), (1, 0, "honour"), (0, 1, "honour")] {
        world
            .grant_justification(EntityId(issuer), EntityId(target), name, monthly(3))
            .unwrap();
    }
    assert!(
        !world
            .has_justification(EntityId(1), EntityId(0), Some("restoration"))
            .unwrap()
    );
    world
        .grant_justification(EntityId(0), EntityId(1), "restoration", monthly(1))
        .unwrap();
    assert_eq!(
        world.justifications().active[&EntityId(0)][&EntityId(1)].len(),
        2
    );
    assert_eq!(
        world.justifications().active[&EntityId(0)][&EntityId(1)]["restoration"].expires_on,
        Some(30)
    );
    world
        .remove_justification(EntityId(0), EntityId(1), "restoration")
        .unwrap();
    assert!(
        world
            .has_justification(EntityId(0), EntityId(1), None)
            .unwrap()
    );
    assert!(
        world
            .has_justification(EntityId(1), EntityId(0), Some("honour"))
            .unwrap()
    );
    let before = world.clone();
    world
        .grant_justification(EntityId(0), EntityId(0), "restoration", monthly(1))
        .unwrap();
    assert_eq!(world, before);
    assert!(
        !world
            .has_justification(EntityId(0), EntityId(0), None)
            .unwrap()
    );
}

#[test]
fn dependent_content_cannot_be_removed_from_a_registered_maintenance_rule() {
    let mut world = world();
    world
        .set_modifiers(Modifiers {
            definitions: [("mandate".into(), ModifierDefinition::default())]
                .into_iter()
                .collect(),
            ..Default::default()
        })
        .unwrap();
    let mut grants = world.justifications().clone();
    grants.definitions.get_mut("honour").unwrap().issuer = Some(Condition::HasModifier {
        target: Target::Current,
        name: "mandate".into(),
    });
    world.set_justifications(grants).unwrap();
    let before = world.clone();
    assert!(world.set_modifiers(Modifiers::default()).is_err());
    assert_eq!(world, before);
}

#[test]
fn monthly_expiration_follows_due_plans_and_remains_correct_after_restore() {
    let mut uninterrupted = world();
    uninterrupted
        .grant_justification(EntityId(0), EntityId(1), "restoration", monthly(1))
        .unwrap();
    for day in [30, 31, 51, 52] {
        uninterrupted
            .schedule(
                EntityId(0),
                day,
                vec![Action::If {
                    condition: condition("restoration"),
                    then: vec![Action::Emit("present".into())],
                    otherwise: vec![Action::Emit("absent".into())],
                }],
            )
            .unwrap();
    }
    uninterrupted.advance_to(31).unwrap();
    let snapshot = uninterrupted.save("grants").unwrap();
    let mut resumed = World::restore(&snapshot, "grants", uninterrupted.schema()).unwrap();
    for state in [&mut uninterrupted, &mut resumed] {
        state.advance_to(51).unwrap();
        assert_eq!(state.events().last().unwrap().2, "present");
        assert!(
            !state
                .has_justification(EntityId(0), EntityId(1), Some("restoration"))
                .unwrap()
        );
        state.advance_to(52).unwrap();
        assert_eq!(state.events().last().unwrap().2, "absent");
    }
    assert_eq!(
        uninterrupted.save("grants").unwrap(),
        resumed.save("grants").unwrap()
    );
}

#[test]
fn month_start_deadline_is_retained_until_the_following_monthly_update() {
    let mut world = world();
    world.advance_to(20).unwrap();
    world
        .grant_justification(EntityId(0), EntityId(1), "restoration", monthly(1))
        .unwrap();
    assert_eq!(
        world.justifications().active[&EntityId(0)][&EntityId(1)]["restoration"].expires_on,
        Some(51)
    );
    world.advance_to(51).unwrap();
    assert!(
        world
            .has_justification(EntityId(0), EntityId(1), Some("restoration"))
            .unwrap()
    );
    let saved = world.save("boundary").unwrap();
    let mut resumed = World::restore(&saved, "boundary", world.schema()).unwrap();
    resumed.advance_to(82).unwrap();
    assert!(
        !resumed
            .has_justification(EntityId(0), EntityId(1), Some("restoration"))
            .unwrap()
    );
}

#[test]
fn calendar_month_addition_clamps_only_the_destination_month() {
    let mut world = world();
    world.advance_to(81).unwrap();
    assert_eq!(
        world.date().unwrap(),
        Some(WorldDate {
            year: 1445,
            month: 1,
            day: 31
        })
    );
    world
        .grant_justification(EntityId(0), EntityId(1), "restoration", monthly(1))
        .unwrap();
    world
        .grant_justification(EntityId(0), EntityId(1), "honour", monthly(2))
        .unwrap();
    let grants = &world.justifications().active[&EntityId(0)][&EntityId(1)];
    assert_eq!(
        world
            .calendar()
            .unwrap()
            .date(grants["restoration"].expires_on.unwrap())
            .unwrap(),
        WorldDate {
            year: 1445,
            month: 2,
            day: 28
        }
    );
    assert_eq!(
        world
            .calendar()
            .unwrap()
            .date(grants["honour"].expires_on.unwrap())
            .unwrap(),
        WorldDate {
            year: 1445,
            month: 3,
            day: 31
        }
    );
    assert!(world.calendar().unwrap().after_months(0, u32::MAX).is_err());
    let custom = Calendar {
        start: WorldDate {
            year: 8,
            month: 12,
            day: 30,
        },
        month_lengths: [25, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 30],
    };
    assert_eq!(
        custom.date(custom.after_months(0, 1).unwrap()).unwrap(),
        WorldDate {
            year: 9,
            month: 1,
            day: 25
        }
    );
}

#[test]
fn qualification_precedes_delayed_queries_and_follows_live_ownership() {
    let mut world = world();
    let mut definitions = world.justifications().clone();
    definitions.definitions.get_mut("honour").unwrap().target = Some(Condition::Always(false));
    world.set_justifications(definitions).unwrap();
    world
        .grant_justification(EntityId(0), EntityId(1), "honour", monthly(1))
        .unwrap();
    world
        .grant_justification(EntityId(0), EntityId(2), "restoration", monthly(1))
        .unwrap();
    assert!(
        world
            .has_justification(EntityId(0), EntityId(1), Some("honour"))
            .unwrap()
    );
    assert!(
        world
            .has_justification(EntityId(0), EntityId(2), Some("restoration"))
            .unwrap()
    );
    world
        .schedule(
            EntityId(0),
            1,
            vec![Action::If {
                condition: condition("honour"),
                then: vec![Action::Emit("stale".into())],
                otherwise: vec![Action::Emit("disqualified".into())],
            }],
        )
        .unwrap();
    world.advance_to(1).unwrap();
    assert_eq!(world.events()[0].2, "disqualified");
    assert!(
        !world
            .has_justification(EntityId(0), EntityId(2), Some("restoration"))
            .unwrap()
    );
    world
        .grant_justification(
            EntityId(0),
            EntityId(1),
            "restoration",
            GrantTerm::Permanent,
        )
        .unwrap();
    world.set_owner(EntityId(4), EntityId(0)).unwrap();
    world.advance_to(2).unwrap();
    assert!(
        !world
            .has_justification(EntityId(0), EntityId(1), Some("restoration"))
            .unwrap()
    );
}

#[test]
fn uncompiled_qualification_and_unknown_inventories_never_become_known_absence() {
    let mut world = world();
    let mut definitions = world.justifications().clone();
    definitions.definitions.get_mut("honour").unwrap().issuer = None;
    world.set_justifications(definitions).unwrap();
    world
        .grant_justification(EntityId(0), EntityId(1), "honour", monthly(1))
        .unwrap();
    assert!(
        world
            .has_justification(EntityId(0), EntityId(1), Some("honour"))
            .unwrap()
    );
    world.advance_to(1).unwrap();
    assert!(
        world
            .has_justification(EntityId(0), EntityId(1), Some("honour"))
            .is_err()
    );
    let saved = world.save("unknown").unwrap();
    let restored = World::restore(&saved, "unknown", world.schema()).unwrap();
    assert!(
        restored
            .has_justification(EntityId(0), EntityId(1), Some("honour"))
            .is_err()
    );
    let mut unknown = world.justifications().clone();
    unknown.unknown_countries.insert(EntityId(1));
    world.set_justifications(unknown).unwrap();
    assert!(
        world
            .has_justification(EntityId(1), EntityId(0), Some("honour"))
            .is_err()
    );
    assert!(
        world
            .grant_justification(EntityId(1), EntityId(0), "honour", GrantTerm::Permanent)
            .is_err()
    );
}

#[test]
fn ordinary_expiration_precedes_due_conditions_and_delayed_grants_use_their_execution_date() {
    let mut world = world();
    world
        .grant_justification(EntityId(0), EntityId(1), "restoration", GrantTerm::Days(2))
        .unwrap();
    world
        .schedule(
            EntityId(0),
            2,
            vec![
                Action::If {
                    condition: condition("restoration"),
                    then: vec![Action::Emit("stale".into())],
                    otherwise: vec![Action::Emit("expired".into())],
                },
                Action::GrantJustification {
                    issuer: current(),
                    target: named("BBB"),
                    name: "restoration".into(),
                    term: monthly(1),
                },
            ],
        )
        .unwrap();
    world.advance_to(2).unwrap();
    assert_eq!(world.events()[0].2, "expired");
    assert_eq!(
        world.justifications().active[&EntityId(0)][&EntityId(1)]["restoration"].expires_on,
        Some(32)
    );
}

#[test]
fn invalid_replacement_and_malformed_saved_grants_are_rejected_without_partial_writes() {
    let mut world = world();
    let action = Action::GrantJustification {
        issuer: current(),
        target: named("BBB"),
        name: "restoration".into(),
        term: monthly(1),
    };
    world.schedule(EntityId(0), 1, vec![action]).unwrap();
    let before = world.clone();
    let mut state = world.justifications().clone();
    state.definitions.remove("restoration");
    assert!(world.set_justifications(state).is_err());
    assert_eq!(world, before);
    assert!(
        world
            .grant_justification(EntityId(0), EntityId(3), "restoration", monthly(1))
            .is_err()
    );
    assert!(
        world
            .grant_justification(EntityId(0), EntityId(1), "missing", monthly(1))
            .is_err()
    );
    assert!(
        world
            .grant_justification(EntityId(0), EntityId(1), "restoration", GrantTerm::Days(0))
            .is_err()
    );
    assert_eq!(world, before);
    world.advance_to(1).unwrap();
    let saved: serde_json::Value =
        serde_json::from_str(&world.save("validation").unwrap()).unwrap();
    for value in [serde_json::json!(0), serde_json::json!(99999999)] {
        let mut broken = saved.clone();
        broken["justifications"]["active"]["0"]["1"]["restoration"]["expires_on"] = value;
        if broken["justifications"]["active"]["0"]["1"]["restoration"]["expires_on"] == 0 {
            broken["justifications"]["active"]["0"]["1"]["restoration"]["monthly"] = false.into();
        }
        assert!(World::restore(&broken.to_string(), "validation", world.schema()).is_err());
    }
}
