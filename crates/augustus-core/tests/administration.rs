// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    AccrualRule, Action, Administration, AdministrativeCategory, AreaStatus, AttributeBase,
    AttributeChannel, AttributeFormula, AttributeTerm, Attributes, Calendar, CoreType,
    CountryResources, EntityId, EntityKind, FieldId, MONTHLY_AUTONOMY_DELTA_ATTRIBUTE,
    MetricDefinition, Metrics, MonthlyAccruals, ProvinceAdministration, Schema, Target, Taxation,
    World, WorldDate,
};

fn world() -> World {
    let mut world = World::new(Schema::new(["base"]).unwrap());
    let a = world.add_entity("A", EntityKind::Country);
    let b = world.add_entity("B", EntityKind::Country);
    let capital = world.add_entity("Capital", EntityKind::Province);
    let border = world.add_entity("Border", EntityKind::Province);
    world.set_owner(capital, a).unwrap();
    world.set_owner(border, a).unwrap();
    world.set_capital(a, capital).unwrap();
    let administration = Administration {
        areas: ["shared".into()].into_iter().collect(),
        regions: Default::default(),
        superregions: Default::default(),
        continents: Default::default(),
        country_areas: [
            (
                a,
                [("shared".into(), AreaStatus::State)].into_iter().collect(),
            ),
            (
                b,
                [("shared".into(), AreaStatus::Territory)]
                    .into_iter()
                    .collect(),
            ),
        ]
        .into_iter()
        .collect(),
        provinces: [
            (
                capital,
                ProvinceAdministration {
                    area: Some("shared".into()),
                    cores_known: true,
                    cores: [(a, CoreType::Full)].into_iter().collect(),
                    raw_autonomy_micros: Some(750_000),
                    ..Default::default()
                },
            ),
            (
                border,
                ProvinceAdministration {
                    area: Some("shared".into()),
                    cores_known: true,
                    cores: [(a, CoreType::Territorial), (b, CoreType::Full)]
                        .into_iter()
                        .collect(),
                    raw_autonomy_micros: Some(100_000),
                    ..Default::default()
                },
            ),
        ]
        .into_iter()
        .collect(),
        rules: serde_json::from_str(
            r#"{
            "state_full_minimum_micros":0,"state_territorial_minimum_micros":350000,
            "state_noncore_minimum_micros":0,"territory_core_minimum_micros":800000,
            "territory_noncore_minimum_micros":800000,"capital_maximum_micros":0
        }"#,
        )
        .unwrap(),
    };
    world.set_administration(administration).unwrap();
    world
}

fn query(world: &World, entity: usize) -> augustus_core::AdministrationEvaluation {
    world
        .administration()
        .evaluate(world, EntityId(entity))
        .unwrap()
}

