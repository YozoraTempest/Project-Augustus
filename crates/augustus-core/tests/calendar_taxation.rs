// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    Action, Administration, AdministrationRules, AreaStatus, AttributeBase, AttributeChannel,
    AttributeFormula, AttributeTerm, Attributes, Calendar, CoreType, CountryLedger,
    CountryResource, CountryResources, EntityId, EntityKind, FieldId,
    MONTHLY_AUTONOMY_DELTA_ATTRIBUTE, MonthlyExternalCashflows, MonthlyStocks, MonthlySystem,
    ProvinceAdministration, Schema, StockRule, Target, Taxation, World, WorldDate,
};
use std::collections::{BTreeMap, BTreeSet};

fn date(year: i16, month: u8, day: u8) -> WorldDate {
    WorldDate { year, month, day }
}

fn calendar(start: WorldDate) -> Calendar {
    Calendar {
        start,
        month_lengths: [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31],
    }
}

fn scenario() -> World {
    let mut world = World::new(Schema::new(["annual_tax"]).unwrap());
    let alpha = world.add_entity("Alpha", EntityKind::Country);
    let beta = world.add_entity("Beta", EntityKind::Country);
    let north = world.add_entity("North", EntityKind::Province);
    let south = world.add_entity("South", EntityKind::Province);
    let mut attributes = Attributes::default();
    for (province, owner, annual) in [(north, alpha, 12_000), (south, beta, 24_000)] {
        world.set_owner(province, owner).unwrap();
        world.set(province, FieldId(0), annual).unwrap();
        attributes.formulas.insert(
            province,
            [(
                "annual_tax".into(),
                AttributeFormula {
                    base: AttributeBase::Field {
                        field: FieldId(0),
                        micros_per_unit: 1000,
                    },
                    channels: vec![],
                    minimum_micros: None,
                    maximum_micros: None,
                    result_quantum_micros: None,
                    unresolved: BTreeSet::new(),
                },
            )]
            .into_iter()
            .collect(),
        );
    }
    world.set_attributes(attributes).unwrap();
    world
        .set_country_resources(
            [(alpha, 10_000), (beta, 20_000)]
                .into_iter()
                .map(|(id, treasury)| {
                    (
                        id,
                        CountryResources {
                            treasury_milli: Some(treasury),
                            ..Default::default()
                        },
                    )
                })
                .collect(),
        )
        .unwrap();
    world.set_calendar(calendar(date(1444, 11, 11))).unwrap();
    world
        .set_taxation(Taxation::new(
            "annual_tax",
            [alpha, beta].into_iter().collect(),
        ))
        .unwrap();
    world
}

fn balance(world: &World, id: usize) -> Option<i64> {
    world.country_resources()[&EntityId(id)].treasury_milli
}

fn with_country_tax(mut world: World, unknown: bool) -> World {
    let mut attributes = world.attributes().clone();
    for (country, micros) in [(EntityId(0), 6_006_000), (EntityId(1), 18_000_000)] {
        attributes.formulas.entry(country).or_default().insert(
            "country_tax".into(),
            AttributeFormula {
                base: if unknown && country == EntityId(0) {
                    AttributeBase::Unknown {
                        reason: "unresolved_country_tax".into(),
                    }
                } else {
                    AttributeBase::Constant { micros }
                },
                channels: vec![],
                minimum_micros: None,
                maximum_micros: None,
                result_quantum_micros: None,
                unresolved: BTreeSet::new(),
            },
        );
    }
    world.set_attributes(attributes).unwrap();
    world
        .set_taxation(
            Taxation::new("annual_tax", [EntityId(0), EntityId(1)].into())
                .with_country_annual_attribute("country_tax"),
        )
        .unwrap();
    world
}

#[test]
fn country_tax_is_added_once_and_resumes_with_the_same_monthly_receipts() {
    let mut continuous = with_country_tax(scenario(), false);
    let east = continuous.add_entity("East", EntityKind::Province);
    continuous.set_owner(east, EntityId(0)).unwrap();
    continuous.set(east, FieldId(0), 12_000).unwrap();
    let mut attributes = continuous.attributes().clone();
    attributes
        .formulas
        .insert(east, attributes.formulas[&EntityId(2)].clone());
    continuous.set_attributes(attributes).unwrap();
    let mut segmented = continuous.clone();
    continuous.advance_to(51).unwrap();
    segmented.advance_to(20).unwrap();
    assert_eq!(balance(&segmented, 0), Some(12_500));
    assert_eq!(balance(&segmented, 1), Some(23_500));
    let snapshot = segmented.save("country-tax").unwrap();
    let mut resumed = World::restore(&snapshot, "country-tax", segmented.schema()).unwrap();
    resumed.advance_to(51).unwrap();
    assert_eq!(continuous, resumed);
    assert_eq!(balance(&continuous, 0), Some(15_000));
    assert_eq!(balance(&continuous, 1), Some(27_000));
}

