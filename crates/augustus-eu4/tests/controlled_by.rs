// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{Action, EntityKind, ScopeSelector, Target};
use augustus_eu4::{
    ContentRoot, ContentSet, ContentSource, ScriptCompiler, ScriptLibrary, TextDocument,
    TextEncoding, VanillaExternalState, VanillaWorld,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

const MISSIONS: &str = r#"
controller_series = {
    slot = 5
    potential = { OR = { tag = AAA tag = BBB } }

    controlled_by_root = {
        position = 1
        trigger = { 1 = { controlled_by = ROOT } }
        provinces_to_highlight = {
            OR = { province_id = 1 province_id = 2 }
            controlled_by = ROOT
        }
        effect = { add_treasury = 3 }
    }
    controlled_by_literal = {
        position = 2
        trigger = { 1 = { controlled_by = BBB } }
        provinces_to_highlight = { province_id = 1 controlled_by = BBB }
        effect = {}
    }
    controlled_by_owner = {
        position = 3
        trigger = { 1 = { controlled_by = owner } }
        provinces_to_highlight = { province_id = 1 controlled_by = owner }
        effect = {}
    }
    controlled_by_unknown = {
        position = 4
        trigger = { 4 = { NOT = { controlled_by = ROOT } } }
        provinces_to_highlight = { province_id = 4 NOT = { controlled_by = ROOT } }
        effect = {}
    }
    controlled_by_absent = {
        position = 5
        trigger = { 2 = { NOT = { controlled_by = ROOT } } }
        provinces_to_highlight = { province_id = 2 NOT = { controlled_by = ROOT } }
        effect = {}
    }
}
"#;

const EVENT_TARGET_MISSIONS: &str = r#"
event_target_series = {
    slot = 6
    potential = { tag = AAA }

    save_controller_target = {
        position = 1
        trigger = { always = yes }
        effect = {
            bind_controller_country = yes
            country_event = { id = controlled_by.target }
        }
    }
    check_controller_target = {
        position = 2
        trigger = { 1 = { controlled_by = event_target:controller_country } }
        effect = {}
    }
}
"#;

fn base_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world")
}

fn write_mod(root: &Path, missions: &str) {
    fs::create_dir_all(root.join("missions")).unwrap();
    fs::write(
        root.join("descriptor.mod"),
        "name=\"Current Controller Conditions Test\"\nsupported_version=\"1.37.*\"\n",
    )
    .unwrap();
    fs::write(root.join("missions/controllers.txt"), missions).unwrap();
}

fn initial_state() -> VanillaExternalState {
    serde_json::from_str(
        r#"{
            "country_resources": {
                "AAA": { "treasury_milli": 0 },
                "BBB": { "treasury_milli": 0 }
            },
            "country_mission_completions": { "AAA": [], "BBB": [] },
            "province_owners": {
                "1": "AAA",
                "2": "BBB",
                "4": "AAA"
            },
            "province_controllers": {
                "1": "BBB",
                "2": "AAA",
                "4": null
            }
        }"#,
    )
    .unwrap()
}

fn load_world(mod_root: &Path, state: &VanillaExternalState) -> VanillaWorld {
    let content = ContentSet::resolve(vec![
        ContentSource::base(ContentRoot::open(base_root()).unwrap()),
        ContentSource::mod_directory(mod_root, TextEncoding::Utf8).unwrap(),
    ])
    .unwrap();
    VanillaWorld::load_from_content_with_players_and_external_state(
        &content,
        "1444.11.11",
        &BTreeSet::from(["AAA".into(), "BBB".into()]),
        state,
    )
    .unwrap()
}

fn mission(world: &VanillaWorld, country: &str, id: &str) -> augustus_eu4::VanillaMissionView {
    world
        .country_missions(country)
        .unwrap()
        .missions
        .into_iter()
        .find(|candidate| candidate.id == id)
        .unwrap()
}

fn controller_action(country: Option<&str>) -> Vec<Action> {
    vec![Action::SetProvinceController {
        target: Target::Current,
        controller: country.map(|name| {
            augustus_core::EntityRef::Selected(ScopeSelector::Named {
                kind: EntityKind::Country,
                name: name.into(),
            })
        }),
    }]
}

