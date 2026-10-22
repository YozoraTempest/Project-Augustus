// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::BTreeMap;

use augustus_core::{
    Action, AffiliationDimension, Affiliations, ArmyId, EntityKind, EntityRef, RegimentId,
    RegimentState, Schema, ScopeContext, ScopeSelector, Technology, TechnologyLevel, UnitTemplate,
    UnitTemplateProperty, UnitTemplates, World,
};

fn fixture() -> (World, augustus_core::EntityId, augustus_core::EntityId) {
    let mut world = World::new(Schema::new(std::iter::empty::<String>()).unwrap());
    let country = world.add_entity("SWE", EntityKind::Country);
    let province = world.add_entity("1", EntityKind::Province);
    (world, country, province)
}

fn set_unit_selection_data(world: &mut World, country: augustus_core::EntityId, level: u16) {
    let mut affiliations = Affiliations::default();
    affiliations.0.insert(
        "unit_type".into(),
        AffiliationDimension {
            members: BTreeMap::from([(country, Some("western".into()))]),
            ..Default::default()
        },
    );
    world.set_affiliations(affiliations).unwrap();

    let enabled_by_level = [
        vec![],
        vec![
            "western_infantry_alternative".into(),
            "western_infantry_obsolete".into(),
            "eastern_medieval_infantry".into(),
        ],
        vec![
            "western_medieval_infantry".into(),
            "western_medieval_knights".into(),
        ],
        vec![],
        vec![],
        vec!["western_men_at_arms".into()],
    ];
    let levels = enabled_by_level
        .into_iter()
        .enumerate()
        .map(|(index, enabled_units)| TechnologyLevel {
            year: 1350 + index as i16 * 10,
            values_micros: BTreeMap::new(),
            flags: BTreeMap::new(),
            enabled_units,
            unresolved_properties: Default::default(),
        })
        .collect();
    world
        .set_technology(Technology {
            tables: BTreeMap::from([("military".into(), levels)]),
            countries: BTreeMap::from([(country, BTreeMap::from([("military".into(), level)]))]),
        })
        .unwrap();

    let template = |category: &str, group: &str| UnitTemplate {
        category: category.into(),
        properties: vec![UnitTemplateProperty {
            name: "unit_type".into(),
            value: group.into(),
        }],
    };
    world
        .set_unit_templates(UnitTemplates {
            definitions: BTreeMap::from([
                (
                    "western_medieval_infantry".into(),
                    template("infantry", "western"),
                ),
                (
                    "western_infantry_alternative".into(),
                    template("infantry", "western"),
                ),
                (
                    "western_infantry_obsolete".into(),
                    template("infantry", "western"),
                ),
                (
                    "western_medieval_knights".into(),
                    template("cavalry", "western"),
                ),
                (
                    "eastern_medieval_infantry".into(),
                    template("infantry", "eastern"),
                ),
                (
                    "western_men_at_arms".into(),
                    template("infantry", "western"),
                ),
                ("western_cannons".into(), template("artillery", "western")),
            ]),
        })
        .unwrap();
    let infantry = if level >= 5 {
        "western_men_at_arms"
    } else {
        "western_medieval_infantry"
    };
    world
        .set_selected_unit(country, "infantry", infantry)
        .unwrap();
    world
        .set_selected_unit(country, "cavalry", "western_medieval_knights")
        .unwrap();
}

#[test]
fn army_action_builds_a_stack_of_regiments_and_round_trips() {
    let (mut world, country, province) = fixture();
    let action = [Action::CreateArmy {
        owner: EntityRef::Selected(ScopeSelector::Root),
        location: EntityRef::Selected(ScopeSelector::Named {
            kind: EntityKind::Province,
            name: "1".into(),
        }),
        name: "First Army".into(),
        regiments: vec![
            "western_medieval_infantry".into(),
            "western_medieval_cavalry".into(),
        ],
    }];

    world
        .execute_in_context(ScopeContext::new(country), &action)
        .unwrap();

    assert_eq!(world.armies().len(), 1);
    assert_eq!(world.armies()[0].id, ArmyId(0));
    assert_eq!(world.armies()[0].owner, country);
    assert_eq!(world.armies()[0].location, province);
    assert_eq!(world.armies()[0].name, "First Army");
    assert_eq!(
        world.armies()[0]
            .regiments
            .iter()
            .map(|regiment| (regiment.id, regiment.unit_type.as_str()))
            .collect::<Vec<_>>(),
        vec![
            (RegimentId(0), "western_medieval_infantry"),
            (RegimentId(1), "western_medieval_cavalry"),
        ]
    );
    assert_eq!(world.armies()[0].regiments[0].strength_milli, Some(1_000));
    assert_eq!(world.armies()[0].regiments[0].morale_milli, Some(1_000));

    let saved = world.save("army-units:1").unwrap();
    let restored = World::restore(&saved, "army-units:1", world.schema()).unwrap();
    assert_eq!(restored, world);
}

