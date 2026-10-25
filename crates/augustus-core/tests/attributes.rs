// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::{BTreeMap, BTreeSet};

use augustus_core::{
    Action, AffiliationDimension, Affiliations, AttributeBase, AttributeChannel, AttributeFormula,
    AttributeTerm, Attributes, EntityId, EntityKind, FieldId, GovernmentReform, GovernmentState,
    Governments, ModifierDefinition, ModifierInstance, Modifiers, Politics, Schema, World,
};

fn world() -> World {
    let mut world = World::new(Schema::new(["base"]).unwrap());
    world.add_entity("A", EntityKind::Country);
    world.add_entity("B", EntityKind::Country);
    let p = world.add_entity("North", EntityKind::Province);
    let q = world.add_entity("East", EntityKind::Province);
    world.set_owner(p, EntityId(0)).unwrap();
    world.set_owner(q, EntityId(0)).unwrap();
    world.set(p, FieldId(0), 4000).unwrap();
    world.set(q, FieldId(0), 6000).unwrap();
    world
}

fn constant(source: &str, micros: i64) -> AttributeTerm {
    AttributeTerm::Constant {
        source: source.into(),
        micros,
    }
}

fn formula(base: AttributeBase) -> AttributeFormula {
    AttributeFormula {
        base,
        channels: Vec::new(),
        minimum_micros: None,
        maximum_micros: None,
        result_quantum_micros: None,
        unresolved: Default::default(),
    }
}

fn attributes(formula: AttributeFormula) -> Attributes {
    Attributes {
        formulas: [(
            EntityId(2),
            [("yield".into(), formula)].into_iter().collect(),
        )]
        .into_iter()
        .collect(),
        ..Default::default()
    }
}

fn evaluate(world: &World) -> augustus_core::AttributeEvaluation {
    world
        .attributes()
        .evaluate(world, EntityId(2), "yield")
        .unwrap()
}

fn free_city_world() -> (
    World,
    EntityId,
    EntityId,
    EntityId,
    EntityId,
    EntityId,
    EntityId,
) {
    let mut world = World::new(Schema::new(["base"]).unwrap());
    let emperor = world.add_entity("Emperor", EntityKind::Country);
    let free_city = world.add_entity("Free City", EntityKind::Country);
    let outside_city = world.add_entity("Outside City", EntityKind::Country);
    let successor = world.add_entity("Successor", EntityKind::Country);
    let free_city_province = world.add_entity("Free City Province", EntityKind::Province);
    let outside_province = world.add_entity("Outside Province", EntityKind::Province);
    let successor_province = world.add_entity("Successor Province", EntityKind::Province);
    world.set_owner(free_city_province, free_city).unwrap();
    world.set_owner(outside_province, outside_city).unwrap();
    world.set_owner(successor_province, successor).unwrap();

    let free_city_reform: BTreeSet<String> = ["free_city".into()].into_iter().collect();
    world
        .set_governments(Governments {
            reforms: [("free_city".into(), GovernmentReform::default())]
                .into_iter()
                .collect(),
            countries: [
                (
                    emperor,
                    GovernmentState {
                        reforms: Some(BTreeSet::new()),
                        ..GovernmentState::default()
                    },
                ),
                (
                    free_city,
                    GovernmentState {
                        reforms: Some(free_city_reform.clone()),
                        ..GovernmentState::default()
                    },
                ),
                (
                    outside_city,
                    GovernmentState {
                        reforms: Some(free_city_reform),
                        ..GovernmentState::default()
                    },
                ),
                (
                    successor,
                    GovernmentState {
                        reforms: Some(BTreeSet::new()),
                        ..GovernmentState::default()
                    },
                ),
            ]
            .into_iter()
            .collect(),
            ..Governments::default()
        })
        .unwrap();

    let mut membership = AffiliationDimension::default();
    membership.groups.insert("hre".into(), None);
    membership
        .members
        .insert(free_city_province, Some("hre".into()));
    membership
        .members
        .insert(successor_province, Some("hre".into()));
    membership.absent_members.insert(outside_province);
    world
        .set_affiliations(Affiliations(
            [("imperial_membership".into(), membership)]
                .into_iter()
                .collect(),
        ))
        .unwrap();

    let mut politics = Politics::default();
    politics.offices.insert("imperial_emperor".into(), emperor);
    world.set_politics(politics).unwrap();
    (
        world,
        emperor,
        free_city,
        outside_city,
        successor,
        free_city_province,
        outside_province,
    )
}

