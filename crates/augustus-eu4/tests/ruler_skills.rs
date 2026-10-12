// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::*;
use augustus_eu4::*;
use std::{fs, path::Path};

fn document(text: &str) -> TextDocument {
    TextDocument::from_bytes(
        "events/self_authored.txt",
        text.as_bytes().to_vec(),
        TextEncoding::Utf8,
    )
}

#[test]
fn translated_changes_and_conditions_read_current_ruler_with_content_bounds() {
    let mut outcomes = Vec::new();
    for optimise in [false, true] {
        let mut world = World::new(Schema::new(["unused"]).unwrap());
        world.add_entity("AAA", EntityKind::Country);
        world
            .set_politics(Politics {
                ruler_skill_bounds: Some(RulerSkillBounds {
                    minimum: 2,
                    maximum: 4,
                }),
                countries: [(
                    EntityId(0),
                    CountryPolitics {
                        ruler: Some(PersonState {
                            administration: Some(5),
                            diplomacy: Some(3),
                            military: Some(1),
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                )]
                .into(),
                ..Default::default()
            })
            .unwrap();
        let library = ScriptLibrary::new();
        let text = "country_event={id=skill.start is_triggered_only=yes immediate={
            if={limit={adm=5} log=historical}
            change_adm=0.999 change_dip=1.999 change_mil=-1.999
            if={limit={adm=4 NOT={adm=5} dip=4 mil=2} log=changed}
            change_adm=-100000 change_dip=-0.999 change_mil=100000
            if={limit={adm=2.999 NOT={adm=3} dip=4 mil=4} log=bounded}
        } option={name=ok}}";
        let compiled = ScriptCompiler::new(world.schema(), &library, optimise)
            .compile_events(&document(text))
            .unwrap();
        world
            .set_event_catalog(EventCatalog::new(compiled.definitions).unwrap())
            .unwrap();
        world.trigger_event("skill.start", EntityId(0)).unwrap();
        assert_eq!(
            world
                .events()
                .iter()
                .map(|(_, _, text)| text.as_str())
                .collect::<Vec<_>>(),
            ["historical", "changed", "bounded"]
        );
        outcomes.push(world.save("fixture").unwrap());
    }
    assert_eq!(outcomes[0], outcomes[1]);
}

#[test]
fn invalid_skill_operands_and_province_scopes_keep_source_diagnostics() {
    let schema = Schema::new(["unused"]).unwrap();
    let library = ScriptLibrary::new();
    for body in [
        "capital_scope={change_adm=1}",
        "change_dip=2147483648",
        "change_mil=other",
        "if={limit={adm=BBB} log=bad}",
        "capital_scope={if={limit={mil=1} log=bad}}",
    ] {
        let text = format!(
            "country_event={{id=bad is_triggered_only=yes immediate={{{body}}} option={{name=ok}}}}"
        );
        let error = ScriptCompiler::new(&schema, &library, true)
            .compile_events(&document(&text))
            .unwrap_err();
        assert!(error.location().is_some(), "{error}");
        assert!(error.to_string().contains("self_authored.txt"));
    }
}

#[test]
fn loading_declared_skill_bounds_preserves_historical_values_and_initial_queries() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world");
    let temp = tempfile::tempdir().unwrap();
    for entry in walkdir::WalkDir::new(&source) {
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
    let text = fs::read_to_string(&defines).unwrap();
    let text = text.replace(
        "HRE_RANK = 3,",
        "HRE_RANK = 3,\nMONARCH_MIN_SKILL = 2,\nMONARCH_MAX_SKILL = 3,",
    );
    fs::write(&defines, &text).unwrap();
    fs::create_dir_all(temp.path().join("common/estates")).unwrap();
    fs::write(temp.path().join("common/estates/skill.txt"), "estate_skill={trigger={adm=2 NOT={adm=3} mil=4}} estate_owner_skill={trigger={capital_scope={owner={dip=3}}}}").unwrap();
    let mut loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let world = loaded.initial.world();
    let country = loaded.initial.country_entity("AAA").unwrap();
    assert_eq!(world.ruler_skill(country, RulerSkill::Military).unwrap(), 4);
    assert_eq!(
        world.politics().ruler_skill_bounds,
        Some(RulerSkillBounds {
            minimum: 2,
            maximum: 3
        })
    );
    assert_eq!(
        loaded.report.setup.ruler_skill_bounds,
        world.politics().ruler_skill_bounds
    );
    assert_eq!(
        loaded.country("AAA").unwrap().estates.unwrap()["estate_skill"].active,
        Some(true)
    );
    assert_eq!(
        loaded.country("AAA").unwrap().estates.unwrap()["estate_owner_skill"].active,
        Some(true)
    );
    loaded
        .initial
        .execute(
            country,
            &[Action::ChangeRulerSkill {
                target: Target::Current,
                skill: RulerSkill::Military,
                delta: 0,
            }],
        )
        .unwrap();
    assert_eq!(
        loaded
            .initial
            .world()
            .ruler_skill(country, RulerSkill::Military)
            .unwrap(),
        3
    );
    fs::write(
        &defines,
        text.replace("MONARCH_MIN_SKILL = 2,", "MONARCH_MIN_SKILL = 4,"),
    )
    .unwrap();
    assert!(
        VanillaWorld::load(temp.path(), "1444.11.11")
            .err()
            .unwrap()
            .to_string()
            .contains("exceeds")
    );
    fs::write(&defines, text.replace("MONARCH_MAX_SKILL = 3,", "")).unwrap();
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(
        loaded
            .initial
            .world()
            .politics()
            .ruler_skill_bounds
            .is_none()
    );
    fs::write(&defines, &text).unwrap();
    fs::create_dir_all(temp.path().join("common/on_actions")).unwrap();
    let startup = temp.path().join("common/on_actions/skill.txt");
    fs::write(&startup, "on_startup={if={limit={tag=AAA} change_mil=0}}").unwrap();
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert_eq!(
        loaded
            .initial
            .world()
            .ruler_skill(country, RulerSkill::Military)
            .unwrap(),
        3
    );
    fs::write(
        &startup,
        "on_startup={if={limit={tag=AAA} change_mil=0 unknown_after_skill=yes}}",
    )
    .unwrap();
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert_eq!(loaded.report.estates.applied_startup_effects, 0);
    assert!(!loaded.report.estates.startup_unresolved.is_empty());
    assert_eq!(
        loaded
            .initial
            .world()
            .ruler_skill(country, RulerSkill::Military)
            .unwrap(),
        4
    );
}
