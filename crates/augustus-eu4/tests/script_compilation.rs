// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    Action, AdvisorState, Advisors, AttributeBase, AttributeChannel, AttributeFormula,
    AttributeTerm, Attributes, Calendar, EntityKind, EventCatalog, RulerSkill, Schema, Target,
    World, WorldDate,
};
use augustus_eu4::{
    ContentRoot, ContentSet, ContentSource, DefinitionKind, ScriptCompiler, ScriptLibrary,
    TextDocument, TextEncoding,
};

fn document(text: &str) -> TextDocument {
    TextDocument::from_bytes(
        "self-authored.txt",
        text.as_bytes().to_vec(),
        TextEncoding::Utf8,
    )
}

#[test]
fn remove_advisor_by_category_compiles_and_clears_only_that_category() {
    let schema = Schema::new([] as [&str; 0]).unwrap();
    let compiled = ScriptCompiler::new(&schema, &ScriptLibrary::new(), true)
        .compile(
            &document("remove_advisor_by_category = MIL"),
            EntityKind::Country,
        )
        .unwrap();
    assert!(matches!(
        compiled.plan.as_slice(),
        [Action::RemoveAdvisorByCategory {
            target: Target::Current,
            category: RulerSkill::Military
        }]
    ));

    let mut world = World::new(schema.clone());
    let country = world.add_entity("AAA", EntityKind::Country);
    world
        .set_advisors(Advisors {
            countries: [(
                country,
                Some(vec![
                    AdvisorState {
                        type_name: "treasurer".into(),
                        category: RulerSkill::Administration,
                        skill: 2,
                        cost_multiplier_milli: None,
                    },
                    AdvisorState {
                        type_name: "army_reformer".into(),
                        category: RulerSkill::Military,
                        skill: 1,
                        cost_multiplier_milli: None,
                    },
                ]),
            )]
            .into(),
            ..Default::default()
        })
        .unwrap();
    world
        .set_attributes(Attributes {
            formulas: [(
                country,
                [(
                    "monthly_mil".into(),
                    AttributeFormula {
                        base: AttributeBase::Constant { micros: 3_000_000 },
                        channels: vec![AttributeChannel {
                            flat: vec![AttributeTerm::AdvisorSkill {
                                target: Target::Current,
                                skill: RulerSkill::Military,
                                micros_per_unit: 1_000_000,
                            }],
                            rates: Vec::new(),
                        }],
                        minimum_micros: None,
                        maximum_micros: None,
                        result_quantum_micros: None,
                        unresolved: Default::default(),
                    },
                )]
                .into(),
            )]
            .into(),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(
        world
            .attributes()
            .evaluate(&world, country, "monthly_mil")
            .unwrap()
            .value_micros,
        Some(4_000_000)
    );
    world.execute(country, &compiled.plan).unwrap();
    assert_eq!(
        world.advisors().countries[&country],
        Some(vec![AdvisorState {
            type_name: "treasurer".into(),
            category: RulerSkill::Administration,
            skill: 2,
            cost_multiplier_milli: None,
        }])
    );
    assert_eq!(
        world
            .attributes()
            .evaluate(&world, country, "monthly_mil")
            .unwrap()
            .value_micros,
        Some(3_000_000)
    );

    let snapshot = world.save("advisor-removal").unwrap();
    let restored = World::restore(&snapshot, "advisor-removal", world.schema()).unwrap();
    assert_eq!(
        restored
            .attributes()
            .evaluate(&restored, country, "monthly_mil")
            .unwrap()
            .value_micros,
        Some(3_000_000)
    );
}

#[test]
fn advisor_removal_updates_unknown_slot_and_invalid_operands_are_rejected() {
    let schema = Schema::new([] as [&str; 0]).unwrap();
    let library = ScriptLibrary::new();
    let compiled = ScriptCompiler::new(&schema, &library, true)
        .compile(
            &document("remove_advisor_by_category = DIP"),
            EntityKind::Country,
        )
        .unwrap();
    let mut unknown = World::new(schema.clone());
    let country = unknown.add_entity("AAA", EntityKind::Country);
    unknown.execute(country, &compiled.plan).unwrap();
    assert_eq!(
        unknown
            .advisors()
            .advisor_in_category(country, RulerSkill::Diplomacy),
        Some(None)
    );
    assert_eq!(
        unknown
            .advisors()
            .advisor_in_category(country, RulerSkill::Administration),
        None
    );

    for (text, scope) in [
        ("remove_advisor_by_category = ART", EntityKind::Country),
        ("remove_advisor_by_category = MIL", EntityKind::Province),
    ] {
        assert!(
            ScriptCompiler::new(&schema, &library, true)
                .compile(&document(text), scope)
                .is_err()
        );
    }
}

#[test]
fn hire_advisor_compiles_with_content_category_and_updates_only_that_slot() {
    let schema = Schema::new([] as [&str; 0]).unwrap();
    let library = ScriptLibrary::new()
        .with_advisor_types([("army_reformer".into(), RulerSkill::Military)].into());
    let compiled = ScriptCompiler::new(&schema, &library, true)
        .compile(
            &document(
                "remove_advisor_by_category = MIL hire_advisor = { type = army_reformer skill = 1 cost_multiplier = 1.0 }",
            ),
            EntityKind::Country,
        )
        .unwrap();
    assert!(matches!(
        compiled.plan.as_slice(),
        [
            Action::RemoveAdvisorByCategory {
                category: RulerSkill::Military,
                ..
            },
            Action::HireAdvisor {
                advisor: AdvisorState {
                    type_name,
                    category: RulerSkill::Military,
                    skill: 1,
                    cost_multiplier_milli: Some(1_000),
                },
                ..
            }
        ] if type_name == "army_reformer"
    ));

    let mut world = World::new(schema.clone());
    let country = world.add_entity("AAA", EntityKind::Country);
    world.execute(country, &compiled.plan).unwrap();
    let military = world
        .advisors()
        .advisor_in_category(country, RulerSkill::Military)
        .unwrap()
        .unwrap();
    assert_eq!(military.type_name, "army_reformer");
    assert_eq!(military.cost_multiplier_milli, Some(1_000));
    assert_eq!(
        world
            .advisors()
            .advisor_in_category(country, RulerSkill::Administration),
        None
    );

    let snapshot = world.save("advisor-hire").unwrap();
    let restored = World::restore(&snapshot, "advisor-hire", world.schema()).unwrap();
    assert_eq!(
        restored
            .advisors()
            .advisor_in_category(country, RulerSkill::Military)
            .unwrap()
            .unwrap()
            .cost_multiplier_milli,
        Some(1_000)
    );
}

#[test]
fn hire_advisor_rejects_unknown_types_and_unresolved_selectors() {
    let schema = Schema::new([] as [&str; 0]).unwrap();
    let library = ScriptLibrary::new()
        .with_advisor_types([("army_reformer".into(), RulerSkill::Military)].into());
    for text in [
        "hire_advisor = { type = missing_advisor skill = 1 }",
        "hire_advisor = { type = army_reformer skill = 4 }",
        "hire_advisor = { type = army_reformer skill = 1.5 }",
        "hire_advisor = { type = army_reformer skill = 1 religion = ROOT }",
    ] {
        assert!(
            ScriptCompiler::new(&schema, &library, true)
                .compile(&document(text), EntityKind::Country)
                .is_err(),
            "unexpectedly compiled {text}"
        );
    }
}

#[test]
fn delayed_advisor_replacement_executes_after_save_restore() {
    let schema = Schema::new([] as [&str; 0]).unwrap();
    let library = ScriptLibrary::new()
        .with_advisor_types([("army_reformer".into(), RulerSkill::Military)].into());
    let events = ScriptCompiler::new(&schema, &library, true)
        .compile_events(&document(
            "country_event={id=probe.1 is_triggered_only=yes hidden=yes immediate={remove_advisor_by_category=MIL hire_advisor={type=army_reformer skill=2 cost_multiplier=0.5}}}",
        ))
        .unwrap();
    let catalog = EventCatalog::new(events.definitions)
        .unwrap()
        .with_conditions(events.conditions)
        .unwrap()
        .with_once_events(events.once_events)
        .unwrap();
    let call = ScriptCompiler::new(&schema, &library, true)
        .compile(
            &document("country_event={id=probe.1 days=25}"),
            EntityKind::Country,
        )
        .unwrap();

    let mut world = World::new(schema.clone());
    let country = world.add_entity("AAA", EntityKind::Country);
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
        .set_advisors(Advisors {
            countries: [(
                country,
                Some(vec![AdvisorState {
                    type_name: "army_reformer".into(),
                    category: RulerSkill::Military,
                    skill: 1,
                    cost_multiplier_milli: None,
                }]),
            )]
            .into(),
            ..Default::default()
        })
        .unwrap();
    world.set_event_catalog(catalog).unwrap();
    world.execute(country, &call.plan).unwrap();
    world.advance_to(24).unwrap();
    assert!(matches!(
        world.advisors().countries[&country],
        Some(ref advisors) if advisors.len() == 1
    ));

    let mut resumed = World::restore_with_events(
        &world.save("advisor-delay").unwrap(),
        "advisor-delay",
        world.schema(),
        Vec::new(),
        world.event_catalog().clone(),
    )
    .unwrap();
    resumed.advance_to(25).unwrap();
    assert_eq!(
        resumed.advisors().countries[&country],
        Some(vec![AdvisorState {
            type_name: "army_reformer".into(),
            category: RulerSkill::Military,
            skill: 2,
            cost_multiplier_milli: Some(500),
        }])
    );
}

#[test]
fn conditional_trigger_chains_keep_branch_results_empty_defaults_and_scopes() {
    let cases = [
        ("if={limit={always=no} always=no}", true),
        ("if={limit={always=yes} always=no}", false),
        ("if={limit={always=yes} always=yes}", true),
        ("if={limit={always=yes}}", true),
        ("if={limit={always=no}}", true),
        ("not_in_mission_preview_mode={key=BOH_RELIGION}", true),
        ("NOT={map_setup=map_setup_random}", true),
        ("was_tag=GOL", false),
        ("if={limit={} always=no}", false),
        (
            "if={limit={always=yes} always=no} else_if={limit={always=yes} always=yes} else={always=yes}",
            false,
        ),
        (
            "if={limit={always=no} always=no} else_if={limit={always=yes} always=yes} else={always=no}",
            true,
        ),
        (
            "if={limit={always=no} always=no} else_if={limit={always=yes} always=no} else={always=yes}",
            false,
        ),
        (
            "if={limit={always=no} always=no} else_if={limit={always=no} always=no}",
            true,
        ),
        (
            "if={limit={always=no} always=yes} else_if={limit={always=no} always=yes} else={always=no}",
            false,
        ),
        (
            "if={limit={always=no} always=no} else_if={limit={always=no} always=no} else={always=yes}",
            true,
        ),
        ("if={limit={always=no} always=no} else={}", true),
        ("if={limit={always=yes} always=yes always=no}", false),
        ("if={limit={always=yes always=no} always=no}", true),
        ("always=no if={limit={always=no} always=yes}", false),
        (
            "if={limit={always=no} always=no} if={limit={always=yes} always=no}",
            false,
        ),
        (
            "if={limit={always=yes} if={limit={always=no} always=no}}",
            true,
        ),
        (
            "if={limit={always=yes} if={limit={always=yes} always=no}}",
            false,
        ),
        ("OR={if={limit={always=no} always=no} always=no}", true),
        ("NOT={if={limit={always=no} always=no}}", false),
        (
            "if={limit={always=no} always=no} else_if={limit={always=yes} always=yes} else_if={limit={always=yes} always=no} else={always=no}",
            true,
        ),
        (
            "if={limit={tag=AAA} capital_scope={owner={tag=AAA}}} else={always=no}",
            true,
        ),
        (
            "capital_scope={if={limit={owner={tag=AAA}} owner={tag=AAA}} else={always=no}}",
            true,
        ),
        (
            "capital_scope={if={limit={owner={tag=AAA}} owner={tag=BBB}} else={always=yes}}",
            false,
        ),
    ];
    let schema = Schema::new([] as [&str; 0]).unwrap();
    let library = ScriptLibrary::new();
    for (trigger, expected) in cases {
        let mut snapshots = Vec::new();
        for optimise in [false, true] {
            let compiled = ScriptCompiler::new(&schema, &library, optimise)
                .compile(
                    &document(&format!(
                        "if={{limit={{{trigger}}} log=accepted}} else={{log=rejected}}"
                    )),
                    EntityKind::Country,
                )
                .unwrap();
            let mut current = World::new(schema.clone());
            let country = current.add_entity("AAA", EntityKind::Country);
            current.add_entity("BBB", EntityKind::Country);
            let capital = current.add_entity("Port", EntityKind::Province);
            current.set_owner(capital, country).unwrap();
            current.set_capital(country, capital).unwrap();
            current.execute(country, &compiled.plan).unwrap();
            assert_eq!(
                current.events().last().unwrap().2,
                if expected { "accepted" } else { "rejected" },
                "{trigger}; optimise={optimise}"
            );
            snapshots.push(current.save("conditional-triggers").unwrap());
        }
        assert_eq!(snapshots[0], snapshots[1]);
    }
}

#[test]
fn conditional_trigger_syntax_is_checked_in_dormant_branches() {
    let schema = Schema::new([] as [&str; 0]).unwrap();
    let library = ScriptLibrary::new();
    for trigger in [
        "else={always=yes}",
        "else_if={limit={always=yes} always=yes}",
        "if={always=yes}",
        "if={limit={always=yes} limit={always=no}}",
        "if={limit={always=yes} always=yes} else={unregistered=yes}",
        "if={limit={always=yes} always=yes} else_if={limit={always=no} owner={always=yes}}",
        "not_in_mission_preview_mode=yes",
        "not_in_mission_preview_mode={}",
        "not_in_mission_preview_mode={key=BOH_RELIGION extra=yes}",
        "not_in_mission_preview_mode={key=$BRANCH}",
        "if={limit={always=no}} always=yes else={always=yes}",
        "if={limit={always=no}} else={} else_if={limit={always=yes}}",
        "if={limit={always=no}} else={} else={}",
    ] {
        for optimise in [false, true] {
            let error = ScriptCompiler::new(&schema, &library, optimise)
                .compile(
                    &document(&format!("if={{limit={{{trigger}}}}}")),
                    EntityKind::Country,
                )
                .unwrap_err()
                .to_string();
            assert!(error.contains("self-authored.txt"), "{trigger}: {error}");
        }
    }
}

#[test]
fn nested_conditional_selectors_have_linear_content_and_restore_in_pending_tasks() {
    let schema = Schema::new([] as [&str; 0]).unwrap();
    let library = ScriptLibrary::new();
    let mut trigger = "always=yes".to_owned();
    for _ in 0..12 {
        trigger = format!("if={{limit={{{trigger}}} tag=AAA}} else={{always=no}}");
    }
    let compiled = ScriptCompiler::new(&schema, &library, false)
        .compile(
            &document(&format!(
                "if={{limit={{{trigger}}} log=selected}} else={{log=wrong}}"
            )),
            EntityKind::Country,
        )
        .unwrap();
    assert!(serde_json::to_string(&compiled.plan).unwrap().len() < 20_000);
    let mut current = World::new(schema.clone());
    let country = current.add_entity("AAA", EntityKind::Country);
    current.schedule(country, 1, compiled.plan).unwrap();
    let snapshot = current.save("conditional-depth").unwrap();
    let mut restored = World::restore(&snapshot, "conditional-depth", &schema).unwrap();
    current.advance_to(1).unwrap();
    restored.advance_to(1).unwrap();
    assert_eq!(current, restored);
    assert_eq!(current.events().last().unwrap().2, "selected");
}

#[test]
fn branch_chains_select_one_path_with_and_without_constant_folding() {
    let schema = Schema::new(["unused"]).unwrap();
    let library = ScriptLibrary::new();
    let text = document(
        "if={limit={always=no} log=wrong} else_if={limit={always=yes} log=second} else_if={limit={always=yes} log=wrong} else={log=wrong} if={limit={always=yes}} else_if={limit={always=yes} log=wrong} if={limit={always=no}} else_if={limit={always=no}} else={log=fallback} log=after",
    );
    for optimise in [false, true] {
        let plan = ScriptCompiler::new(&schema, &library, optimise)
            .compile(&text, EntityKind::Country)
            .unwrap()
            .plan;
        let mut world = World::new(schema.clone());
        let country = world.add_entity("AAA", EntityKind::Country);
        world.execute(country, &plan).unwrap();
        assert_eq!(
            world
                .events()
                .iter()
                .map(|e| e.2.as_str())
                .collect::<Vec<_>>(),
            ["second", "fallback", "after"]
        );
    }
}

#[test]
fn detached_or_malformed_branches_are_rejected_even_after_an_always_true_branch() {
    let schema = Schema::new(["unused"]).unwrap();
    let library = ScriptLibrary::new();
    for text in [
        "else_if={limit={always=yes}}",
        "else={log=wrong}",
        "if={limit={always=yes}} else_if={log=wrong}",
        "if={limit={always=yes}} else_if={limit={always=no unknown=yes}}",
        "if={limit={always=yes}} else_if={limit={always=no} unknown=yes}",
        "if={limit={always=yes}} log=between else_if={limit={always=yes}}",
        "if={limit={always=yes}} else={} else_if={limit={always=yes}}",
    ] {
        assert!(
            ScriptCompiler::new(&schema, &library, true)
                .compile(&document(text), EntityKind::Country)
                .is_err(),
            "{text}"
        );
    }
}

#[test]
fn option_metadata_compiles_guards_without_pruning_effects_or_renumbering() {
    let schema = Schema::new(["unused"]).unwrap();
    let mut library = ScriptLibrary::new();
    library.add_triggers(&document("human={ai=no}")).unwrap();
    let event = document(
        "country_event={id=test is_triggered_only=yes option={name=zero trigger={always=no} log=zero} option={name=one trigger={human=yes} log=one}}",
    );
    for optimise in [false, true] {
        let compiled = ScriptCompiler::new(&schema, &library, optimise)
            .compile_events(&event)
            .unwrap();
        assert_eq!(
            compiled.definitions[0]
                .options
                .iter()
                .map(|option| option.id.as_str())
                .collect::<Vec<_>>(),
            ["0", "1"]
        );
        assert_eq!(
            compiled.definitions[0].options[0].effects,
            [augustus_core::Action::Emit("zero".into())]
        );
        assert_eq!(compiled.option_conditions["test"].len(), 2);
    }
    for body in [
        "option={name=zero trigger={always=no} trigger={always=yes}}",
        "option={name=zero trigger={ai=maybe}}",
        "option={name=zero trigger={always=no} unknown_effect=yes}",
        "option={name=zero trigger={always=no unknown_trigger=yes}}",
    ] {
        let text = format!("country_event={{id=test is_triggered_only=yes {body}}}");
        let error = ScriptCompiler::new(&schema, &library, true)
            .compile_events(&document(&text))
            .unwrap_err();
        assert!(
            error.to_string().contains("self-authored.txt:1:"),
            "{error}"
        );
    }
    let error = ScriptCompiler::new(&schema, &library, true)
        .compile_events(&document(
            "province_event={id=test is_triggered_only=yes option={name=zero trigger={ai=yes}}}",
        ))
        .unwrap_err();
    assert!(error.to_string().contains("country scope"));
}

#[test]
fn estate_privilege_and_metric_triggers_compile_to_typed_live_conditions() {
    let schema = Schema::new([] as [&str; 0]).unwrap();
    let library = ScriptLibrary::new();
    let text = document(
        "if={limit={has_estate_privilege=charter estate_loyalty={estate=guild loyalty=30} estate_influence={estate=guild influence=40}} log=enabled}",
    );
    for optimise in [false, true] {
        let plan = ScriptCompiler::new(&schema, &library, optimise)
            .compile(&text, EntityKind::Country)
            .unwrap()
            .plan;
        let [augustus_core::Action::If { condition, .. }] = plan.as_slice() else {
            panic!("expected one conditional action");
        };
        let augustus_core::Condition::All(conditions) = condition else {
            panic!("expected three estate conditions");
        };
        assert!(matches!(
            conditions[0],
            augustus_core::Condition::EstatePrivilege { .. }
        ));
        assert!(matches!(
            conditions[1],
            augustus_core::Condition::EstateLoyaltyAtLeast {
                loyalty_milli: 30_000,
                ..
            }
        ));
        assert!(matches!(
            conditions[2],
            augustus_core::Condition::EstateInfluenceAtLeast {
                influence_milli: 40_000,
                ..
            }
        ));
    }

    for input in [
        "if={limit={has_estate_privilege=$dynamic$} log=bad}",
        "if={limit={estate_loyalty={estate=guild loyalty=-1}} log=bad}",
        "if={limit={estate_loyalty={estate=guild loyalty=101}} log=bad}",
        "if={limit={estate_influence={estate=guild}} log=bad}",
        "if={limit={estate_influence={estate=guild influence=-1}} log=bad}",
    ] {
        assert!(
            ScriptCompiler::new(&schema, &library, true)
                .compile(&document(input), EntityKind::Country)
                .is_err(),
            "{input}"
        );
    }
    let error = ScriptCompiler::new(&schema, &library, true)
        .compile_events(&document(
            "province_event={id=test is_triggered_only=yes option={name=one trigger={has_estate_privilege=charter}}}",
        ))
        .unwrap_err();
    assert!(error.to_string().contains("country scope"));
}

#[test]
fn has_idea_compiles_as_a_typed_country_condition() {
    let schema = Schema::new([] as [&str; 0]).unwrap();
    let library = ScriptLibrary::new();
    let compiled = ScriptCompiler::new(&schema, &library, true)
        .compile(
            &document("if={limit={has_idea=quest_for_the_new_world} log=found}"),
            EntityKind::Country,
        )
        .unwrap();
    let [
        augustus_core::Action::If {
            condition: augustus_core::Condition::All(conditions),
            ..
        },
    ] = compiled.plan.as_slice()
    else {
        panic!(
            "has_idea should compile to a typed condition: {:#?}",
            compiled.plan
        );
    };
    let [augustus_core::Condition::HasIdea { id, .. }] = conditions.as_slice() else {
        panic!("has_idea should compile as a typed condition inside its block");
    };
    assert_eq!(id, "quest_for_the_new_world");

    for value in ["$dynamic$", "@dynamic@", ""] {
        assert!(
            ScriptCompiler::new(&schema, &library, true)
                .compile(
                    &document(&format!("if={{limit={{has_idea={value}}} log=bad}}")),
                    EntityKind::Country,
                )
                .is_err()
        );
    }
}

#[test]
fn primary_culture_and_culture_group_use_loaded_country_affiliations() {
    use augustus_core::{AffiliationAcceptance, AffiliationDimension, Affiliations};

    let schema = Schema::new([] as [&str; 0]).unwrap();
    let library = ScriptLibrary::new();
    for (script, expected) in [
        ("primary_culture=hausa", true),
        ("culture_group=west_african", true),
        ("accepted_culture=hausa", true),
        ("accepted_culture=songhai", true),
        ("accepted_culture=mandinka", false),
        ("primary_culture=songhai", false),
        ("culture_group=east_african", false),
        ("technology_group=western", true),
        ("technology_group=chinese", false),
    ] {
        let compiled = ScriptCompiler::new(&schema, &library, true)
            .compile(
                &document(&format!(
                    "if={{limit={{{script}}} log=matched}} else={{log=unmatched}}"
                )),
                EntityKind::Country,
            )
            .unwrap();
        let mut world = World::new(schema.clone());
        let hausa = world.add_entity("HAU", EntityKind::Country);
        world
            .set_affiliations(Affiliations(
                [
                    (
                        "culture".into(),
                        AffiliationDimension {
                            groups: [
                                ("hausa".into(), Some("west_african".into())),
                                ("songhai".into(), Some("east_african".into())),
                                ("mandinka".into(), Some("east_african".into())),
                            ]
                            .into(),
                            members: [(hausa, Some("hausa".into()))].into(),
                            acceptance: [(
                                hausa,
                                AffiliationAcceptance {
                                    members: Some(["songhai".into()].into_iter().collect()),
                                    groups: Some(["west_african".into()].into_iter().collect()),
                                },
                            )]
                            .into(),
                            ..Default::default()
                        },
                    ),
                    (
                        "technology_group".into(),
                        AffiliationDimension {
                            members: [(hausa, Some("western".into()))].into(),
                            ..Default::default()
                        },
                    ),
                ]
                .into(),
            ))
            .unwrap();
        world.execute(hausa, &compiled.plan).unwrap();
        assert_eq!(
            world.events().last().unwrap().2,
            if expected { "matched" } else { "unmatched" },
            "{script}"
        );
    }
}

#[test]
fn accepted_culture_is_distinct_from_primary_culture_and_unknown_policy() {
    use augustus_core::{AffiliationAcceptance, AffiliationDimension, Affiliations};

    let schema = Schema::new([] as [&str; 0]).unwrap();
    let library = ScriptLibrary::new();
    let compiled = ScriptCompiler::new(&schema, &library, true)
        .compile(
            &document("if={limit={accepted_culture=beta} log=accepted} else={log=not_accepted}"),
            EntityKind::Country,
        )
        .unwrap();

    let mut world = World::new(schema);
    let country = world.add_entity("AAA", EntityKind::Country);
    world
        .set_affiliations(Affiliations(
            [(
                "culture".into(),
                AffiliationDimension {
                    acceptance: [(
                        country,
                        AffiliationAcceptance {
                            members: Some(Default::default()),
                            groups: Some(Default::default()),
                        },
                    )]
                    .into(),
                    ..Default::default()
                },
            )]
            .into(),
        ))
        .unwrap();
    world.execute(country, &compiled.plan).unwrap();
    assert_eq!(world.events().last().unwrap().2, "not_accepted");

    let mut affiliations = world.affiliations().clone();
    let acceptance = affiliations
        .0
        .get_mut("culture")
        .unwrap()
        .acceptance
        .get_mut(&country)
        .unwrap();
    acceptance.members = None;
    acceptance.groups = None;
    world.set_affiliations(affiliations).unwrap();
    let event_count = world.events().len();
    assert!(world.execute(country, &compiled.plan).is_err());
    assert_eq!(world.events().len(), event_count);
}

#[test]
fn mercantilism_compiles_to_the_shared_country_resource_condition() {
    let schema = Schema::new([] as [&str; 0]).unwrap();
    let library = ScriptLibrary::new();
    let compiled = ScriptCompiler::new(&schema, &library, true)
        .compile(
            &document("if={limit={mercantilism=50.5} log=threshold}"),
            EntityKind::Country,
        )
        .unwrap();
    let [augustus_core::Action::If { condition, .. }] = compiled.plan.as_slice() else {
        panic!("expected one mercantilism condition");
    };
    let augustus_core::Condition::All(conditions) = condition else {
        panic!("expected a block condition");
    };
    assert!(matches!(
        conditions.as_slice(),
        [augustus_core::Condition::NumericCompare {
            slot: augustus_core::NumericSlot::Resource(
                augustus_core::CountryResource::Mercantilism
            ),
            comparison: augustus_core::NumericComparison::AtLeast,
            value: 50_500,
            ..
        }]
    ));
    for value in ["-1", "100.001", "$dynamic$"] {
        assert!(
            ScriptCompiler::new(&schema, &library, true)
                .compile(
                    &document(&format!("if={{limit={{mercantilism={value}}} log=bad}}")),
                    EntityKind::Country,
                )
                .is_err()
        );
    }
}

#[test]
fn script_view_uses_original_filename_order_and_direct_children() {
    let root = tempfile::tempdir().unwrap();
    let effects = root.path().join("common/scripted_effects");
    std::fs::create_dir_all(effects.join("nested")).unwrap();
    std::fs::write(effects.join("Z_first.txt"), "winner={log=first}").unwrap();
    std::fs::write(effects.join("a_second.txt"), "winner={log=second}").unwrap();
    std::fs::write(effects.join("nested/hidden.txt"), "hidden={log=hidden}").unwrap();
    let content = ContentSet::resolve(vec![ContentSource::base(
        ContentRoot::open(root.path()).unwrap(),
    )])
    .unwrap();
    let files = ScriptLibrary::files(&content, DefinitionKind::ScriptedEffects).unwrap();
    assert_eq!(
        files
            .iter()
            .map(|(_, origin)| origin.relative_path.file_name().unwrap().to_str().unwrap())
            .collect::<Vec<_>>(),
        ["Z_first.txt", "a_second.txt"]
    );
    assert!(ScriptLibrary::files(&content, DefinitionKind::Buildings).is_err());
    let library = ScriptLibrary::from_content(&content, TextEncoding::Utf8).unwrap();
    assert_eq!(library.ignored_definitions().len(), 1);
    let schema = Schema::new(["unused"]).unwrap();
    let script = ScriptCompiler::new(&schema, &library, true)
        .compile(&document("winner=yes"), EntityKind::Country)
        .unwrap();
    let mut world = World::new(schema.clone());
    let country = world.add_entity("Test", EntityKind::Country);
    world.execute(country, &script.plan).unwrap();
    assert_eq!(world.events()[0].2, "first");
    assert!(
        ScriptCompiler::new(&schema, &library, true)
            .compile(&document("hidden=yes"), EntityKind::Country)
            .is_err()
    );
}

#[test]
fn compiled_calls_bind_first_declarations_and_preserve_live_state() {
    let schema = Schema::new(["treasury", "variable/counter"]).unwrap();
    let mut library = ScriptLibrary::new();
    library
        .add_effects(&document(
            "fund={add_treasury=1.125} fund={add_treasury=99}",
        ))
        .unwrap();
    library
        .add_triggers(&document("ready={always=yes} ready={always=no}"))
        .unwrap();
    let input = document(
        "fund=yes capital_scope={set_variable={which=counter value=2.125} change_variable={which=counter value=-0.125} if={limit={check_variable={which=counter value=2}} owner={add_treasury=2}} else={log=incorrect}} if={limit={ready=yes} log=ready} else={log=incorrect}",
    );
    let mut worlds = Vec::new();
    for optimise in [false, true] {
        let script = ScriptCompiler::new(&schema, &library, optimise)
            .compile(&input, EntityKind::Country)
            .unwrap();
        let mut world = World::new(schema.clone());
        let country = world.add_entity("Test", EntityKind::Country);
        let province = world.add_entity("Test capital", EntityKind::Province);
        world.set_owner(province, country).unwrap();
        world.set_capital(country, province).unwrap();
        world.execute(country, &script.plan).unwrap();
        assert_eq!(
            world
                .value(country, schema.field("treasury").unwrap())
                .unwrap(),
            3125
        );
        assert_eq!(
            world
                .value(province, schema.field("variable/counter").unwrap())
                .unwrap(),
            2000
        );
        assert_eq!(world.events()[0].2, "ready");
        assert_eq!(script.folded_branches, usize::from(optimise));
        worlds.push(world);
    }
    assert_eq!(worlds[0], worlds[1]);
    assert_eq!(library.ignored_definitions().len(), 2);
}

#[test]
fn parameterized_scripted_effects_expand_nested_values_and_identifiers() {
    let schema = Schema::new([] as [&str; 0]).unwrap();
    let mut library = ScriptLibrary::new();
    library
        .add_effects(&document(
            "outer={set_country_flag=branch_$name$ inner={message=$message$}} inner={log=$message$}",
        ))
        .unwrap();
    let script = ScriptCompiler::new(&schema, &library, true)
        .compile(
            &document("outer={name=bohemian message=expanded}"),
            EntityKind::Country,
        )
        .unwrap();
    let mut world = World::new(schema.clone());
    let country = world.add_entity("BOH", EntityKind::Country);
    world.execute(country, &script.plan).unwrap();
    assert!(world.has_marker(country, "branch_bohemian").unwrap());
    assert_eq!(world.events().last().unwrap().2, "expanded");

    let error = ScriptCompiler::new(&schema, &library, true)
        .compile(&document("outer={name=bohemian}"), EntityKind::Country)
        .unwrap_err();
    assert!(error.to_string().contains("unbound parameters"));
}

#[test]
fn unsupported_semantics_recursion_and_scope_errors_report_source_positions() {
    let schema = Schema::new(["treasury", "variable/x"]).unwrap();
    let mut library = ScriptLibrary::new();
    library.add_effects(&document("loop={loop=yes}")).unwrap();
    for input in [
        "loop=yes",
        "\nadd_prestige=1",
        "owner={add_treasury=1}",
        "add_treasury=1.0001",
        "set_variable={which=missing@ROOT value=1}",
        "if={limit={always=yes} add_prestige=1}",
        "set_variable={which=x value=1 value=2}",
        "else={log=orphan}",
        "if={limit={unsupported=yes} log=x}",
        "if={limit={NOT={}} log=x}",
    ] {
        let error = ScriptCompiler::new(&schema, &library, true)
            .compile(&document(input), EntityKind::Country)
            .unwrap_err();
        assert_eq!(error.path().to_string_lossy(), "self-authored.txt");
        assert!(error.location().is_some(), "{input}");
    }
}

#[test]
fn identifier_case_and_fractional_literals_are_preserved() {
    let schema = Schema::new(["treasury"]).unwrap();
    let mut library = ScriptLibrary::new();
    library
        .add_effects(&document(
            "Bonus={add_treasury=0.001} bonus={add_treasury=-1.2}",
        ))
        .unwrap();
    let script = ScriptCompiler::new(&schema, &library, false)
        .compile(&document("Bonus=yes bonus=yes"), EntityKind::Country)
        .unwrap();
    let mut world = World::new(schema);
    let country = world.add_entity("Test", EntityKind::Country);
    world.execute(country, &script.plan).unwrap();
    assert_eq!(
        world
            .value(country, world.schema().field("treasury").unwrap())
            .unwrap(),
        -1199
    );
}

#[test]
fn library_registration_preserves_unsupported_templates_for_call_site_validation() {
    let mut library = ScriptLibrary::new();
    library
        .add_effects(&TextDocument::from_bytes(
            "effect-library.txt",
            b"list={\nlog={first second}\n} operator={\nlog>text\n} good={log=good}".to_vec(),
            TextEncoding::Utf8,
        ))
        .unwrap();
    library
        .add_triggers(&TextDocument::from_bytes(
            "trigger-library.txt",
            b"list={\nalways={yes no}\n} operator={\nalways>yes\n}".to_vec(),
            TextEncoding::Utf8,
        ))
        .unwrap();
    let schema = Schema::new(["unused"]).unwrap();
    for optimise in [false, true] {
        let compiled = ScriptCompiler::new(&schema, &library, optimise)
            .compile(&document("good=yes"), EntityKind::Country)
            .unwrap();
        assert_eq!(compiled.plan, [augustus_core::Action::Emit("good".into())]);
        assert!(
            compiled
                .sources
                .iter()
                .any(|(path, _)| path == std::path::Path::new("effect-library.txt"))
        );
        for (input, path, message) in [
            (
                "if={limit={always=no} list=yes}",
                "effect-library.txt",
                "unsupported script value",
            ),
            (
                "operator=yes",
                "effect-library.txt",
                "unsupported script operator",
            ),
            (
                "if={limit={always=no list=yes} log=unused}",
                "trigger-library.txt",
                "unsupported script value",
            ),
            (
                "if={limit={operator=no} log=unused}",
                "trigger-library.txt",
                "unsupported script operator",
            ),
        ] {
            let error = ScriptCompiler::new(&schema, &library, optimise)
                .compile(&document(input), EntityKind::Country)
                .unwrap_err();
            assert_eq!(error.path(), std::path::Path::new(path));
            assert!(error.to_string().contains(message), "{error}");
            assert!(error.location().is_some());
        }
    }
    // Explicit input still rejects unsupported grammar during its initial parse.
    assert!(
        ScriptCompiler::new(&schema, &library, true)
            .compile(
                &document("if={limit={always=no} log={first second}}"),
                EntityKind::Country
            )
            .is_err()
    );
    assert!(library.add_effects(&document("invalid=scalar")).is_err());
}

#[test]
fn change_price_compiles_permanent_keyed_impacts_and_rejects_unverified_durations() {
    let schema = Schema::new([] as [&str; 0]).unwrap();
    let library = ScriptLibrary::new();
    let compiled = ScriptCompiler::new(&schema, &library, true)
        .compile(
            &document(
                "change_price = { trade_goods = grain key = PK_TEST_GRAIN_IMPACT value = 0.5 duration = -1 }",
            ),
            EntityKind::Country,
        )
        .unwrap();
    assert!(matches!(
        compiled.plan.as_slice(),
        [Action::SetTradeGoodPriceImpact {
            trade_good,
            key,
            value_milli: 500,
        }] if trade_good == "grain" && key == "PK_TEST_GRAIN_IMPACT"
    ));

    let finite = ScriptCompiler::new(&schema, &library, true)
        .compile(
            &document(
                "change_price = { trade_goods = grain key = temporary value = 0.5 duration = 30 }",
            ),
            EntityKind::Country,
        )
        .unwrap_err();
    assert!(
        format!("{finite:?}").contains("supports only duration = -1"),
        "{finite:?}"
    );

    for invalid in [
        "change_price = { trade_goods = grain value = 0.5 duration = -1 }",
        "change_price = { trade_goods = grain key = x value = 0.5 duration = -1 key = y }",
        "change_price = { trade_goods = grain key = x value = 0.5 duration = -1 other = yes }",
    ] {
        assert!(
            ScriptCompiler::new(&schema, &library, true)
                .compile(&document(invalid), EntityKind::Country)
                .is_err(),
            "{invalid}"
        );
    }
}