fn set_free_city_tax_attribute(world: &mut World, countries: &[EntityId]) {
    let counted = AttributeTerm::OfficeScoped {
        office: "imperial_emperor".into(),
        term: Box::new(AttributeTerm::CountedStaticModifier {
            definition: "free_cities_in_hre".into(),
            property: "global_tax_income".into(),
            reform: "free_city".into(),
            affiliation_dimension: "imperial_membership".into(),
            affiliation_member: "hre".into(),
        }),
    };
    let mut formula = formula(AttributeBase::Constant { micros: 0 });
    formula.channels.push(AttributeChannel {
        flat: vec![counted],
        rates: Vec::new(),
    });
    let definition = ModifierDefinition {
        values_micros: [("global_tax_income".into(), 2_000_000)]
            .into_iter()
            .collect(),
        flags: BTreeMap::new(),
    };
    world
        .set_attributes(Attributes {
            static_modifiers: [("free_cities_in_hre".into(), definition)]
                .into_iter()
                .collect(),
            formulas: countries
                .iter()
                .copied()
                .map(|country| {
                    (
                        country,
                        [("country_annual_tax".into(), formula.clone())]
                            .into_iter()
                            .collect(),
                    )
                })
                .collect(),
            ..Attributes::default()
        })
        .unwrap();
}

fn annual_tax(world: &World, country: EntityId) -> augustus_core::AttributeEvaluation {
    world
        .attributes()
        .evaluate(world, country, "country_annual_tax")
        .unwrap()
}

#[test]
fn counted_static_country_tax_tracks_free_city_reform_hre_membership_and_emperor() {
    let (mut world, emperor, free_city, outside_city, successor, free_city_province, _) =
        free_city_world();
    set_free_city_tax_attribute(&mut world, &[emperor, free_city, outside_city, successor]);
    assert_eq!(annual_tax(&world, emperor).value_micros, Some(2_000_000));
    assert_eq!(annual_tax(&world, free_city).value_micros, Some(0));

    world
        .set_affiliation_member(free_city_province, "imperial_membership", None)
        .unwrap();
    assert_eq!(annual_tax(&world, emperor).value_micros, Some(0));
    world
        .set_affiliation_member(free_city_province, "imperial_membership", Some("hre"))
        .unwrap();

    let mut governments = world.governments().clone();
    governments.countries.get_mut(&free_city).unwrap().reforms = Some(BTreeSet::new());
    world.set_governments(governments).unwrap();
    assert_eq!(annual_tax(&world, emperor).value_micros, Some(0));

    let mut governments = world.governments().clone();
    governments.countries.get_mut(&free_city).unwrap().reforms = Some(["free_city".into()].into());
    world.set_governments(governments).unwrap();
    world
        .execute(
            successor,
            &[Action::SetOfficeHolder {
                office: "imperial_emperor".into(),
            }],
        )
        .unwrap();
    assert_eq!(annual_tax(&world, emperor).value_micros, Some(0));
    assert_eq!(annual_tax(&world, successor).value_micros, Some(2_000_000));
}

