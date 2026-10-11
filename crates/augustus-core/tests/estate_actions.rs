// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    Action, EntityId, EntityKind, EstateDefinition, EstateEffect, EstateMetric, EstatePrivilege,
    EstateState, Estates, ModifierDefinition, Schema, World,
};
use std::collections::BTreeMap;

fn world() -> World {
    let mut world = World::new(Schema::new(std::iter::empty::<&str>()).unwrap());
    let alpha = world.add_entity("alpha", EntityKind::Country);
    let beta = world.add_entity("beta", EntityKind::Country);
    world.add_entity("province", EntityKind::Province);
    world
        .set_estates(Estates {
            definitions: BTreeMap::from([(
                "guild".into(),
                EstateDefinition {
                    privileges: ["charter".into()].into(),
                    ..Default::default()
                },
            )]),
            privileges: BTreeMap::from([(
                "charter".into(),
                EstatePrivilege {
                    effect: EstateEffect {
                        modifier: ModifierDefinition {
                            values_micros: BTreeMap::from([("tax".into(), 100_000)]),
                            ..Default::default()
                        },
                        unresolved_effects: ["grant".into()].into(),
                        ..Default::default()
                    },
                    ..Default::default()
                },
            )]),
            countries: [alpha, beta]
                .into_iter()
                .map(|entity| {
                    (
                        entity,
                        BTreeMap::from([(
                            "guild".into(),
                            EstateState {
                                active: Some(true),
                                loyalty_milli: Some(50_000),
                                land_share_micros: Some(400_000),
                                privileges: Some(Default::default()),
                                ..Default::default()
                            },
                        )]),
                    )
                })
                .collect(),
            ..Default::default()
        })
        .unwrap();
    world
}

fn grant(granted: bool) -> Action {
    Action::SetEstatePrivilege {
        estate: "guild".into(),
        privilege: "charter".into(),
        granted,
    }
}
fn resolve() -> Action {
    Action::ResolveEstatePrivilegeEffect {
        privilege: "charter".into(),
        effect: "grant".into(),
    }
}

#[test]
fn callback_resolution_is_country_specific_saved_and_invalidated_on_selection_change() {
    let mut world = world();
    world
        .execute(EntityId(0), &[grant(true), resolve()])
        .unwrap();
    world.execute(EntityId(1), &[grant(true)]).unwrap();
    assert!(
        world
            .estates()
            .property_effects(EntityId(0), "tax")
            .unwrap()
            .unresolved
            .is_empty()
    );
    assert!(
        !world
            .estates()
            .property_effects(EntityId(1), "tax")
            .unwrap()
            .unresolved
            .is_empty()
    );
    let saved = world.save("estate-actions").unwrap();
    assert_eq!(
        World::restore(&saved, "estate-actions", world.schema()).unwrap(),
        world
    );
    let mut corrupt: serde_json::Value = serde_json::from_str(&saved).unwrap();
    corrupt["estates"]["resolved_privilege_effects"]["0"]["charter"] =
        serde_json::json!(["not_declared"]);
    assert!(World::restore(&corrupt.to_string(), "estate-actions", world.schema()).is_err());
    world
        .execute(EntityId(0), &[grant(false), grant(true)])
        .unwrap();
    assert!(
        !world
            .estates()
            .property_effects(EntityId(0), "tax")
            .unwrap()
            .unresolved
            .is_empty()
    );
    assert!(world.estates().resolved_privilege_effects.is_empty());
}

#[test]
fn metric_actions_preserve_unknowns_and_reject_invalid_changes_without_partial_metric_writes() {
    let mut world = world();
    let mut estates = world.estates().clone();
    estates
        .countries
        .get_mut(&EntityId(1))
        .unwrap()
        .get_mut("guild")
        .unwrap()
        .loyalty_milli = None;
    world.set_estates(estates).unwrap();
    let loyalty = |amount| Action::ChangeEstateMetric {
        estate: "guild".into(),
        metric: EstateMetric::Loyalty,
        amount,
        minimum: Some(0),
    };
    world.execute(EntityId(1), &[loyalty(7_000)]).unwrap();
    assert!(
        world.estates().countries[&EntityId(1)]["guild"]
            .loyalty_milli
            .is_none()
    );
    world.execute(EntityId(0), &[loyalty(-100_000)]).unwrap();
    assert_eq!(
        world.estates().countries[&EntityId(0)]["guild"].loyalty_milli,
        Some(0)
    );
    assert!(
        world
            .execute(
                EntityId(0),
                &[
                    loyalty(7_000),
                    Action::ChangeEstateMetric {
                        estate: "guild".into(),
                        metric: EstateMetric::LandShare,
                        amount: 700_000,
                        minimum: None
                    }
                ]
            )
            .is_err()
    );
    assert_eq!(
        world.estates().countries[&EntityId(0)]["guild"].loyalty_milli,
        Some(7_000)
    );
    assert_eq!(
        world.estates().countries[&EntityId(0)]["guild"].land_share_micros,
        Some(400_000)
    );
    assert!(world.save("failed").is_err());
}
