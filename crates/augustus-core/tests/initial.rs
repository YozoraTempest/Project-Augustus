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