#[test]
fn counted_static_country_tax_matches_twelve_hre_free_cities_and_updates_live() {
    let (mut world, emperor, first_city, outside_city, successor, first_province, _) =
        free_city_world();
    let free_city_reforms: BTreeSet<String> = ["free_city".into()].into_iter().collect();
    let mut cities = vec![(first_city, first_province)];
    let mut governments = world.governments().clone();
    let mut affiliations = world.affiliations().clone();
    let imperial = affiliations.0.get_mut("imperial_membership").unwrap();

    for index in 1..12 {
        let country = world.add_entity(format!("Free City {index}"), EntityKind::Country);
        let province =
            world.add_entity(format!("Free City Province {index}"), EntityKind::Province);
        world.set_owner(province, country).unwrap();
        governments.countries.insert(
            country,
            GovernmentState {
                reforms: Some(free_city_reforms.clone()),
                ..GovernmentState::default()
            },
        );
        imperial.members.insert(province, Some("hre".into()));
        cities.push((country, province));
    }

    let second_province = world.add_entity("Second Free City Province", EntityKind::Province);
    world.set_owner(second_province, first_city).unwrap();
    imperial.members.insert(second_province, Some("hre".into()));
    world.set_governments(governments).unwrap();
    world.set_affiliations(affiliations).unwrap();

    let tax_countries = std::iter::once(emperor)
        .chain(std::iter::once(outside_city))
        .chain(std::iter::once(successor))
        .chain(cities.iter().map(|(country, _)| *country))
        .collect::<Vec<_>>();
    set_free_city_tax_attribute(&mut world, &tax_countries);

    let mut attributes = world.attributes().clone();
    attributes.static_modifiers.insert(
        "all_nations".into(),
        ModifierDefinition {
            values_micros: [("global_tax_income".into(), 12_000_000)]
                .into_iter()
                .collect(),
            flags: BTreeMap::new(),
        },
    );
    for country in &tax_countries {
        attributes
            .formulas
            .get_mut(country)
            .unwrap()
            .get_mut("country_annual_tax")
            .unwrap()
            .channels[0]
            .flat
            .insert(
                0,
                AttributeTerm::Static {
                    definition: "all_nations".into(),
                    property: "global_tax_income".into(),
                },
            );
    }
    world.set_attributes(attributes).unwrap();

    assert_eq!(annual_tax(&world, emperor).value_micros, Some(36_000_000));

    world
        .set_affiliation_member(first_province, "imperial_membership", None)
        .unwrap();
    assert_eq!(annual_tax(&world, emperor).value_micros, Some(36_000_000));

    world
        .set_affiliation_member(second_province, "imperial_membership", None)
        .unwrap();
    assert_eq!(annual_tax(&world, emperor).value_micros, Some(34_000_000));

    world
        .set_affiliation_member(first_province, "imperial_membership", Some("hre"))
        .unwrap();
    assert_eq!(annual_tax(&world, emperor).value_micros, Some(36_000_000));
    world
        .set_affiliation_member(second_province, "imperial_membership", Some("hre"))
        .unwrap();
    assert_eq!(annual_tax(&world, emperor).value_micros, Some(36_000_000));

    let mut governments = world.governments().clone();
    governments.countries.get_mut(&first_city).unwrap().reforms = Some(BTreeSet::new());
    world.set_governments(governments).unwrap();
    assert_eq!(annual_tax(&world, emperor).value_micros, Some(34_000_000));

    let mut governments = world.governments().clone();
    governments.countries.get_mut(&first_city).unwrap().reforms = Some(free_city_reforms);
    world.set_governments(governments).unwrap();
    world
        .execute(
            successor,
            &[Action::SetOfficeHolder {
                office: "imperial_emperor".into(),
            }],
        )
        .unwrap();
    assert_eq!(annual_tax(&world, emperor).value_micros, Some(12_000_000));
    assert_eq!(annual_tax(&world, successor).value_micros, Some(36_000_000));
}

#[test]
fn counted_static_country_tax_keeps_unknown_reform_or_membership_unresolved() {
    let (mut world, emperor, free_city, _, successor, _free_city_province, _) = free_city_world();
    set_free_city_tax_attribute(&mut world, &[emperor, free_city, successor]);

    let mut governments = world.governments().clone();
    governments.countries.get_mut(&free_city).unwrap().reforms = None;
    world.set_governments(governments).unwrap();
    let reform_unknown = annual_tax(&world, emperor);
    assert_eq!(reform_unknown.known_subtotal_micros, None);
    assert_eq!(reform_unknown.value_micros, None);
    assert!(
        reform_unknown
            .unresolved
            .iter()
            .any(|reason| reason.contains("free_city:unknown_selection:Free City"))
    );

    let (mut world, emperor, free_city, _, successor, free_city_province, _) = free_city_world();
    set_free_city_tax_attribute(&mut world, &[emperor, free_city, successor]);
    let mut affiliations = world.affiliations().clone();
    let membership = affiliations.0.get_mut("imperial_membership").unwrap();
    membership.members.insert(free_city_province, None);
    membership.absent_members.remove(&free_city_province);
    world.set_affiliations(affiliations).unwrap();
    let membership_unknown = annual_tax(&world, emperor);
    assert_eq!(membership_unknown.known_subtotal_micros, None);
    assert_eq!(membership_unknown.value_micros, None);
    assert!(
        membership_unknown
            .unresolved
            .iter()
            .any(|reason| reason.contains("imperial_membership:hre:unknown_membership:Free City"))
    );
}

