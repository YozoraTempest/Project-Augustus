// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    AttributeBase, AttributeChannel, AttributeFormula, AttributeTerm, Attributes, Calendar,
    CountryResources, EntityId, EntityKind, EstateDefinition, EstateEffect, EstateInfluenceBand,
    EstateLoyaltyDecay, EstatePrivilege, EstateRules, EstateState, Estates, Schema, Target,
    Taxation, VanillaEstateInfluenceScaleInput, World, WorldDate,
    calculate_vanilla_estate_influence_scale,
};

#[test]
fn vanilla_estate_influence_scale_uses_strict_thresholds_and_zero_special_case() {
    let input = |raw_influence_milli| VanillaEstateInfluenceScaleInput {
        raw_influence_milli,
        additional_value_milli: 0,
        first_threshold_milli: 20_000,
        second_threshold_milli: 40_000,
        third_threshold_milli: 60_000,
    };

    for (value, expected_scale) in [
        (-1, 0),
        (0, 0),
        (1, 250),
        (19_999, 250),
        (20_000, 500),
        (39_999, 500),
        (40_000, 750),
        (59_999, 750),
        (60_000, 1_000),
        (100_001, 1_000),
    ] {
        let result = calculate_vanilla_estate_influence_scale(input(value));
        assert_eq!(result.scale_milli, expected_scale, "input {value}");
    }
}

#[test]
fn vanilla_estate_influence_scale_clamps_before_adding_context() {
    let from_negative =
        calculate_vanilla_estate_influence_scale(VanillaEstateInfluenceScaleInput {
            raw_influence_milli: -10,
            additional_value_milli: 10,
            first_threshold_milli: 20_000,
            second_threshold_milli: 40_000,
            third_threshold_milli: 60_000,
        });
    assert_eq!(from_negative.clamped_influence_milli, 0);
    assert_eq!(from_negative.tier_input_milli, 10);
    assert_eq!(from_negative.scale_milli, 250);

    let from_above_max =
        calculate_vanilla_estate_influence_scale(VanillaEstateInfluenceScaleInput {
            raw_influence_milli: 150_000,
            additional_value_milli: -50_000,
            first_threshold_milli: 20_000,
            second_threshold_milli: 40_000,
            third_threshold_milli: 60_000,
        });
    assert_eq!(from_above_max.clamped_influence_milli, 100_000);
    assert_eq!(from_above_max.tier_input_milli, 50_000);
    assert_eq!(from_above_max.scale_milli, 750);
}