#[test]
fn country_tax_overflow_does_not_credit_an_earlier_country() {
    let mut current = with_country_tax(scenario(), false);
    let mut attributes = current.attributes().clone();
    attributes
        .formulas
        .get_mut(&EntityId(1))
        .unwrap()
        .get_mut("country_tax")
        .unwrap()
        .base = AttributeBase::Constant { micros: i64::MAX };
    current.set_attributes(attributes).unwrap();
    assert!(current.advance_to(20).is_err());
    assert_eq!(balance(&current, 0), Some(10_000));
    assert_eq!(balance(&current, 1), Some(20_000));
    assert_eq!(current.taxation().unwrap().settlement_count, 0);
}

#[test]
fn unresolved_country_tax_keeps_the_known_province_subtotal_uncredited() {
    let mut current = with_country_tax(scenario(), true);
    current.advance_to(20).unwrap();
    let account = &current
        .taxation()
        .unwrap()
        .last_settlement
        .as_ref()
        .unwrap()
        .accounts[&EntityId(0)];
    assert_eq!(account.known_subtotal_milli, 1000);
    assert_eq!(account.income_milli, None);
    assert_eq!(account.credited_milli, None);
    assert!(account.unresolved.contains("unresolved_country_tax"));
    assert_eq!(balance(&current, 1), Some(23_500));
}

#[test]
fn external_country_annual_tax_overrides_content_source_without_double_counting() {
    let mut current = with_country_tax(scenario(), false);
    current
        .set_taxation(
            Taxation::new("annual_tax", [EntityId(0), EntityId(1)].into())
                .with_country_annual_attribute("country_tax")
                .with_external_country_annual_milli(BTreeMap::from([(EntityId(0), Some(24_000))])),
        )
        .unwrap();

    current.advance_to(20).unwrap();
    let account = &current
        .taxation()
        .unwrap()
        .last_settlement
        .as_ref()
        .unwrap()
        .accounts[&EntityId(0)];
    assert_eq!(account.known_subtotal_milli, 3_000);
    assert_eq!(account.income_milli, Some(3_000));
    assert_eq!(account.treasury_after_milli, Some(13_000));

    let saved = current.save("external-country-tax").unwrap();
    assert!(saved.contains("external_country_annual_milli"));
    let restored = World::restore(&saved, "external-country-tax", current.schema()).unwrap();
    assert_eq!(restored, current);
}

#[test]
fn external_province_annual_tax_replaces_content_value() {
    let mut current = scenario();
    current
        .set_taxation(
            Taxation::new(
                "annual_tax",
                [EntityId(0), EntityId(1)].into_iter().collect(),
            )
            .with_external_province_annual_milli(BTreeMap::from([(EntityId(2), Some(36_000))])),
        )
        .unwrap();

    current.advance_to(20).unwrap();
    let account = &current
        .taxation()
        .unwrap()
        .last_settlement
        .as_ref()
        .unwrap()
        .accounts[&EntityId(0)];
    assert_eq!(account.known_subtotal_milli, 3_000);
    assert_eq!(account.income_milli, Some(3_000));
    assert_eq!(account.treasury_after_milli, Some(13_000));

    let saved = current.save("external-province-tax").unwrap();
    assert!(saved.contains("external_province_annual_milli"));
    let restored = World::restore(&saved, "external-province-tax", current.schema()).unwrap();
    assert_eq!(restored, current);
}