#[test]
fn counted_capital_static_modifier_uses_capital_membership_and_excludes_free_cities() {
    let mut world = World::new(Schema::new(["base"]).unwrap());
    let emperor = world.add_entity("Emperor", EntityKind::Country);
    let member = world.add_entity("Member", EntityKind::Country);
    let free_city = world.add_entity("Free City", EntityKind::Country);
    let outside = world.add_entity("Outside", EntityKind::Country);
    let uncertain = world.add_entity("Uncertain", EntityKind::Country);
    let emperor_capital = world.add_entity("Emperor Capital", EntityKind::Province);
    let member_capital = world.add_entity("Member Capital", EntityKind::Province);
    let free_city_capital = world.add_entity("Free City Capital", EntityKind::Province);
    let outside_capital = world.add_entity("Outside Capital", EntityKind::Province);
    let outside_hre_province = world.add_entity("Outside HRE Province", EntityKind::Province);
    let uncertain_capital = world.add_entity("Uncertain Capital", EntityKind::Province);

    for (country, province) in [
        (emperor, emperor_capital),
        (member, member_capital),
        (free_city, free_city_capital),
        (outside, outside_capital),
        (outside, outside_hre_province),
        (uncertain, uncertain_capital),
    ] {
        world.set_owner(province, country).unwrap();
    }
    for (country, capital) in [
        (emperor, emperor_capital),
        (member, member_capital),
        (free_city, free_city_capital),
        (outside, outside_capital),
        (uncertain, uncertain_capital),
    ] {
        world.set_capital(country, capital).unwrap();
    }

    let free_city_reforms: BTreeSet<String> = ["free_city".into()].into_iter().collect();
    world
        .set_governments(Governments {
            reforms: [("free_city".into(), GovernmentReform::default())]
                .into_iter()
                .collect(),
            countries: [
                (
                    emperor,
                    GovernmentState {
                        reforms: Some(BTreeSet::new()),
                        ..Default::default()
                    },
                ),
                (
                    member,
                    GovernmentState {
                        reforms: Some(BTreeSet::new()),
                        ..Default::default()
                    },
                ),
                (
                    free_city,
                    GovernmentState {
                        reforms: Some(free_city_reforms),
                        ..Default::default()
                    },
                ),
                (
                    outside,
                    GovernmentState {
                        reforms: Some(BTreeSet::new()),
                        ..Default::default()
                    },
                ),
                (
                    uncertain,
                    GovernmentState {
                        reforms: Some(BTreeSet::new()),
                        ..Default::default()
                    },
                ),
            ]
            .into_iter()
            .collect(),
            ..Governments::default()
        })
        .unwrap();

    let mut imperial = AffiliationDimension::default();
    imperial.groups.insert("hre".into(), None);
    for province in [
        emperor_capital,
        member_capital,
        free_city_capital,
        outside_hre_province,
    ] {
        imperial.members.insert(province, Some("hre".into()));
    }
    imperial.absent_members.insert(outside_capital);
    imperial.members.insert(uncertain_capital, None);
    world
        .set_affiliations(Affiliations(
            [("imperial_membership".into(), imperial)]
                .into_iter()
                .collect(),
        ))
        .unwrap();
    world
        .set_politics(Politics {
            offices: [("imperial_emperor".into(), emperor)].into_iter().collect(),
            ..Politics::default()
        })
        .unwrap();

    let mut force_limit = formula(AttributeBase::Constant { micros: 0 });
    force_limit.channels.push(AttributeChannel {
        flat: vec![AttributeTerm::OfficeScoped {
            office: "imperial_emperor".into(),
            term: Box::new(AttributeTerm::CountedCapitalStaticModifier {
                definition: "states_in_hre".into(),
                property: "land_forcelimit".into(),
                excluded_reform: "free_city".into(),
                affiliation_dimension: "imperial_membership".into(),
                affiliation_member: "hre".into(),
            }),
        }],
        rates: Vec::new(),
    });
    world
        .set_attributes(Attributes {
            static_modifiers: [(
                "states_in_hre".into(),
                ModifierDefinition {
                    values_micros: [("land_forcelimit".into(), 500_000)].into_iter().collect(),
                    flags: BTreeMap::new(),
                },
            )]
            .into_iter()
            .collect(),
            formulas: [(
                emperor,
                [("country_land_forcelimit_flat".into(), force_limit)]
                    .into_iter()
                    .collect(),
            )]
            .into_iter()
            .collect(),
            ..Attributes::default()
        })
        .unwrap();

    let unknown_membership = world
        .attributes()
        .evaluate(&world, emperor, "country_land_forcelimit_flat")
        .unwrap();
    assert_eq!(unknown_membership.value_micros, None);
    assert!(unknown_membership.unresolved.iter().any(|reason| {
        reason.contains("imperial_membership:hre:unknown_capital_membership:Uncertain")
    }));

    world
        .set_affiliation_member(uncertain_capital, "imperial_membership", Some("hre"))
        .unwrap();
    assert_eq!(
        world
            .attributes()
            .evaluate(&world, emperor, "country_land_forcelimit_flat")
            .unwrap()
            .value_micros,
        Some(1_500_000)
    );

    world.set_capital(outside, outside_hre_province).unwrap();
    assert_eq!(
        world
            .attributes()
            .evaluate(&world, emperor, "country_land_forcelimit_flat")
            .unwrap()
            .value_micros,
        Some(2_000_000)
    );
}