#[test]
fn monthly_autonomy_clamps_before_tax_and_resumes_with_identical_state() {
    let mut world = world();
    let border = EntityId(3);
    let mut administration = world.administration().clone();
    administration
        .provinces
        .get_mut(&border)
        .unwrap()
        .raw_autonomy_micros = Some(350_500);
    world.set_administration(administration).unwrap();

    let tax_formula = |base_micros| AttributeFormula {
        base: AttributeBase::Constant {
            micros: base_micros,
        },
        channels: vec![AttributeChannel {
            flat: Vec::new(),
            rates: vec![AttributeTerm::Autonomy],
        }],
        minimum_micros: None,
        maximum_micros: None,
        result_quantum_micros: None,
        unresolved: Default::default(),
    };
    let monthly_autonomy = AttributeFormula {
        base: AttributeBase::Constant { micros: 0 },
        channels: vec![AttributeChannel {
            flat: vec![AttributeTerm::Constant {
                source: "monthly_autonomy".into(),
                micros: -1_000,
            }],
            rates: Vec::new(),
        }],
        minimum_micros: None,
        maximum_micros: None,
        result_quantum_micros: None,
        unresolved: Default::default(),
    };
    world
        .set_metrics(Metrics {
            definitions: [(
                "tick".into(),
                MetricDefinition {
                    kind: EntityKind::Country,
                    minimum: 0,
                    maximum: i64::MAX,
                    baseline: 0,
                    divisor: 1,
                    modifier: None,
                    unresolved_properties: Default::default(),
                    below_baseline: None,
                    growth_ceiling: false,
                },
            )]
            .into(),
            entities: [(EntityId(0), [("tick".into(), Some(0))].into())].into(),
            ..Default::default()
        })
        .unwrap();
    world
        .set_attributes(Attributes {
            formulas: [
                (
                    EntityId(0),
                    [(
                        "monthly_income".into(),
                        AttributeFormula {
                            base: AttributeBase::Constant { micros: 0 },
                            channels: Vec::new(),
                            minimum_micros: None,
                            maximum_micros: None,
                            result_quantum_micros: None,
                            unresolved: Default::default(),
                        },
                    )]
                    .into(),
                ),
                (EntityId(2), [("tax".into(), tax_formula(0))].into()),
                (
                    border,
                    [
                        (MONTHLY_AUTONOMY_DELTA_ATTRIBUTE.into(), monthly_autonomy),
                        ("tax".into(), tax_formula(1_200_000_000)),
                    ]
                    .into(),
                ),
            ]
            .into(),
            ..Default::default()
        })
        .unwrap();
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
        .set_country_resources(
            [(
                EntityId(0),
                CountryResources {
                    treasury_milli: Some(0),
                    ..Default::default()
                },
            )]
            .into(),
        )
        .unwrap();
    world
        .set_monthly_accruals(MonthlyAccruals {
            rules: [(
                EntityId(0),
                [(
                    "tick".into(),
                    AccrualRule {
                        attribute: "monthly_income".into(),
                        divisor: 1,
                    },
                )]
                .into(),
            )]
            .into(),
            ..Default::default()
        })
        .unwrap();
    world
        .set_taxation(Taxation::new("tax", [EntityId(0)].into()))
        .unwrap();

    let mut overflow = world.clone();
    let mut overflow_attributes = overflow.attributes().clone();
    for province in [EntityId(2), border] {
        overflow_attributes
            .formulas
            .get_mut(&province)
            .unwrap()
            .get_mut("tax")
            .unwrap()
            .base = AttributeBase::Field {
            field: FieldId(0),
            micros_per_unit: 1,
        };
        overflow
            .schedule(
                province,
                20,
                vec![Action::Set {
                    target: Target::Current,
                    field: FieldId(0),
                    value: i64::MAX,
                }],
            )
            .unwrap();
    }
    overflow.set_attributes(overflow_attributes).unwrap();
    assert!(overflow.advance_to(20).is_err());
    assert_eq!(
        overflow.administration().provinces[&border].raw_autonomy_micros,
        Some(350_500)
    );

    world.advance_to(20).unwrap();
    assert_eq!(
        world.administration().provinces[&border].raw_autonomy_micros,
        Some(350_000)
    );
    assert_eq!(query(&world, 3).effective_autonomy_micros, Some(350_000));
    assert_eq!(
        world
            .taxation()
            .unwrap()
            .last_settlement
            .as_ref()
            .unwrap()
            .accounts[&EntityId(0)]
            .income_milli,
        Some(65_000)
    );

    let saved = world.save("monthly-autonomy").unwrap();
    let mut resumed = World::restore(&saved, "monthly-autonomy", world.schema()).unwrap();
    world.advance_to(51).unwrap();
    resumed.advance_to(51).unwrap();
    assert_eq!(world, resumed);
    assert_eq!(
        world.administration().provinces[&border].raw_autonomy_micros,
        Some(350_000)
    );
    assert_eq!(
        world
            .taxation()
            .unwrap()
            .last_settlement
            .as_ref()
            .unwrap()
            .accounts[&EntityId(0)]
            .income_milli,
        Some(65_000)
    );
}