#[test]
fn army_requires_a_country_a_province_and_at_least_one_regiment() {
    let (mut world, country, province) = fixture();
    assert!(world.create_army(country, province, "", &[]).is_err());
    assert!(
        world
            .create_army(country, province, "", &["bad type".into()])
            .is_err()
    );
    assert!(
        world
            .create_army(province, province, "", &["infantry".into()])
            .is_err()
    );
    assert!(
        world
            .create_army(country, country, "", &["infantry".into()])
            .is_err()
    );
    assert!(world.armies().is_empty());
}

#[test]
fn old_snapshots_without_armies_load_and_invalid_army_references_fail() {
    let (world, _, _) = fixture();
    let saved = world.save("army-units:1").unwrap();
    let mut legacy: serde_json::Value = serde_json::from_str(&saved).unwrap();
    legacy.as_object_mut().unwrap().remove("armies");
    let restored = World::restore(&legacy.to_string(), "army-units:1", world.schema()).unwrap();
    assert_eq!(restored, world);

    let (mut world, country, province) = fixture();
    world
        .create_army(country, province, "Test", &["infantry".into()])
        .unwrap();
    let saved = world.save("army-units:1").unwrap();
    let mut corrupt: serde_json::Value = serde_json::from_str(&saved).unwrap();
    corrupt["armies"]["armies"][0]["location"] = serde_json::json!(u64::MAX);
    assert!(World::restore(&corrupt.to_string(), "army-units:1", world.schema()).is_err());
}

#[test]
fn regiment_state_round_trips_and_defaults_when_loading_older_snapshots() {
    let (mut world, country, province) = fixture();
    world
        .create_army_with_regiment_states(
            country,
            province,
            "Test",
            &["infantry".into()],
            &[RegimentState {
                strength_milli: None,
                morale_milli: Some(1_234),
                home_province: Some(province),
            }],
        )
        .unwrap();
    let saved = world.save("army-units:1").unwrap();
    let restored = World::restore(&saved, "army-units:1", world.schema()).unwrap();
    assert_eq!(restored, world);
    assert_eq!(restored.armies()[0].regiments[0].strength_milli, None);
    assert_eq!(restored.armies()[0].regiments[0].morale_milli, Some(1_234));
    assert_eq!(
        restored.armies()[0].regiments[0].home_province,
        Some(province)
    );

    let mut legacy: serde_json::Value = serde_json::from_str(&saved).unwrap();
    let regiment = legacy["armies"]["armies"][0]["regiments"][0]
        .as_object_mut()
        .unwrap();
    regiment.remove("strength_milli");
    regiment.remove("morale_milli");
    regiment.remove("home_province");
    let restored = World::restore(&legacy.to_string(), "army-units:1", world.schema()).unwrap();
    assert_eq!(restored.armies()[0].regiments[0].strength_milli, None);
    assert_eq!(restored.armies()[0].regiments[0].morale_milli, None);
    assert_eq!(restored.armies()[0].regiments[0].home_province, None);

    let mut corrupt: serde_json::Value = serde_json::from_str(&saved).unwrap();
    corrupt["armies"]["armies"][0]["regiments"][0]["home_province"] = serde_json::json!(u64::MAX);
    assert!(World::restore(&corrupt.to_string(), "army-units:1", world.schema()).is_err());
}

