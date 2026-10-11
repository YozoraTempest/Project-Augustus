// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    AttributeBase, AttributeChannel, AttributeFormula, AttributeTerm, Attributes, Calendar,
    CountryResources, EntityId, EntityKind, EstateDefinition, EstateEffect, EstateInfluenceBand,
    EstatePrivilege, EstateRules, EstateState, Estates, Schema, Target, Taxation, World, WorldDate,
};

fn effect(value: i64) -> EstateEffect {
    EstateEffect {
        modifier: augustus_core::ModifierDefinition {
            values_micros: [("tax".into(), value)].into_iter().collect(),
            ..Default::default()
        },
        ..Default::default()
    }
}

fn world() -> World {
    let mut world = World::new(Schema::new([] as [&str; 0]).unwrap());
    let a = world.add_entity("A", EntityKind::Country);
    let b = world.add_entity("B", EntityKind::Country);
    let p = world.add_entity("P", EntityKind::Province);
    world.set_owner(p, a).unwrap();
    let estates = Estates {
        definitions: [(
            "guild".into(),
            EstateDefinition {
                angry: effect(-200_000),
                neutral: effect(200_000),
                happy: effect(400_000),
                land_ownership: effect(100_000),
                privileges: ["charter".into()].into_iter().collect(),
            },
        )]
        .into_iter()
        .collect(),
        privileges: [(
            "charter".into(),
            EstatePrivilege {
                effect: effect(-100_000),
                land_ownership: effect(200_000),
            },
        )]
        .into_iter()
        .collect(),
        rules: EstateRules {
            angry_below_milli: Some(30_000),
            happy_from_milli: Some(60_000),
            influence_bands: Some(
                [
                    (0, 250_000),
                    (20_000, 500_000),
                    (40_000, 750_000),
                    (60_000, 1_000_000),
                ]
                .into_iter()
                .map(|(minimum_milli, scale_micros)| EstateInfluenceBand {
                    minimum_milli,
                    scale_micros,
                })
                .collect(),
            ),
        },
        countries: [
            (
                a,
                [(
                    "guild".into(),
                    EstateState {
                        active: Some(true),
                        loyalty_milli: Some(50_000),
                        influence_milli: Some(40_000),
                        land_share_micros: Some(250_000),
                        privileges: Some(["charter".into()].into_iter().collect()),
                    },
                )]
                .into_iter()
                .collect(),
            ),
            (
                b,
                [(
                    "guild".into(),
                    EstateState {
                        active: Some(false),
                        ..Default::default()
                    },
                )]
                .into_iter()
                .collect(),
            ),
        ]
        .into_iter()
        .collect(),
        ..Default::default()
    };
    world.set_estates(estates).unwrap();
    world
        .set_attributes(Attributes {
            formulas: [(
                p,
                [(
                    "annual".into(),
                    AttributeFormula {
                        base: AttributeBase::Constant { micros: 12_000_000 },
                        channels: vec![AttributeChannel {
                            flat: vec![],
                            rates: vec![AttributeTerm::Estates {
                                target: Target::Owner,
                                property: "tax".into(),
                            }],
                        }],
                        minimum_micros: None,
                        maximum_micros: None,
                        unresolved: Default::default(),
                    },
                )]
                .into_iter()
                .collect(),
            )]
            .into_iter()
            .collect(),
            ..Default::default()
        })
        .unwrap();
    world
}

fn tax(world: &World) -> augustus_core::AttributeEvaluation {
    world
        .attributes()
        .evaluate(world, EntityId(2), "annual")
        .unwrap()
}

#[test]
fn current_loyalty_influence_land_privileges_and_owner_choose_effects() {
    let mut world = world();
    assert_eq!(tax(&world).value_micros, Some(13_500_000));
    for (loyalty, influence, expected) in [
        (29_999, 40_000, 9_900_000),
        (30_000, 19_999, 12_300_000),
        (50_000, 20_000, 12_900_000),
        (60_000, 60_000, 16_500_000),
    ] {
        let mut estates = world.estates().clone();
        let state = estates
            .countries
            .get_mut(&EntityId(0))
            .unwrap()
            .get_mut("guild")
            .unwrap();
        state.loyalty_milli = Some(loyalty);
        state.influence_milli = Some(influence);
        world.set_estates(estates).unwrap();
        assert_eq!(tax(&world).value_micros, Some(expected));
    }
    let mut estates = world.estates().clone();
    let state = estates
        .countries
        .get_mut(&EntityId(0))
        .unwrap()
        .get_mut("guild")
        .unwrap();
    state.privileges.as_mut().unwrap().clear();
    state.land_share_micros = Some(0);
    world.set_estates(estates).unwrap();
    assert_eq!(tax(&world).value_micros, Some(16_800_000));
    world.set_owner(EntityId(2), EntityId(1)).unwrap();
    assert_eq!(tax(&world).value_micros, Some(12_000_000));
}

