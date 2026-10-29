// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::BTreeMap;

use augustus_core::{
    CountryState, EntityId, EntityKind, InitialWorld, MetricDefinition, Metrics,
    NativeMigrationCountryState, NativeMigrationStates, ProvinceClass, ProvinceState,
    VanillaNativeMigrationTransferInput,
};

fn fixture(
    mark_destination_absent: bool,
    old_controller: bool,
    destination_controller: bool,
) -> (InitialWorld, EntityId, EntityId, EntityId) {
    let mut initial = InitialWorld::new(
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
        vec![
            ProvinceState {
                id: 1,
                name: "Old".into(),
                color: [4, 5, 6],
                class: ProvinceClass::Land,
                has_port: None,
                owner: Some("AAA".into()),
                controller: old_controller.then(|| "AAA".into()),
                cores: Vec::new(),
                culture: None,
                religion: None,
                trade_good: None,
                tax_milli: None,
                production_milli: None,
                manpower_milli: None,
                is_city: None,
                in_empire: None,
            },
            ProvinceState {
                id: 2,
                name: "Destination".into(),
                color: [7, 8, 9],
                class: ProvinceClass::Land,
                has_port: None,
                owner: None,
                controller: destination_controller.then(|| "AAA".into()),
                cores: Vec::new(),
                culture: None,
                religion: None,
                trade_good: None,
                tax_milli: None,
                production_milli: None,
                manpower_milli: None,
                is_city: None,
                in_empire: None,
            },
        ],
    )
    .unwrap();
    let country = initial.country_entity("AAA").unwrap();
    let old = initial.province_entity(1).unwrap();
    let destination = initial.province_entity(2).unwrap();
    if mark_destination_absent {
        initial.set_province_owner(2, None).unwrap();
    }
    initial.set_province_tribal_owner(1, Some("AAA")).unwrap();
    initial.set_province_tribal_owner(2, Some("AAA")).unwrap();
    let metric_definition = |name: &str| {
        (
            name.to_owned(),
            MetricDefinition {
                kind: EntityKind::Country,
                minimum: 0,
                maximum: 1_000,
                baseline: 0,
                divisor: 1,
                modifier: None,
                unresolved_properties: Default::default(),
                below_baseline: None,
                growth_ceiling: false,
            },
        )
    };
    initial
        .set_metrics(Metrics {
            definitions: BTreeMap::from([
                metric_definition("adm_power"),
                metric_definition("dip_power"),
                metric_definition("mil_power"),
            ]),
            entities: BTreeMap::from([(
                country,
                BTreeMap::from([
                    ("adm_power".into(), Some(10)),
                    ("dip_power".into(), Some(20)),
                    ("mil_power".into(), Some(30)),
                ]),
            )]),
            ceilings: BTreeMap::new(),
        })
        .unwrap();
    initial
        .set_native_migration_states(NativeMigrationStates {
            countries: BTreeMap::from([(
                country,
                NativeMigrationCountryState {
                    provinces: Some(vec![old]),
                    migration_steps: Some(2),
                },
            )]),
        })
        .unwrap();
    (initial, country, old, destination)
}

fn input(
    country: EntityId,
    old: EntityId,
    destination: EntityId,
) -> VanillaNativeMigrationTransferInput {
    VanillaNativeMigrationTransferInput {
        source_country: country,
        expected_current_capital: old,
        destination_province: destination,
        expected_migration_steps: 2,
        next_migration_steps: 3,
        adm_cost: 1,
        dip_cost: 2,
        mil_cost: 3,
    }
}

#[test]
fn transfer_moves_owner_capital_steps_and_power_atomically() {
    let (mut initial, country, old, destination) = fixture(true, false, false);
    let result = initial
        .execute_vanilla_native_migration_transfer(input(country, old, destination))
        .unwrap();
    assert_eq!(result.power_before, [10, 20, 30]);
    assert_eq!(result.power_after, [9, 18, 27]);
    assert_eq!(result.migration_steps_after, 3);
    assert_eq!(
        initial.world().entity(country).unwrap().capital,
        Some(destination)
    );
    assert_eq!(initial.world().entity(old).unwrap().owner, None);
    assert_eq!(
        initial.world().entity(destination).unwrap().owner,
        Some(country)
    );
    assert_eq!(initial.bound_capital("AAA"), Some(2));
    assert_eq!(initial.owned_provinces("AAA"), vec![2]);
    assert_eq!(initial.province(1).unwrap().owner, None);
    assert_eq!(initial.province(2).unwrap().owner.as_deref(), Some("AAA"));
    assert_eq!(initial.province(1).unwrap().controller, None);
    assert_eq!(
        initial.province(2).unwrap().controller.as_deref(),
        Some("AAA")
    );

    let saved = initial.world().save("native-migration-transfer").unwrap();
    initial
        .restore(&saved, "native-migration-transfer")
        .unwrap();
    assert_eq!(initial.bound_capital("AAA"), Some(2));
    assert_eq!(initial.owned_provinces("AAA"), vec![2]);
    assert_eq!(initial.province(1).unwrap().owner, None);
    assert_eq!(initial.province(2).unwrap().owner.as_deref(), Some("AAA"));
    assert_eq!(initial.province(1).unwrap().controller, None);
    assert_eq!(
        initial.province(2).unwrap().controller.as_deref(),
        Some("AAA")
    );
}