#[test]
fn geographic_membership_is_shared_live_and_validated_on_replace_and_restore() {
    let mut world = world();
    let mut administration = world.administration().clone();
    administration.areas.insert("remote".into());
    administration.regions = [
        ("home".into(), ["shared".into()].into_iter().collect()),
        ("away".into(), ["remote".into()].into_iter().collect()),
    ]
    .into_iter()
    .collect();
    administration.superregions = [("continent".into(), ["home".into()].into_iter().collect())]
        .into_iter()
        .collect();
    world.set_administration(administration.clone()).unwrap();
    assert_eq!(
        world.administration().in_region(EntityId(3), "home"),
        Some(true)
    );
    assert_eq!(
        world.administration().in_region(EntityId(3), "away"),
        Some(false)
    );
    assert_eq!(
        world
            .administration()
            .in_superregion(EntityId(3), "continent"),
        Some(true)
    );
    assert_eq!(
        world.administration().in_region(EntityId(3), "missing"),
        None
    );
    assert_eq!(world.administration().in_region(EntityId(0), "home"), None);
    let save = world.save("geography").unwrap();
    assert_eq!(
        world,
        World::restore(&save, "geography", world.schema()).unwrap()
    );
    administration.provinces.get_mut(&EntityId(3)).unwrap().area = Some("remote".into());
    world.set_administration(administration.clone()).unwrap();
    assert_eq!(
        world
            .administration()
            .in_superregion(EntityId(3), "continent"),
        Some(false)
    );
    let before = world.clone();
    for invalid in 0..3 {
        let mut invalid_groups = administration.clone();
        match invalid {
            0 => {
                invalid_groups
                    .regions
                    .get_mut("away")
                    .unwrap()
                    .insert("shared".into());
            }
            1 => {
                invalid_groups
                    .regions
                    .get_mut("home")
                    .unwrap()
                    .insert("missing".into());
            }
            _ => {
                invalid_groups
                    .superregions
                    .get_mut("continent")
                    .unwrap()
                    .insert("missing".into());
            }
        }
        assert!(world.set_administration(invalid_groups).is_err());
        assert_eq!(world, before);
    }
    let mut corrupt: serde_json::Value = serde_json::from_str(&save).unwrap();
    corrupt["administration"]["superregions"]["continent"] = serde_json::json!(["missing"]);
    assert!(World::restore(&corrupt.to_string(), "geography", world.schema()).is_err());
}

fn administrative_attributes() -> Attributes {
    let sources = [
        (AdministrativeCategory::StateFullCore, "full", 200_000),
        (
            AdministrativeCategory::StateTerritorialCore,
            "partial",
            -250_000,
        ),
        (AdministrativeCategory::StateNonCore, "unclaimed", -100_000),
        (AdministrativeCategory::TerritoryCore, "territory", -500_000),
        (
            AdministrativeCategory::TerritoryNonCore,
            "frontier",
            -750_000,
        ),
    ];
    let term = AttributeTerm::Administration {
        mapping: "claims".into(),
        property: "yield_rate".into(),
    };
    let formula = AttributeFormula {
        base: AttributeBase::Constant { micros: 4_000_000 },
        channels: vec![AttributeChannel {
            flat: Vec::new(),
            rates: vec![term],
        }],
        minimum_micros: None,
        maximum_micros: None,
        result_quantum_micros: None,
        unresolved: Default::default(),
    };
    Attributes {
        administration_mappings: [(
            "claims".into(),
            sources
                .iter()
                .map(|&(category, name, _)| (category, name.into()))
                .collect(),
        )]
        .into_iter()
        .collect(),
        static_modifiers: sources
            .iter()
            .map(|&(_, name, rate)| {
                (
                    name.into(),
                    augustus_core::ModifierDefinition {
                        values_micros: [("yield_rate".into(), rate)].into_iter().collect(),
                        flags: Default::default(),
                    },
                )
            })
            .collect(),
        formulas: [EntityId(2), EntityId(3)]
            .into_iter()
            .map(|entity| {
                (
                    entity,
                    [("yield".into(), formula.clone())].into_iter().collect(),
                )
            })
            .collect(),
        ..Default::default()
    }
}

fn yield_result(world: &World) -> augustus_core::AttributeEvaluation {
    world
        .attributes()
        .evaluate(world, EntityId(3), "yield")
        .unwrap()
}

#[test]
fn administrative_effects_follow_claims_status_and_ownership_before_autonomy() {
    let mut world = world();
    let mut attributes = administrative_attributes();
    attributes
        .formulas
        .get_mut(&EntityId(3))
        .unwrap()
        .get_mut("yield")
        .unwrap()
        .channels
        .push(AttributeChannel {
            flat: Vec::new(),
            rates: vec![AttributeTerm::Autonomy],
        });
    world.set_attributes(attributes).unwrap();
    assert_eq!(
        query(&world, 3).category(),
        Some(AdministrativeCategory::StateTerritorialCore)
    );
    let initial = yield_result(&world);
    assert_eq!(initial.steps[0].output_micros, Some(3_000_000));
    assert_eq!(initial.steps[0].contributions[0].source, "static:partial");
    assert_eq!(initial.value_micros, Some(1_950_000));
    world.set_owner(EntityId(3), EntityId(1)).unwrap();
    assert_eq!(
        query(&world, 3).category(),
        Some(AdministrativeCategory::TerritoryCore)
    );
    assert_eq!(yield_result(&world).value_micros, Some(400_000));
    let mut administration = world.administration().clone();
    administration
        .provinces
        .get_mut(&EntityId(3))
        .unwrap()
        .cores
        .remove(&EntityId(1));
    world.set_administration(administration.clone()).unwrap();
    assert_eq!(
        query(&world, 3).category(),
        Some(AdministrativeCategory::TerritoryNonCore)
    );
    assert_eq!(yield_result(&world).value_micros, Some(200_000));
    administration
        .country_areas
        .get_mut(&EntityId(1))
        .unwrap()
        .insert("shared".into(), AreaStatus::State);
    world.set_administration(administration.clone()).unwrap();
    assert_eq!(
        query(&world, 3).category(),
        Some(AdministrativeCategory::StateNonCore)
    );
    assert_eq!(yield_result(&world).value_micros, Some(3_240_000));
    administration
        .provinces
        .get_mut(&EntityId(3))
        .unwrap()
        .cores
        .insert(EntityId(1), CoreType::Full);
    world.set_administration(administration).unwrap();
    assert_eq!(
        query(&world, 3).category(),
        Some(AdministrativeCategory::StateFullCore)
    );
    assert_eq!(yield_result(&world).value_micros, Some(4_320_000));
}

