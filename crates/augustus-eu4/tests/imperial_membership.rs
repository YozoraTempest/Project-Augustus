// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::{collections::BTreeMap, fs, path::Path};

use augustus_core::{
    Action, AffiliationDimension, Affiliations, Condition, EntityId, EntityKind, Schema, Target,
    World,
};
use augustus_eu4::{ScriptCompiler, ScriptLibrary, TextDocument, TextEncoding, VanillaWorld};

fn document(text: &str) -> TextDocument {
    TextDocument::from_bytes(
        "self-authored-imperial-membership.txt",
        text.as_bytes().to_vec(),
        TextEncoding::Utf8,
    )
}

fn fixture() -> tempfile::TempDir {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world");
    let temp = tempfile::tempdir().unwrap();
    for entry in walkdir::WalkDir::new(&root) {
        let entry = entry.unwrap();
        let target = temp.path().join(entry.path().strip_prefix(&root).unwrap());
        if entry.file_type().is_dir() {
            fs::create_dir_all(target).unwrap();
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
    temp
}

fn write(root: &Path, name: &str, text: &str) {
    let path = root.join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn country_membership() -> Condition {
    Condition::With {
        target: Target::Capital,
        kind: EntityKind::Province,
        condition: Box::new(Condition::AffiliationPresent {
            target: Target::Current,
            dimension: "imperial_membership".into(),
        }),
    }
}

#[test]
fn countries_query_the_current_capital_and_provinces_query_their_own_membership() {
    let schema = Schema::new(["unused"]).unwrap();
    let library = ScriptLibrary::new();
    for optimise in [false, true] {
        let mut world = World::new(schema.clone());
        let country = world.add_entity("AAA", EntityKind::Country);
        let inside = world.add_entity("Inside", EntityKind::Province);
        let outside = world.add_entity("Outside", EntityKind::Province);
        let unknown = world.add_entity("Unknown", EntityKind::Province);
        world.set_owner(inside, country).unwrap();
        world.set_owner(outside, country).unwrap();
        world.set_owner(unknown, country).unwrap();
        world.set_capital(country, outside).unwrap();
        world
            .set_affiliations(Affiliations(BTreeMap::from([(
                "imperial_membership".into(),
                AffiliationDimension {
                    groups: [("hre".into(), None)].into(),
                    members: [(inside, Some("hre".into())), (unknown, None)].into(),
                    absent_members: [outside].into(),
                    acceptance: BTreeMap::new(),
                },
            )])))
            .unwrap();
        let plan = ScriptCompiler::new(&schema, &library, optimise)
            .compile(
                &document("if={limit={is_part_of_hre=no any_owned_province={is_part_of_hre=yes}} log=outside_with_member}"),
                EntityKind::Country,
            )
            .unwrap()
            .plan;
        world.execute(country, &plan).unwrap();
        assert_eq!(world.events().last().unwrap().2, "outside_with_member");
        let inside_plan = ScriptCompiler::new(&schema, &library, optimise)
            .compile(
                &document(
                    "if={limit={is_part_of_hre=yes owner={is_part_of_hre=no}} log=province_member}",
                ),
                EntityKind::Province,
            )
            .unwrap()
            .plan;
        world.execute(inside, &inside_plan).unwrap();
        assert_eq!(world.events().last().unwrap().2, "province_member");
        world.set_capital(country, inside).unwrap();
        assert!(country_membership().evaluate(&world, country).unwrap());
        world
            .execute(
                country,
                &[Action::Schedule {
                    delay: 1,
                    plan: vec![Action::If {
                        condition: country_membership(),
                        then: vec![Action::Emit("wrong".into())],
                        otherwise: vec![Action::Emit("membership_changed".into())],
                    }],
                }],
            )
            .unwrap();
        world
            .set_affiliation_member(inside, "imperial_membership", None)
            .unwrap();
        let mut restored =
            World::restore(&world.save("membership").unwrap(), "membership", &schema).unwrap();
        world.advance_to(1).unwrap();
        restored.advance_to(1).unwrap();
        assert_eq!(
            world.save("membership").unwrap(),
            restored.save("membership").unwrap()
        );
        assert_eq!(world.events().last().unwrap().2, "membership_changed");
        world.set_capital(country, unknown).unwrap();
        assert!(country_membership().evaluate(&world, country).is_err());
        assert!(
            Condition::Not(Box::new(country_membership()))
                .evaluate(&world, country)
                .is_err()
        );
    }
}

#[test]
fn history_defaults_dates_and_initial_system_conditions_use_the_same_membership() {
    let temp = fixture();
    write(
        temp.path(),
        "history/provinces/2 - East.txt",
        "owner=AAA hre=yes 1450.1.1={hre=no}",
    );
    write(
        temp.path(),
        "common/ideas/membership.txt",
        "member_ideas={free=yes trigger={is_part_of_hre=yes} one={}}",
    );
    write(
        temp.path(),
        "common/estates/membership.txt",
        "estate_member={trigger={is_part_of_hre=yes}}",
    );
    write(
        temp.path(),
        "common/on_actions/membership.txt",
        "on_startup={if={limit={tag=AAA is_part_of_hre=yes} log=member_start}} ",
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let entity = loaded.initial.country_entity("AAA").unwrap();
    assert!(
        country_membership()
            .evaluate(loaded.initial.world(), entity)
            .unwrap()
    );
    assert_eq!(
        loaded.initial.world().events().last().unwrap().2,
        "member_start"
    );
    assert_eq!(
        loaded.country("AAA").unwrap().estates.unwrap()["estate_member"].active,
        Some(true)
    );
    assert!(
        loaded
            .country("AAA")
            .unwrap()
            .ideas
            .unwrap()
            .groups
            .as_ref()
            .unwrap()
            .contains_key("member_ideas")
    );
    let later = VanillaWorld::load(temp.path(), "1450.1.1").unwrap();
    assert!(
        !country_membership()
            .evaluate(later.initial.world(), entity)
            .unwrap()
    );
    let outside = loaded.initial.province_entity(4).unwrap();
    assert_eq!(
        loaded
            .initial
            .world()
            .affiliations()
            .present("imperial_membership", outside),
        Some(false)
    );
    let missing = loaded.initial.province_entity(3).unwrap();
    assert_eq!(
        loaded
            .initial
            .world()
            .affiliations()
            .present("imperial_membership", missing),
        None
    );
}

#[test]
fn unexecuted_history_changes_and_failed_startup_cannot_establish_membership() {
    let temp = fixture();
    let path = "history/provinces/2 - East.txt";
    let original = "owner=AAA hre=yes";
    write(temp.path(), path, original);
    write(
        temp.path(),
        "common/scripted_effects/membership.txt",
        "custom_membership_change={set_in_empire=no} wrapper_membership_change={custom_membership_change=yes}",
    );
    let entity = VanillaWorld::load(temp.path(), "1444.11.11")
        .unwrap()
        .initial
        .country_entity("AAA")
        .unwrap();
    write(
        temp.path(),
        path,
        &(original.to_owned() + " 1450.1.1={custom_membership_change=yes}"),
    );
    assert!(
        country_membership()
            .evaluate(
                VanillaWorld::load(temp.path(), "1444.11.11")
                    .unwrap()
                    .initial
                    .world(),
                entity
            )
            .unwrap()
    );
    assert!(
        country_membership()
            .evaluate(
                VanillaWorld::load(temp.path(), "1450.1.1")
                    .unwrap()
                    .initial
                    .world(),
                entity
            )
            .is_err()
    );
    for effect in [
        "set_in_empire=no",
        "if={limit={always=yes} hre=no}",
        "effect={set_in_empire=no}",
        "custom_membership_change=yes",
        "wrapper_membership_change=yes",
    ] {
        write(temp.path(), path, &(original.to_owned() + " " + effect));
        let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
        assert!(
            country_membership()
                .evaluate(loaded.initial.world(), entity)
                .is_err(),
            "{effect}"
        );
        assert!(
            Condition::Not(Box::new(country_membership()))
                .evaluate(loaded.initial.world(), entity)
                .is_err(),
            "{effect}"
        );
    }
    write(temp.path(), path, original);
    write(
        temp.path(),
        "common/on_actions/membership.txt",
        "on_startup={log=rolled_back unsupported_effect=yes}",
    );
    let failed = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(!failed.report.estates.startup_unresolved.is_empty());
    assert!(failed.initial.world().events().is_empty());
    assert!(
        country_membership()
            .evaluate(failed.initial.world(), entity)
            .is_err()
    );
    let outside = failed.initial.province_entity(4).unwrap();
    assert_eq!(
        failed
            .initial
            .world()
            .affiliations()
            .present("imperial_membership", outside),
        None
    );
}

#[test]
fn unrelated_country_history_effect_does_not_hide_explicit_membership() {
    let temp = fixture();
    write(
        temp.path(),
        "history/provinces/2 - East.txt",
        "owner=AAA hre=no",
    );
    write(
        temp.path(),
        "common/scripted_effects/unrelated.txt",
        "unrelated_country_setup={add_prestige=1}",
    );
    let country_history = temp.path().join("history/countries/AAA - Alpha.txt");
    let mut history = fs::read_to_string(&country_history).unwrap();
    history.push_str("\neffect={unrelated_country_setup=yes}\n");
    write(temp.path(), "history/countries/AAA - Alpha.txt", &history);

    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let province = loaded.initial.province_entity(2).unwrap();
    assert_eq!(
        loaded
            .initial
            .world()
            .affiliations()
            .present("imperial_membership", province),
        Some(false)
    );
}

#[test]
fn boolean_operands_are_validated_before_optimisation() {
    let schema = Schema::new(["unused"]).unwrap();
    let library = ScriptLibrary::new();
    for optimise in [false, true] {
        for text in [
            "if={limit={always=no is_part_of_hre=maybe} log=wrong}",
            "if={limit={is_part_of_hre={value=yes}} log=wrong}",
            "if={limit={is_part_of_hre=AAA} log=wrong}",
        ] {
            let error = ScriptCompiler::new(&schema, &library, optimise)
                .compile(&document(text), EntityKind::Country)
                .unwrap_err();
            assert!(error.location().is_some(), "{error}");
        }
    }
    // Explicit absence of a capital differs from an unknown capital reference.
    let mut world = World::new(schema);
    let country = world.add_entity("Absent", EntityKind::Country);
    assert!(country_membership().evaluate(&world, country).is_err());
    world.mark_target_absent(country, Target::Capital).unwrap();
    assert!(!country_membership().evaluate(&world, country).unwrap());
    assert_eq!(country, EntityId(0));
}