#[test]
fn unknown_metrics_only_block_relevant_effects_and_never_credit_partial_income() {
    let mut world = world();
    let mut estates = world.estates().clone();
    estates
        .countries
        .get_mut(&EntityId(0))
        .unwrap()
        .get_mut("guild")
        .unwrap()
        .loyalty_milli = None;
    world.set_estates(estates.clone()).unwrap();
    assert_eq!(tax(&world).known_subtotal_micros, Some(11_700_000));
    assert_eq!(tax(&world).value_micros, None);
    assert!(
        tax(&world)
            .unresolved
            .contains("estates:guild:unknown_loyalty_band")
    );
    assert!(
        world
            .estates()
            .property_effects(EntityId(0), "unrelated")
            .unwrap()
            .unresolved
            .is_empty()
    );
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
        .set_taxation(Taxation::new("annual", [EntityId(0)].into_iter().collect()))
        .unwrap();
    world
        .set_country_resources(
            [(
                EntityId(0),
                CountryResources {
                    treasury_milli: Some(10_000),
                    ..Default::default()
                },
            )]
            .into_iter()
            .collect(),
        )
        .unwrap();
    world.advance_to(20).unwrap();
    let account = &world
        .taxation()
        .unwrap()
        .last_settlement
        .as_ref()
        .unwrap()
        .accounts[&EntityId(0)];
    assert_eq!(account.known_subtotal_milli, 975);
    assert_eq!(account.credited_milli, None);
    assert_eq!(world.country_resources()[&EntityId(0)].treasury_milli, None);
    estates
        .countries
        .get_mut(&EntityId(0))
        .unwrap()
        .get_mut("guild")
        .unwrap()
        .active = None;
    estates
        .countries
        .get_mut(&EntityId(0))
        .unwrap()
        .get_mut("guild")
        .unwrap()
        .privileges = Some(Default::default());
    estates.definitions.get_mut("guild").unwrap().angry = Default::default();
    estates.definitions.get_mut("guild").unwrap().neutral = Default::default();
    estates.definitions.get_mut("guild").unwrap().happy = Default::default();
    estates.definitions.get_mut("guild").unwrap().land_ownership = Default::default();
    world.set_estates(estates).unwrap();
    assert_eq!(tax(&world).value_micros, Some(12_000_000));
}

#[test]
fn complete_native_estate_tax_settles_and_resumes_without_repeating_receipts() {
    let mut world = world();
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
        .set_taxation(Taxation::new("annual", [EntityId(0)].into_iter().collect()))
        .unwrap();
    world
        .set_country_resources(
            [(
                EntityId(0),
                CountryResources {
                    treasury_milli: Some(10_000),
                    ..Default::default()
                },
            )]
            .into_iter()
            .collect(),
        )
        .unwrap();
    world.advance_to(20).unwrap();
    assert_eq!(
        world.country_resources()[&EntityId(0)].treasury_milli,
        Some(11_125)
    );
    let saved = world.save("native-estates").unwrap();
    let mut restored = World::restore(&saved, "native-estates", world.schema()).unwrap();
    assert_eq!(world, restored);
    world.advance_to(51).unwrap();
    restored.advance_to(51).unwrap();
    assert_eq!(world, restored);
    assert_eq!(
        world.country_resources()[&EntityId(0)].treasury_milli,
        Some(12_250)
    );
}

#[test]
fn unexecuted_privilege_effects_taint_properties_and_invalid_replacements_are_atomic() {
    let mut world = world();
    let mut estates = world.estates().clone();
    estates
        .privileges
        .get_mut("charter")
        .unwrap()
        .effect
        .unresolved_effects
        .insert("callback".into());
    world.set_estates(estates.clone()).unwrap();
    assert!(
        world
            .estates()
            .property_effects(EntityId(0), "unrelated")
            .unwrap()
            .unresolved
            .contains("estates:guild:privilege:charter:unexecuted:callback")
    );
    let before = world.clone();
    for kind in 0..5 {
        let mut invalid = estates.clone();
        match kind {
            0 => invalid.rules.influence_bands.as_mut().unwrap().reverse(),
            1 => {
                invalid
                    .countries
                    .get_mut(&EntityId(0))
                    .unwrap()
                    .get_mut("guild")
                    .unwrap()
                    .loyalty_milli = Some(100_001)
            }
            2 => {
                invalid.countries.insert(EntityId(2), Default::default());
            }
            3 => {
                invalid
                    .countries
                    .get_mut(&EntityId(0))
                    .unwrap()
                    .get_mut("guild")
                    .unwrap()
                    .privileges
                    .as_mut()
                    .unwrap()
                    .insert("other".into());
            }
            _ => {
                invalid
                    .privileges
                    .get_mut("charter")
                    .unwrap()
                    .effect
                    .modifier
                    .values_micros
                    .insert("tax".into(), i64::MAX);
            }
        }
        assert!(world.set_estates(invalid).is_err());
        assert_eq!(world, before);
    }
    let mut corrupt: serde_json::Value =
        serde_json::from_str(&world.save("estates").unwrap()).unwrap();
    corrupt["estates"]["countries"]["0"]["guild"]["land_share_micros"] =
        serde_json::json!(1_000_001);
    assert!(World::restore(&corrupt.to_string(), "estates", world.schema()).is_err());
    corrupt.as_object_mut().unwrap().remove("estates");
    assert!(
        World::restore(&corrupt.to_string(), "estates", world.schema())
            .unwrap()
            .estates()
            .is_empty()
    );
}