#[test]
fn unresolved_external_province_annual_tax_keeps_other_provinces_usable() {
    let mut current = scenario();
    let extra = current.add_entity("Extra", EntityKind::Province);
    current.set_owner(extra, EntityId(0)).unwrap();
    current.set(extra, FieldId(0), 6_000).unwrap();
    let mut attributes = current.attributes().clone();
    attributes
        .formulas
        .insert(extra, attributes.formulas[&EntityId(2)].clone());
    current.set_attributes(attributes).unwrap();
    current
        .set_taxation(
            Taxation::new(
                "annual_tax",
                [EntityId(0), EntityId(1)].into_iter().collect(),
            )
            .with_external_province_annual_milli(BTreeMap::from([(EntityId(2), None)])),
        )
        .unwrap();

    current.advance_to(20).unwrap();
    let account = &current
        .taxation()
        .unwrap()
        .last_settlement
        .as_ref()
        .unwrap()
        .accounts[&EntityId(0)];
    assert_eq!(account.known_subtotal_milli, 500);
    assert_eq!(account.income_milli, None);
    assert!(
        account
            .unresolved
            .contains("province:North:unknown_external_annual_tax")
    );
    assert_eq!(account.treasury_after_milli, None);
}

#[test]
fn unresolved_external_country_annual_tax_preserves_known_province_subtotal() {
    let mut current = with_country_tax(scenario(), false);
    current
        .set_taxation(
            Taxation::new("annual_tax", [EntityId(0), EntityId(1)].into())
                .with_country_annual_attribute("country_tax")
                .with_external_country_annual_milli(BTreeMap::from([(EntityId(0), None)])),
        )
        .unwrap();

    current.advance_to(20).unwrap();
    let account = &current
        .taxation()
        .unwrap()
        .last_settlement
        .as_ref()
        .unwrap()
        .accounts[&EntityId(0)];
    assert_eq!(account.known_subtotal_milli, 1_000);
    assert_eq!(account.income_milli, None);
    assert!(
        account
            .unresolved
            .contains("unknown_external_country_annual_tax")
    );
    assert_eq!(account.treasury_after_milli, None);
}

#[test]
fn monthly_autonomy_updates_before_tax_and_survives_save_resume() {
    let mut world = World::new(Schema::new(["annual_tax"]).unwrap());
    let country = world.add_entity("Alpha", EntityKind::Country);
    let province = world.add_entity("North", EntityKind::Province);
    world.set_owner(province, country).unwrap();
    world.set(province, FieldId(0), 24_000).unwrap();

    let mut attributes = Attributes::default();
    attributes.formulas.insert(
        province,
        [
            (
                "annual_tax".into(),
                AttributeFormula {
                    base: AttributeBase::Field {
                        field: FieldId(0),
                        micros_per_unit: 1_000,
                    },
                    channels: vec![AttributeChannel {
                        flat: vec![],
                        rates: vec![AttributeTerm::Autonomy],
                    }],
                    minimum_micros: None,
                    maximum_micros: None,
                    result_quantum_micros: None,
                    unresolved: BTreeSet::new(),
                },
            ),
            (
                MONTHLY_AUTONOMY_DELTA_ATTRIBUTE.into(),
                AttributeFormula {
                    base: AttributeBase::Constant { micros: -1_000 },
                    channels: vec![],
                    minimum_micros: None,
                    maximum_micros: None,
                    result_quantum_micros: None,
                    unresolved: BTreeSet::new(),
                },
            ),
        ]
        .into_iter()
        .collect(),
    );
    attributes.formulas.insert(
        country,
        [(
            "stock_income".into(),
            AttributeFormula {
                base: AttributeBase::Constant { micros: 0 },
                channels: vec![],
                minimum_micros: None,
                maximum_micros: None,
                result_quantum_micros: None,
                unresolved: BTreeSet::new(),
            },
        )]
        .into_iter()
        .collect(),
    );
    world.set_attributes(attributes).unwrap();
    world
        .set_administration(Administration {
            areas: ["test".into()].into(),
            country_areas: [(country, [("test".into(), AreaStatus::State)].into())].into(),
            provinces: [(
                province,
                ProvinceAdministration {
                    area: Some("test".into()),
                    cores_known: true,
                    cores: [(country, CoreType::Full)].into(),
                    raw_autonomy_micros: Some(35_000),
                    history_autonomy_delta_micros: 0,
                    unresolved: BTreeSet::new(),
                },
            )]
            .into(),
            rules: AdministrationRules {
                state_full_minimum_micros: Some(0),
                capital_maximum_micros: Some(1_000_000),
                ..Default::default()
            },
            ..Default::default()
        })
        .unwrap();
    world.set_calendar(calendar(date(1444, 11, 11))).unwrap();
    world
        .set_country_resources(
            [(
                country,
                CountryResources {
                    treasury_milli: Some(0),
                    manpower_milli: Some(0),
                    ..Default::default()
                },
            )]
            .into(),
        )
        .unwrap();
    world
        .set_taxation(Taxation::new("annual_tax", [country].into()))
        .unwrap();
    world
        .set_monthly_stocks(Some(MonthlyStocks {
            rules: [(
                country,
                [(
                    CountryResource::Manpower,
                    StockRule {
                        income_attribute: "stock_income".into(),
                        divisor: 1,
                        ceiling_attribute: None,
                        condition: None,
                    },
                )]
                .into(),
            )]
            .into(),
            settlement_count: 0,
            last_settlement: None,
        }))
        .unwrap();
    world
        .set_monthly_cashflows(Some(MonthlyExternalCashflows::new(
            [(country, Some(0))].into(),
        )))
        .unwrap();

    let mut continuous = world.clone();
    let mut resumed = world;
    continuous.advance_to(51).unwrap();
    resumed.advance_to(20).unwrap();
    assert_eq!(
        resumed.administration().provinces[&province].raw_autonomy_micros,
        Some(34_000)
    );
    assert_eq!(
        resumed
            .taxation()
            .unwrap()
            .last_settlement
            .as_ref()
            .unwrap()
            .accounts[&country]
            .income_milli,
        Some(1_932)
    );
    let snapshot = resumed.save("monthly-autonomy-tax-order:1").unwrap();
    resumed = World::restore(&snapshot, "monthly-autonomy-tax-order:1", resumed.schema()).unwrap();
    resumed.advance_to(51).unwrap();

    assert_eq!(
        resumed.administration().provinces[&province].raw_autonomy_micros,
        Some(33_000)
    );
    assert_eq!(
        resumed
            .taxation()
            .unwrap()
            .last_settlement
            .as_ref()
            .unwrap()
            .accounts[&country]
            .income_milli,
        Some(1_934)
    );
    assert_eq!(
        resumed.country_resources()[&country].treasury_milli,
        Some(3_866)
    );
    assert_eq!(resumed, continuous);
}