#[test]
fn vanilla_estate_influence_scale_clamps_before_comparing_high_thresholds() {
    let result = calculate_vanilla_estate_influence_scale(VanillaEstateInfluenceScaleInput {
        raw_influence_milli: 150_000,
        additional_value_milli: 0,
        first_threshold_milli: 120_000,
        second_threshold_milli: 130_000,
        third_threshold_milli: 140_000,
    });

    assert_eq!(result.clamped_influence_milli, 100_000);
    assert_eq!(result.tier_input_milli, 100_000);
    assert_eq!(result.scale_milli, 250);
}

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
                ..Default::default()
            },
        )]
        .into_iter()
        .collect(),
        rules: EstateRules {
            angry_below_milli: Some(30_000),
            happy_from_milli: Some(60_000),
            influence_bands: Some(
                [
                    (0, 0),
                    (1, 250_000),
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
            loyalty_target_base_milli: None,
            loyalty_decay: None,
        },
        countries: [
            (
                a,
                [(
                    "guild".into(),
                    EstateState {
                        active: Some(true),
                        loyalty_milli: Some(50_000),
                        loyalty_target_milli: None,
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
                        result_quantum_micros: None,
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

#[test]
fn estate_loyalty_decay_matches_original_monthly_adjustments() {
    let decay = EstateLoyaltyDecay {
        base_min_annual_milli: 2_000,
        base_max_annual_milli: 5_000,
    };

    for (current, target, expected) in [
        (49_571, 40_300, 49_358),
        (49_615, 44_600, 49_424),
        (49_513, 34_400, 49_271),
        (40_300, 49_571, 40_513),
        (50_000, 50_000, 50_000),
        (99_990, 100_000, 100_000),
        (10, 0, 0),
    ] {
        assert_eq!(
            decay.next_loyalty_milli(current, target).unwrap(),
            expected,
            "current={current}, target={target}"
        );
    }
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
fn zero_estate_influence_disables_the_source_modifier_but_positive_values_keep_the_floor() {
    let mut world = world();

    for (influence_milli, expected_micros) in [(0, 0), (1, 50_000), (19_999, 50_000)] {
        let mut estates = world.estates().clone();
        estates
            .countries
            .get_mut(&EntityId(0))
            .unwrap()
            .get_mut("guild")
            .unwrap()
            .influence_milli = Some(influence_milli);
        world.set_estates(estates).unwrap();

        let evaluation = world
            .estates()
            .property_effects(EntityId(0), "tax")
            .unwrap();
        let source = evaluation
            .contributions
            .iter()
            .find(|contribution| contribution.source == "estates:guild:neutral")
            .unwrap();
        assert_eq!(
            source.micros, expected_micros,
            "influence {influence_milli}"
        );
    }
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
    for kind in 0..6 {
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
            4 => {
                invalid.rules.loyalty_target_base_milli = Some(i64::from(i32::MAX) + 1);
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

#[test]
fn estate_conditions_read_live_privileges_and_metrics_without_defaulting_unknowns() {
    let current = world();
    let a = EntityId(0);
    let b = EntityId(1);
    assert!(
        augustus_core::Condition::EstatePrivilege {
            target: Target::Current,
            privilege: "charter".into(),
        }
        .evaluate(&current, a)
        .unwrap()
    );
    assert!(
        !augustus_core::Condition::EstatePrivilege {
            target: Target::Current,
            privilege: "charter".into(),
        }
        .evaluate(&current, b)
        .unwrap()
    );
    assert!(
        augustus_core::Condition::EstateLoyaltyAtLeast {
            target: Target::Current,
            estate: "guild".into(),
            loyalty_milli: 50_000,
        }
        .evaluate(&current, a)
        .unwrap()
    );
    assert!(
        !augustus_core::Condition::EstateLoyaltyAtLeast {
            target: Target::Current,
            estate: "guild".into(),
            loyalty_milli: 50_001,
        }
        .evaluate(&current, a)
        .unwrap()
    );
    assert!(
        augustus_core::Condition::EstateInfluenceAtLeast {
            target: Target::Current,
            estate: "guild".into(),
            influence_milli: 40_000,
        }
        .evaluate(&current, a)
        .unwrap()
    );

    let mut live = world();
    live.execute(
        a,
        &[
            augustus_core::Action::ChangeEstateMetric {
                estate: "guild".into(),
                metric: augustus_core::EstateMetric::Loyalty,
                amount: 10_000,
                minimum: None,
            },
            augustus_core::Action::If {
                condition: augustus_core::Condition::EstateLoyaltyAtLeast {
                    target: Target::Current,
                    estate: "guild".into(),
                    loyalty_milli: 60_000,
                },
                then: vec![augustus_core::Action::Emit("loyalty-updated".into())],
                otherwise: Vec::new(),
            },
            augustus_core::Action::SetEstatePrivilege {
                estate: "guild".into(),
                privilege: "charter".into(),
                granted: false,
            },
            augustus_core::Action::If {
                condition: augustus_core::Condition::EstatePrivilege {
                    target: Target::Current,
                    privilege: "charter".into(),
                },
                then: vec![augustus_core::Action::Emit("privilege-retained".into())],
                otherwise: vec![augustus_core::Action::Emit("privilege-removed".into())],
            },
        ],
    )
    .unwrap();
    assert_eq!(
        live.events()
            .iter()
            .map(|event| event.2.as_str())
            .collect::<Vec<_>>(),
        ["loyalty-updated", "privilege-removed"]
    );

    let mut unknown = world();
    let mut estates = unknown.estates().clone();
    estates
        .countries
        .get_mut(&a)
        .unwrap()
        .get_mut("guild")
        .unwrap()
        .influence_milli = None;
    unknown.set_estates(estates).unwrap();
    assert!(
        augustus_core::Condition::EstateInfluenceAtLeast {
            target: Target::Current,
            estate: "guild".into(),
            influence_milli: 1,
        }
        .evaluate(&unknown, a)
        .is_err()
    );
}

#[test]
fn conditional_privilege_modifiers_use_documented_scales_and_keep_unknowns() {
    let mut current = world();
    let country = EntityId(0);
    let mut estates = current.estates().clone();
    estates
        .privileges
        .get_mut("charter")
        .unwrap()
        .effect
        .conditional_modifiers = vec![
        (
            augustus_core::EstateConditionalScale::Fixed,
            augustus_core::Condition::EstatePrivilege {
                target: Target::Current,
                privilege: "charter".into(),
            },
        ),
        (
            augustus_core::EstateConditionalScale::Influence,
            augustus_core::Condition::EstateLoyaltyAtLeast {
                target: Target::Current,
                estate: "guild".into(),
                loyalty_milli: 30_000,
            },
        ),
        (
            augustus_core::EstateConditionalScale::Loyalty,
            augustus_core::Condition::EstateInfluenceAtLeast {
                target: Target::Current,
                estate: "guild".into(),
                influence_milli: 30_000,
            },
        ),
    ]
    .into_iter()
    .map(
        |(scale, condition)| augustus_core::EstateConditionalModifier {
            condition,
            scale,
            modifier: augustus_core::ModifierDefinition {
                values_micros: [("tax".into(), 1_000_000)].into_iter().collect(),
                ..Default::default()
            },
            unresolved_properties: Default::default(),
        },
    )
    .collect();
    current.set_estates(estates.clone()).unwrap();

    let evaluation = current
        .estates()
        .property_effects_with_world(&current, country, "tax")
        .unwrap();
    let conditional_values = evaluation
        .contributions
        .iter()
        .filter(|contribution| contribution.source.contains(":conditional:"))
        .map(|contribution| contribution.micros)
        .collect::<Vec<_>>();
    assert_eq!(conditional_values, [1_000_000, 400_000, 500_000]);
    let saved = current.save("conditional-estates").unwrap();
    let restored = World::restore(&saved, "conditional-estates", current.schema()).unwrap();
    let restored = restored
        .estates()
        .property_effects_with_world(&restored, country, "tax")
        .unwrap();
    assert_eq!(
        restored
            .contributions
            .iter()
            .filter(|contribution| contribution.source.contains(":conditional:"))
            .map(|contribution| contribution.micros)
            .collect::<Vec<_>>(),
        [1_000_000, 400_000, 500_000]
    );

    estates
        .countries
        .get_mut(&country)
        .unwrap()
        .get_mut("guild")
        .unwrap()
        .influence_milli = None;
    current.set_estates(estates).unwrap();
    let evaluation = current
        .estates()
        .property_effects_with_world(&current, country, "tax")
        .unwrap();
    assert_eq!(
        evaluation
            .contributions
            .iter()
            .filter(|contribution| contribution.source.contains(":conditional:"))
            .map(|contribution| contribution.micros)
            .collect::<Vec<_>>(),
        Vec::<i64>::new()
    );
    assert!(evaluation.unresolved.iter().any(|reason| {
        reason.contains("conditional:1:unknown_scale")
            || reason.contains("conditional:2:condition:")
    }));

    let static_query = current.estates().property_effects(country, "tax").unwrap();
    assert!(
        static_query
            .unresolved
            .iter()
            .any(|reason| reason.contains("conditional:") && reason.contains("world_required:tax"))
    );
}

#[test]
fn conditional_estate_modifiers_reject_attribute_dependencies_on_replace_and_restore() {
    let mut current = world();
    let mut estates = current.estates().clone();
    estates
        .privileges
        .get_mut("charter")
        .unwrap()
        .effect
        .conditional_modifiers = vec![augustus_core::EstateConditionalModifier {
        condition: augustus_core::Condition::Always(false),
        scale: augustus_core::EstateConditionalScale::Fixed,
        modifier: effect(100_000).modifier,
        unresolved_properties: Default::default(),
    }];
    current.set_estates(estates.clone()).unwrap();
    let before = current.clone();
    let saved = current.save("estate-dependencies").unwrap();

    for comparison in [
        augustus_core::Condition::NumericCompare {
            target: Target::Current,
            slot: augustus_core::NumericSlot::Attribute {
                name: "annual".into(),
                divisor: 1,
            },
            comparison: augustus_core::NumericComparison::AtLeast,
            value: 0,
        },
        augustus_core::Condition::NumericCompareEntities {
            first: augustus_core::EntityRef::Relative(Target::Current),
            second: augustus_core::EntityRef::Relative(Target::Current),
            slot: augustus_core::NumericSlot::Attribute {
                name: "annual".into(),
                divisor: 1,
            },
            comparison: augustus_core::NumericComparison::Equal,
        },
    ] {
        let condition = augustus_core::Condition::All(vec![
            augustus_core::Condition::Always(false),
            augustus_core::Condition::Not(Box::new(augustus_core::Condition::AnyOwnedProvince {
                target: Target::Current,
                condition: Box::new(comparison),
            })),
        ]);
        let mut invalid = estates.clone();
        invalid
            .privileges
            .get_mut("charter")
            .unwrap()
            .effect
            .conditional_modifiers[0]
            .condition = condition.clone();
        let error = current.set_estates(invalid).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("cannot depend on another attribute")
        );
        assert_eq!(current, before);

        let mut corrupt: serde_json::Value = serde_json::from_str(&saved).unwrap();
        corrupt["estates"]["privileges"]["charter"]["effect"]["conditional_modifiers"][0]["condition"] =
            serde_json::to_value(condition).unwrap();
        assert!(
            World::restore(
                &corrupt.to_string(),
                "estate-dependencies",
                current.schema()
            )
            .is_err()
        );
    }
}

#[test]
fn estate_replacement_validates_conditions_against_candidate_state_and_rolls_back_overflow() {
    let mut current = world();
    let country = EntityId(0);
    let mut estates = current.estates().clone();
    estates
        .countries
        .get_mut(&country)
        .unwrap()
        .get_mut("guild")
        .unwrap()
        .privileges
        .as_mut()
        .unwrap()
        .clear();
    current.set_estates(estates.clone()).unwrap();
    let before = current.clone();
    estates
        .countries
        .get_mut(&country)
        .unwrap()
        .get_mut("guild")
        .unwrap()
        .privileges
        .as_mut()
        .unwrap()
        .insert("charter".into());
    estates
        .privileges
        .get_mut("charter")
        .unwrap()
        .effect
        .conditional_modifiers = vec![augustus_core::EstateConditionalModifier {
        condition: augustus_core::Condition::EstatePrivilege {
            target: Target::Current,
            privilege: "charter".into(),
        },
        scale: augustus_core::EstateConditionalScale::Fixed,
        modifier: effect(i64::MAX).modifier,
        unresolved_properties: Default::default(),
    }];
    assert!(matches!(
        current.set_estates(estates),
        Err(augustus_core::Error::Overflow)
    ));
    assert_eq!(current, before);
}

#[test]
fn component_replacement_preserves_registries_used_by_estate_conditions() {
    use augustus_core::{
        Condition, GovernmentMetric, GovernmentState, Governments, MetricDefinition, Metrics,
        NumericComparison, NumericSlot,
    };
    let mut current = world();
    let country = EntityId(0);
    current
        .set_governments(Governments {
            metrics: [(
                "authority".into(),
                GovernmentMetric {
                    minimum_milli: 0,
                    maximum_milli: 100_000,
                    baseline_milli: 0,
                    divisor_milli: 100_000,
                    modifier: None,
                    unresolved_properties: Default::default(),
                },
            )]
            .into(),
            countries: [(
                country,
                GovernmentState {
                    kind: Some("civic".into()),
                    reforms: Some(Default::default()),
                    metrics: Some([("authority".into(), Some(50_000))].into()),
                    ..Default::default()
                },
            )]
            .into(),
            ..Default::default()
        })
        .unwrap();
    current
        .set_metrics(Metrics {
            definitions: [(
                "capacity".into(),
                MetricDefinition {
                    kind: EntityKind::Country,
                    minimum: 0,
                    maximum: 100,
                    baseline: 0,
                    divisor: 100,
                    modifier: None,
                    unresolved_properties: Default::default(),
                    below_baseline: None,
                    growth_ceiling: false,
                },
            )]
            .into(),
            entities: [(country, [("capacity".into(), Some(50))].into())].into(),
            ..Default::default()
        })
        .unwrap();
    let mut estates = current.estates().clone();
    estates
        .privileges
        .get_mut("charter")
        .unwrap()
        .effect
        .conditional_modifiers = vec![augustus_core::EstateConditionalModifier {
        condition: Condition::All(vec![
            Condition::NumericCompare {
                target: Target::Current,
                slot: NumericSlot::Government("authority".into()),
                comparison: NumericComparison::AtLeast,
                value: 50_000,
            },
            Condition::NumericCompare {
                target: Target::Current,
                slot: NumericSlot::Metric("capacity".into()),
                comparison: NumericComparison::AtLeast,
                value: 50,
            },
        ]),
        scale: augustus_core::EstateConditionalScale::Fixed,
        modifier: effect(100_000).modifier,
        unresolved_properties: Default::default(),
    }];
    current.set_estates(estates).unwrap();
    let before = current.clone();
    assert!(
        current
            .set_governments(Governments::default())
            .unwrap_err()
            .to_string()
            .contains("government metric")
    );
    assert_eq!(current, before);
    assert!(
        current
            .set_metrics(Metrics::default())
            .unwrap_err()
            .to_string()
            .contains("metric")
    );
    assert_eq!(current, before);
    let saved = current.save("estate-registries").unwrap();
    assert_eq!(
        World::restore(&saved, "estate-registries", current.schema()).unwrap(),
        current
    );
}
