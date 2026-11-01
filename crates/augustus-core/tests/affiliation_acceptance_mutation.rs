// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::{BTreeMap, BTreeSet};

use augustus_core::{
    Action, AffiliationAcceptance, AffiliationDimension, AffiliationRelation, Affiliations,
    AttributeBase, AttributeChannel, AttributeFormula, AttributeTerm, Attributes, Condition,
    EntityId, EntityKind, ModifierDefinition, Schema, Target, World,
};

fn world() -> (World, EntityId, EntityId, EntityId, EntityId) {
    let mut world = World::new(Schema::new(["yield"]).unwrap());
    let alpha = world.add_entity("AAA", EntityKind::Country);
    let beta = world.add_entity("BBB", EntityKind::Country);
    let no_policy = world.add_entity("CCC", EntityKind::Country);
    let province = world.add_entity("Port", EntityKind::Province);
    world.set_owner(province, alpha).unwrap();

    let affiliations = Affiliations(BTreeMap::from([
        (
            "culture".into(),
            AffiliationDimension {
                groups: BTreeMap::from([
                    ("primary".into(), Some("north".into())),
                    ("mystery".into(), None),
                    ("alliance".into(), Some("league".into())),
                    ("outsider".into(), Some("south".into())),
                ]),
                members: BTreeMap::from([
                    (alpha, Some("primary".into())),
                    (beta, Some("outsider".into())),
                    (province, Some("mystery".into())),
                ]),
                acceptance: BTreeMap::from([
                    (
                        alpha,
                        AffiliationAcceptance {
                            members: Some(BTreeSet::from(["mystery".into(), "alliance".into()])),
                            groups: Some(BTreeSet::from(["league".into()])),
                        },
                    ),
                    (
                        beta,
                        AffiliationAcceptance {
                            members: Some(BTreeSet::new()),
                            groups: Some(BTreeSet::new()),
                        },
                    ),
                ]),
                ..Default::default()
            },
        ),
        (
            "faith".into(),
            AffiliationDimension {
                groups: BTreeMap::from([
                    ("sun".into(), Some("light".into())),
                    ("moon".into(), Some("night".into())),
                ]),
                members: BTreeMap::from([
                    (alpha, Some("sun".into())),
                    (beta, Some("moon".into())),
                    (province, Some("sun".into())),
                ]),
                acceptance: BTreeMap::from([(
                    alpha,
                    AffiliationAcceptance {
                        members: Some(BTreeSet::from(["moon".into()])),
                        groups: Some(BTreeSet::new()),
                    },
                )]),
                ..Default::default()
            },
        ),
    ]));
    world.set_affiliations(affiliations).unwrap();

    let mut attributes = Attributes {
        static_modifiers: BTreeMap::from([
            (
                "accepted".into(),
                ModifierDefinition {
                    values_micros: BTreeMap::from([("yield".into(), 300_000)]),
                    ..Default::default()
                },
            ),
            (
                "foreign".into(),
                ModifierDefinition {
                    values_micros: BTreeMap::from([("yield".into(), -200_000)]),
                    ..Default::default()
                },
            ),
        ]),
        ..Default::default()
    };
    attributes.formulas.insert(
        province,
        BTreeMap::from([(
            "yield".into(),
            AttributeFormula {
                base: AttributeBase::Constant { micros: 1_000_000 },
                channels: vec![AttributeChannel {
                    flat: vec![AttributeTerm::AffiliationRelation {
                        dimension: "culture".into(),
                        target: Target::Owner,
                        definitions: BTreeMap::from([
                            (AffiliationRelation::Accepted, "accepted".into()),
                            (AffiliationRelation::OtherGroup, "foreign".into()),
                        ]),
                        property: "yield".into(),
                    }],
                    rates: Vec::new(),
                }],
                minimum_micros: None,
                maximum_micros: None,
                result_quantum_micros: None,
                unresolved: BTreeSet::new(),
            },
        )]),
    );
    world.set_attributes(attributes).unwrap();
    (world, alpha, beta, no_policy, province)
}