#[test]
fn tax_only_monthly_system_updates_autonomy_before_tax() {
    let mut world = World::new(Schema::new(["annual_tax"]).unwrap());
    let country = world.add_entity("Alpha", EntityKind::Country);
    let province = world.add_entity("North", EntityKind::Province);
    world.set_owner(province, country).unwrap();
    world.set(province, FieldId(0), 24_000).unwrap();
    world
        .set_attributes(Attributes {
            formulas: [(
                province,
                [
                    (
                        "annual_tax".into(),
                        AttributeFormula {
                            base: AttributeBase::Field {
                                field: FieldId(0),
                                micros_per_unit: 1_000,
                            },
                            channels: vec![AttributeChannel {
                                flat: vec![],
                                rates: vec![AttributeTerm::Autonomy],
                            }],
                            minimum_micros: None,
                            maximum_micros: None,
                            result_quantum_micros: None,
                            unresolved: BTreeSet::new(),
                        },
                    ),
                    (
                        MONTHLY_AUTONOMY_DELTA_ATTRIBUTE.into(),
                        AttributeFormula {
                            base: AttributeBase::Constant { micros: -1_000 },
                            channels: vec![],
                            minimum_micros: None,
                            maximum_micros: None,
                            result_quantum_micros: None,
                            unresolved: BTreeSet::new(),
                        },
                    ),
                ]
                .into_iter()
                .collect(),
            )]
            .into_iter()
            .collect(),
            ..Default::default()
        })
        .unwrap();
    world
        .set_administration(Administration {
            areas: ["test".into()].into(),
            country_areas: [(country, [("test".into(), AreaStatus::State)].into())].into(),
            provinces: [(
                province,
                ProvinceAdministration {
                    area: Some("test".into()),
                    cores_known: true,
                    cores: [(country, CoreType::Full)].into(),
                    raw_autonomy_micros: Some(35_000),
                    ..Default::default()
                },
            )]
            .into(),
            rules: AdministrationRules {
                state_full_minimum_micros: Some(0),
                capital_maximum_micros: Some(1_000_000),
                ..Default::default()
            },
            ..Default::default()
        })
        .unwrap();
    world
        .set_country_resources(
            [(
                country,
                CountryResources {
                    treasury_milli: Some(0),
                    ..Default::default()
                },
            )]
            .into(),
        )
        .unwrap();
    world.set_calendar(calendar(date(1444, 11, 11))).unwrap();
    world
        .set_taxation(Taxation::new("annual_tax", [country].into()))
        .unwrap();

    world.advance_to(20).unwrap();

    assert_eq!(
        world.administration().provinces[&province].raw_autonomy_micros,
        Some(34_000)
    );
    assert_eq!(
        world
            .taxation()
            .unwrap()
            .last_settlement
            .as_ref()
            .unwrap()
            .accounts[&country]
            .income_milli,
        Some(1_932)
    );
    assert_eq!(
        world.country_ledgers()[&country].income_sources_milli[0],
        1_932
    );
}