#[test]
fn administrative_category_does_not_require_known_autonomy_but_requires_known_claims() {
    let mut world = world();
    world.set_attributes(administrative_attributes()).unwrap();
    let mut administration = world.administration().clone();
    administration
        .provinces
        .get_mut(&EntityId(3))
        .unwrap()
        .raw_autonomy_micros = None;
    world.set_administration(administration.clone()).unwrap();
    assert_eq!(yield_result(&world).value_micros, Some(3_000_000));
    administration
        .provinces
        .get_mut(&EntityId(3))
        .unwrap()
        .cores
        .insert(EntityId(0), CoreType::Unspecified);
    world.set_administration(administration.clone()).unwrap();
    let unknown = yield_result(&world);
    assert_eq!(unknown.value_micros, None);
    assert!(
        unknown
            .unresolved
            .contains("administration:unknown_category")
    );
    administration
        .country_areas
        .get_mut(&EntityId(0))
        .unwrap()
        .insert("shared".into(), AreaStatus::Territory);
    world.set_administration(administration.clone()).unwrap();
    assert_eq!(yield_result(&world).value_micros, Some(2_000_000));
    let province = administration.provinces.get_mut(&EntityId(3)).unwrap();
    province.cores.clear();
    province.cores_known = false;
    world.set_administration(administration.clone()).unwrap();
    assert_eq!(yield_result(&world).value_micros, None);
    administration
        .provinces
        .get_mut(&EntityId(3))
        .unwrap()
        .cores_known = true;
    administration.country_areas.clear();
    world.set_administration(administration).unwrap();
    assert_eq!(yield_result(&world).value_micros, None);
}

#[test]
fn only_selected_administrative_sources_can_introduce_missing_properties() {
    let mut world = world();
    let mut attributes = administrative_attributes();
    attributes
        .static_modifiers
        .get_mut("territory")
        .unwrap()
        .values_micros
        .clear();
    attributes.static_unresolved_properties.insert(
        "territory".into(),
        ["yield_rate".into()].into_iter().collect(),
    );
    world.set_attributes(attributes).unwrap();
    assert_eq!(yield_result(&world).value_micros, Some(3_000_000));
    world.set_owner(EntityId(3), EntityId(1)).unwrap();
    assert!(
        yield_result(&world)
            .unresolved
            .contains("administration:unresolved_property:territory:yield_rate")
    );
    let mut attributes = world.attributes().clone();
    attributes.static_modifiers.remove("territory");
    attributes.static_unresolved_properties.clear();
    world.set_attributes(attributes).unwrap();
    assert!(
        yield_result(&world)
            .unresolved
            .contains("administration:missing_definition:territory")
    );
    let before = world.clone();
    let mut invalid = world.attributes().clone();
    invalid.administration_mappings.clear();
    assert!(world.set_attributes(invalid).is_err());
    assert_eq!(world, before);
    let mut invalid = world.attributes().clone();
    invalid.static_unresolved_properties.insert(
        "missing".into(),
        ["yield_rate".into()].into_iter().collect(),
    );
    assert!(world.set_attributes(invalid).is_err());
    assert_eq!(world, before);
    let mut invalid = world.attributes().clone();
    invalid
        .static_unresolved_properties
        .insert("full".into(), ["yield_rate".into()].into_iter().collect());
    assert!(world.set_attributes(invalid).is_err());
    assert_eq!(world, before);
}