fn accepted(world: &World, province: EntityId) -> Option<i64> {
    world
        .attributes()
        .evaluate(world, province, "yield")
        .unwrap()
        .value_micros
}

#[test]
fn acceptance_mutation_is_idempotent_and_changes_only_the_requested_member() {
    let (mut world, alpha, beta, _, _) = world();
    let original = world.affiliations().clone();

    world
        .set_affiliation_acceptance_member(alpha, "culture", "mystery", true)
        .unwrap();
    let after_add = world.affiliations().clone();
    assert_eq!(after_add, original);
    world
        .set_affiliation_acceptance_member(alpha, "culture", "mystery", true)
        .unwrap();
    assert_eq!(world.affiliations(), &after_add);

    let mut expected = original.clone();
    expected
        .0
        .get_mut("culture")
        .unwrap()
        .acceptance
        .get_mut(&alpha)
        .unwrap()
        .members
        .as_mut()
        .unwrap()
        .remove("mystery");
    world
        .set_affiliation_acceptance_member(alpha, "culture", "mystery", false)
        .unwrap();
    assert_eq!(world.affiliations(), &expected);
    world
        .set_affiliation_acceptance_member(alpha, "culture", "mystery", false)
        .unwrap();
    assert_eq!(world.affiliations(), &expected);
    world
        .set_affiliation_acceptance_member(alpha, "culture", "mystery", true)
        .unwrap();
    world
        .set_affiliation_acceptance_member(alpha, "culture", "mystery", true)
        .unwrap();
    assert_eq!(world.affiliations(), &original);

    assert_eq!(
        world.affiliations().0["culture"].acceptance[&beta],
        original.0["culture"].acceptance[&beta]
    );
    assert_eq!(world.affiliations().0["faith"], original.0["faith"]);
}

#[test]
fn explicit_acceptance_updates_live_relations_and_attribute_values() {
    let (mut world, alpha, _, _, province) = world();
    assert_eq!(
        world.affiliations().relation("culture", province, alpha),
        Some(AffiliationRelation::Accepted)
    );
    assert_eq!(accepted(&world, province), Some(1_300_000));

    let mut affiliations = world.affiliations().clone();
    affiliations
        .0
        .get_mut("culture")
        .unwrap()
        .acceptance
        .get_mut(&alpha)
        .unwrap()
        .groups = None;
    world.set_affiliations(affiliations).unwrap();
    world
        .set_affiliation_acceptance_member(alpha, "culture", "mystery", false)
        .unwrap();
    assert_eq!(
        world
            .affiliations()
            .accepts_member("culture", alpha, "mystery"),
        None
    );
    assert_eq!(
        world.affiliations().relation("culture", province, alpha),
        None
    );
    assert_eq!(accepted(&world, province), None);

    world
        .set_affiliation_acceptance_member(alpha, "culture", "mystery", true)
        .unwrap();
    assert_eq!(
        world
            .affiliations()
            .accepts_member("culture", alpha, "mystery"),
        Some(true)
    );
    assert_eq!(accepted(&world, province), Some(1_300_000));

    let mut affiliations = world.affiliations().clone();
    affiliations
        .0
        .get_mut("culture")
        .unwrap()
        .acceptance
        .get_mut(&alpha)
        .unwrap()
        .groups = Some(BTreeSet::from(["league".into()]));
    world.set_affiliations(affiliations).unwrap();
    world
        .set_affiliation_member(province, "culture", Some("alliance"))
        .unwrap();
    world
        .set_affiliation_acceptance_member(alpha, "culture", "alliance", false)
        .unwrap();
    assert_eq!(
        world
            .affiliations()
            .accepts_member("culture", alpha, "alliance"),
        Some(true)
    );
    assert_eq!(
        world.affiliations().relation("culture", province, alpha),
        Some(AffiliationRelation::Accepted)
    );
    assert_eq!(accepted(&world, province), Some(1_300_000));
}