#[test]
fn unknown_ownership_preserves_known_subtotals_without_declaring_complete_income() {
    let mut current = world();
    let unknown = current.add_entity("Unresolved", EntityKind::Province);
    let mut definitions = Attributes::default();
    for (entity, value) in [
        (EntityId(2), 4_000_000),
        (EntityId(3), 6_000_000),
        (unknown, 2_000_000),
    ] {
        definitions.formulas.insert(
            entity,
            [(
                "yield".into(),
                formula(AttributeBase::Constant { micros: value }),
            )]
            .into(),
        );
    }
    current.set_attributes(definitions).unwrap();
    let snapshot = current.save("ownership-totals").unwrap();
    for country in [EntityId(0), EntityId(1)] {
        let result = current
            .attributes()
            .sum_owned(&current, country, "yield")
            .unwrap();
        assert_eq!(result.value_micros, None);
        assert!(result.unresolved.contains("unknown_province_ownership"));
    }
    let known = current
        .attributes()
        .sum_owned(&current, EntityId(0), "yield")
        .unwrap();
    assert_eq!(known.province_count, 2);
    assert_eq!(known.known_subtotal_micros, 10_000_000);
    assert_eq!(current.save("ownership-totals").unwrap(), snapshot);
    let mut restored = World::restore(&snapshot, "ownership-totals", current.schema()).unwrap();
    assert_eq!(
        restored
            .attributes()
            .sum_owned(&restored, EntityId(0), "yield")
            .unwrap(),
        known
    );

    restored
        .mark_target_absent(unknown, augustus_core::Target::Owner)
        .unwrap();
    assert_eq!(
        restored
            .attributes()
            .sum_owned(&restored, EntityId(0), "yield")
            .unwrap()
            .value_micros,
        Some(10_000_000)
    );
    assert_eq!(
        restored
            .attributes()
            .sum_owned(&restored, EntityId(1), "yield")
            .unwrap()
            .value_micros,
        Some(0)
    );
    restored.set_owner(unknown, EntityId(1)).unwrap();
    assert_eq!(
        restored
            .attributes()
            .sum_owned(&restored, EntityId(1), "yield")
            .unwrap()
            .value_micros,
        Some(2_000_000)
    );
    restored.set_owner(EntityId(2), EntityId(1)).unwrap();
    restored.set_owner(EntityId(2), EntityId(0)).unwrap();
    assert_eq!(
        restored.owned_provinces(EntityId(0)).unwrap(),
        [EntityId(3), EntityId(2)]
    );
    assert_eq!(
        restored
            .attributes()
            .sum_owned(&restored, EntityId(0), "yield")
            .unwrap()
            .value_micros,
        Some(10_000_000)
    );
    let snapshot = restored.save("ownership-totals").unwrap();
    let resumed = World::restore(&snapshot, "ownership-totals", restored.schema()).unwrap();
    assert_eq!(resumed, restored);
}

