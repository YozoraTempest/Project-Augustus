// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    Administration, AreaStatus, AttributeBase, AttributeChannel, AttributeFormula, AttributeTerm,
    Attributes, CoreType, EntityId, EntityKind, ProvinceAdministration, Schema, World,
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