#[test]
fn missions_and_highlights_follow_live_controller_independently_of_owner() {
    let fixture = tempfile::tempdir().unwrap();
    write_mod(fixture.path(), MISSIONS);
    let mut world = load_world(fixture.path(), &initial_state());
    let before_queries = world.save().unwrap();

    let aaa = world.country_missions("AAA").unwrap();
    let bbb = world.country_missions("BBB").unwrap();
    assert!(aaa.selection_known && bbb.selection_known);
    assert_eq!(
        mission(&world, "AAA", "controlled_by_root").requirements_met,
        Some(false)
    );
    assert_eq!(
        mission(&world, "BBB", "controlled_by_root").requirements_met,
        Some(true)
    );
    assert_eq!(
        mission(&world, "AAA", "controlled_by_literal").requirements_met,
        Some(true)
    );
    assert_eq!(
        mission(&world, "BBB", "controlled_by_literal").requirements_met,
        Some(true)
    );
    assert_eq!(
        mission(&world, "AAA", "controlled_by_owner").requirements_met,
        Some(false)
    );
    assert_eq!(
        mission(&world, "BBB", "controlled_by_owner").requirements_met,
        Some(false)
    );
    assert_eq!(
        mission(&world, "AAA", "controlled_by_unknown").requirements_met,
        None
    );
    assert_eq!(
        mission(&world, "BBB", "controlled_by_unknown").requirements_met,
        None
    );
    assert_eq!(
        mission(&world, "AAA", "controlled_by_absent").requirements_met,
        Some(false)
    );
    assert_eq!(
        mission(&world, "BBB", "controlled_by_absent").requirements_met,
        Some(true)
    );

    assert_eq!(
        world
            .query_mission_province_highlights("AAA", "controlled_by_root")
            .unwrap()
            .provinces,
        BTreeSet::from([2])
    );
    assert_eq!(
        world
            .query_mission_province_highlights("BBB", "controlled_by_root")
            .unwrap()
            .provinces,
        BTreeSet::from([1])
    );
    let unknown = world
        .query_mission_province_highlights("AAA", "controlled_by_unknown")
        .unwrap();
    assert!(unknown.provinces.is_empty());
    assert!(unknown.unknown_provinces.contains_key(&4));
    assert_eq!(
        world.save().unwrap(),
        before_queries,
        "queries do not mutate snapshots"
    );

    let province_one = world.initial.province_entity(1).unwrap();
    let aaa_entity = world.initial.country_entity("AAA").unwrap();
    let bbb_entity = world.initial.country_entity("BBB").unwrap();
    assert_eq!(
        world.initial.world().entity(province_one).unwrap().owner,
        Some(aaa_entity)
    );
    world
        .initial
        .execute(province_one, &controller_action(Some("AAA")))
        .unwrap();
    assert_eq!(
        world
            .initial
            .world()
            .province_controller_state(province_one)
            .unwrap(),
        Some(Some(aaa_entity))
    );
    assert_eq!(
        world.initial.world().entity(province_one).unwrap().owner,
        Some(aaa_entity),
        "changing the controller does not transfer ownership"
    );
    assert_eq!(
        mission(&world, "AAA", "controlled_by_root").requirements_met,
        Some(true)
    );
    assert_eq!(
        mission(&world, "BBB", "controlled_by_root").requirements_met,
        Some(false)
    );
    assert_eq!(
        mission(&world, "AAA", "controlled_by_owner").requirements_met,
        Some(true)
    );
    assert_eq!(
        mission(&world, "AAA", "controlled_by_literal").requirements_met,
        Some(false)
    );
    assert_eq!(
        world
            .query_mission_province_highlights("AAA", "controlled_by_root")
            .unwrap()
            .provinces,
        BTreeSet::from([1, 2])
    );
    assert!(mission(&world, "AAA", "controlled_by_root").claimable);

    let live_snapshot = world.save().unwrap();
    world
        .initial
        .execute(province_one, &controller_action(Some("BBB")))
        .unwrap();
    assert_eq!(
        mission(&world, "AAA", "controlled_by_root").requirements_met,
        Some(false)
    );
    world.restore(&live_snapshot).unwrap();
    assert_eq!(world.save().unwrap(), live_snapshot);
    assert_eq!(
        mission(&world, "AAA", "controlled_by_root").requirements_met,
        Some(true)
    );

    world.claim_mission("AAA", "controlled_by_root").unwrap();
    let claimed_snapshot = world.save().unwrap();
    let treasury = world
        .initial
        .world()
        .country_resources()
        .get(&aaa_entity)
        .and_then(|resources| resources.treasury_milli);
    assert_eq!(treasury, Some(3_000));
    assert_eq!(
        mission(&world, "AAA", "controlled_by_root").completed,
        Some(true)
    );
    assert!(world.claim_mission("AAA", "controlled_by_root").is_err());
    assert_eq!(
        world.save().unwrap(),
        claimed_snapshot,
        "repeat claim is atomic"
    );
    assert_eq!(
        world.initial.world().entity(province_one).unwrap().owner,
        Some(aaa_entity)
    );
    assert_eq!(
        world
            .initial
            .world()
            .province_controller_state(province_one)
            .unwrap(),
        Some(Some(aaa_entity))
    );
    assert_ne!(aaa_entity, bbb_entity);
}

