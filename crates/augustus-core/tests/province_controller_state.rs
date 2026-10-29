// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{CountryState, EntityId, InitialWorld, ProvinceClass, ProvinceState};

fn fixture() -> (InitialWorld, EntityId, EntityId) {
    let initial = InitialWorld::new(
        vec![CountryState {
            id: "AAA".into(),
            name: "Alpha".into(),
            color: [1, 2, 3],
            declared_capital: Some(1),
            primary_culture: None,
            religion: None,
            government: None,
            technology_group: None,
            unit_type: None,
        }],
        vec![ProvinceState {
            id: 1,
            name: "Capital".into(),
            color: [4, 5, 6],
            class: ProvinceClass::Land,
            has_port: None,
            owner: Some("AAA".into()),
            controller: None,
            cores: Vec::new(),
            culture: None,
            religion: None,
            trade_good: None,
            tax_milli: None,
            production_milli: None,
            manpower_milli: None,
            is_city: None,
            in_empire: None,
        }],
    )
    .unwrap();
    let country = initial.country_entity("AAA").unwrap();
    let province = initial.province_entity(1).unwrap();
    (initial, country, province)
}

#[test]
fn controller_state_distinguishes_unknown_present_and_known_absent_through_restore() {
    let (mut initial, country, province) = fixture();
    assert_eq!(
        initial.world().province_controller_state(province).unwrap(),
        None
    );

    initial.set_province_controller(1, Some("AAA")).unwrap();
    assert_eq!(
        initial.world().province_controller_state(province).unwrap(),
        Some(Some(country))
    );
    assert_eq!(
        initial.province(1).unwrap().controller.as_deref(),
        Some("AAA")
    );

    initial.set_province_controller(1, None).unwrap();
    assert_eq!(
        initial.world().province_controller_state(province).unwrap(),
        Some(None)
    );
    assert_eq!(initial.province(1).unwrap().controller, None);

    let saved = initial.world().save("province-controller-state").unwrap();
    initial
        .restore(&saved, "province-controller-state")
        .unwrap();
    assert_eq!(
        initial.world().province_controller_state(province).unwrap(),
        Some(None)
    );
    assert_eq!(initial.province(1).unwrap().controller, None);
}

#[test]
fn invalid_controller_update_is_atomic_and_old_snapshots_keep_unknown_state() {
    let (mut initial, _country, province) = fixture();
    let before = initial.world().save("province-controller-state").unwrap();
    assert!(initial.set_province_controller(1, Some("ZZZ")).is_err());
    assert_eq!(
        initial.world().save("province-controller-state").unwrap(),
        before
    );
    assert_eq!(
        initial.world().province_controller_state(province).unwrap(),
        None
    );

    let mut old_snapshot: serde_json::Value = serde_json::from_str(&before).unwrap();
    old_snapshot
        .as_object_mut()
        .unwrap()
        .remove("province_controllers");
    initial
        .restore(&old_snapshot.to_string(), "province-controller-state")
        .unwrap();
    assert_eq!(
        initial.world().province_controller_state(province).unwrap(),
        None
    );
}

#[test]
fn previous_controller_state_distinguishes_unknown_present_and_known_absent_through_restore() {
    let (mut initial, country, province) = fixture();
    assert_eq!(
        initial
            .world()
            .province_previous_controller_state(province)
            .unwrap(),
        None
    );

    initial
        .set_province_previous_controller(1, Some("AAA"))
        .unwrap();
    assert_eq!(
        initial
            .world()
            .province_previous_controller_state(province)
            .unwrap(),
        Some(Some(country))
    );

    initial.set_province_previous_controller(1, None).unwrap();
    assert_eq!(
        initial
            .world()
            .province_previous_controller_state(province)
            .unwrap(),
        Some(None)
    );

    let saved = initial
        .world()
        .save("province-previous-controller")
        .unwrap();
    initial
        .restore(&saved, "province-previous-controller")
        .unwrap();
    assert_eq!(
        initial
            .world()
            .province_previous_controller_state(province)
            .unwrap(),
        Some(None)
    );
}

#[test]
fn invalid_previous_controller_update_is_atomic_and_old_snapshots_keep_unknown_state() {
    let (mut initial, _country, province) = fixture();
    let before = initial
        .world()
        .save("province-previous-controller")
        .unwrap();
    assert!(
        initial
            .set_province_previous_controller(1, Some("ZZZ"))
            .is_err()
    );
    assert_eq!(
        initial
            .world()
            .save("province-previous-controller")
            .unwrap(),
        before
    );
    assert_eq!(
        initial
            .world()
            .province_previous_controller_state(province)
            .unwrap(),
        None
    );

    let mut old_snapshot: serde_json::Value = serde_json::from_str(&before).unwrap();
    old_snapshot
        .as_object_mut()
        .unwrap()
        .remove("province_previous_controllers");
    initial
        .restore(&old_snapshot.to_string(), "province-previous-controller")
        .unwrap();
    assert_eq!(
        initial
            .world()
            .province_previous_controller_state(province)
            .unwrap(),
        None
    );
}