#[test]
fn invalid_acceptance_mutations_preserve_the_snapshot_and_random_state() {
    let (mut world, alpha, _, no_policy, province) = world();
    let before = world.save("acceptance-mutation").unwrap();
    let invalid = [
        (EntityId(99), "culture", "mystery"),
        (province, "culture", "mystery"),
        (alpha, "missing", "mystery"),
        (alpha, "", "mystery"),
        (alpha, "culture", ""),
        (alpha, "culture", "unregistered"),
        (no_policy, "culture", "mystery"),
    ];
    for (country, dimension, member) in invalid {
        assert!(
            world
                .set_affiliation_acceptance_member(country, dimension, member, true)
                .is_err()
        );
        assert_eq!(world.save("acceptance-mutation").unwrap(), before);
    }

    let mut affiliations = world.affiliations().clone();
    affiliations
        .0
        .get_mut("culture")
        .unwrap()
        .acceptance
        .get_mut(&alpha)
        .unwrap()
        .members = None;
    world.set_affiliations(affiliations).unwrap();
    let unknown_policy = world.save("acceptance-mutation").unwrap();
    for accepted in [true, false] {
        assert!(
            world
                .set_affiliation_acceptance_member(alpha, "culture", "mystery", accepted)
                .is_err()
        );
        assert_eq!(world.save("acceptance-mutation").unwrap(), unknown_policy);
    }
}

#[test]
fn action_conditions_read_acceptance_changes_immediately() {
    let (mut world, alpha, _, _, _) = world();
    let accepts_beta = Condition::AffiliationAcceptsMember {
        target: Target::Current,
        dimension: "culture".into(),
        member: "outsider".into(),
    };
    world
        .execute(
            alpha,
            &[
                Action::SetAffiliationAcceptance {
                    target: Target::Current,
                    dimension: "culture".into(),
                    member: "outsider".into(),
                    accepted: true,
                },
                Action::If {
                    condition: accepts_beta.clone(),
                    then: vec![Action::Emit("added".into())],
                    otherwise: vec![Action::Emit("stale".into())],
                },
                Action::SetAffiliationAcceptance {
                    target: Target::Current,
                    dimension: "culture".into(),
                    member: "outsider".into(),
                    accepted: false,
                },
                Action::If {
                    condition: accepts_beta,
                    then: vec![Action::Emit("stale".into())],
                    otherwise: vec![Action::Emit("removed".into())],
                },
            ],
        )
        .unwrap();
    assert_eq!(
        world
            .events()
            .iter()
            .map(|event| event.2.as_str())
            .collect::<Vec<_>>(),
        ["added", "removed"]
    );
    assert_eq!(
        world
            .affiliations()
            .accepts_member("culture", alpha, "outsider"),
        Some(false)
    );
}

#[test]
fn scheduled_acceptance_action_survives_save_restore() {
    let (mut continuous, alpha, _, _, _) = world();
    continuous
        .execute(
            alpha,
            &[Action::Schedule {
                delay: 1,
                plan: vec![
                    Action::SetAffiliationAcceptance {
                        target: Target::Current,
                        dimension: "culture".into(),
                        member: "outsider".into(),
                        accepted: true,
                    },
                    Action::If {
                        condition: Condition::AffiliationAcceptsMember {
                            target: Target::Current,
                            dimension: "culture".into(),
                            member: "outsider".into(),
                        },
                        then: vec![Action::Emit("accepted".into())],
                        otherwise: vec![Action::Emit("stale".into())],
                    },
                ],
            }],
        )
        .unwrap();
    let saved = continuous.save("scheduled-acceptance").unwrap();
    let mut resumed = World::restore(&saved, "scheduled-acceptance", continuous.schema()).unwrap();

    for state in [&mut continuous, &mut resumed] {
        state.advance_to(1).unwrap();
        assert_eq!(
            state
                .affiliations()
                .accepts_member("culture", alpha, "outsider"),
            Some(true)
        );
        assert_eq!(state.events().last().unwrap().2, "accepted");
    }
    assert_eq!(continuous, resumed);
    assert_eq!(
        continuous.save("scheduled-acceptance").unwrap(),
        resumed.save("scheduled-acceptance").unwrap()
    );
}