#[test]
fn controller_owner_and_country_event_target_are_live_operands() {
    let fixture = tempfile::tempdir().unwrap();
    write_mod(fixture.path(), MISSIONS);
    let mut world = load_world(fixture.path(), &initial_state());
    let document = TextDocument::from_bytes(
        "controlled-by-event.txt",
        br#"
country_event = {
    id = controlled_by.1
    is_triggered_only = yes
    trigger = { 1 = { controlled_by = event_target:controller_country } }
    immediate = { save_global_event_target_as = controller_country }
    option = { name = test_option }
}
"#
        .to_vec(),
        TextEncoding::Utf8,
    );
    let global_types = BTreeMap::from([("controller_country".into(), EntityKind::Country)]);
    let library = ScriptLibrary::new();
    let compiled = ScriptCompiler::new(world.initial.world().schema(), &library, true)
        .with_global_scope_types(global_types.clone())
        .compile_events(&document)
        .unwrap();
    let definition = compiled
        .definitions
        .iter()
        .find(|definition| definition.id == "controlled_by.1")
        .unwrap();
    let condition = compiled.conditions.get("controlled_by.1").unwrap();
    let aaa = world.initial.country_entity("AAA").unwrap();
    let bbb = world.initial.country_entity("BBB").unwrap();
    world.initial.execute(bbb, &definition.immediate).unwrap();
    assert!(condition.evaluate(world.initial.world(), aaa).unwrap());

    let province = world.initial.province_entity(1).unwrap();
    world
        .initial
        .execute(province, &controller_action(Some("AAA")))
        .unwrap();
    assert!(!condition.evaluate(world.initial.world(), aaa).unwrap());

    let undeclared = TextDocument::from_bytes(
        "controlled-by-undeclared.txt",
        b"country_event = { id = controlled_by.2 is_triggered_only = yes trigger = { 1 = { controlled_by = event_target:missing_country } } option = { name = test_option } }".to_vec(),
        TextEncoding::Utf8,
    );
    assert!(
        ScriptCompiler::new(world.initial.world().schema(), &library, true)
            .compile_events(&undeclared)
            .is_err(),
        "controlled_by must reject event targets without a country declaration"
    );

    let known_country = ScriptCompiler::new(world.initial.world().schema(), &library, true)
        .with_global_scope_types(BTreeMap::from([(
            "missing_country".into(),
            EntityKind::Country,
        )]))
        .compile_events(&undeclared)
        .unwrap()
        .conditions
        .remove("controlled_by.2")
        .unwrap();
    assert!(!known_country.evaluate(world.initial.world(), aaa).unwrap());
    world
        .initial
        .execute(
            aaa,
            &[Action::BindGlobalScope {
                name: "missing_country".into(),
            }],
        )
        .unwrap();
    assert!(known_country.evaluate(world.initial.world(), aaa).unwrap());

    let wrong_kind = TextDocument::from_bytes(
        "controlled-by-wrong-kind.txt",
        b"country_event = { id = controlled_by.3 is_triggered_only = yes trigger = { 1 = { controlled_by = event_target:province_target } } option = { name = test_option } }".to_vec(),
        TextEncoding::Utf8,
    );
    let wrong_kind_types = BTreeMap::from([("province_target".into(), EntityKind::Province)]);
    assert!(
        ScriptCompiler::new(world.initial.world().schema(), &library, true)
            .with_global_scope_types(wrong_kind_types)
            .compile_events(&wrong_kind)
            .is_err()
    );

    for operand in ["FROM", "REB"] {
        let invalid = TextDocument::from_bytes(
            format!("controlled-by-{operand}.txt"),
            format!("country_event = {{ id = controlled_by.invalid is_triggered_only = yes trigger = {{ 1 = {{ controlled_by = {operand} }} }} option = {{ name = test_option }} }}").into_bytes(),
            TextEncoding::Utf8,
        );
        assert!(
            ScriptCompiler::new(world.initial.world().schema(), &library, true)
                .compile_events(&invalid)
                .is_err(),
            "unsupported controller operand {operand} must be rejected"
        );
    }

    let wrong_scope = TextDocument::from_bytes(
        "controlled-by-country-scope.txt",
        b"country_event = { id = controlled_by.invalid is_triggered_only = yes trigger = { controlled_by = AAA } option = { name = test_option } }".to_vec(),
        TextEncoding::Utf8,
    );
    assert!(
        ScriptCompiler::new(world.initial.world().schema(), &library, true)
            .compile_events(&wrong_scope)
            .is_err()
    );
}