#[test]
fn channels_separate_additive_rates_from_successive_multiplication_and_clamp_last() {
    let mut world = world();
    let mut formula = formula(AttributeBase::Constant { micros: 10_000_000 });
    formula.channels = vec![
        AttributeChannel {
            flat: vec![constant("flat", 2_000_000)],
            rates: vec![constant("first", 200_000), constant("second", 300_000)],
        },
        AttributeChannel {
            flat: vec![constant("after_rate", 1_000_000)],
            rates: vec![constant("independent", -200_000)],
        },
    ];
    formula.maximum_micros = Some(15_000_000);
    world.set_attributes(attributes(formula)).unwrap();
    let result = evaluate(&world);
    assert_eq!(result.steps[0].output_micros, Some(18_000_000));
    assert_eq!(result.steps[0].rate_micros, 500_000);
    assert_eq!(result.steps[1].output_micros, Some(15_200_000));
    assert_eq!(result.value_micros, Some(15_000_000));
    assert_eq!(result.steps[0].contributions[2].source, "second");
    let saved = world.save("attrs").unwrap();
    assert_eq!(
        World::restore(&saved, "attrs", world.schema()).unwrap(),
        world
    );
    assert_eq!(world.save("attrs").unwrap(), saved);
}

#[test]
fn product_of_sums_truncates_intermediate_efficiency_and_final_tax() {
    let calculate = |base_tax_milli, local_efficiency_micros, autonomy_delta_micros| {
        let mut world = world();
        world.set(EntityId(2), FieldId(0), base_tax_milli).unwrap();
        let mut formula = formula(AttributeBase::Field {
            field: FieldId(0),
            micros_per_unit: 1000,
        });
        formula.channels.push(AttributeChannel {
            flat: Vec::new(),
            rates: vec![AttributeTerm::ProductOfSums {
                source: "province_tax_efficiency_and_autonomy".into(),
                factors: vec![
                    vec![constant("local_efficiency", local_efficiency_micros)],
                    vec![
                        constant("autonomy_factor_base", 1_000_000),
                        constant("autonomy_delta", autonomy_delta_micros),
                    ],
                ],
                quantum_micros: 1000,
            }],
        });
        formula.result_quantum_micros = Some(1000);
        world.set_attributes(attributes(formula)).unwrap();
        let result = evaluate(&world);
        (world, result)
    };

    let (trieste, result) = calculate(4000, 670_000, -59_000);
    assert_eq!(result.steps[0].rate_micros, -370_000);
    assert_eq!(result.known_subtotal_micros, Some(2_520_000));
    assert_eq!(result.value_micros, Some(2_520_000));
    let saved = trieste.save("province-tax").unwrap();
    let restored = World::restore(&saved, "province-tax", trieste.schema()).unwrap();
    assert_eq!(evaluate(&restored), result);

    let (_, rounded) = calculate(1500, 1_000_000, -33_000);
    assert_eq!(rounded.steps[0].rate_micros, -33_000);
    assert_eq!(rounded.steps[0].output_micros, Some(1_450_000));
    assert!(rounded.steps[0].contributions.iter().any(|contribution| {
        contribution.source == "formula"
            && contribution.property == "result_truncation"
            && contribution.micros == -500
    }));
}