#[test]
fn transfer_rejects_unknown_destination_and_stale_capital_without_mutation() {
    let (mut unknown, country, old, destination) = fixture(false, false, false);
    let before_unknown = unknown.world().save("native-migration-transfer").unwrap();
    assert!(
        unknown
            .execute_vanilla_native_migration_transfer(input(country, old, destination))
            .is_err()
    );
    assert_eq!(
        unknown.world().save("native-migration-transfer").unwrap(),
        before_unknown
    );

    let (mut stale, country, old, destination) = fixture(true, false, false);
    let before_stale = stale.world().save("native-migration-transfer").unwrap();
    let mut request = input(country, old, destination);
    request.expected_current_capital = destination;
    assert!(
        stale
            .execute_vanilla_native_migration_transfer(request)
            .is_err()
    );
    assert_eq!(
        stale.world().save("native-migration-transfer").unwrap(),
        before_stale
    );
}

#[test]
fn transfer_rejects_known_controller_projection_without_mutation() {
    let (mut initial, country, old, destination) = fixture(true, false, true);
    let before = initial.world().save("native-migration-transfer").unwrap();
    assert!(
        initial
            .execute_vanilla_native_migration_transfer(input(country, old, destination))
            .is_err()
    );
    assert_eq!(
        initial.world().save("native-migration-transfer").unwrap(),
        before
    );
}

#[test]
fn transfer_clears_old_controller_and_assigns_destination_controller_atomically() {
    let (mut initial, country, old, destination) = fixture(true, true, false);
    let result = initial
        .execute_vanilla_native_migration_transfer(input(country, old, destination))
        .unwrap();
    assert_eq!(result.destination_province, destination);
    assert_eq!(initial.province(1).unwrap().controller, None);
    assert_eq!(
        initial.province(2).unwrap().controller.as_deref(),
        Some("AAA")
    );
    assert_eq!(
        initial.world().province_controller_state(old).unwrap(),
        Some(None)
    );
    assert_eq!(
        initial
            .world()
            .province_controller_state(destination)
            .unwrap(),
        Some(Some(country))
    );
    assert_eq!(
        initial
            .world()
            .province_previous_controller_state(old)
            .unwrap(),
        Some(Some(country))
    );
    assert_eq!(
        initial
            .world()
            .province_previous_controller_state(destination)
            .unwrap(),
        None
    );
    assert_eq!(
        initial.world().province_tribal_owner_state(old).unwrap(),
        Some(Some(country))
    );
    assert_eq!(
        initial
            .world()
            .province_tribal_owner_state(destination)
            .unwrap(),
        Some(Some(country))
    );
    let saved = initial.world().save("native-migration-transfer").unwrap();
    initial
        .restore(&saved, "native-migration-transfer")
        .unwrap();
    assert_eq!(initial.province(1).unwrap().controller, None);
    assert_eq!(
        initial.province(2).unwrap().controller.as_deref(),
        Some("AAA")
    );
    assert_eq!(
        initial
            .world()
            .province_previous_controller_state(old)
            .unwrap(),
        Some(Some(country))
    );
}

#[test]
fn transfer_keeps_previous_controller_unknown_when_old_controller_is_unknown() {
    let (mut initial, country, old, destination) = fixture(true, false, false);
    initial
        .set_province_previous_controller(2, Some("AAA"))
        .unwrap();
    initial
        .execute_vanilla_native_migration_transfer(input(country, old, destination))
        .unwrap();
    assert_eq!(
        initial
            .world()
            .province_previous_controller_state(old)
            .unwrap(),
        None
    );
    assert_eq!(
        initial
            .world()
            .province_previous_controller_state(destination)
            .unwrap(),
        Some(Some(country))
    );
}

#[test]
fn transfer_accepts_explicit_zero_migration_steps_and_preserves_native_relations() {
    let (mut initial, country, old, destination) = fixture(true, true, false);
    let mut transfer = input(country, old, destination);
    transfer.next_migration_steps = 0;
    initial
        .execute_vanilla_native_migration_transfer(transfer)
        .unwrap();

    assert_eq!(
        initial.world().native_migration_states().countries[&country].migration_steps,
        Some(0)
    );
    assert_eq!(
        initial.world().native_migration_states().countries[&country].provinces,
        Some(vec![old])
    );
    assert_eq!(
        initial.world().province_tribal_owner_state(old).unwrap(),
        Some(Some(country))
    );
    assert_eq!(
        initial
            .world()
            .province_tribal_owner_state(destination)
            .unwrap(),
        Some(Some(country))
    );
}