#[test]
fn loaded_missions_and_selected_events_resolve_live_country_event_targets() {
    let fixture = tempfile::tempdir().unwrap();
    write_mod(
        fixture.path(),
        &format!("{MISSIONS}\n{EVENT_TARGET_MISSIONS}"),
    );
    fs::create_dir_all(fixture.path().join("events")).unwrap();
    fs::create_dir_all(fixture.path().join("common/scripted_effects")).unwrap();
    fs::write(
        fixture
            .path()
            .join("common/scripted_effects/controller.txt"),
        "bind_controller_country = { save_global_event_target_as = controller_country }",
    )
    .unwrap();
    fs::write(
        fixture.path().join("events/controller_target.txt"),
        "country_event = { id = controlled_by.target is_triggered_only = yes trigger = { 1 = { controlled_by = event_target:controller_country } } immediate = { save_global_event_target_as = controller_country } option = { name = target_checked } }",
    )
    .unwrap();

    let mut world = load_world(fixture.path(), &initial_state());
    let selected_event_condition = world
        .initial
        .world()
        .event_catalog()
        .conditions()
        .get("controlled_by.target")
        .unwrap()
        .clone();
    assert_eq!(
        mission(&world, "AAA", "check_controller_target").requirements_met,
        Some(false),
        "an unbound event target does not match"
    );
    let aaa = world.initial.country_entity("AAA").unwrap();
    assert!(
        !selected_event_condition
            .evaluate(world.initial.world(), aaa)
            .unwrap()
    );
    let setup = mission(&world, "AAA", "save_controller_target");
    assert!(setup.claimable, "{:?}", setup.unresolved);

    let province = world.initial.province_entity(1).unwrap();
    world
        .initial
        .execute(province, &controller_action(Some("AAA")))
        .unwrap();
    world
        .claim_mission("AAA", "save_controller_target")
        .unwrap();
    assert_eq!(
        mission(&world, "AAA", "check_controller_target").requirements_met,
        Some(true)
    );
    assert!(
        selected_event_condition
            .evaluate(world.initial.world(), aaa)
            .unwrap()
    );
    assert!(
        world
            .initial
            .world()
            .pending_events()
            .iter()
            .any(|pending| pending.event == "controlled_by.target"),
        "mission-selected event compiles and schedules with the live target"
    );

    let snapshot = world.save().unwrap();
    world
        .initial
        .execute(
            province,
            &[Action::BindGlobalScope {
                name: "controller_country".into(),
            }],
        )
        .unwrap();
    assert_eq!(
        mission(&world, "AAA", "check_controller_target").requirements_met,
        Some(false),
        "a province-valued global target does not match a country controller"
    );
    assert!(
        !selected_event_condition
            .evaluate(world.initial.world(), aaa)
            .unwrap()
    );
    world.restore(&snapshot).unwrap();
    assert_eq!(world.save().unwrap(), snapshot);
    assert_eq!(
        mission(&world, "AAA", "check_controller_target").requirements_met,
        Some(true),
        "the country event target survives save and restore"
    );
    assert!(
        selected_event_condition
            .evaluate(world.initial.world(), aaa)
            .unwrap()
    );
}