#[test]
fn administrative_effects_reach_monthly_credits_and_resume_identically() {
    let mut world = world();
    world.set_attributes(administrative_attributes()).unwrap();
    world
        .set_calendar(Calendar {
            start: WorldDate {
                year: 1000,
                month: 11,
                day: 11,
            },
            month_lengths: [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31],
        })
        .unwrap();
    world
        .set_country_resources(
            [
                (
                    EntityId(0),
                    CountryResources {
                        treasury_milli: Some(50_000),
                        ..Default::default()
                    },
                ),
                (
                    EntityId(1),
                    CountryResources {
                        treasury_milli: Some(0),
                        ..Default::default()
                    },
                ),
            ]
            .into_iter()
            .collect(),
        )
        .unwrap();
    world
        .set_taxation(Taxation::new("yield", [EntityId(0), EntityId(1)].into()))
        .unwrap();
    world.advance_to(20).unwrap();
    assert_eq!(
        world.country_resources()[&EntityId(0)].treasury_milli,
        Some(50_650)
    );
    world.set_owner(EntityId(3), EntityId(1)).unwrap();
    let saved = world.save("administrative-yield").unwrap();
    let mut resumed = World::restore(&saved, "administrative-yield", world.schema()).unwrap();
    world.advance_to(51).unwrap();
    resumed.advance_to(51).unwrap();
    assert_eq!(
        world.country_resources()[&EntityId(0)].treasury_milli,
        Some(51_050)
    );
    assert_eq!(
        world.country_resources()[&EntityId(1)].treasury_milli,
        Some(166)
    );
    assert_eq!(world, resumed);
    assert_eq!(
        world.save("administrative-yield").unwrap(),
        resumed.save("administrative-yield").unwrap()
    );
}

#[test]
fn area_status_and_core_type_follow_current_owner_and_capital() {
    let mut world = world();
    let capital = query(&world, 2);
    assert_eq!(capital.minimum_autonomy_micros, Some(0));
    assert_eq!(capital.effective_autonomy_micros, Some(0));
    assert!(capital.is_capital_area && capital.is_capital);
    let border = query(&world, 3);
    assert_eq!(border.owner_core, Some(CoreType::Territorial));
    assert_eq!(border.minimum_autonomy_micros, Some(350_000));
    assert_eq!(border.effective_autonomy_micros, Some(350_000));
    assert!(border.is_capital_area && !border.is_capital);
    world.set_owner(EntityId(3), EntityId(1)).unwrap();
    let border = query(&world, 3);
    assert_eq!(border.area_status, Some(AreaStatus::Territory));
    assert_eq!(border.owner_core, Some(CoreType::Full));
    assert_eq!(border.effective_autonomy_micros, Some(800_000));
    assert!(!border.is_capital_area);
    world.set_capital(EntityId(1), EntityId(3)).unwrap();
    assert_eq!(query(&world, 3).effective_autonomy_micros, Some(0));
    assert!(query(&world, 3).is_capital);
    world.set_owner(EntityId(3), EntityId(0)).unwrap();
    assert!(!query(&world, 3).is_capital); // A stale capital reference cannot confer a cap.
    assert_eq!(query(&world, 3).effective_autonomy_micros, Some(350_000));
}

#[test]
fn unknown_core_type_state_and_raw_value_do_not_become_zero() {
    let mut world = world();
    let mut administration = world.administration().clone();
    administration
        .provinces
        .get_mut(&EntityId(3))
        .unwrap()
        .cores
        .insert(EntityId(0), CoreType::Unspecified);
    world.set_administration(administration.clone()).unwrap();
    assert_eq!(query(&world, 3).effective_autonomy_micros, None);
    assert!(query(&world, 3).unresolved.contains("core_type"));
    administration
        .country_areas
        .get_mut(&EntityId(0))
        .unwrap()
        .insert("shared".into(), AreaStatus::Territory);
    world.set_administration(administration.clone()).unwrap();
    // The territory floor is known independently of which kind of core is present.
    assert_eq!(query(&world, 3).effective_autonomy_micros, Some(800_000));
    administration
        .provinces
        .get_mut(&EntityId(3))
        .unwrap()
        .raw_autonomy_micros = None;
    world.set_administration(administration.clone()).unwrap();
    assert_eq!(query(&world, 3).effective_autonomy_micros, None);
    assert!(query(&world, 3).unresolved.contains("initial_autonomy"));
    administration.country_areas.clear();
    administration
        .provinces
        .get_mut(&EntityId(2))
        .unwrap()
        .raw_autonomy_micros = None;
    world.set_administration(administration).unwrap();
    assert_eq!(query(&world, 3).area_status, None);
    assert_eq!(query(&world, 3).minimum_autonomy_micros, None);
    // A zero capital ceiling determines the selected rule result even with missing inputs.
    assert_eq!(query(&world, 2).effective_autonomy_micros, Some(0));
    assert!(query(&world, 2).unresolved.contains("area_status"));
}