#[test]
fn regiment_state_requires_matching_count_and_nonnegative_values() {
    let (mut world, country, province) = fixture();
    assert!(
        world
            .create_army_with_regiment_states(country, province, "Test", &["infantry".into()], &[],)
            .is_ok()
    );
    assert_eq!(world.armies()[0].regiments[0].strength_milli, None);
    assert_eq!(world.armies()[0].regiments[0].morale_milli, None);

    let (mut world, country, province) = fixture();
    assert!(
        world
            .create_army_with_regiment_states(
                country,
                province,
                "Test",
                &["infantry".into()],
                &[RegimentState::default(), RegimentState::default()],
            )
            .is_err()
    );
    assert!(
        world
            .create_army_with_regiment_states(
                country,
                province,
                "Test",
                &["infantry".into()],
                &[RegimentState {
                    strength_milli: None,
                    morale_milli: Some(-1),
                    home_province: None,
                }],
            )
            .is_err()
    );
    assert!(
        world
            .create_army_with_regiment_states(
                country,
                province,
                "Test",
                &["infantry".into()],
                &[RegimentState {
                    strength_milli: Some(-1),
                    morale_milli: None,
                    home_province: None,
                }],
            )
            .is_err()
    );
    assert!(world.armies().is_empty());
}

#[test]
fn category_effect_uses_the_explicit_country_unit_selection() {
    let (mut world, country, province) = fixture();
    set_unit_selection_data(&mut world, country, 3);
    let available = world.available_unit_templates(country).unwrap();
    assert_eq!(
        available["infantry"],
        [
            "western_infantry_alternative",
            "western_infantry_obsolete",
            "western_medieval_infantry"
        ]
    );
    assert!(!available["infantry"].contains(&"eastern_medieval_infantry".into()));
    assert_eq!(
        world.selected_unit(country, "infantry"),
        Some("western_medieval_infantry")
    );
    let action = [Action::CreateArmyFromCategories {
        owner: EntityRef::Selected(ScopeSelector::Root),
        location: EntityRef::Selected(ScopeSelector::Named {
            kind: EntityKind::Province,
            name: "1".into(),
        }),
        name: String::new(),
        categories: vec!["infantry".into(), "cavalry".into()],
    }];

    world
        .execute_in_context(ScopeContext::new(country), &action)
        .unwrap();

    assert_eq!(world.armies().len(), 1);
    assert_eq!(world.armies()[0].owner, country);
    assert_eq!(world.armies()[0].location, province);
    assert_eq!(
        world.armies()[0]
            .regiments
            .iter()
            .map(|regiment| regiment.unit_type.as_str())
            .collect::<Vec<_>>(),
        ["western_medieval_infantry", "western_medieval_knights"]
    );

    set_unit_selection_data(&mut world, country, 5);
    let army = world
        .create_army_from_categories(country, province, "Later Army", &["infantry".into()])
        .unwrap()
        .unwrap();
    assert_eq!(army, ArmyId(1));
    assert_eq!(world.armies()[1].regiments[0].id, RegimentId(2));
    assert_eq!(
        world.armies()[1].regiments[0].unit_type,
        "western_men_at_arms"
    );
    let saved = world.save("army-units:1").unwrap();
    let restored = World::restore(&saved, "army-units:1", world.schema()).unwrap();
    assert_eq!(
        restored.selected_unit(country, "infantry"),
        Some("western_men_at_arms")
    );
}

#[test]
fn unavailable_unit_category_is_a_no_op() {
    let (mut world, country, province) = fixture();
    set_unit_selection_data(&mut world, country, 3);

    assert_eq!(
        world
            .create_army_from_categories(country, province, "", &["artillery".into()])
            .unwrap(),
        None
    );
    assert!(
        world
            .set_selected_unit(country, "infantry", "eastern_medieval_infantry")
            .is_err()
    );
    assert!(
        world
            .set_selected_unit(country, "artillery", "western_medieval_infantry")
            .is_err()
    );
    world.clear_selected_unit(country, "infantry").unwrap();
    assert_eq!(world.selected_unit(country, "infantry"), None);
    assert_eq!(
        world
            .create_army_from_categories(country, province, "", &["infantry".into()])
            .unwrap(),
        None
    );
    assert!(world.armies().is_empty());
}