#[test]
fn world_fields_owners_buildings_goods_and_attached_modifiers_are_read_live() {
    let mut world = world();
    world
        .set_economy(
            serde_json::from_str(
                r#"{
        "buildings":{"mill":{}},"trade_goods":{"grain":{},"cloth":{}},"trade_nodes":{},
        "provinces":{"2":{"buildings":["mill"],"trade_good":"grain"}}
    }"#,
            )
            .unwrap(),
        )
        .unwrap();
    let definition = |value| ModifierDefinition {
        values_micros: [("yield.rate".into(), value)].into_iter().collect(),
        flags: BTreeMap::new(),
    };
    world
        .set_modifiers(Modifiers {
            definitions: [("boost".into(), definition(100_000))]
                .into_iter()
                .collect(),
            active: [(
                EntityId(0),
                [("boost".into(), ModifierInstance::default())]
                    .into_iter()
                    .collect(),
            )]
            .into_iter()
            .collect(),
            ..Default::default()
        })
        .unwrap();
    let mut formula = formula(AttributeBase::Field {
        field: FieldId(0),
        micros_per_unit: 1000,
    });
    formula.channels.push(AttributeChannel {
        flat: Vec::new(),
        rates: vec![
            AttributeTerm::Buildings {
                property: "yield.rate".into(),
            },
            AttributeTerm::TradeGood {
                property: "yield.rate".into(),
            },
            AttributeTerm::Modifiers {
                target: augustus_core::Target::Owner,
                property: "yield.rate".into(),
            },
        ],
    });
    let mut attributes = attributes(formula);
    attributes
        .building_modifiers
        .insert("mill".into(), definition(300_000));
    attributes
        .trade_good_modifiers
        .insert("grain".into(), definition(200_000));
    attributes
        .trade_good_modifiers
        .insert("cloth".into(), definition(-50_000));
    world.set_attributes(attributes).unwrap();
    assert_eq!(evaluate(&world).value_micros, Some(6_400_000));
    world.set(EntityId(2), FieldId(0), 5000).unwrap();
    assert_eq!(evaluate(&world).value_micros, Some(8_000_000));
    world.set_owner(EntityId(2), EntityId(1)).unwrap();
    assert_eq!(evaluate(&world).value_micros, Some(7_500_000));
    let mut economy = world.economy().clone();
    economy
        .provinces
        .get_mut(&EntityId(2))
        .unwrap()
        .buildings
        .clear();
    economy.provinces.get_mut(&EntityId(2)).unwrap().trade_good = Some("cloth".into());
    world.set_economy(economy).unwrap();
    assert_eq!(evaluate(&world).value_micros, Some(4_750_000));
    assert_eq!(
        world
            .attributes()
            .sum_owned(&world, EntityId(1), "yield")
            .unwrap()
            .value_micros,
        Some(4_750_000)
    );
    let aggregate = world
        .attributes()
        .sum_owned(&world, EntityId(0), "yield")
        .unwrap();
    assert_eq!(aggregate.province_count, 1);
    assert!(aggregate.value_micros.is_none());
    assert!(
        aggregate
            .unresolved
            .contains("province:East:missing_attribute:yield")
    );
}

#[test]
fn unknown_inputs_and_unresolved_sources_do_not_turn_partial_totals_into_complete_values() {
    let mut world = world();
    let mut known = formula(AttributeBase::Constant { micros: 3_000_000 });
    known.unresolved.insert("autonomy".into());
    let mut attributes = attributes(known);
    attributes.formulas.insert(
        EntityId(3),
        [(
            "yield".into(),
            formula(AttributeBase::Unknown {
                reason: "missing_base".into(),
            }),
        )]
        .into_iter()
        .collect(),
    );
    world.set_attributes(attributes).unwrap();
    let before = world.clone();
    assert_eq!(evaluate(&world).known_subtotal_micros, Some(3_000_000));
    assert_eq!(evaluate(&world).value_micros, None);
    let unknown = world
        .attributes()
        .evaluate(&world, EntityId(3), "yield")
        .unwrap();
    assert_eq!(unknown.known_subtotal_micros, None);
    let aggregate = world
        .attributes()
        .sum_owned(&world, EntityId(0), "yield")
        .unwrap();
    assert_eq!(aggregate.known_subtotal_micros, 3_000_000);
    assert!(aggregate.unresolved.contains("province:East:unknown_base"));
    assert_eq!(aggregate.value_micros, None);
    assert_eq!(world, before);
}

