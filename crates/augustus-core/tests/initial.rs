// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{CountryState, InitialWorld, ProvinceState, Target};

fn countries() -> Vec<CountryState> {
    serde_json::from_str(
        r#"[
        {"id":"BBB","name":"Beta","color":[50,180,140],"declared_capital":1},
        {"id":"AAA","name":"Alpha","color":[220,50,80],"declared_capital":2}
    ]"#,
    )
    .unwrap()
}

fn provinces() -> Vec<ProvinceState> {
    serde_json::from_str(r#"[
        {"id":2,"name":"East","color":[10,240,10],"class":"land","owner":"AAA","cores":["AAA"],"tax_milli":4000},
        {"id":1,"name":"North","color":[240,10,10],"class":"land","owner":"BBB","cores":["BBB"],"tax_milli":2125}
    ]"#).unwrap()
}

#[test]
fn native_input_order_does_not_change_entity_ids_or_relations() {
    let world = InitialWorld::new(countries(), provinces()).unwrap();
    let mut countries = countries();
    let mut provinces = provinces();
    countries.reverse();
    provinces.reverse();
    let reordered = InitialWorld::new(countries, provinces).unwrap();
    assert_eq!(world.world(), reordered.world());
    assert_eq!(
        world.world().save("test-world").unwrap(),
        reordered.world().save("test-world").unwrap()
    );
    let alpha = world.country_entity("AAA").unwrap();
    let east = world.province_entity(2).unwrap();
    assert_eq!(world.world().target(east, Target::Owner).unwrap(), alpha);
    assert_eq!(world.world().target(alpha, Target::Capital).unwrap(), east);
    assert_eq!(
        world
            .world()
            .value(
                east,
                world.world().schema().field("province.tax_base").unwrap()
            )
            .unwrap(),
        4000
    );
    assert_eq!(world.province(2).unwrap().production_milli, None);
}

#[test]
fn native_input_rejects_duplicate_identity_and_unresolved_references() {
    let mut duplicate = countries();
    duplicate.push(duplicate[0].clone());
    assert!(InitialWorld::new(duplicate, provinces()).is_err());
    let mut unknown_core = provinces();
    unknown_core[0].cores.push("ZZZ".into());
    assert!(InitialWorld::new(countries(), unknown_core).is_err());
    let mut unknown_capital = countries();
    unknown_capital[0].declared_capital = Some(999);
    assert!(InitialWorld::new(unknown_capital, provinces()).is_err());
    let mut foreign_capital = countries();
    foreign_capital[0].declared_capital = Some(2);
    let world = InitialWorld::new(foreign_capital, provinces()).unwrap();
    assert_eq!(world.bound_capital("BBB"), None);
    assert_eq!(world.country("BBB").unwrap().declared_capital, Some(2));
}

#[test]
fn initial_world_exposes_port_checked_naval_creation() {
    let mut provinces = provinces();
    provinces[0].has_port = Some(true);
    let mut initial = InitialWorld::new(countries(), provinces).unwrap();
    let owner = initial.country_entity("AAA").unwrap();
    let port = initial.province_entity(2).unwrap();

    let id = initial
        .create_naval_unit(owner, port, "light_ship")
        .unwrap();
    assert_eq!(initial.naval_units().len(), 1);
    assert_eq!(initial.naval_units()[0].id, id);
    assert_eq!(initial.naval_units()[0].owner, owner);
    assert_eq!(initial.naval_units()[0].location, port);
    assert_eq!(initial.fleets().len(), 1);
    assert_eq!(initial.fleets()[0].ships, vec![id]);
}

#[test]
fn initial_world_can_enqueue_explicit_naval_construction_orders() {
    let mut provinces = provinces();
    provinces[0].has_port = Some(true);
    let mut initial = InitialWorld::new(countries(), provinces).unwrap();
    let owner = initial.country_entity("AAA").unwrap();
    let port = initial.province_entity(2).unwrap();

    let ids = initial
        .enqueue_naval_constructions(&[augustus_core::NavalConstructionInput {
            owner,
            location: port,
            unit_type: "light_ship".into(),
            completion_day: 3,
        }])
        .unwrap();

    assert_eq!(ids, [augustus_core::NavalConstructionId(0)]);
    assert_eq!(initial.world().naval_constructions().len(), 1);
}

#[test]
fn initial_world_can_enqueue_paid_naval_construction_orders() {
    use augustus_core::{CountryResources, PaidNavalConstructionInput};

    let mut provinces = provinces();
    provinces[0].has_port = Some(true);
    let mut initial = InitialWorld::new(countries(), provinces).unwrap();
    let owner = initial.country_entity("AAA").unwrap();
    let port = initial.province_entity(2).unwrap();
    initial
        .set_country_resources(
            [(
                owner,
                CountryResources {
                    treasury_milli: Some(50),
                    ..Default::default()
                },
            )]
            .into(),
        )
        .unwrap();

    let ids = initial
        .enqueue_paid_naval_constructions(&[PaidNavalConstructionInput {
            construction: augustus_core::NavalConstructionInput {
                owner,
                location: port,
                unit_type: "light_ship".into(),
                completion_day: 3,
            },
            cost_milli: 100,
        }])
        .unwrap();

    assert_eq!(ids, [augustus_core::NavalConstructionId(0)]);
    assert_eq!(
        initial.world().country_resources()[&owner].treasury_milli,
        Some(-50)
    );
    assert_eq!(initial.world().naval_constructions().len(), 1);
}