#[test]
fn monthly_tax_ledger_overflow_rolls_back_tax_and_treasury() {
    let mut world = scenario();
    let country = EntityId(0);
    let mut ledger = CountryLedger {
        income_sources_milli: vec![0; 19],
        ..Default::default()
    };
    ledger.income_sources_milli[0] = i64::MAX;
    world
        .set_country_ledgers([(country, ledger.clone())].into())
        .unwrap();
    let treasury = world.country_resources()[&country].treasury_milli;

    assert!(world.advance_to(20).is_err());

    assert_eq!(world.country_ledgers()[&country], ledger);
    assert_eq!(world.country_resources()[&country].treasury_milli, treasury);
    assert_eq!(world.taxation().unwrap().settlement_count, 0);
    assert!(world.taxation().unwrap().last_settlement.is_none());
}

#[test]
fn country_tax_configuration_rejects_missing_or_empty_attributes() {
    let mut current = scenario();
    let before = current.clone();
    for attribute in ["", "missing"] {
        assert!(
            current
                .set_taxation(
                    Taxation::new("annual_tax", [EntityId(0), EntityId(1)].into())
                        .with_country_annual_attribute(attribute),
                )
                .is_err()
        );
        assert_eq!(current, before);
    }
    assert!(
        !current
            .save("unchanged-tax")
            .unwrap()
            .contains("country_annual_attribute")
    );
}

#[test]
fn unknown_ownership_blocks_monthly_credit_until_the_missing_relation_is_resolved() {
    let mut current = scenario();
    let unknown = current.add_entity("Unresolved", EntityKind::Province);
    let snapshot = current.save("tax-ownership").unwrap();
    current.advance_to(20).unwrap();
    for country in [EntityId(0), EntityId(1)] {
        let account = &current
            .taxation()
            .unwrap()
            .last_settlement
            .as_ref()
            .unwrap()
            .accounts[&country];
        assert_eq!(account.income_milli, None);
        assert_eq!(account.credited_milli, None);
        assert_eq!(account.treasury_after_milli, None);
        assert!(account.unresolved.contains("unknown_province_ownership"));
    }
    assert_eq!(
        current
            .taxation()
            .unwrap()
            .last_settlement
            .as_ref()
            .unwrap()
            .accounts[&EntityId(0)]
            .known_subtotal_milli,
        1000
    );
    let mut resumed = World::restore(&snapshot, "tax-ownership", current.schema()).unwrap();
    resumed.advance_to(20).unwrap();
    assert_eq!(current, resumed);
    current.mark_target_absent(unknown, Target::Owner).unwrap();
    current.advance_to(51).unwrap();
    let account = &current
        .taxation()
        .unwrap()
        .last_settlement
        .as_ref()
        .unwrap()
        .accounts[&EntityId(0)];
    assert_eq!(account.income_milli, Some(1000));
    assert_eq!(account.credited_milli, None);
    assert!(account.unresolved.contains("unknown_treasury"));

    let mut resolved_before_settlement =
        World::restore(&snapshot, "tax-ownership", resumed.schema()).unwrap();
    resolved_before_settlement
        .mark_target_absent(unknown, Target::Owner)
        .unwrap();
    resolved_before_settlement.advance_to(20).unwrap();
    assert_eq!(balance(&resolved_before_settlement, 0), Some(11_000));
    assert_eq!(balance(&resolved_before_settlement, 1), Some(22_000));
}