#[test]
fn invalid_formulas_and_aggregate_overflow_are_atomic_and_invalid_saves_are_rejected() {
    let mut world = world();
    let old = world.save("attrs").unwrap();
    assert!(!old.contains("attributes"));
    assert_eq!(
        World::restore(&old, "attrs", world.schema()).unwrap(),
        world
    );
    for base in [
        AttributeBase::Field {
            field: FieldId(1),
            micros_per_unit: 1,
        },
        AttributeBase::Field {
            field: FieldId(0),
            micros_per_unit: i64::MAX,
        },
    ] {
        assert!(world.set_attributes(attributes(formula(base))).is_err());
    }
    let mut invalid = attributes(formula(AttributeBase::Constant { micros: 1 }));
    invalid
        .formulas
        .get_mut(&EntityId(2))
        .unwrap()
        .get_mut("yield")
        .unwrap()
        .channels
        .push(AttributeChannel {
            flat: vec![AttributeTerm::Static {
                definition: "missing".into(),
                property: "rate".into(),
            }],
            rates: vec![],
        });
    assert!(world.set_attributes(invalid).is_err());
    let mut overflow = attributes(formula(AttributeBase::Constant { micros: i64::MAX }));
    overflow.formulas.insert(
        EntityId(3),
        [(
            "yield".into(),
            formula(AttributeBase::Constant { micros: 1 }),
        )]
        .into_iter()
        .collect(),
    );
    assert!(
        world
            .set_attributes(overflow)
            .unwrap_err()
            .to_string()
            .contains("overflow")
    );
    assert_eq!(world.save("attrs").unwrap(), old);
    world
        .set_attributes(attributes(formula(AttributeBase::Constant { micros: 1 })))
        .unwrap();
    let mut save: serde_json::Value = serde_json::from_str(&world.save("attrs").unwrap()).unwrap();
    save["attributes"]["formulas"]["2"]["yield"]["minimum_micros"] = 10.into();
    save["attributes"]["formulas"]["2"]["yield"]["maximum_micros"] = 1.into();
    assert!(World::restore(&save.to_string(), "attrs", world.schema()).is_err());
}

#[test]
fn product_of_rate_sums_truncates_the_combined_factor_before_applying_base() {
    for (local_efficiency, autonomy, expected) in [
        (670_000, -59_000, 2_520_000),
        (1_000_000, -34_000, 3_864_000),
    ] {
        let mut world = world();
        let mut formula = formula(AttributeBase::Field {
            field: FieldId(0),
            micros_per_unit: 1000,
        });
        formula.result_quantum_micros = Some(1000);
        formula.minimum_micros = Some(0);
        formula.channels.push(AttributeChannel {
            flat: Vec::new(),
            rates: vec![AttributeTerm::ProductOfSums {
                source: "province_tax_efficiency".into(),
                factors: vec![
                    vec![constant("local_tax_efficiency", local_efficiency)],
                    vec![
                        constant("autonomy_baseline", 1_000_000),
                        constant("autonomy", autonomy),
                    ],
                ],
                quantum_micros: 1000,
            }],
        });
        world.set_attributes(attributes(formula)).unwrap();
        let actual = evaluate(&world);
        assert_eq!(actual.known_subtotal_micros, Some(expected));
        assert_eq!(actual.steps[0].output_micros, Some(expected));

        let saved = world.save("tax-rate-product").unwrap();
        let restored = World::restore(&saved, "tax-rate-product", world.schema()).unwrap();
        assert_eq!(evaluate(&restored), actual);
    }
}

#[test]
fn quantized_rate_product_truncates_combined_efficiency_before_applying_base() {
    for (local_efficiency, autonomy, expected) in [
        (670_000, -59_000, 2_520_000),
        (1_000_000, -34_000, 3_864_000),
    ] {
        let mut world = world();
        let mut formula = formula(AttributeBase::Field {
            field: FieldId(0),
            micros_per_unit: 1000,
        });
        formula.result_quantum_micros = Some(1000);
        formula.channels.push(AttributeChannel {
            flat: Vec::new(),
            rates: vec![AttributeTerm::ProductOfSums {
                source: "province_tax_efficiency".into(),
                factors: vec![
                    vec![constant("local_tax_efficiency", local_efficiency)],
                    vec![
                        constant("autonomy_baseline", 1_000_000),
                        constant("autonomy", autonomy),
                    ],
                ],
                quantum_micros: 1000,
            }],
        });
        world.set_attributes(attributes(formula)).unwrap();
        let actual = evaluate(&world);
        assert_eq!(actual.known_subtotal_micros, Some(expected));
        assert_eq!(actual.steps[0].output_micros, Some(expected));

        let saved = world.save("tax-rate-product").unwrap();
        let restored = World::restore(&saved, "tax-rate-product", world.schema()).unwrap();
        assert_eq!(evaluate(&restored), actual);
    }
}