#[test]
fn initial_world_can_enqueue_naval_constructions_with_expenses() {
    use augustus_core::{CountryResources, PaidNavalConstructionInput};

    let mut provinces = provinces();
    provinces[0].has_port = Some(true);
    let mut initial = InitialWorld::new(countries(), provinces).unwrap();
    let owner = initial.country_entity("AAA").unwrap();
    let port = initial.province_entity(2).unwrap();
    initial
        .set_country_resources(
            [(
                owner,
                CountryResources {
                    treasury_milli: Some(500),
                    ..Default::default()
                },
            )]
            .into(),
        )
        .unwrap();

    let ids = initial
        .enqueue_naval_constructions_with_expenses(
            &[PaidNavalConstructionInput {
                construction: augustus_core::NavalConstructionInput {
                    owner,
                    location: port,
                    unit_type: "light_ship".into(),
                    completion_day: 3,
                },
                cost_milli: 100,
            }],
            5,
        )
        .unwrap();

    assert_eq!(ids, [augustus_core::NavalConstructionId(0)]);
    assert_eq!(
        initial.world().country_resources()[&owner].treasury_milli,
        Some(400)
    );
    assert_eq!(initial.world().country_ledgers()[&owner].pending_milli, 0);
    assert_eq!(
        initial.world().country_ledgers()[&owner].categories[&5].expense_milli,
        100
    );
    assert_eq!(initial.world().naval_constructions().len(), 1);
}

#[test]
fn initial_world_binds_grouped_fleets_at_non_port_locations() {
    let mut provinces = provinces();
    provinces[0].has_port = Some(false);
    let mut initial = InitialWorld::new(countries(), provinces).unwrap();
    let owner = initial.country_entity("AAA").unwrap();
    let location = initial.province_entity(2).unwrap();
    let ship_types = vec!["galley".into(), "carrack".into()];

    let id = initial
        .create_fleet(owner, location, Some("Imported Fleet".into()), &ship_types)
        .unwrap();

    assert_eq!(id, augustus_core::FleetId(0));
    assert_eq!(initial.naval_units().len(), 2);
    assert_eq!(initial.fleets().len(), 1);
    assert_eq!(initial.fleets()[0].location, location);
    assert_eq!(initial.fleets()[0].name.as_deref(), Some("Imported Fleet"));
    assert_eq!(initial.fleets()[0].ships.len(), 2);
}

#[test]
fn initial_world_exposes_army_creation_and_query() {
    let mut initial = InitialWorld::new(countries(), provinces()).unwrap();
    let owner = initial.country_entity("AAA").unwrap();
    let location = initial.province_entity(2).unwrap();
    let unit_types = vec!["western_infantry".into()];

    let id = initial
        .create_army(owner, location, "First Army", &unit_types)
        .unwrap();
    assert_eq!(initial.armies().len(), 1);
    assert_eq!(initial.armies()[0].id, id);
    assert_eq!(initial.armies()[0].owner, owner);
    assert_eq!(initial.armies()[0].location, location);
}

#[test]
fn province_views_follow_live_core_and_owner_relations_and_restore_input_order() {
    use augustus_core::{
        Action, Administration, CoreType, EntityKind, EntityRef, ProvinceAdministration,
        ScopeSelector,
    };
    let mut provinces = provinces();
    provinces[0].cores = vec!["BBB".into(), "AAA".into()];
    let mut countries = countries();
    countries
        .iter_mut()
        .find(|country| country.id == "AAA")
        .unwrap()
        .declared_capital = None;
    let mut initial = InitialWorld::new(countries, provinces).unwrap();
    let a = initial.country_entity("AAA").unwrap();
    let b = initial.country_entity("BBB").unwrap();
    let p = initial.province_entity(2).unwrap();
    initial
        .set_administration(Administration {
            provinces: [(
                p,
                ProvinceAdministration {
                    cores_known: true,
                    cores: [(a, CoreType::Full), (b, CoreType::Full)].into(),
                    ..Default::default()
                },
            )]
            .into(),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(initial.province(2).unwrap().cores, ["BBB", "AAA"]);
    let save = initial.world().save("view").unwrap();
    initial
        .execute(
            p,
            &[
                Action::SetProvinceCore {
                    province: EntityRef::Relative(Target::Current),
                    country: EntityRef::Selected(ScopeSelector::Named {
                        kind: EntityKind::Country,
                        name: "AAA".into(),
                    }),
                    core: None,
                },
                Action::SetOwner {
                    target: Target::Current,
                    owner: EntityRef::Selected(ScopeSelector::Named {
                        kind: EntityKind::Country,
                        name: "BBB".into(),
                    }),
                },
            ],
        )
        .unwrap();
    assert_eq!(initial.province(2).unwrap().cores, ["BBB"]);
    assert_eq!(initial.province(2).unwrap().owner.as_deref(), Some("BBB"));
    assert_eq!(initial.owned_provinces("BBB").len(), 2);
    assert_eq!(initial.owned_provinces("AAA").len(), 0);
    let changed = initial.world().save("view").unwrap();
    initial.restore(&save, "view").unwrap();
    assert_eq!(initial.province(2).unwrap().cores, ["BBB", "AAA"]);
    assert_eq!(initial.province(2).unwrap().owner.as_deref(), Some("AAA"));
    initial.restore(&changed, "view").unwrap();
    assert_eq!(initial.province(2).unwrap().cores, ["BBB"]);
    let mut unknown = Administration::default();
    unknown
        .provinces
        .insert(p, ProvinceAdministration::default());
    initial.set_administration(unknown).unwrap();
    assert_eq!(initial.province(2).unwrap().cores, ["BBB", "AAA"]);
}