#[test]
fn tribal_owner_state_distinguishes_unknown_present_and_known_absent_through_restore() {
    let (mut initial, country, province) = fixture();
    assert_eq!(
        initial
            .world()
            .province_tribal_owner_state(province)
            .unwrap(),
        None
    );

    initial.set_province_tribal_owner(1, Some("AAA")).unwrap();
    assert_eq!(
        initial
            .world()
            .province_tribal_owner_state(province)
            .unwrap(),
        Some(Some(country))
    );

    initial.set_province_tribal_owner(1, None).unwrap();
    assert_eq!(
        initial
            .world()
            .province_tribal_owner_state(province)
            .unwrap(),
        Some(None)
    );

    let saved = initial.world().save("province-tribal-owner").unwrap();
    initial.restore(&saved, "province-tribal-owner").unwrap();
    assert_eq!(
        initial
            .world()
            .province_tribal_owner_state(province)
            .unwrap(),
        Some(None)
    );

    let mut old_snapshot: serde_json::Value = serde_json::from_str(&saved).unwrap();
    old_snapshot
        .as_object_mut()
        .unwrap()
        .remove("province_tribal_owners");
    initial
        .restore(&old_snapshot.to_string(), "province-tribal-owner")
        .unwrap();
    assert_eq!(
        initial
            .world()
            .province_tribal_owner_state(province)
            .unwrap(),
        None
    );
}

#[test]
fn original_capital_state_distinguishes_unknown_present_and_known_absent_through_restore() {
    let (mut initial, country, province) = fixture();
    assert_eq!(
        initial
            .world()
            .country_original_capital_state(country)
            .unwrap(),
        None
    );

    initial
        .set_country_original_capital("AAA", Some(1))
        .unwrap();
    assert_eq!(
        initial
            .world()
            .country_original_capital_state(country)
            .unwrap(),
        Some(Some(province))
    );
    assert_eq!(
        initial.country_original_capital_state("AAA").unwrap(),
        Some(Some(1))
    );

    initial.set_country_original_capital("AAA", None).unwrap();
    assert_eq!(
        initial.country_original_capital_state("AAA").unwrap(),
        Some(None)
    );

    let saved = initial.world().save("country-original-capital").unwrap();
    initial.restore(&saved, "country-original-capital").unwrap();
    assert_eq!(
        initial.country_original_capital_state("AAA").unwrap(),
        Some(None)
    );

    let mut old_snapshot: serde_json::Value = serde_json::from_str(&saved).unwrap();
    old_snapshot
        .as_object_mut()
        .unwrap()
        .remove("country_original_capitals");
    initial
        .restore(&old_snapshot.to_string(), "country-original-capital")
        .unwrap();
    assert_eq!(initial.country_original_capital_state("AAA").unwrap(), None);
}

#[test]
fn dated_capital_history_roundtrips_and_preserves_unknown_old_snapshots() {
    let (mut initial, country, province) = fixture();
    let date = augustus_core::WorldDate {
        year: 1446,
        month: 1,
        day: 2,
    };
    assert_eq!(
        initial.world().country_capital_history(country).unwrap(),
        None
    );
    initial
        .set_country_capital_history("AAA", std::collections::BTreeMap::from([(date, Some(1))]))
        .unwrap();
    assert_eq!(
        initial.country_capital_history("AAA").unwrap(),
        Some(std::collections::BTreeMap::from([(date, Some(1))]))
    );
    let saved = initial.world().save("country-capital-history").unwrap();
    initial.restore(&saved, "country-capital-history").unwrap();
    assert_eq!(
        initial.country_capital_history("AAA").unwrap(),
        Some(std::collections::BTreeMap::from([(date, Some(1))]))
    );

    let mut old_snapshot: serde_json::Value = serde_json::from_str(&saved).unwrap();
    old_snapshot
        .as_object_mut()
        .unwrap()
        .remove("country_capital_history");
    initial
        .restore(&old_snapshot.to_string(), "country-capital-history")
        .unwrap();
    assert_eq!(initial.country_capital_history("AAA").unwrap(), None);
    assert_eq!(country, initial.country_entity("AAA").unwrap());
    assert_eq!(province, initial.province_entity(1).unwrap());
}
