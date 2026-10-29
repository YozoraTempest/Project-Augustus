// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::BTreeMap;

use augustus_core::{
    Action, CountryState, EntityId, EntityKind, InitialWorld, MetricDefinition, Metrics,
    NativeMigrationCountryState, NativeMigrationStates, NumericOperation, NumericSlot,
    ProvinceClass, ProvinceState, Target, VanillaNativeMigrationExecutionInput,
};

fn fixture() -> (InitialWorld, EntityId, EntityId) {
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
        vec![ProvinceState {
            id: 1,
            name: "One".into(),
            color: [4, 5, 6],
            class: ProvinceClass::Land,
            has_port: None,
            owner: Some("AAA".into()),
            controller: Some("AAA".into()),
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
                metric_definition("other_power"),
            ]),
            entities: BTreeMap::from([(
                country,
                BTreeMap::from([
                    ("adm_power".into(), Some(10)),
                    ("dip_power".into(), Some(20)),
                    ("mil_power".into(), Some(30)),
                    ("other_power".into(), None),
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
                    provinces: Some(vec![province]),
                    migration_steps: Some(2),
                },
            )]),
        })
        .unwrap();
    (initial, country, province)
}

fn input(country: EntityId, steps: u32, next_steps: u32) -> VanillaNativeMigrationExecutionInput {
    VanillaNativeMigrationExecutionInput {
        source_country: country,
        expected_migration_steps: steps,
        next_migration_steps: next_steps,
        adm_cost: 1,
        dip_cost: 2,
        mil_cost: 3,
    }
}

#[test]
fn execution_rechecks_and_atomically_debits_power_and_updates_explicit_steps() {
    let (mut initial, country, _) = fixture();
    let result = initial
        .execute_vanilla_native_migration(input(country, 2, 3))
        .unwrap();
    assert_eq!(result.before, [10, 20, 30]);
    assert_eq!(result.after, [9, 18, 27]);
    assert_eq!(result.migration_steps_before, 2);
    assert_eq!(result.migration_steps_after, 3);
    let metrics = initial.world().metrics();
    assert_eq!(metrics.entities[&country]["adm_power"], Some(9));
    assert_eq!(metrics.entities[&country]["dip_power"], Some(18));
    assert_eq!(metrics.entities[&country]["mil_power"], Some(27));
    assert_eq!(
        initial.world().native_migration_states().countries[&country].migration_steps,
        Some(3)
    );
}

#[test]
fn execution_rejects_insufficient_power_without_partial_writes() {
    let (mut initial, country, _) = fixture();
    let before = initial.world().save("native-migration-execution").unwrap();
    let mut request = input(country, 2, 3);
    request.mil_cost = 31;
    assert!(initial.execute_vanilla_native_migration(request).is_err());
    assert_eq!(
        initial.world().save("native-migration-execution").unwrap(),
        before
    );
}

#[test]
fn execution_rejects_unknown_or_stale_context_without_partial_writes() {
    let (mut initial, country, _) = fixture();
    let before = initial.world().save("native-migration-execution").unwrap();

    let mut stale = input(country, 1, 2);
    assert!(initial.execute_vanilla_native_migration(stale).is_err());
    assert_eq!(
        initial.world().save("native-migration-execution").unwrap(),
        before
    );

    stale = input(country, 2, 3);
    initial
        .set_metric_values(BTreeMap::from([(
            country,
            BTreeMap::from([("mil_power".into(), None)]),
        )]))
        .unwrap();
    let before_unknown = initial.world().save("native-migration-execution").unwrap();
    assert!(initial.execute_vanilla_native_migration(stale).is_err());
    assert_eq!(
        initial.world().save("native-migration-execution").unwrap(),
        before_unknown
    );
}

#[test]
fn execution_preserves_unbased_deltas_for_unrelated_metrics() {
    let (mut initial, country, _) = fixture();
    initial
        .execute(
            country,
            &[Action::UpdateNumeric {
                target: Target::Current,
                slot: NumericSlot::Metric("other_power".into()),
                operation: NumericOperation::Add,
                value: 5,
            }],
        )
        .unwrap();
    assert_eq!(
        initial
            .world()
            .unbased_numeric_delta(country, &NumericSlot::Metric("other_power".into())),
        Some(5)
    );
    initial
        .execute_vanilla_native_migration(input(country, 2, 3))
        .unwrap();
    assert_eq!(
        initial
            .world()
            .unbased_numeric_delta(country, &NumericSlot::Metric("other_power".into())),
        Some(5)
    );
}
