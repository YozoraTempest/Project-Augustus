// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::BTreeMap;

use augustus_core::{
    CountryState, EntityId, EntityKind, InitialWorld, NativeMigrationCountryState,
    NativeMigrationStates, ProvinceClass, ProvinceState, Schema, World,
};

fn fixture_world() -> (
    World,
    EntityId,
    EntityId,
    EntityId,
    EntityId,
    EntityId,
    EntityId,
) {
    let mut world = World::new(Schema::new([] as [&str; 0]).unwrap());
    let aaa = world.add_entity("AAA", EntityKind::Country);
    let bbb = world.add_entity("BBB", EntityKind::Country);
    let ccc = world.add_entity("CCC", EntityKind::Country);
    let province_one = world.add_entity("1", EntityKind::Province);
    let province_two = world.add_entity("2", EntityKind::Province);
    world.set_owner(province_two, bbb).unwrap();
    (
        world,
        aaa,
        bbb,
        ccc,
        province_one,
        province_two,
        EntityId(99),
    )
}

fn initial_world() -> InitialWorld {
    InitialWorld::new(
        vec![CountryState {
            id: "AAA".into(),
            name: "Alpha".into(),
            color: [10, 20, 30],
            declared_capital: None,
            primary_culture: None,
            religion: None,
            government: None,
            technology_group: None,
            unit_type: None,
        }],
        vec![ProvinceState {
            id: 1,
            name: "One".into(),
            color: [30, 20, 10],
            class: ProvinceClass::Land,
            has_port: None,
            owner: None,
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
    .unwrap()
}

#[test]
fn migration_state_distinguishes_unknown_empty_and_ordered_provinces() {
    let (mut world, aaa, bbb, ccc, province_one, province_two, _) = fixture_world();
    let states = NativeMigrationStates {
        countries: BTreeMap::from([
            (
                aaa,
                NativeMigrationCountryState {
                    provinces: None,
                    migration_steps: None,
                },
            ),
            (
                bbb,
                NativeMigrationCountryState {
                    provinces: Some(Vec::new()),
                    migration_steps: Some(0),
                },
            ),
            (
                ccc,
                NativeMigrationCountryState {
                    provinces: Some(vec![province_two, province_one]),
                    migration_steps: Some(2),
                },
            ),
        ]),
    };

    world.set_native_migration_states(states.clone()).unwrap();
    assert_eq!(world.native_migration_states(), &states);
    assert_eq!(
        world.native_migration_states().countries[&aaa].provinces,
        None
    );
    assert_eq!(
        world.native_migration_states().countries[&bbb].provinces,
        Some(Vec::new())
    );
    assert_eq!(
        world.native_migration_states().countries[&ccc].provinces,
        Some(vec![province_two, province_one])
    );

    world.advance_to(3).unwrap();
    assert_eq!(world.native_migration_states(), &states);
}

#[test]
fn invalid_migration_state_is_rejected_atomically() {
    let (mut world, aaa, _bbb, _ccc, province_one, _, unknown) = fixture_world();
    let current = NativeMigrationStates {
        countries: BTreeMap::from([(
            aaa,
            NativeMigrationCountryState {
                provinces: Some(vec![province_one]),
                migration_steps: Some(1),
            },
        )]),
    };
    world.set_native_migration_states(current.clone()).unwrap();
    let before = world.save("native-migration-state").unwrap();

    let invalid_states = [
        NativeMigrationStates {
            countries: BTreeMap::from([(unknown, NativeMigrationCountryState::default())]),
        },
        NativeMigrationStates {
            countries: BTreeMap::from([(province_one, NativeMigrationCountryState::default())]),
        },
        NativeMigrationStates {
            countries: BTreeMap::from([(
                aaa,
                NativeMigrationCountryState {
                    provinces: Some(vec![aaa]),
                    migration_steps: None,
                },
            )]),
        },
        NativeMigrationStates {
            countries: BTreeMap::from([(
                aaa,
                NativeMigrationCountryState {
                    provinces: Some(vec![unknown]),
                    migration_steps: None,
                },
            )]),
        },
        NativeMigrationStates {
            countries: BTreeMap::from([(
                aaa,
                NativeMigrationCountryState {
                    provinces: Some(vec![province_one, province_one]),
                    migration_steps: None,
                },
            )]),
        },
    ];

    for invalid in invalid_states {
        assert!(world.set_native_migration_states(invalid).is_err());
        assert_eq!(world.native_migration_states(), &current);
        assert_eq!(world.save("native-migration-state").unwrap(), before);
    }
}

#[test]
fn migration_state_round_trips_and_legacy_snapshots_default_to_unknown() {
    let (mut world, aaa, _, _, province_one, province_two, _) = fixture_world();
    let states = NativeMigrationStates {
        countries: BTreeMap::from([(
            aaa,
            NativeMigrationCountryState {
                provinces: Some(vec![province_two, province_one]),
                migration_steps: Some(4),
            },
        )]),
    };
    world.set_native_migration_states(states.clone()).unwrap();

    let saved = world.save("native-migration-state").unwrap();
    let restored = World::restore(&saved, "native-migration-state", world.schema()).unwrap();
    assert_eq!(restored.native_migration_states(), &states);
    assert_eq!(restored.save("native-migration-state").unwrap(), saved);

    let mut legacy: serde_json::Value = serde_json::from_str(&saved).unwrap();
    assert!(
        legacy
            .as_object_mut()
            .unwrap()
            .remove("native_migration_states")
            .is_some()
    );
    let restored_legacy = World::restore(
        &legacy.to_string(),
        "native-migration-state",
        world.schema(),
    )
    .unwrap();
    assert_eq!(
        restored_legacy.native_migration_states(),
        &NativeMigrationStates::default()
    );

    let (empty, ..) = fixture_world();
    let empty_save = empty.save("native-migration-empty").unwrap();
    assert!(!empty_save.contains("native_migration_states"));
}

#[test]
fn invalid_saved_migration_state_is_rejected_and_identity_ignores_runtime_values() {
    let (world, aaa, _, _, province_one, _, _) = fixture_world();
    let saved = world.save("native-migration-state").unwrap();
    let mut invalid: serde_json::Value = serde_json::from_str(&saved).unwrap();
    invalid["native_migration_states"] = serde_json::to_value(NativeMigrationStates {
        countries: BTreeMap::from([(
            aaa,
            NativeMigrationCountryState {
                provinces: Some(vec![province_one, province_one]),
                migration_steps: Some(2),
            },
        )]),
    })
    .unwrap();
    assert!(
        World::restore(
            &invalid.to_string(),
            "native-migration-state",
            world.schema()
        )
        .is_err()
    );

    let mut initial = initial_world();
    let identity = initial.content_identity_state().unwrap();
    let country = initial.country_entity("AAA").unwrap();
    let province = initial.province_entity(1).unwrap();
    initial
        .set_native_migration_states(NativeMigrationStates {
            countries: BTreeMap::from([(
                country,
                NativeMigrationCountryState {
                    provinces: Some(vec![province]),
                    migration_steps: Some(3),
                },
            )]),
        })
        .unwrap();
    assert_eq!(initial.content_identity_state().unwrap(), identity);
}