#[test]
fn autonomy_is_a_separate_live_factor_and_unknown_factor_blocks_subtotal() {
    let mut world = world();
    let formula = AttributeFormula {
        base: AttributeBase::Constant { micros: 4_000_000 },
        channels: vec![
            AttributeChannel {
                flat: Vec::new(),
                rates: vec![AttributeTerm::Constant {
                    source: "bonus".into(),
                    micros: 250_000,
                }],
            },
            AttributeChannel {
                flat: Vec::new(),
                rates: vec![AttributeTerm::Autonomy],
            },
        ],
        minimum_micros: None,
        maximum_micros: None,
        result_quantum_micros: None,
        unresolved: Default::default(),
    };
    world
        .set_attributes(Attributes {
            formulas: [(
                EntityId(3),
                [("yield".into(), formula)].into_iter().collect(),
            )]
            .into_iter()
            .collect(),
            ..Default::default()
        })
        .unwrap();
    let evaluate = |w: &World| w.attributes().evaluate(w, EntityId(3), "yield").unwrap();
    assert_eq!(evaluate(&world).value_micros, Some(3_250_000));
    assert_eq!(evaluate(&world).steps[1].contributions[0].micros, -350_000);
    world.set_owner(EntityId(3), EntityId(1)).unwrap();
    assert_eq!(evaluate(&world).value_micros, Some(1_000_000));
    let mut administration = world.administration().clone();
    administration
        .provinces
        .get_mut(&EntityId(3))
        .unwrap()
        .raw_autonomy_micros = None;
    world.set_administration(administration.clone()).unwrap();
    assert_eq!(evaluate(&world).known_subtotal_micros, None);
    assert_eq!(evaluate(&world).steps[0].output_micros, Some(5_000_000));
    assert_eq!(evaluate(&world).steps[1].output_micros, None);
    assert_eq!(evaluate(&world).steps[1].input_micros, Some(5_000_000));
    assert_eq!(
        world
            .attributes()
            .sum_owned(&world, EntityId(1), "yield")
            .unwrap()
            .value_micros,
        None
    );
    administration
        .provinces
        .get_mut(&EntityId(3))
        .unwrap()
        .raw_autonomy_micros = Some(900_000);
    world.set_administration(administration).unwrap();
    assert_eq!(evaluate(&world).value_micros, Some(500_000));
    let saved = world.save("admin").unwrap();
    assert_eq!(
        world,
        World::restore(&saved, "admin", world.schema()).unwrap()
    );
    assert_eq!(saved, world.save("admin").unwrap());
}

#[test]
fn invalid_replacement_and_corrupt_saved_references_are_rejected() {
    let mut world = world();
    let before = world.clone();
    for invalid in 0..5 {
        let mut administration = world.administration().clone();
        match invalid {
            0 => administration.rules.territory_core_minimum_micros = Some(1_000_001),
            1 => {
                administration.provinces.get_mut(&EntityId(3)).unwrap().area =
                    Some("missing".into())
            }
            2 => {
                administration
                    .provinces
                    .get_mut(&EntityId(3))
                    .unwrap()
                    .cores
                    .insert(EntityId(2), CoreType::Full);
            }
            3 => {
                administration
                    .provinces
                    .get_mut(&EntityId(3))
                    .unwrap()
                    .raw_autonomy_micros = Some(-1)
            }
            _ => {
                administration
                    .provinces
                    .get_mut(&EntityId(3))
                    .unwrap()
                    .cores_known = false
            }
        }
        assert!(world.set_administration(administration).is_err());
        assert_eq!(world, before);
    }
    let mut saved: serde_json::Value = serde_json::from_str(&world.save("admin").unwrap()).unwrap();
    saved["administration"]["provinces"]["3"]["cores"]["2"] = serde_json::json!("full");
    assert!(World::restore(&saved.to_string(), "admin", world.schema()).is_err());
    saved.as_object_mut().unwrap().remove("administration");
    let old = World::restore(&saved.to_string(), "admin", world.schema()).unwrap();
    assert!(old.administration().is_empty());
    assert!(!old.save("admin").unwrap().contains("administration"));
}
