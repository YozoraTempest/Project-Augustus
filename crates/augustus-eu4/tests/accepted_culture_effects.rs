// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::{BTreeMap, BTreeSet};

use augustus_core::{
    Action, AffiliationAcceptance, AffiliationDimension, Affiliations, EntityId, EntityKind,
    Schema, Target, World,
};
use augustus_eu4::{ScriptCompiler, ScriptLibrary, TextDocument, TextEncoding};

fn document(text: &str) -> TextDocument {
    TextDocument::from_bytes(
        "self-authored-accepted-cultures.txt",
        text.as_bytes().to_vec(),
        TextEncoding::Utf8,
    )
}

fn world(beta_registered: bool, policy: Option<AffiliationAcceptance>) -> (World, EntityId) {
    let mut world = World::new(Schema::new([] as [&str; 0]).unwrap());
    let country = world.add_entity("AAA", EntityKind::Country);
    let mut groups = BTreeMap::from([("alpha".into(), Some("north".into()))]);
    if beta_registered {
        groups.insert("beta".into(), Some("south".into()));
    }
    let acceptance = policy
        .map(|policy| BTreeMap::from([(country, policy)]))
        .unwrap_or_default();
    world
        .set_affiliations(Affiliations(BTreeMap::from([(
            "culture".into(),
            AffiliationDimension {
                groups,
                members: BTreeMap::from([(country, Some("alpha".into()))]),
                acceptance,
                ..Default::default()
            },
        )])))
        .unwrap();
    (world, country)
}

fn known_empty_policy() -> AffiliationAcceptance {
    AffiliationAcceptance {
        members: Some(BTreeSet::new()),
        groups: Some(BTreeSet::new()),
    }
}

#[test]
fn accepted_culture_effects_compile_execute_and_expand_scripted_effects() {
    let schema = Schema::new([] as [&str; 0]).unwrap();
    let mut library = ScriptLibrary::new();
    library
        .add_effects(&document("accept_beta={add_accepted_culture=beta}"))
        .unwrap();

    let expected_add = Action::SetAffiliationAcceptance {
        target: Target::Current,
        dimension: "culture".into(),
        member: "beta".into(),
        accepted: true,
    };
    let expected_remove = Action::SetAffiliationAcceptance {
        target: Target::Current,
        dimension: "culture".into(),
        member: "beta".into(),
        accepted: false,
    };

    let mut worlds = Vec::new();
    for optimise in [false, true] {
        let add = ScriptCompiler::new(&schema, &library, optimise)
            .compile(&document("add_accepted_culture=beta"), EntityKind::Country)
            .unwrap();
        let remove = ScriptCompiler::new(&schema, &library, optimise)
            .compile(
                &document("remove_accepted_culture=beta"),
                EntityKind::Country,
            )
            .unwrap();
        let expanded = ScriptCompiler::new(&schema, &library, optimise)
            .compile(&document("accept_beta=yes"), EntityKind::Country)
            .unwrap();
        assert_eq!(add.plan.as_slice(), std::slice::from_ref(&expected_add));
        assert_eq!(
            remove.plan.as_slice(),
            std::slice::from_ref(&expected_remove)
        );
        assert_eq!(expanded.plan, add.plan);

        let (mut world, country) = world(true, Some(known_empty_policy()));
        assert_eq!(
            world
                .affiliations()
                .accepts_member("culture", country, "beta"),
            Some(false)
        );
        world.execute(country, &expanded.plan).unwrap();
        assert_eq!(
            world
                .affiliations()
                .accepts_member("culture", country, "beta"),
            Some(true)
        );
        world.execute(country, &remove.plan).unwrap();
        assert_eq!(
            world
                .affiliations()
                .accepts_member("culture", country, "beta"),
            Some(false)
        );
        worlds.push(world);
    }
    assert_eq!(worlds[0], worlds[1]);
}

#[test]
fn dynamic_empty_block_and_wrong_scope_operands_are_rejected_before_optimization() {
    let schema = Schema::new([] as [&str; 0]).unwrap();
    let library = ScriptLibrary::new();
    let invalid = [
        "add_accepted_culture=\"\"",
        "add_accepted_culture=ROOT",
        "add_accepted_culture=FROM",
        "add_accepted_culture=PREV",
        "add_accepted_culture=THIS",
        "add_accepted_culture=AAA",
        "add_accepted_culture=123",
        "add_accepted_culture=-1",
        "remove_accepted_culture=+1",
        "add_accepted_culture=-999999999999999999999999",
        "remove_accepted_culture=+999999999999999999999999",
        "add_accepted_culture=$culture$",
        "add_accepted_culture=@culture@",
        "add_accepted_culture=event_target:culture",
        "remove_accepted_culture=event_target:culture",
        "add_accepted_culture={culture=beta}",
        "if={limit={always=no} add_accepted_culture=$culture$}",
        "if={limit={always=no} remove_accepted_culture=event_target:culture}",
        "if={limit={always=no} add_accepted_culture=-1}",
        "if={limit={always=no} remove_accepted_culture=+999999999999999999999999}",
        "capital_scope={add_accepted_culture=beta}",
        "capital_scope={remove_accepted_culture=beta}",
    ];
    for optimise in [false, true] {
        for input in invalid {
            let error = ScriptCompiler::new(&schema, &library, optimise)
                .compile(&document(input), EntityKind::Country)
                .unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains("self-authored-accepted-cultures.txt:1:"),
                "{input}: {error}"
            );
        }
    }
}

#[test]
fn runtime_rejects_unknown_inputs_without_fabricating_acceptance() {
    let schema = Schema::new([] as [&str; 0]).unwrap();
    let library = ScriptLibrary::new();
    for (member, policy, expected_error) in [
        (
            "ghost",
            Some(known_empty_policy()),
            "unregistered affiliation member ghost",
        ),
        (
            "beta",
            Some(AffiliationAcceptance {
                members: None,
                groups: None,
            }),
            "unknown explicit affiliation acceptance",
        ),
        ("beta", None, "unknown explicit affiliation acceptance"),
    ] {
        for effect in ["add_accepted_culture", "remove_accepted_culture"] {
            let script = ScriptCompiler::new(&schema, &library, true)
                .compile(
                    &document(&format!("{effect}={member}")),
                    EntityKind::Country,
                )
                .unwrap();
            let (mut state, country) = world(true, policy.clone());
            let before = state.affiliations().clone();
            let random_before = state.eu4_random_state();
            let error = state.execute(country, &script.plan).unwrap_err();
            assert!(
                error.to_string().contains(expected_error),
                "{effect}: {error}"
            );
            assert_eq!(state.affiliations(), &before);
            assert_eq!(state.eu4_random_state(), random_before);
            assert!(
                state.save("rejected-culture").is_err(),
                "runtime errors stop execution and saving, as for other actions"
            );
        }
    }
}
