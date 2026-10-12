// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::*;
use augustus_eu4::*;
use std::{fs, path::Path};

fn scene() -> World {
    let mut world = World::new(Schema::new(["unused"]).unwrap());
    world.add_entity("AAA", EntityKind::Country);
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
        .set_politics(Politics {
            ruler_skill_bounds: Some(RulerSkillBounds {
                minimum: 0,
                maximum: 6,
            }),
            countries: [(
                EntityId(0),
                CountryPolitics {
                    national_focus: Some(NationalFocus::default()),
                    ruler: Some(PersonState {
                        administration: Some(2),
                        diplomacy: Some(3),
                        military: Some(4),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            )]
            .into(),
            ..Default::default()
        })
        .unwrap();
    let definitions = ["adm_power", "dip_power", "mil_power"]
        .into_iter()
        .map(|name| {
            (
                name.into(),
                MetricDefinition {
                    kind: EntityKind::Country,
                    minimum: if name == "dip_power" { -999 } else { -100 },
                    maximum: i64::MAX,
                    baseline: 0,
                    divisor: 1,
                    modifier: None,
                    unresolved_properties: Default::default(),
                    below_baseline: None,
                    growth_ceiling: true,
                },
            )
        })
        .collect();
    world
        .set_metrics(Metrics {
            definitions,
            entities: [(
                EntityId(0),
                ["adm_power", "dip_power", "mil_power"]
                    .into_iter()
                    .map(|k| (k.into(), Some(10)))
                    .collect(),
            )]
            .into(),
            ceilings: [(
                EntityId(0),
                ["adm_power", "dip_power", "mil_power"]
                    .into_iter()
                    .map(|k| (k.into(), Some(20)))
                    .collect(),
            )]
            .into(),
        })
        .unwrap();
    world
        .set_modifiers(Modifiers {
            definitions: ["first", "second"]
                .into_iter()
                .map(|name| {
                    (
                        name.into(),
                        ModifierDefinition {
                            values_micros: [
                                "country_admin_power",
                                "country_diplomatic_power",
                                "country_military_power",
                            ]
                            .into_iter()
                            .map(|k| (k.into(), 600_000))
                            .collect(),
                            ..Default::default()
                        },
                    )
                })
                .collect(),
            ..Default::default()
        })
        .unwrap();
    let mut attributes = Attributes::default();
    let mut accruals = MonthlyAccruals::default();
    for (name, skill, property) in [
        ("adm", RulerSkill::Administration, "country_admin_power"),
        ("dip", RulerSkill::Diplomacy, "country_diplomatic_power"),
        ("mil", RulerSkill::Military, "country_military_power"),
    ] {
        let attribute = format!("monthly_{name}");
        attributes.formulas.entry(EntityId(0)).or_default().insert(
            attribute.clone(),
            AttributeFormula {
                base: AttributeBase::Constant { micros: 3_000_000 },
                channels: vec![AttributeChannel {
                    flat: vec![
                        AttributeTerm::RulerSkill {
                            target: Target::Current,
                            skill,
                            micros_per_unit: 1_000_000,
                        },
                        AttributeTerm::NationalFocus {
                            target: Target::Current,
                            skill,
                            increase_micros: 2_000_000,
                            decrease_micros: -1_000_000,
                        },
                        AttributeTerm::TruncatedSum {
                            source: "power_sources".into(),
                            quantum_micros: 1_000_000,
                            terms: vec![AttributeTerm::Modifiers {
                                target: Target::Current,
                                property: property.into(),
                            }],
                        },
                    ],
                    rates: vec![],
                }],
                minimum_micros: None,
                maximum_micros: None,
                unresolved: Default::default(),
            },
        );
        accruals.rules.entry(EntityId(0)).or_default().insert(
            format!("{name}_power"),
            AccrualRule {
                attribute,
                divisor: 1_000_000,
            },
        );
    }
    world.set_attributes(attributes).unwrap();
    world.set_monthly_accruals(accruals).unwrap();
    world
}

#[test]
fn translated_focus_income_conditions_and_month_boundary_share_authoritative_state() {
    let text = "country_event={id=income.start is_triggered_only=yes immediate={
        if={limit={monthly_adm=5 monthly_dip=6 monthly_mil=7} log=initial}
        change_national_focus=ADM
        if={limit={national_focus=ADM monthly_adm=7 NOT={monthly_dip=6}} log=focused}
        add_country_modifier={name=first duration=-1} add_country_modifier={name=second duration=-1}
        change_mil=1
        if={limit={monthly_adm=8 monthly_dip=6 monthly_mil=8.999 NOT={monthly_mil=9}} log=modified}
        country_event={id=income.month days=20}
    } option={name=continue}}
    country_event={id=income.month is_triggered_only=yes immediate={
        if={limit={adm_power=10 NOT={adm_power=11}} log=before_credit}
        change_national_focus=none
        if={limit={NOT={national_focus=ADM} monthly_adm=6 monthly_dip=7 monthly_mil=9} log=cleared}
    } option={name=continue}}";
    let doc = TextDocument::from_bytes(
        "events/self_authored.txt",
        text.as_bytes().to_vec(),
        TextEncoding::Utf8,
    );
    let mut snapshots = Vec::new();
    for optimise in [false, true] {
        let mut world = scene();
        let library = ScriptLibrary::new();
        let compiled = ScriptCompiler::new(world.schema(), &library, optimise)
            .with_metric_ids(world.metrics().definitions.keys().cloned().collect())
            .with_modifier_ids(world.modifiers().definitions.keys().cloned().collect())
            .compile_events(&doc)
            .unwrap();
        world
            .set_event_catalog(EventCatalog::new(compiled.definitions).unwrap())
            .unwrap();
        world.trigger_event("income.start", EntityId(0)).unwrap();
        world.advance_to(19).unwrap();
        let snapshot = world.save("fixture").unwrap();
        let mut resumed = World::restore_with_events(
            &snapshot,
            "fixture",
            world.schema(),
            vec![],
            world.event_catalog().clone(),
        )
        .unwrap();
        world.advance_to(20).unwrap();
        resumed.advance_to(20).unwrap();
        assert_eq!(
            world.metrics().entities[&EntityId(0)],
            [
                ("adm_power".into(), Some(16)),
                ("dip_power".into(), Some(17)),
                ("mil_power".into(), Some(19))
            ]
            .into()
        );
        assert_eq!(
            world
                .events()
                .iter()
                .map(|(_, _, s)| s.as_str())
                .collect::<Vec<_>>(),
            ["initial", "focused", "modified", "before_credit", "cleared"]
        );
        assert_eq!(
            world.save("fixture").unwrap(),
            resumed.save("fixture").unwrap()
        );
        snapshots.push(world.save("fixture").unwrap());
    }
    assert_eq!(snapshots[0], snapshots[1]);
}

#[test]
fn invalid_focus_income_operands_and_scopes_keep_source_diagnostics() {
    let world = scene();
    let library = ScriptLibrary::new();
    for body in [
        "change_national_focus=other",
        "if={limit={national_focus=none} log=bad}",
        "capital_scope={change_national_focus=ADM}",
        "capital_scope={if={limit={monthly_adm=1} log=bad}}",
        "if={limit={monthly_dip=2147483648} log=bad}",
    ] {
        let text = format!("country_event={{id=bad immediate={{{body}}} option={{name=ok}}}}");
        let doc = TextDocument::from_bytes(
            "events/self_authored.txt",
            text.into_bytes(),
            TextEncoding::Utf8,
        );
        let error = ScriptCompiler::new(world.schema(), &library, true)
            .compile_events(&doc)
            .unwrap_err();
        assert!(error.location().is_some());
        assert!(error.to_string().contains("self_authored.txt"));
    }
}

#[test]
fn history_focus_defines_and_unknown_producers_are_reported_without_guessed_stock() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world");
    let temp = tempfile::tempdir().unwrap();
    for entry in walkdir::WalkDir::new(source.clone()) {
        let entry = entry.unwrap();
        let target = temp
            .path()
            .join(entry.path().strip_prefix(&source).unwrap());
        if entry.file_type().is_dir() {
            fs::create_dir_all(target).unwrap();
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
    let defines = temp.path().join("common/defines.lua");
    let original = fs::read_to_string(&defines).unwrap();
    fs::write(&defines,original.replace("HRE_RANK = 3,","HRE_RANK = 3,\nBASE_POWER_INCREASE = 5,\nNAT_FOCUS_INCREASE = 4,\nNAT_FOCUS_DECREASE = -3,")).unwrap();
    let history = fs::read_dir(temp.path().join("history/countries"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| p.file_name().unwrap().to_string_lossy().starts_with("AAA"))
        .unwrap();
    let original_history = fs::read_to_string(&history).unwrap();
    fs::write(&history,format!("{original_history}\n1444.11.10={{national_focus=MIL}}\n1444.11.12={{national_focus=ADM}}\n")).unwrap();
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let world = loaded.initial.world();
    let country = loaded.initial.country_entity("AAA").unwrap();
    assert_eq!(
        world.national_focus(country).unwrap().skill,
        Some(RulerSkill::Military)
    );
    assert_eq!(
        loaded.report.setup.monarch_power_income_defines["BASE_POWER_INCREASE"],
        5
    );
    let evaluation = world
        .attributes()
        .evaluate(world, country, "monthly_mil")
        .unwrap();
    assert!(evaluation.value_micros.is_none());
    assert!(
        evaluation
            .unresolved
            .contains("power_income:advisors_not_initialized")
    );
    assert_eq!(world.metrics().entities[&country]["adm_power"], None);
    assert!(world.monthly_accruals().is_some());
    fs::write(
        &history,
        format!(
            "{original_history}\n1444.11.10={{if={{limit={{always=yes}} national_focus=MIL}}}}\n"
        ),
    )
    .unwrap();
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(loaded.initial.world().national_focus(country).is_err());
    fs::write(&defines, original).unwrap();
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let evaluation = loaded
        .initial
        .world()
        .attributes()
        .evaluate(loaded.initial.world(), country, "monthly_adm")
        .unwrap();
    assert!(
        evaluation
            .unresolved
            .contains("power_income:missing_base_define")
    );
}