#[test]
fn loaded_missions_reject_undeclared_country_event_targets() {
    let fixture = tempfile::tempdir().unwrap();
    write_mod(
        fixture.path(),
        r#"
undeclared_target_series = {
    slot = 7
    potential = { tag = AAA }
    check_target = {
        position = 1
        trigger = { 1 = { controlled_by = event_target:missing_country } }
        effect = {}
    }
}
"#,
    );

    let world = load_world(fixture.path(), &initial_state());
    let check = mission(&world, "AAA", "check_target");
    assert!(
        !check.claimable
            && check
                .unresolved
                .iter()
                .any(|reason| reason.contains("controlled_by event target must be a country")),
        "undeclared target should block the mission with a diagnostic: {:?}",
        check.unresolved
    );
}

#[test]
fn loaded_missions_reject_province_event_target_declarations() {
    let fixture = tempfile::tempdir().unwrap();
    write_mod(
        fixture.path(),
        r#"
province_target_series = {
    slot = 7
    potential = { tag = AAA }
    check_target = {
        position = 1
        trigger = { 1 = { controlled_by = event_target:controller_target } }
        effect = {}
    }
}
"#,
    );
    fs::create_dir_all(fixture.path().join("events")).unwrap();
    fs::write(
        fixture.path().join("events/province_target.txt"),
        "province_event = { id = controlled_by.province_target is_triggered_only = yes immediate = { save_global_event_target_as = controller_target } option = { name = target_saved } }",
    )
    .unwrap();

    let world = load_world(fixture.path(), &initial_state());
    let check = mission(&world, "AAA", "check_target");
    assert!(
        !check.claimable
            && check
                .unresolved
                .iter()
                .any(|reason| reason.contains("controlled_by event target must be a country")),
        "province-valued target should block the mission with a diagnostic: {:?}",
        check.unresolved
    );
}

#[test]
fn loaded_missions_reject_conflicting_event_target_declarations() {
    let fixture = tempfile::tempdir().unwrap();
    write_mod(
        fixture.path(),
        r#"
conflicting_target_series = {
    slot = 7
    potential = { tag = AAA }
    check_target = {
        position = 1
        trigger = { 1 = { controlled_by = event_target:controller_target } }
        effect = {}
    }
}
"#,
    );
    fs::create_dir_all(fixture.path().join("events")).unwrap();
    fs::write(
        fixture.path().join("events/country_target.txt"),
        "country_event = { id = controlled_by.country_target is_triggered_only = yes immediate = { save_global_event_target_as = controller_target } option = { name = target_saved } }",
    )
    .unwrap();
    fs::write(
        fixture.path().join("events/province_target.txt"),
        "province_event = { id = controlled_by.province_target is_triggered_only = yes immediate = { save_global_event_target_as = controller_target } option = { name = target_saved } }",
    )
    .unwrap();

    let world = load_world(fixture.path(), &initial_state());
    let check = mission(&world, "AAA", "check_target");
    assert!(
        !check.claimable
            && check
                .unresolved
                .iter()
                .any(|reason| reason.contains("controlled_by event target must be a country")),
        "conflicting target declarations should block the mission: {:?}",
        check.unresolved
    );
}

#[test]
fn explicit_absence_is_false_under_controlled_by_and_true_only_after_negation() {
    let fixture = tempfile::tempdir().unwrap();
    write_mod(fixture.path(), MISSIONS);
    let mut world = load_world(fixture.path(), &initial_state());
    let province = world.initial.province_entity(2).unwrap();
    assert_eq!(
        mission(&world, "AAA", "controlled_by_absent").requirements_met,
        Some(false)
    );
    assert_eq!(
        mission(&world, "BBB", "controlled_by_absent").requirements_met,
        Some(true)
    );

    world
        .initial
        .execute(province, &controller_action(None))
        .unwrap();
    assert_eq!(
        world
            .initial
            .world()
            .province_controller_state(province)
            .unwrap(),
        Some(None)
    );
    assert_eq!(
        mission(&world, "AAA", "controlled_by_absent").requirements_met,
        Some(true)
    );
    assert_eq!(
        mission(&world, "BBB", "controlled_by_absent").requirements_met,
        Some(true)
    );
    let absent_snapshot = world.save().unwrap();
    world.restore(&absent_snapshot).unwrap();
    assert_eq!(world.save().unwrap(), absent_snapshot);
    assert_eq!(
        mission(&world, "AAA", "controlled_by_absent").requirements_met,
        Some(true)
    );
}
