// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{Action, Attributes, EntityId, EntityKind, Schema, Target, World};

fn world() -> World {
    let mut world = World::new(Schema::new(["value"]).unwrap());
    world.add_entity("Country", EntityKind::Country);
    world.add_entity("Province", EntityKind::Province);
    world.set_owner(EntityId(1), EntityId(0)).unwrap();
    world.set_affiliations(serde_json::from_str(r#"{
        "choice":{"groups":{"first":"family","second":"family"},"members":{"0":null},"acceptance":{"0":{"members":[],"groups":[]}}},
        "other":{"groups":{"fixed":"group"},"members":{"0":"fixed"},"acceptance":{}}
    }"#).unwrap()).unwrap();
    let attributes: Attributes = serde_json::from_str(r#"{
        "static_modifiers":{},"building_modifiers":{},"trade_good_modifiers":{},
        "affiliation_modifiers":{"choice":{"country":{
            "first":{"modifier":{"values_micros":{"rate":250000},"flags":{}},"unresolved_properties":[]},
            "second":{"modifier":{"values_micros":{"rate":-500000},"flags":{}},"unresolved_properties":[]}
        }}},
        "formulas":{"1":{"yield":{"base":{"kind":"constant","micros":1000000},
            "channels":[{"flat":[],"rates":[{"kind":"affiliation_modifiers","dimension":"choice","target":"Owner","scope":"country","property":"rate"}]}],
            "minimum_micros":null,"maximum_micros":null,"unresolved":[]}}}
    }"#).unwrap();
    world.set_attributes(attributes).unwrap();
    world
}

fn value(world: &World) -> Option<i64> {
    world
        .attributes()
        .evaluate(world, EntityId(1), "yield")
        .unwrap()
        .value_micros
}

#[test]
fn membership_changes_recalculate_owner_attributes_and_distinguish_absence_from_unknown() {
    let mut world = world();
    let other = world.affiliations().0["other"].clone();
    let acceptance = world.affiliations().0["choice"].acceptance.clone();
    assert_eq!(value(&world), None);
    for (member, expected) in [
        (Some("first"), 1_250_000),
        (Some("second"), 500_000),
        (Some("second"), 500_000),
        (None, 1_000_000),
        (Some("first"), 1_250_000),
    ] {
        world
            .set_affiliation_member(EntityId(0), "choice", member)
            .unwrap();
        assert_eq!(value(&world), Some(expected));
        assert_eq!(
            world.affiliations().present("choice", EntityId(0)),
            Some(member.is_some())
        );
        assert_eq!(world.affiliations().0["other"], other);
        assert_eq!(world.affiliations().0["choice"].acceptance, acceptance);
    }
}

#[test]
fn invalid_membership_changes_do_not_modify_the_world() {
    let mut world = world();
    let before = world.save("mutation").unwrap();
    for (entity, dimension, member) in [
        (EntityId(99), "choice", Some("first")),
        (EntityId(0), "missing", None),
        (EntityId(0), "", Some("first")),
        (EntityId(0), "choice", Some("")),
        (EntityId(0), "choice", Some("missing")),
    ] {
        assert!(
            world
                .set_affiliation_member(entity, dimension, member)
                .is_err()
        );
        assert_eq!(world.save("mutation").unwrap(), before);
    }
}

#[test]
fn delayed_membership_actions_resume_with_their_scope_and_live_attribute_inputs() {
    let mut continuous = world();
    continuous
        .execute(
            EntityId(1),
            &[Action::Schedule {
                delay: 2,
                plan: vec![Action::SetAffiliation {
                    target: Target::Owner,
                    dimension: "choice".into(),
                    member: Some("second".into()),
                }],
            }],
        )
        .unwrap();
    continuous
        .set_affiliation_member(EntityId(0), "choice", Some("first"))
        .unwrap();
    assert_eq!(value(&continuous), Some(1_250_000));
    let mut resumed = World::restore(
        &continuous.save("mutation").unwrap(),
        "mutation",
        continuous.schema(),
    )
    .unwrap();
    for world in [&mut continuous, &mut resumed] {
        world.advance_to(2).unwrap();
        assert_eq!(value(world), Some(500_000));
        world
            .execute(
                EntityId(0),
                &[Action::SetAffiliation {
                    target: Target::Current,
                    dimension: "choice".into(),
                    member: None,
                }],
            )
            .unwrap();
        assert_eq!(value(world), Some(1_000_000));
    }
    assert_eq!(continuous, resumed);
}