#[test]
fn modifier_expiry_then_due_effects_precede_monthly_attribute_reads() {
    let mut world = scenario();
    world.set_modifiers(serde_json::from_str(r#"{"definitions":{"boost":{"values_micros":{"rate":1000000},"flags":{}}},"active":{}}"#).unwrap()).unwrap();
    let mut attributes = world.attributes().clone();
    attributes
        .formulas
        .get_mut(&EntityId(2))
        .unwrap()
        .get_mut("annual_tax")
        .unwrap()
        .channels
        .push(augustus_core::AttributeChannel {
            flat: vec![],
            rates: vec![augustus_core::AttributeTerm::Modifiers {
                target: Target::Owner,
                property: "rate".into(),
            }],
        });
    world.set_attributes(attributes).unwrap();
    world
        .attach_modifier(
            EntityId(0),
            "boost",
            Some(20),
            None,
            augustus_core::ModifierRenewal::Replace,
        )
        .unwrap();
    world
        .schedule(
            EntityId(0),
            20,
            vec![
                Action::If {
                    condition: augustus_core::Condition::HasModifier {
                        target: Target::Current,
                        name: "boost".into(),
                    },
                    then: vec![Action::Emit("stale".into())],
                    otherwise: vec![Action::Emit("expired".into())],
                },
                Action::AttachModifier {
                    target: Target::Current,
                    name: "boost".into(),
                    duration: Some(20),
                    hidden: None,
                    renewal: augustus_core::ModifierRenewal::Replace,
                    duration_label: None,
                },
            ],
        )
        .unwrap();
    let saved = world.save("tax-modifiers").unwrap();
    let mut resumed = World::restore(&saved, "tax-modifiers", world.schema()).unwrap();
    for state in [&mut world, &mut resumed] {
        state.advance_to(20).unwrap();
        assert_eq!(state.events()[0].2, "expired");
        assert_eq!(balance(state, 0), Some(12000));
        state.advance_to(51).unwrap();
        assert_eq!(balance(state, 0), Some(13000));
        assert!(!state.has_modifier(EntityId(0), "boost").unwrap());
    }
    assert_eq!(
        world.save("tax-modifiers").unwrap(),
        resumed.save("tax-modifiers").unwrap()
    );
}

#[test]
fn calendar_has_content_defined_month_lengths_and_checked_dates() {
    let c = calendar(date(1444, 2, 28));
    assert_eq!(c.date(1).unwrap(), date(1444, 3, 1));
    assert_eq!(c.date(365).unwrap(), date(1445, 2, 28));
    assert_eq!(c.day_offset(date(1444, 3, 1)).unwrap(), 1);
    assert!(c.day_offset(date(1444, 2, 27)).is_err());
    assert_eq!(c.next_month(0).unwrap(), 1);
    assert_eq!(
        calendar(date(1600, 2, 28)).date(1).unwrap(),
        date(1600, 3, 1)
    );
    let native = Calendar {
        start: date(1, 1, 30),
        month_lengths: [30; 12],
    };
    assert_eq!(native.date(1).unwrap(), date(1, 2, 1));
    assert_eq!(native.date(360).unwrap(), date(2, 1, 30));
    for invalid in [
        date(0, 1, 1),
        date(1444, 0, 1),
        date(1444, 13, 1),
        date(1444, 2, 29),
        date(1444, 1, 0),
    ] {
        assert!(calendar(invalid).date(0).is_err());
    }
    assert!(
        Calendar {
            start: date(1444, 1, 1),
            month_lengths: [0; 12]
        }
        .date(0)
        .is_err()
    );
    assert!(calendar(date(i16::MAX, 12, 31)).date(1).is_err());
    let mut world = scenario();
    let before = world.clone();
    assert!(world.advance_to(u64::MAX).is_err());
    assert_eq!(world, before);
}

#[test]
fn crossings_settle_once_and_a_year_has_twelve_receipts() {
    let mut world = scenario();
    world.advance_to(19).unwrap();
    assert_eq!(world.date().unwrap(), Some(date(1444, 11, 30)));
    assert_eq!(world.taxation().unwrap().settlement_count, 0);
    assert_eq!(balance(&world, 0), Some(10_000));
    world.advance_to(20).unwrap();
    assert_eq!(world.date().unwrap(), Some(date(1444, 12, 1)));
    assert_eq!(balance(&world, 0), Some(11_000));
    assert_eq!(balance(&world, 1), Some(22_000));
    let once = world.clone();
    world.advance_to(20).unwrap();
    assert_eq!(world, once);
    world.advance_to(51).unwrap();
    assert_eq!(world.date().unwrap(), Some(date(1445, 1, 1)));
    world.advance_to(365).unwrap();
    assert_eq!(world.date().unwrap(), Some(date(1445, 11, 11)));
    assert_eq!(world.taxation().unwrap().settlement_count, 12);
    assert_eq!(balance(&world, 0), Some(22_000));
    assert_eq!(balance(&world, 1), Some(44_000));
    assert!(world.advance_to(364).is_err());
}

#[test]
fn first_of_month_start_does_not_generate_an_initial_receipt() {
    let mut world = scenario();
    world.set_calendar(calendar(date(1444, 11, 1))).unwrap();
    world.advance_to(0).unwrap();
    assert_eq!(world.taxation().unwrap().settlement_count, 0);
    world.advance_to(30).unwrap();
    assert_eq!(world.taxation().unwrap().settlement_count, 1);
    assert_eq!(world.date().unwrap(), Some(date(1444, 12, 1)));
}

#[test]
fn taxation_reads_current_ownership_and_rounds_after_aggregation() {
    let mut world = scenario();
    world.set_owner(EntityId(3), EntityId(0)).unwrap();
    world.set(EntityId(2), FieldId(0), 1006).unwrap();
    world.set(EntityId(3), FieldId(0), 1006).unwrap();
    world.advance_to(20).unwrap();
    assert_eq!(balance(&world, 0), Some(10_167));
    assert_eq!(balance(&world, 1), Some(20_000));
    world.set_owner(EntityId(2), EntityId(1)).unwrap();
    world.set(EntityId(2), FieldId(0), 36_000).unwrap();
    world.advance_to(51).unwrap();
    assert_eq!(balance(&world, 0), Some(10_250));
    assert_eq!(balance(&world, 1), Some(23_000));
}

#[test]
fn due_tasks_precede_tax_and_resumed_execution_matches_daily_and_continuous_runs() {
    let mut continuous = scenario();
    for annual in [24_000, 36_000] {
        continuous
            .schedule(
                EntityId(2),
                20,
                vec![Action::Set {
                    target: Target::Current,
                    field: FieldId(0),
                    value: annual,
                }],
            )
            .unwrap();
    }
    continuous
        .schedule(
            EntityId(0),
            51,
            vec![Action::RandomBranch {
                then: vec![Action::Emit("branch a".into())],
                otherwise: vec![Action::Emit("branch b".into())],
            }],
        )
        .unwrap();
    let mut daily = continuous.clone();
    let mut resumed = continuous.clone();
    resumed.advance_to(20).unwrap();
    assert_eq!(balance(&resumed, 0), Some(13_000));
    let snapshot = resumed.save("independent-tax-fixture:1").unwrap();
    resumed = World::restore(&snapshot, "independent-tax-fixture:1", resumed.schema()).unwrap();
    resumed.advance_to(365).unwrap();
    continuous.advance_to(365).unwrap();
    for day in 1..=365 {
        daily.advance_to(day).unwrap();
    }
    assert_eq!(resumed, continuous);
    assert_eq!(daily, continuous);
    assert_eq!(balance(&resumed, 0), Some(46_000));
}

#[test]
fn unknown_income_taints_the_balance_and_known_subtotals_are_never_credited() {
    let mut world = scenario();
    let mut attributes = world.attributes().clone();
    attributes
        .formulas
        .get_mut(&EntityId(2))
        .unwrap()
        .get_mut("annual_tax")
        .unwrap()
        .unresolved
        .insert("pending_rule".into());
    world.set_attributes(attributes.clone()).unwrap();
    world.advance_to(20).unwrap();
    let tax = &world
        .taxation()
        .unwrap()
        .last_settlement
        .as_ref()
        .unwrap()
        .accounts[&EntityId(0)];
    assert_eq!(tax.known_subtotal_milli, 1000);
    assert_eq!(tax.income_milli, None);
    assert_eq!(tax.credited_milli, None);
    assert_eq!(tax.treasury_before_milli, Some(10_000));
    assert_eq!(balance(&world, 0), None);
    assert_eq!(balance(&world, 1), Some(22_000));
    attributes
        .formulas
        .get_mut(&EntityId(2))
        .unwrap()
        .get_mut("annual_tax")
        .unwrap()
        .unresolved
        .clear();
    world.set_attributes(attributes).unwrap();
    world.advance_to(51).unwrap();
    let tax = &world
        .taxation()
        .unwrap()
        .last_settlement
        .as_ref()
        .unwrap()
        .accounts[&EntityId(0)];
    assert_eq!(tax.income_milli, Some(1000));
    assert_eq!(tax.credited_milli, None);
    assert!(tax.unresolved.contains("unknown_treasury"));
    assert_eq!(balance(&world, 0), None);
    let saved = world.save("unknown-fixture").unwrap();
    assert_eq!(
        World::restore(&saved, "unknown-fixture", world.schema()).unwrap(),
        world
    );
}

#[test]
fn country_tax_clamps_at_the_native_treasury_maximum() {
    let mut world = scenario();
    let mut resources = world.country_resources().clone();
    resources.get_mut(&EntityId(1)).unwrap().treasury_milli = Some(999_999_500);
    world.set_country_resources(resources).unwrap();

    world.advance_to(20).unwrap();
    assert_eq!(balance(&world, 0), Some(11_000));
    assert_eq!(balance(&world, 1), Some(1_000_000_000));
    assert_eq!(world.taxation().unwrap().settlement_count, 1);
    assert!(world.save("treasury-tax-bound").is_ok());
}

#[test]
fn corrupted_receipts_and_provider_selections_are_rejected() {
    let mut world = scenario();
    world.advance_to(51).unwrap();
    let snapshot: serde_json::Value =
        serde_json::from_str(&world.save("fixture").unwrap()).unwrap();
    let mut bad = snapshot.clone();
    bad["taxation"]["settlement_count"] = 1.into();
    assert!(World::restore(&bad.to_string(), "fixture", world.schema()).is_err());
    bad = snapshot.clone();
    bad["taxation"]["last_settlement"]["day"] = 20.into();
    assert!(World::restore(&bad.to_string(), "fixture", world.schema()).is_err());
    bad = snapshot.clone();
    bad["taxation"]["last_settlement"]["accounts"]["0"]["credited_milli"] = 999.into();
    assert!(World::restore(&bad.to_string(), "fixture", world.schema()).is_err());
    bad = snapshot;
    bad["monthly_system_id"] = "different-system:1".into();
    let mut restored = World::restore(&bad.to_string(), "fixture", world.schema()).unwrap();
    assert_eq!(restored.monthly_system_id(), Some("different-system:1"));
    assert!(
        restored
            .advance_with_monthly_system(51, &augustus_core::MonthlyTax)
            .is_err()
    );
}

struct NativeMonthly;

impl MonthlySystem for NativeMonthly {
    fn id(&self) -> &str {
        "native-monthly-fixture:1"
    }
    fn validate(&self, world: &World) -> Result<(), augustus_core::Error> {
        world.value(EntityId(0), FieldId(0)).map(|_| ())
    }
    fn settle(&self, world: &mut World) -> Result<(), augustus_core::Error> {
        world.execute(
            EntityId(0),
            &[Action::Add {
                target: Target::Current,
                field: FieldId(0),
                value: 777,
            }],
        )
    }
}

#[test]
fn native_executor_replaces_the_whole_monthly_system_and_is_identified_in_saves() {
    let mut world = scenario();
    world.select_monthly_system(&NativeMonthly).unwrap();
    assert!(world.taxation().is_none());
    assert!(world.advance_to(20).is_err());
    world
        .advance_with_monthly_system(20, &NativeMonthly)
        .unwrap();
    assert_eq!(world.value(EntityId(0), FieldId(0)).unwrap(), 777);
    assert_eq!(balance(&world, 0), Some(10_000));
    let saved = world.save("native-monthly-content").unwrap();
    let mut restored = World::restore(&saved, "native-monthly-content", world.schema()).unwrap();
    assert_eq!(
        restored.monthly_system_id(),
        Some("native-monthly-fixture:1")
    );
    assert!(
        restored
            .advance_with_monthly_system(51, &augustus_core::MonthlyTax)
            .is_err()
    );
    restored
        .advance_with_monthly_system(365, &NativeMonthly)
        .unwrap();
    world
        .advance_with_monthly_system(365, &NativeMonthly)
        .unwrap();
    assert_eq!(restored, world);
    assert_eq!(world.value(EntityId(0), FieldId(0)).unwrap(), 12 * 777);
    assert!(world.select_monthly_system(&NativeMonthly).is_err());
}
