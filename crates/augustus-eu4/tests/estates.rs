// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    Action, EstateDefinition, EstateEffect, EstateMetric, EstatePrivilege, EstateRules,
    EstateState, Estates, ModifierDefinition,
};
use augustus_eu4::{
    ContentRoot, ContentSet, ContentSource, VanillaExternalEstateState, VanillaExternalState,
    VanillaWorld,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world")
}

fn startup_fixture() -> tempfile::TempDir {
    let temp = land_fixture();
    let path = "history/countries/AAA - Alpha.txt";
    let text = fs::read_to_string(temp.path().join(path)).unwrap();
    write(
        temp.path(),
        path,
        &text.replace("set_estate_privilege = charter", ""),
    );
    write(
        temp.path(),
        "common/estate_privileges/test.txt",
        r#"
        charter = {
            benefits = { global_tax_modifier = 0.1 }
            is_valid = { tag = AAA }
            on_granted = {
                if = { limit = { has_estate_privilege = charter }
                    unreachable_effect = yes
                }
                else = { add_estate_loyalty = { estate = estate_guild loyalty = 13 } }
                change_estate_land_share = { estate = estate_guild share = 2 }
            }
            on_revoked = {
                if = { limit = { has_estate_privilege = charter }
                    add_estate_loyalty = { estate = estate_guild loyalty = -11 }
                }
                else = { unreachable_effect = yes }
                change_estate_land_share = { estate = estate_guild share = -2 }
            }
        }
    "#,
    );
    temp
}

#[test]
fn external_privilege_validity_is_resolved_after_pending_startup() {
    let temp = startup_fixture();
    write(
        temp.path(),
        "common/on_actions/test.txt",
        "on_startup={unsupported_effect=yes}",
    );
    let state = VanillaExternalState {
        estate_privileges: BTreeMap::from([(
            "AAA".into(),
            BTreeMap::from([(
                "estate_guild".into(),
                Some(BTreeSet::from(["charter".into()])),
            )]),
        )]),
        estate_states: BTreeMap::from([(
            "AAA".into(),
            BTreeMap::from([(
                "estate_guild".into(),
                VanillaExternalEstateState {
                    loyalty_milli: Some(50_000),
                    land_share_micros: Some(150_000),
                    active_influences: None,
                },
            )]),
        )]),
        ..Default::default()
    };
    let loaded = VanillaWorld::load_with_external_state(temp.path(), "1444.11.11", &state).unwrap();
    assert!(!loaded.report.estates.pending_startup_effects.is_empty());
    let country = loaded.initial.country_entity("AAA").unwrap();
    assert_eq!(
        loaded.initial.world().estates().resolved_privilege_effects[&country]["charter"],
        BTreeSet::from(["is_valid".into()])
    );
}

#[test]
fn plaintext_save_binds_estate_state_and_resolves_privilege_validity() {
    let temp = startup_fixture();
    write(
        temp.path(),
        "common/estate_privileges/test.txt",
        "charter = { benefits = { global_tax_modifier = 0.1 } is_valid = { has_country_flag = permission_flag } on_granted = {} }",
    );
    write(temp.path(), "common/on_actions/test.txt", "on_startup = {}");
    let save_path = temp.path().join("estate-state.eu4");
    let save_text = r#"EU4txt
date=1444.11.11
player="AAA"
countries={
    AAA={
        human=yes
        flags={ permission_flag=1444.11.11 }
        estate={
            type="estate_guild"
            loyalty=50.000
            territory=15.000
            active_influences={ 4 }
            granted_privileges={ { charter 1444.11.11 } }
        }
    }
}
provinces={
    -1={ owner="AAA" local_autonomy=0.000 }
}
"#;
    fs::write(&save_path, save_text).unwrap();
    let content = ContentSet::resolve(vec![ContentSource::base(
        ContentRoot::open(temp.path()).unwrap(),
    )])
    .unwrap();

    let loaded = VanillaWorld::load_from_content_with_save(&content, &save_path).unwrap();
    let country = loaded.initial.country_entity("AAA").unwrap();
    let view = loaded.country("AAA").unwrap();
    let estate = &view.estates.unwrap()["estate_guild"];

    assert!(view.flags.unwrap().contains_key("permission_flag"));
    assert_eq!(estate.loyalty_milli, Some(50_000));
    assert_eq!(estate.land_share_micros, Some(150_000));
    assert_eq!(estate.privileges, Some(BTreeSet::from(["charter".into()])));
    assert_eq!(
        loaded.initial.world().estates().resolved_privilege_effects[&country]["charter"],
        BTreeSet::from(["is_valid".into()])
    );
    assert_eq!(
        view.declared_estate_effects.known_totals_micros["global_tax_modifier"],
        100_000
    );

    let denied_save_path = temp.path().join("estate-state-without-permission.eu4");
    let denied_save_text = save_text.replace("flags={ permission_flag=1444.11.11 }", "flags={}");
    fs::write(&denied_save_path, denied_save_text).unwrap();
    let denied = VanillaWorld::load_from_content_with_save(&content, &denied_save_path).unwrap();
    let denied_country = denied.initial.country_entity("AAA").unwrap();
    assert!(
        !denied
            .initial
            .world()
            .estates()
            .resolved_privilege_effects
            .get(&denied_country)
            .and_then(|effects| effects.get("charter"))
            .is_some_and(|effects| effects.contains("is_valid"))
    );
    assert!(
        denied
            .country("AAA")
            .unwrap()
            .declared_estate_effects
            .unresolved
            .iter()
            .any(|reason| reason.ends_with(":unexecuted:is_valid"))
    );
}

#[test]
fn external_valid_privilege_keeps_default_loyalty_when_save_has_no_scalar() {
    let temp = startup_fixture();
    write(temp.path(), "common/on_actions/test.txt", "on_startup={}");
    let state = VanillaExternalState {
        estate_privileges: BTreeMap::from([(
            "AAA".into(),
            BTreeMap::from([
                (
                    "estate_guild".into(),
                    Some(BTreeSet::from(["charter".into()])),
                ),
                ("estate_council".into(), Some(BTreeSet::new())),
                ("estate_order".into(), Some(BTreeSet::new())),
            ]),
        )]),
        ..Default::default()
    };
    let loaded = VanillaWorld::load_with_external_state(temp.path(), "1444.11.11", &state).unwrap();
    assert_eq!(
        loaded.country("AAA").unwrap().estates.unwrap()["estate_guild"].loyalty_milli,
        Some(50_000)
    );
}

fn loyalty(amount: i64) -> Action {
    Action::ChangeEstateMetric {
        estate: "estate_guild".into(),
        metric: EstateMetric::Loyalty,
        amount,
        minimum: Some(0),
    }
}

#[test]
fn estate_trigger_matches_explicit_unit_type_and_preserves_missing_as_unknown() {
    let temp = land_fixture();
    append(
        temp.path(),
        "history/countries/AAA - Alpha.txt",
        "unit_type = eastern",
    );
    append(
        temp.path(),
        "common/estates/test.txt",
        "estate_eastern = { trigger = { unit_type = eastern } base_influence = 4 }",
    );

    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert_eq!(
        loaded.country("AAA").unwrap().estates.unwrap()["estate_eastern"].active,
        Some(true)
    );
    assert_eq!(
        loaded.country("BBB").unwrap().estates.unwrap()["estate_eastern"].active,
        None
    );
    assert!(
        loaded.report.estates.unresolved_countries["BBB"]
            .contains("estate_eastern:unknown_input:unit_type")
    );
}

#[test]
fn startup_named_values_share_candidates_choices_delays_and_rollback() {
    let temp = startup_fixture();
    write(
        temp.path(),
        "common/on_actions/test.txt",
        r#"on_startup={events={numeric.before} if={limit={tag=AAA} if={limit={is_variable_equal={which=count value=2.125}} log=startup} set_variable={which=other value=-0.001} multiply_variable={which=other value=0.5} if={limit={is_variable_equal={which=other value=0}} log=truncated} country_event={id=numeric.later days=1}}}"#,
    );
    write(
        temp.path(),
        "events/test.txt",
        r#"
      country_event={id=numeric.before is_triggered_only=yes trigger={tag=AAA check_variable={which=count value=0}}
        immediate={change_variable={which=count value=2.125} log=before}
        option={name=accept change_variable={which=count value=0.375}}
        after={if={limit={is_variable_equal={which=count value=2.5}} log=after}}}
      country_event={id=numeric.later is_triggered_only=yes trigger={check_variable={which=count value=2.5}} immediate={log=later} option={name=accept}}
    "#,
    );
    let mut loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(
        loaded.report.estates.pending_startup_effects.is_empty(),
        "{:?}",
        loaded.report.estates.startup_unresolved
    );
    let country = loaded.initial.country_entity("AAA").unwrap();
    let other = loaded.initial.country_entity("BBB").unwrap();
    let slot = augustus_core::NumericSlot::Named("count".into());
    assert_eq!(
        loaded
            .initial
            .world()
            .numeric_value(country, &slot)
            .unwrap(),
        2125
    );
    assert_eq!(
        loaded.initial.world().numeric_value(other, &slot).unwrap(),
        0
    );
    let waiting = loaded.save().unwrap();
    let mut restored = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    restored.restore(&waiting).unwrap();
    for world in [&mut loaded, &mut restored] {
        world
            .initial
            .choose_event(&augustus_core::EventChoice {
                pending: augustus_core::PendingEventId(0),
                option: "0".into(),
            })
            .unwrap();
        world.advance(1).unwrap();
    }
    assert_eq!(loaded.save().unwrap(), restored.save().unwrap());
    assert_eq!(
        loaded
            .initial
            .world()
            .events()
            .iter()
            .map(|e| (e.0, e.2.as_str()))
            .collect::<Vec<_>>(),
        [
            (0, "before"),
            (0, "startup"),
            (0, "truncated"),
            (0, "after"),
            (1, "later")
        ]
    );
    write(
        temp.path(),
        "common/on_actions/test.txt",
        "on_startup={if={limit={tag=AAA} set_variable={which=count value=1} log=prefix unsupported=yes}}",
    );
    let failed = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(!failed.report.estates.pending_startup_effects.is_empty());
    assert!(failed.initial.world().named_values().is_empty());
    assert!(
        failed
            .initial
            .world()
            .numeric_value(country, &slot)
            .is_err()
    );
    assert!(failed.initial.world().events().is_empty());
    write(
        temp.path(),
        "common/on_actions/test.txt",
        "on_startup={if={limit={tag=AAA} if={limit={check_variable={which=count value=0}} log=incorrect}}}",
    );
    let other_date = VanillaWorld::load(temp.path(), "1450.1.1").unwrap();
    assert!(!other_date.report.estates.pending_startup_effects.is_empty());
    assert!(
        other_date
            .initial
            .world()
            .numeric_value(country, &slot)
            .is_err()
    );
}

#[test]
fn startup_modifier_state_drives_queries_attributes_choices_expiry_and_rollback() {
    let temp = startup_fixture();
    let path = "common/event_modifiers/test_modifiers.txt";
    let mut text = fs::read_to_string(temp.path().join(path)).unwrap();
    text.push_str("\nsample_boost={global_tax_modifier=0.125}\nsample_guard={}\nsample_ambiguous={global_tax_modifier=0.1 global_tax_modifier=0.2}\n");
    write(temp.path(), path, &text);
    write(
        temp.path(),
        "common/on_actions/test.txt",
        r#"on_startup={events={modifier.before} if={limit={tag=AAA} add_country_modifier={name=sample_boost duration=4} if={limit={has_country_modifier=sample_boost} log=startup} country_event={id=modifier.expired days=4}}}"#,
    );
    write(
        temp.path(),
        "events/test.txt",
        r#"
      country_event={id=modifier.before is_triggered_only=yes trigger={tag=AAA NOT={has_country_modifier=sample_boost}} immediate={add_country_modifier={name=sample_guard duration=-1} log=before} option={name=accept add_country_modifier={name=sample_boost duration=-1}} after={if={limit={has_country_modifier=sample_boost} log=after}}}
      country_event={id=modifier.expired is_triggered_only=yes trigger={NOT={has_country_modifier=sample_boost}} immediate={log=expired} option={name=accept remove_country_modifier=sample_guard}}
    "#,
    );
    let mut loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(loaded.report.estates.startup_unresolved.is_empty());
    let country = loaded.initial.country_entity("AAA").unwrap();
    let province = loaded.country("AAA").unwrap().bound_capital.unwrap();
    assert!(loaded.country("AAA").unwrap().modifier_attachments_known);
    let other = loaded.initial.country_entity("BBB").unwrap();
    assert!(
        loaded
            .initial
            .world()
            .has_modifier(country, "sample_boost")
            .unwrap()
    );
    assert!(
        !loaded
            .initial
            .world()
            .has_modifier(other, "sample_boost")
            .unwrap()
    );
    assert!(
        loaded.province(province).unwrap().attributes["tax_basis"]
            .steps
            .iter()
            .flat_map(|s| &s.contributions)
            .any(|c| c.source == "modifier:AAA:sample_boost")
    );
    loaded
        .initial
        .choose_event(&augustus_core::EventChoice {
            pending: augustus_core::PendingEventId(0),
            option: "0".into(),
        })
        .unwrap();
    let waiting = loaded.save().unwrap();
    let mut resumed = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    resumed.restore(&waiting).unwrap();
    for state in [&mut loaded, &mut resumed] {
        state.advance(4).unwrap();
        state
            .initial
            .choose_event(&augustus_core::EventChoice {
                pending: augustus_core::PendingEventId(1),
                option: "0".into(),
            })
            .unwrap();
        assert!(
            !state
                .initial
                .world()
                .has_modifier(country, "sample_boost")
                .unwrap()
        );
        assert!(
            !state.province(province).unwrap().attributes["tax_basis"]
                .steps
                .iter()
                .flat_map(|s| &s.contributions)
                .any(|c| c.source == "modifier:AAA:sample_boost")
        );
        assert_eq!(
            state
                .initial
                .world()
                .events()
                .iter()
                .map(|e| (e.0, e.2.as_str()))
                .collect::<Vec<_>>(),
            [(0, "before"), (0, "startup"), (0, "after"), (4, "expired")]
        );
    }
    assert_eq!(loaded.save().unwrap(), resumed.save().unwrap());
    loaded
        .initial
        .execute(
            country,
            &[Action::AttachModifier {
                target: augustus_core::Target::Current,
                name: "sample_ambiguous".into(),
                duration: None,
                hidden: None,
                renewal: augustus_core::ModifierRenewal::Replace,
                duration_label: None,
            }],
        )
        .unwrap();
    assert!(
        loaded.province(province).unwrap().attributes["tax_basis"]
            .unresolved
            .contains("modifier:sample_ambiguous:global_tax_modifier:unknown_property")
    );
    loaded
        .initial
        .execute(
            country,
            &[Action::RemoveModifier {
                target: augustus_core::Target::Current,
                name: "sample_ambiguous".into(),
            }],
        )
        .unwrap();
    assert!(
        !loaded.province(province).unwrap().attributes["tax_basis"]
            .unresolved
            .contains("modifier:sample_ambiguous:global_tax_modifier:unknown_property")
    );
    write(
        temp.path(),
        "common/on_actions/test.txt",
        "on_startup={if={limit={tag=AAA} add_country_modifier={name=sample_guard duration=-1 desc=charter_duration}}}",
    );
    let described = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(described.report.estates.startup_unresolved.is_empty());
    let country = described.initial.country_entity("AAA").unwrap();
    assert_eq!(
        described.initial.world().modifiers().active[&country]["sample_guard"].expires_on,
        None
    );
    write(
        temp.path(),
        "common/on_actions/test.txt",
        "on_startup={if={limit={tag=AAA} add_country_modifier={name=sample_boost duration=4} add_country_modifier={name=sample_guard duration=-1 message=unverified}}}",
    );
    let failed = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(!failed.report.estates.startup_unresolved.is_empty());
    assert!(
        failed
            .report
            .estates
            .startup_unresolved
            .iter()
            .any(|reason| reason.contains("unsupported modifier parameter"))
    );
    assert!(!failed.country("AAA").unwrap().modifier_attachments_known);
    assert!(
        failed
            .initial
            .world()
            .modifiers()
            .unknown_entities
            .contains(&country)
    );
    assert!(
        failed
            .initial
            .world()
            .has_modifier(country, "sample_boost")
            .is_err()
    );
    assert!(
        !failed
            .initial
            .world()
            .modifiers()
            .active
            .get(&country)
            .is_some_and(|v| v.contains_key("sample_boost"))
    );
}

#[test]
fn startup_mission_records_are_live_per_country_and_survive_pending_choices() {
    let temp = startup_fixture();
    write(
        temp.path(),
        "missions/test.txt",
        r#"sample_series={slot=1 generic=no potential={tag=AAA}
      sample_first={trigger={always=yes} effect={log=unexecuted_reward}}
      sample_later={position=2 trigger={always=no} effect={log=unexecuted_reward}}
    }"#,
    );
    write(
        temp.path(),
        "common/on_actions/test.txt",
        r#"on_startup={
      events={records.before}
      if={limit={tag=AAA}
        if={limit={mission_completed=sample_first} log=incorrect} else={log=fresh}
        complete_mission=sample_first complete_mission=sample_first
        if={limit={mission_completed=sample_first} log=done}
        country_event={id=records.later days=1}
      }
    }"#,
    );
    write(
        temp.path(),
        "events/test.txt",
        r#"
      country_event={id=records.before is_triggered_only=yes trigger={tag=AAA NOT={mission_completed=sample_first}}
        immediate={log=before} option={name=accept complete_mission=sample_later}
        after={if={limit={mission_completed=sample_later} log=after}}
      }
      country_event={id=records.later is_triggered_only=yes trigger={mission_completed=sample_later}
        immediate={log=later} option={name=accept}
      }
    "#,
    );
    let mut loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(
        loaded.report.estates.pending_startup_effects.is_empty(),
        "{:?}",
        loaded.report.estates.startup_unresolved
    );
    let first = loaded.initial.country_entity("AAA").unwrap();
    let other = loaded.initial.country_entity("BBB").unwrap();
    assert!(
        loaded
            .initial
            .world()
            .completion_recorded(first, "sample_first")
            .unwrap()
    );
    assert!(
        !loaded
            .initial
            .world()
            .completion_recorded(other, "sample_first")
            .unwrap()
    );
    assert_eq!(
        loaded
            .initial
            .world()
            .events()
            .iter()
            .map(|e| e.2.as_str())
            .collect::<Vec<_>>(),
        ["before", "fresh", "done"]
    );
    let pending = loaded.initial.world().pending_events()[0].id;
    loaded
        .initial
        .choose_event(&augustus_core::EventChoice {
            pending,
            option: "0".into(),
        })
        .unwrap();
    let saved = loaded.save().unwrap();
    let mut resumed = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    resumed.restore(&saved).unwrap();
    loaded.advance(1).unwrap();
    resumed.advance(1).unwrap();
    assert_eq!(loaded.save().unwrap(), resumed.save().unwrap());
    assert_eq!(
        loaded
            .initial
            .world()
            .events()
            .iter()
            .map(|e| (e.0, e.2.as_str()))
            .collect::<Vec<_>>(),
        [
            (0, "before"),
            (0, "fresh"),
            (0, "done"),
            (0, "after"),
            (1, "later")
        ]
    );
}

#[test]
fn incomplete_startup_rolls_back_mission_records_and_other_dates_stay_unknown() {
    for (date, body) in [
        (
            "1444.11.11",
            "complete_mission=sample_first log=prefix unsupported=yes",
        ),
        (
            "1450.1.1",
            "if={limit={mission_completed=sample_first} log=done} else={log=not_done}",
        ),
    ] {
        let temp = startup_fixture();
        write(
            temp.path(),
            "missions/test.txt",
            "sample_series={slot=1 sample_first={position=1 trigger={always=yes} effect={log=reward}}}",
        );
        write(
            temp.path(),
            "common/on_actions/test.txt",
            &format!("on_startup={{if={{limit={{tag=AAA}} {body}}}}}"),
        );
        let loaded = VanillaWorld::load(temp.path(), date).unwrap();
        assert!(
            !loaded.report.estates.pending_startup_effects.is_empty(),
            "{date}"
        );
        let completions = loaded.initial.world().completions();
        assert!(completions.entities.is_empty());
        assert!(completions.identifiers.contains("sample_first"));
        assert!(loaded.initial.world().objectives().entities.is_empty());
        let entity = loaded.initial.country_entity("AAA").unwrap();
        assert!(
            loaded
                .initial
                .world()
                .completion_recorded(entity, "sample_first")
                .is_err()
        );
        assert!(loaded.initial.world().events().is_empty());
    }
}

#[test]
fn startup_follows_event_references_inside_hidden_containers() {
    let temp = startup_fixture();
    write(
        temp.path(),
        "common/on_actions/test.txt",
        r#"on_startup={
      if={limit={tag=AAA} hidden_effect={country_event={id=group.first} log=startup}}
    }"#,
    );
    write(
        temp.path(),
        "events/groups.txt",
        r#"
      country_event={id=group.first is_triggered_only=yes
        immediate={hidden_effect={log=first hidden_effect={country_event={id=group.second}}}}
        option={name=accept hidden_effect={country_event={id=group.third days=1}}}
        after={hidden_effect={log=after}}
      }
      country_event={id=group.second is_triggered_only=yes
        immediate={hidden_effect={log=second}} option={name=accept}
      }
      country_event={id=group.third is_triggered_only=yes
        immediate={hidden_effect={log=third}} option={name=accept}
      }
    "#,
    );
    let mut loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(
        loaded.report.estates.pending_startup_effects.is_empty(),
        "{:?}",
        loaded.report.estates.startup_unresolved
    );
    assert_eq!(
        loaded
            .initial
            .world()
            .events()
            .iter()
            .map(|e| e.2.as_str())
            .collect::<Vec<_>>(),
        ["first", "second", "startup"]
    );
    let pending = loaded
        .initial
        .world()
        .pending_events()
        .iter()
        .find(|event| event.event == "group.first")
        .unwrap()
        .id;
    loaded
        .initial
        .choose_event(&augustus_core::EventChoice {
            pending,
            option: "0".into(),
        })
        .unwrap();
    let saved = loaded.save().unwrap();
    let mut resumed = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    resumed.restore(&saved).unwrap();
    loaded.advance(1).unwrap();
    resumed.advance(1).unwrap();
    assert_eq!(loaded.save().unwrap(), resumed.save().unwrap());
    assert_eq!(
        loaded
            .initial
            .world()
            .events()
            .iter()
            .map(|e| (e.0, e.2.as_str()))
            .collect::<Vec<_>>(),
        [
            (0, "first"),
            (0, "second"),
            (0, "startup"),
            (0, "after"),
            (1, "third")
        ]
    );
}

#[test]
fn startup_binds_preview_only_event_references_without_dispatch_and_rolls_back_invalid_bodies() {
    let temp = startup_fixture();
    write(
        temp.path(),
        "common/on_actions/test.txt",
        r#"on_startup={if={limit={tag=AAA}
        tooltip={country_event={id=preview.ghost days=1}}
        country_event={id=preview.parent}
    }}"#,
    );
    write(
        temp.path(),
        "events/previews.txt",
        r#"
        country_event={id=preview.parent is_triggered_only=yes
            immediate={tooltip={country_event={id=preview.ghost}} log=parent}
            option={name=accept tooltip={country_event={id=preview.ghost days=1}} log=option}
            after={log=after}}
        country_event={id=preview.ghost is_triggered_only=yes
            immediate={log=unexpected} option={name=accept}}
    "#,
    );
    let mut loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(
        loaded.report.estates.pending_startup_effects.is_empty(),
        "{:?}",
        loaded.report.estates.startup_unresolved
    );
    assert!(
        loaded
            .initial
            .world()
            .event_catalog()
            .definitions()
            .contains_key("preview.ghost")
    );
    assert_eq!(loaded.initial.world().pending_events().len(), 1);
    assert_eq!(loaded.initial.world().pending_tasks(), 0);
    let saved = loaded.save().unwrap();
    let mut resumed = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    resumed.restore(&saved).unwrap();
    for state in [&mut loaded, &mut resumed] {
        state
            .initial
            .choose_event(&augustus_core::EventChoice {
                pending: augustus_core::PendingEventId(0),
                option: "0".into(),
            })
            .unwrap();
        state.advance(2).unwrap();
        assert!(state.initial.world().pending_events().is_empty());
        assert_eq!(state.initial.world().pending_tasks(), 0);
        assert_eq!(
            state
                .initial
                .world()
                .events()
                .iter()
                .map(|e| e.2.as_str())
                .collect::<Vec<_>>(),
            ["parent", "option", "after"]
        );
    }
    assert_eq!(loaded.save().unwrap(), resumed.save().unwrap());
    for body in [
        "tooltip={unsupported_effect=yes}",
        "tooltip={country_event={id=missing}}",
    ] {
        write(
            temp.path(),
            "common/on_actions/test.txt",
            &format!("on_startup={{log=prefix {body}}}"),
        );
        let blocked = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
        assert!(!blocked.report.estates.pending_startup_effects.is_empty());
        assert!(blocked.initial.world().events().is_empty());
        assert!(
            blocked
                .initial
                .world()
                .event_catalog()
                .definitions()
                .is_empty()
        );
    }
}

#[test]
fn startup_custom_descriptions_keep_hidden_context_without_changing_event_execution() {
    let temp = startup_fixture();
    write(
        temp.path(),
        "common/on_actions/test.txt",
        r#"on_startup={if={limit={tag=AAA}
        custom_tooltip=startup
        hidden_effect={custom_tooltip=hidden tooltip={custom_tooltip=nested_hidden}}
        country_event={id=text.control}
    }}"#,
    );
    write(
        temp.path(),
        "events/text.txt",
        r#"
        country_event={id=text.control is_triggered_only=yes
            immediate={custom_tooltip=first hidden_effect={custom_tooltip=internal} log=immediate}
            option={name=accept custom_tooltip=choice hidden_effect={custom_tooltip=internal} country_event={id=text.later days=1}}
            after={custom_tooltip=after log=after}}
        country_event={id=text.later is_triggered_only=yes immediate={custom_tooltip=late log=later} option={name=accept}}
    "#,
    );
    let mut loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(
        loaded.report.estates.pending_startup_effects.is_empty(),
        "{:?}",
        loaded.report.estates.startup_unresolved
    );
    let immediate = &loaded.initial.world().event_catalog().definitions()["text.control"].immediate;
    assert!(matches!(&immediate[0],Action::Description {key,hidden:false} if key=="first"));
    assert!(matches!(&immediate[1],Action::Description {key,hidden:true} if key=="internal"));
    let saved = loaded.save().unwrap();
    let mut resumed = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    resumed.restore(&saved).unwrap();
    for state in [&mut loaded, &mut resumed] {
        state
            .initial
            .choose_event(&augustus_core::EventChoice {
                pending: augustus_core::PendingEventId(0),
                option: "0".into(),
            })
            .unwrap();
        state.advance(1).unwrap();
        assert_eq!(
            state
                .initial
                .world()
                .events()
                .iter()
                .map(|e| e.2.as_str())
                .collect::<Vec<_>>(),
            ["immediate", "after", "later"]
        );
        assert_eq!(state.initial.world().pending_events().len(), 1);
    }
    assert_eq!(loaded.save().unwrap(), resumed.save().unwrap());
    for value in ["\"\"", "\"[Root.GetName]\""] {
        write(
            temp.path(),
            "common/on_actions/test.txt",
            &format!("on_startup={{log=prefix hidden_effect={{custom_tooltip={value}}}}}"),
        );
        let blocked = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
        assert!(!blocked.report.estates.pending_startup_effects.is_empty());
        assert!(blocked.initial.world().events().is_empty());
    }
}

#[test]
fn startup_event_conditions_read_loaded_country_identity_and_calendar() {
    let temp = startup_fixture();
    write(
        temp.path(),
        "common/on_actions/test.txt",
        r#"on_startup={
      if={limit={and={tag=AAA has_dlc="Women in History"}} log=startup}
      events={calendar.before calendar.after}
    }"#,
    );
    write(
        temp.path(),
        "events/calendar.txt",
        r#"
      country_event={id=calendar.before is_triggered_only=yes
        trigger={has_dlc="Domination" tag=AAA not={is_year=1445}}
        immediate={log=before} option={name=accept country_event={id=calendar.after days=51}}
      }
      country_event={id=calendar.after is_triggered_only=yes
        trigger={has_dlc="Women in History" TAG=AAA is_year=1445}
        immediate={log=after} option={name=accept}
      }
    "#,
    );
    let mut loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(
        loaded.report.estates.pending_startup_effects.is_empty(),
        "{:?}",
        loaded.report.estates.startup_unresolved
    );
    assert_eq!(loaded.initial.world().pending_events().len(), 1);
    assert_eq!(
        loaded
            .initial
            .world()
            .events()
            .iter()
            .map(|e| e.2.as_str())
            .collect::<Vec<_>>(),
        ["before", "startup"]
    );
    loaded
        .initial
        .choose_event(&augustus_core::EventChoice {
            pending: loaded.initial.world().pending_events()[0].id,
            option: "0".into(),
        })
        .unwrap();
    let saved = loaded.save().unwrap();
    let mut resumed = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    resumed.restore(&saved).unwrap();
    loaded.advance(51).unwrap();
    resumed.advance(51).unwrap();
    assert_eq!(loaded.save().unwrap(), resumed.save().unwrap());
    assert_eq!(loaded.initial.world().events().last().unwrap().2, "after");
    assert_eq!(
        loaded.initial.world().date().unwrap(),
        Some(augustus_core::WorldDate {
            year: 1445,
            month: 1,
            day: 1
        })
    );
}

#[test]
fn startup_affiliation_conditions_read_loaded_history_through_nested_scopes_and_restore() {
    let temp = startup_fixture();
    write(
        temp.path(),
        "common/religions/test.txt",
        "test_religions={test_faith={}} remote_group={remote_faith={}}",
    );
    for path in [
        "history/provinces/1 - North.txt",
        "history/provinces/2 - East.txt",
    ] {
        append(temp.path(), path, "religion=remote_faith");
    }
    write(
        temp.path(),
        "common/on_actions/test.txt",
        r#"on_startup={
        events={faith.initial}
        if={limit={tag=AAA religion=test_faith religion_group=test_religions} log=startup}
    }"#,
    );
    write(
        temp.path(),
        "events/test.txt",
        r#"
        country_event={id=faith.initial is_triggered_only=yes
            trigger={tag=AAA religion=test_faith religion_group=test_religions}
            immediate={log=country capital_scope={if={limit={religion=remote_faith religion_group=remote_group NOT={religion_group=test_religions}} log=province}}}
            option={name=Continue country_event={id=faith.later days=1}}}
        country_event={id=faith.later is_triggered_only=yes
            trigger={religion=test_faith religion_group=test_religions}
            immediate={log=later} option={name=Continue}}
    "#,
    );
    let mut loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(
        loaded.report.estates.pending_startup_effects.is_empty(),
        "{:?}",
        loaded.report.estates.startup_unresolved
    );
    assert_eq!(
        loaded
            .initial
            .world()
            .events()
            .iter()
            .map(|event| event.2.as_str())
            .collect::<Vec<_>>(),
        ["country", "province", "startup"]
    );
    let saved = loaded.save().unwrap();
    let mut resumed = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    resumed.restore(&saved).unwrap();
    for state in [&mut loaded, &mut resumed] {
        state
            .initial
            .choose_event(&augustus_core::EventChoice {
                pending: augustus_core::PendingEventId(0),
                option: "0".into(),
            })
            .unwrap();
        state.advance(1).unwrap();
        assert_eq!(state.initial.world().events().last().unwrap().2, "later");
        assert_eq!(state.initial.world().pending_events().len(), 1);
    }
    assert_eq!(loaded.save().unwrap(), resumed.save().unwrap());
}

#[test]
fn startup_once_records_are_global_persistent_and_part_of_content_identity() {
    let temp = startup_fixture();
    write(
        temp.path(),
        "common/on_actions/test.txt",
        "on_startup={ events={meeting.once meeting.once} country_event={id=meeting.once} }",
    );
    let event = "country_event={id=meeting.once is_triggered_only=yes fire_only_once=yes immediate={log=once} option={name=accept country_event={id=meeting.child days=2} country_event={id=meeting.child days=2}}}";
    write(temp.path(), "events/first.txt", event);
    write(
        temp.path(),
        "events/child.txt",
        "country_event={id=meeting.child is_triggered_only=yes fire_only_once=yes immediate={log=child} option={name=accept}}",
    );
    let mut loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(
        loaded.report.estates.pending_startup_effects.is_empty(),
        "{:?}",
        loaded.report.estates.startup_unresolved
    );
    assert_eq!(loaded.initial.world().pending_events().len(), 1);
    assert_eq!(
        loaded.initial.world().fired_events(),
        &["meeting.once".into()].into()
    );
    let first = loaded.initial.world().pending_events()[0].id;
    loaded
        .initial
        .choose_event(&augustus_core::EventChoice {
            pending: first,
            option: "0".into(),
        })
        .unwrap();
    let saved = loaded.save().unwrap();
    let mut restored = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    restored.restore(&saved).unwrap();
    loaded.advance(2).unwrap();
    restored.advance(2).unwrap();
    assert_eq!(loaded.save().unwrap(), restored.save().unwrap());
    assert_eq!(loaded.initial.world().pending_events().len(), 1);
    assert_eq!(
        loaded
            .initial
            .world()
            .events()
            .iter()
            .map(|e| e.2.as_str())
            .collect::<Vec<_>>(),
        ["once", "child"]
    );
    // Changing an unexecuted child's policy must change immutable content identity.
    let child = fs::read_to_string(temp.path().join("events/child.txt")).unwrap();
    write(
        temp.path(),
        "events/child.txt",
        &child.replace("fire_only_once=yes", "fire_only_once=no"),
    );
    assert!(
        VanillaWorld::load(temp.path(), "1444.11.11")
            .unwrap()
            .restore(&saved)
            .is_err()
    );
}

#[test]
fn startup_candidates_run_before_effects_and_restore_the_shared_event_lifecycle() {
    let temp = startup_fixture();
    write(
        temp.path(),
        "common/on_actions/test.txt",
        r#"on_startup = {
        if = { limit = { is_emperor = yes is_emperor_of_china = no }
            log = startup_effect
            country_event = { id = meeting.false }
        }
        events = { meeting.first meeting.false }
    }"#,
    );
    write(
        temp.path(),
        "events/first.txt",
        r#"
        namespace = meeting
        country_event = { id = meeting.first is_triggered_only = yes
            trigger = { is_emperor = yes }
            immediate = { log = candidate_first }
            option = { name = accept log = option_first country_event = { id = meeting.child days = 2 } }
            after = { log = after_first }
        }
        country_event = { id = unrelated unsupported_array = { x y z } }
        country_event = { id = meeting.false is_triggered_only = yes trigger = { always = no }
            immediate = { log = unreachable } option = { name = accept }
        }
    "#,
    );
    write(
        temp.path(),
        "events/child.txt",
        r#"country_event = { id = meeting.child is_triggered_only = yes trigger = { always = yes }
        immediate = { log = child } option = { name = accept log = option_child }
    }"#,
    );
    let mut loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(
        loaded.report.estates.pending_startup_effects.is_empty(),
        "{:?}",
        loaded.report.estates.startup_unresolved
    );
    assert_eq!(
        loaded
            .initial
            .world()
            .events()
            .iter()
            .map(|e| e.2.as_str())
            .collect::<Vec<_>>(),
        ["candidate_first", "startup_effect"]
    );
    assert_eq!(loaded.initial.world().pending_events().len(), 1);
    let choice = augustus_core::EventChoice {
        pending: loaded.initial.world().pending_events()[0].id,
        option: "0".into(),
    };
    let saved = loaded.save().unwrap();
    let mut restored = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    restored.restore(&saved).unwrap();
    for world in [&mut loaded, &mut restored] {
        world.initial.choose_event(&choice).unwrap();
        world.advance(2).unwrap();
    }
    assert_eq!(loaded.save().unwrap(), restored.save().unwrap());
    assert_eq!(
        loaded
            .initial
            .world()
            .events()
            .iter()
            .map(|e| e.2.as_str())
            .collect::<Vec<_>>(),
        [
            "candidate_first",
            "startup_effect",
            "option_first",
            "after_first",
            "child"
        ]
    );
    assert_eq!(
        loaded.initial.world().pending_events()[0].event,
        "meeting.child"
    );
    let altered = fs::read_to_string(temp.path().join("events/child.txt"))
        .unwrap()
        .replace("option_child", "changed_option");
    write(temp.path(), "events/child.txt", &altered);
    assert!(
        VanillaWorld::load(temp.path(), "1444.11.11")
            .unwrap()
            .restore(&saved)
            .is_err()
    );
}

#[test]
fn unresolved_referenced_events_discard_all_startup_state_and_report_the_source() {
    let temp = startup_fixture();
    write(
        temp.path(),
        "common/on_actions/test.txt",
        "on_startup = { emperor = { save_global_event_target_as = prefix } events = { meeting.first } }",
    );
    write(
        temp.path(),
        "events/first.txt",
        "country_event = {\n id = meeting.first is_triggered_only = yes\n trigger = { unsupported_trigger = yes } option = { name = accept }\n}",
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert_eq!(loaded.report.estates.pending_startup_effects.len(), 1);
    let source = &loaded.report.estates.pending_startup_effects[0];
    assert!(source.path.ends_with("events/first.txt"));
    assert_eq!(source.line, 3);
    assert!(loaded.initial.world().global_scopes().is_empty());
    assert!(loaded.initial.world().pending_events().is_empty());
    assert!(
        loaded
            .initial
            .world()
            .event_catalog()
            .definitions()
            .is_empty()
    );
    assert_eq!(loaded.report.estates.applied_startup_effects, 0);
}

#[test]
fn failed_startup_discards_already_dispatched_pending_events_and_catalog() {
    let temp = startup_fixture();
    write(
        temp.path(),
        "common/on_actions/test.txt",
        "on_startup = { events = { meeting.first } unsupported_effect = yes }",
    );
    write(
        temp.path(),
        "events/first.txt",
        r#"country_event = { id = meeting.first is_triggered_only = yes fire_only_once = yes
        trigger = { is_emperor = yes } immediate = { log = first save_global_event_target_as = prefix }
        option = { name = accept }
    }"#,
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert_eq!(loaded.report.estates.pending_startup_effects.len(), 1);
    assert!(loaded.initial.world().pending_events().is_empty());
    assert!(loaded.initial.world().events().is_empty());
    assert!(loaded.initial.world().fired_events().is_empty());
    assert!(loaded.initial.world().global_scopes().is_empty());
    assert!(
        loaded
            .initial
            .world()
            .event_catalog()
            .definitions()
            .is_empty()
    );
    assert_eq!(loaded.report.estates.applied_startup_effects, 0);
    let saved = loaded.save().unwrap();
    let mut restored = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    restored.restore(&saved).unwrap();
    assert_eq!(saved, restored.save().unwrap());
}

#[test]
fn startup_office_and_global_targets_share_core_state_and_restore() {
    let temp = startup_fixture();
    write(
        temp.path(),
        "common/on_actions/test.txt",
        r#"
        on_startup = { if = { limit = { tag = AAA }
            emperor = { save_global_event_target_as = council }
            event_target:council = { add_estate_loyalty = { estate = estate_guild loyalty = 3 } }
            BBB = { save_global_event_target_as = council }
            event_target:council = { add_estate_loyalty = { estate = estate_guild loyalty = 4 } }
            clear_global_event_target = council
            event_target:council = { unreachable_effect = yes }
            save_global_event_target_as = founder
        } }
    "#,
    );
    let mut loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(loaded.report.estates.pending_startup_effects.is_empty());
    assert_eq!(
        loaded.country("AAA").unwrap().estates.unwrap()["estate_guild"].loyalty_milli,
        Some(53_000)
    );
    assert_eq!(
        loaded.country("BBB").unwrap().estates.unwrap()["estate_guild"].loyalty_milli,
        Some(54_000)
    );
    assert_eq!(
        loaded.initial.world().global_scopes()["founder"],
        loaded.initial.country_entity("AAA").unwrap()
    );
    assert!(
        !loaded
            .initial
            .world()
            .global_scopes()
            .contains_key("council")
    );
    let saved = loaded.save().unwrap();
    let mut restored = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    restored.restore(&saved).unwrap();
    loaded.advance(3).unwrap();
    restored.advance(3).unwrap();
    assert_eq!(loaded.save().unwrap(), restored.save().unwrap());
}

#[test]
fn incomplete_startup_discards_global_bindings_with_other_prefix_writes() {
    let temp = startup_fixture();
    write(
        temp.path(),
        "common/on_actions/test.txt",
        "on_startup = { emperor = { save_global_event_target_as = partial } unsupported_effect = yes }",
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert_eq!(loaded.report.estates.pending_startup_effects.len(), 1);
    assert!(loaded.initial.world().global_scopes().is_empty());
    assert_eq!(loaded.report.estates.applied_startup_effects, 0);
}

#[test]
fn startup_macros_scopes_and_live_callback_conditions_match_native_execution() {
    let temp = startup_fixture();
    write(
        temp.path(),
        "common/scripted_effects/test.txt",
        r#"
        apply_charter = { set_estate_privilege = charter add_estate_loyalty = { estate = estate_guild loyalty = 3 } }
        unrelated = { unsupported_effect = { arbitrary = { data = 1 } } }
    "#,
    );
    let startup_script = r#"
        on_startup = { if = { limit = { tag = AAA }
            if = { limit = { always = no } unreachable_effect = yes }
            else_if = { limit = { has_dlc = "Emperor" } hidden_effect = { every_country = { limit = { tag = AAA } apply_charter = yes } } }
            else = { unreachable_effect = yes }
            BBB = { add_estate_loyalty = { estate = estate_guild loyalty = -100 } }
            every_country = { limit = { tag = AAA }
                if = { limit = { has_estate_privilege = charter } add_estate_loyalty = { estate = estate_guild loyalty = 1 } }
            }
        } }
    "#;
    write(temp.path(), "common/on_actions/test.txt", startup_script);
    let mut loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let states = loaded.country("AAA").unwrap().estates.unwrap();
    assert_eq!(states["estate_guild"].loyalty_milli, Some(67_000));
    assert_eq!(states["estate_guild"].land_share_micros, Some(170_000));
    assert_eq!(
        loaded.country("BBB").unwrap().estates.unwrap()["estate_guild"].loyalty_milli,
        Some(0)
    );
    assert_eq!(loaded.report.estates.applied_startup_effects, 6);
    assert!(loaded.report.estates.pending_startup_effects.is_empty());
    let effects = loaded.country("AAA").unwrap().declared_estate_effects;
    assert_eq!(effects.known_totals_micros["global_tax_modifier"], 100_000);
    assert!(effects.unresolved.is_empty());

    write(temp.path(), "common/on_actions/test.txt", "on_startup = {}");
    let mut native = VanillaWorld::load(temp.path(), "1444.11.11")
        .unwrap()
        .initial;
    let alpha = native.country_entity("AAA").unwrap();
    let beta = native.country_entity("BBB").unwrap();
    native
        .execute(
            alpha,
            &[
                loyalty(13_000),
                Action::ChangeEstateMetric {
                    estate: "estate_guild".into(),
                    metric: EstateMetric::LandShare,
                    amount: 20_000,
                    minimum: None,
                },
                Action::SetEstatePrivilege {
                    estate: "estate_guild".into(),
                    privilege: "charter".into(),
                    granted: true,
                },
                Action::ResolveEstatePrivilegeEffect {
                    privilege: "charter".into(),
                    effect: "is_valid".into(),
                },
                Action::ResolveEstatePrivilegeEffect {
                    privilege: "charter".into(),
                    effect: "on_granted".into(),
                },
                loyalty(3_000),
                loyalty(1_000),
            ],
        )
        .unwrap();
    native.execute(beta, &[loyalty(-100_000)]).unwrap();
    assert_eq!(
        loaded.initial.world().save("same").unwrap(),
        native.world().save("same").unwrap()
    );

    write(temp.path(), "common/on_actions/test.txt", startup_script);
    let saved = loaded.save().unwrap();
    let mut restored = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    restored.restore(&saved).unwrap();
    loaded.advance(45).unwrap();
    restored.restore(&loaded.save().unwrap()).unwrap();
    loaded.advance(320).unwrap();
    restored.advance(320).unwrap();
    assert_eq!(loaded.save().unwrap(), restored.save().unwrap());
    assert_eq!(
        loaded.country("AAA").unwrap().estates.unwrap()["estate_guild"].loyalty_milli,
        Some(67_000)
    );
}

#[test]
fn startup_revocation_runs_callback_and_clears_country_resolution() {
    let temp = startup_fixture();
    write(
        temp.path(),
        "common/on_actions/test.txt",
        "on_startup = { if = { limit = { tag = AAA } set_estate_privilege = charter remove_estate_privilege = charter } }",
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let state = &loaded.country("AAA").unwrap().estates.unwrap()["estate_guild"];
    assert_eq!(state.loyalty_milli, Some(52_000));
    assert_eq!(state.land_share_micros, Some(150_000));
    assert!(state.privileges.as_ref().unwrap().is_empty());
    assert!(
        loaded
            .initial
            .world()
            .estates()
            .resolved_privilege_effects
            .is_empty()
    );
    assert_eq!(loaded.report.estates.applied_startup_effects, 6);

    let temp = land_fixture();
    write(
        temp.path(),
        "common/estate_privileges/test.txt",
        "charter = { on_revoked = { add_estate_loyalty = { estate = estate_guild loyalty = 2 } } }",
    );
    append(
        temp.path(),
        "history/countries/AAA - Alpha.txt",
        "remove_estate_privilege = charter",
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(
        loaded.country("AAA").unwrap().estates.unwrap()["estate_guild"]
            .loyalty_milli
            .is_none()
    );
    assert!(
        loaded.report.estates.unresolved_initialization["AAA"]
            .contains("unresolved_privilege_initialization")
    );
}

#[test]
fn incomplete_startup_discards_prefix_and_preserves_callback_source() {
    let temp = startup_fixture();
    write(
        temp.path(),
        "common/scripted_effects/test.txt",
        "bad_callback = { unsupported_effect = yes }",
    );
    write(
        temp.path(),
        "common/estate_privileges/test.txt",
        "charter = { benefits = { global_tax_modifier = 0.1 } on_granted = { bad_callback = yes } }",
    );
    write(
        temp.path(),
        "common/on_actions/test.txt",
        "on_startup = { if = { limit = { tag = AAA } add_estate_loyalty = { estate = estate_guild loyalty = 2 } set_estate_privilege = charter } }",
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert_eq!(loaded.report.estates.applied_startup_effects, 0);
    assert!(loaded.report.estates.startup_effect_sources.is_empty());
    assert!(
        loaded
            .report
            .estates
            .startup_unresolved
            .contains("unsupported_effect:unsupported_effect")
    );
    assert!(
        loaded.report.estates.pending_startup_effects[0]
            .path
            .ends_with("common/scripted_effects/test.txt")
    );
    assert!(
        loaded
            .initial
            .world()
            .estates()
            .resolved_privilege_effects
            .is_empty()
    );
    for state in loaded.country("AAA").unwrap().estates.unwrap().values() {
        assert!(
            state.loyalty_milli.is_none()
                && state.land_share_micros.is_none()
                && state.privileges.is_none()
        );
    }
    assert!(
        loaded
            .country("AAA")
            .unwrap()
            .declared_estate_effects
            .known_totals_micros
            .is_empty()
    );
    assert!(loaded.save().is_ok());
}

#[test]
fn unknown_conditions_repeated_grants_and_numeric_bounds_block_startup() {
    for body in [
        "if = { limit = { unknown_trigger = yes } add_estate_loyalty = { estate = estate_guild loyalty = 3 } }",
        "set_estate_privilege = charter set_estate_privilege = charter",
        "add_estate_loyalty = { estate = estate_guild loyalty = 100 }",
        "change_estate_land_share = { estate = estate_guild share = -100 }",
    ] {
        let temp = startup_fixture();
        write(
            temp.path(),
            "common/on_actions/test.txt",
            &format!("on_startup = {{ if = {{ limit = {{ tag = AAA }} {body} }} }}"),
        );
        let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
        assert_eq!(loaded.report.estates.applied_startup_effects, 0);
        assert_eq!(loaded.report.estates.pending_startup_effects.len(), 1);
        assert!(
            loaded.country("AAA").unwrap().estates.unwrap()["estate_guild"]
                .loyalty_milli
                .is_none()
        );
        assert!(loaded.save().is_ok());
    }
}

#[test]
fn ambiguous_recursive_and_parameterized_effects_are_pending_with_sources() {
    for declaration in [
        "test_effect = {} test_effect = {}",
        "test_effect = { test_effect = yes }",
        "test_effect = { add_estate_loyalty = { estate = estate_guild loyalty = 1 } }",
    ] {
        let temp = startup_fixture();
        write(temp.path(), "common/scripted_effects/test.txt", declaration);
        let call = if declaration.contains("loyalty") {
            "test_effect = { estate = estate_guild }"
        } else {
            "test_effect = yes"
        };
        write(
            temp.path(),
            "common/on_actions/test.txt",
            &format!("on_startup = {{ if = {{ limit = {{ tag = AAA }} {call} }} }}"),
        );
        let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
        assert_eq!(loaded.report.estates.pending_startup_effects.len(), 1);
        assert_eq!(loaded.report.estates.applied_startup_effects, 0);
    }
    for script in [
        "on_startup = { else = {} }",
        "on_startup = { if = { limit = { always = yes } } else = {} else = {} }",
        "on_startup = { if = { limit = { always = yes } } else = {} else_if = { limit = { always = no } } }",
        "on_startup = {} on_startup = {}",
    ] {
        let temp = startup_fixture();
        write(temp.path(), "common/on_actions/test.txt", script);
        assert!(
            VanillaWorld::load(temp.path(), "1444.11.11")
                .unwrap_err()
                .to_string()
                .replace(char::from(92), "/")
                .contains("common/on_actions/test.txt")
        );
    }
}

#[test]
fn startup_uses_each_living_country_as_its_initial_scope() {
    let temp = startup_fixture();
    write(
        temp.path(),
        "common/on_actions/test.txt",
        "on_startup = { add_estate_loyalty = { estate = estate_guild loyalty = 1 } }",
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    for tag in ["AAA", "BBB"] {
        assert_eq!(
            loaded.country(tag).unwrap().estates.unwrap()["estate_guild"].loyalty_milli,
            Some(51_000)
        );
    }
    assert_eq!(loaded.report.estates.applied_startup_effects, 2);
}
fn write(root: &Path, path: &str, text: &str) {
    fs::create_dir_all(root.join(path).parent().unwrap()).unwrap();
    fs::write(root.join(path), text).unwrap();
}
fn append(root: &Path, path: &str, text: &str) {
    write(
        root,
        path,
        &(fs::read_to_string(root.join(path)).unwrap() + "\n" + text),
    );
}
fn copy_fixture() -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    for entry in walkdir::WalkDir::new(fixture()) {
        let entry = entry.unwrap();
        let target = temp
            .path()
            .join(entry.path().strip_prefix(fixture()).unwrap());
        if entry.file_type().is_dir() {
            fs::create_dir_all(target).unwrap();
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
    let path = "common/defines.lua";
    let defines = fs::read_to_string(temp.path().join(path)).unwrap();
    write(
        temp.path(),
        path,
        &defines.replace(
            "NCountry = {",
            "NCountry = {
        ESTATE_ANGRY_THRESHOLD = 30,
        ESTATE_HAPPY_THRESHOLD = 60,",
        ),
    );
    write(
        temp.path(),
        "common/estates/test.txt",
        "guild = { trigger = { tag = AAA } country_modifier_neutral = { global_tax_modifier = 0.2 } privileges = { charter } } inactive = { trigger = { always = no } }",
    );
    write(
        temp.path(),
        "common/estate_privileges/test.txt",
        "charter = { benefits = { global_tax_modifier = 0.125 } penalties = { global_tax_modifier = -0.025 } is_valid = { always = yes } on_granted = {} }",
    );
    append(
        temp.path(),
        "history/countries/AAA - Alpha.txt",
        "set_estate_privilege = charter",
    );
    temp
}

#[test]
fn translated_content_matches_native_core_with_known_loyalty_and_unknown_influence() {
    let temp = copy_fixture();
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let mut native = VanillaWorld::load(fixture(), "1444.11.11")
        .unwrap()
        .initial
        .world()
        .clone();
    let alpha = loaded.initial.country_entity("AAA").unwrap();
    let beta = loaded.initial.country_entity("BBB").unwrap();
    let numeric = |value| EstateEffect {
        modifier: ModifierDefinition {
            values_micros: BTreeMap::from([("global_tax_modifier".into(), value)]),
            ..Default::default()
        },
        ..Default::default()
    };
    native
        .set_estates(Estates {
            definitions: BTreeMap::from([
                (
                    "guild".into(),
                    EstateDefinition {
                        neutral: numeric(200_000),
                        privileges: ["charter".into()].into(),
                        ..Default::default()
                    },
                ),
                ("inactive".into(), EstateDefinition::default()),
            ]),
            privileges: BTreeMap::from([(
                "charter".into(),
                EstatePrivilege {
                    effect: numeric(100_000),
                    ..Default::default()
                },
            )]),
            rules: EstateRules {
                angry_below_milli: Some(30_000),
                happy_from_milli: Some(60_000),
                influence_bands: None,
                loyalty_target_base_milli: None,
                loyalty_decay: None,
            },
            countries: BTreeMap::from([
                (
                    alpha,
                    BTreeMap::from([
                        (
                            "guild".into(),
                            EstateState {
                                active: Some(true),
                                loyalty_milli: Some(50_000),
                                privileges: Some(["charter".into()].into()),
                                ..Default::default()
                            },
                        ),
                        (
                            "inactive".into(),
                            EstateState {
                                active: Some(false),
                                privileges: Some(Default::default()),
                                ..Default::default()
                            },
                        ),
                    ]),
                ),
                (
                    beta,
                    BTreeMap::from([
                        (
                            "guild".into(),
                            EstateState {
                                active: Some(false),
                                privileges: Some(Default::default()),
                                ..Default::default()
                            },
                        ),
                        (
                            "inactive".into(),
                            EstateState {
                                active: Some(false),
                                privileges: Some(Default::default()),
                                ..Default::default()
                            },
                        ),
                    ]),
                ),
            ]),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(
        native.save("test-estates").unwrap(),
        loaded.initial.world().save("test-estates").unwrap()
    );
    let country = loaded.country("AAA").unwrap();
    assert_eq!(
        country.declared_estate_effects.known_totals_micros["global_tax_modifier"],
        100_000
    );
    assert!(
        country
            .declared_estate_effects
            .unresolved
            .contains("estates:guild:neutral:unknown_scale")
    );
    assert!(
        country.province_attribute_totals["tax_basis"]
            .unresolved
            .iter()
            .any(|r| r.starts_with("estates:guild:"))
    );
    assert_eq!(loaded.report.estates.historical_privileges, 1);
}

#[test]
fn influence_initialization_uses_defines_and_selected_privileges() {
    let temp = copy_fixture();
    write(
        temp.path(),
        "common/estates/test.txt",
        "guild = { trigger = { tag = AAA } base_influence = 10 country_modifier_neutral = { global_tax_modifier = 0.2 } privileges = { charter } } inactive = { trigger = { always = no } }",
    );
    write(
        temp.path(),
        "common/estate_privileges/test.txt",
        "charter = { influence = 0.05 benefits = { global_tax_modifier = 0.1 } is_valid = { always = yes } on_granted = {} }",
    );
    let path = "common/defines.lua";
    let defines = fs::read_to_string(temp.path().join(path)).unwrap();
    write(
        temp.path(),
        path,
        &defines.replace(
            "ESTATE_HAPPY_THRESHOLD = 60,",
            "ESTATE_HAPPY_THRESHOLD = 60,\n        ESTATE_INFLUENCE_LEVEL_1 = 20,\n        ESTATE_INFLUENCE_LEVEL_2 = 40,\n        ESTATE_INFLUENCE_LEVEL_3 = 60,",
        ),
    );

    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let state = &loaded.country("AAA").unwrap().estates.unwrap()["guild"];
    assert_eq!(state.influence_milli, Some(15_000));
    assert_eq!(
        loaded.initial.world().estates().privileges["charter"].influence_milli,
        Some(5_000)
    );
    assert_eq!(
        loaded.initial.world().estates().rules.influence_bands,
        Some(
            [
                (0, 0),
                (1, 250_000),
                (20_000, 500_000),
                (40_000, 750_000),
                (60_000, 1_000_000),
            ]
            .into_iter()
            .map(
                |(minimum_milli, scale_micros)| augustus_core::EstateInfluenceBand {
                    minimum_milli,
                    scale_micros,
                }
            )
            .collect()
        )
    );
    assert!(
        loaded
            .report
            .estates
            .definition_sources
            .contains_key("privilege:charter:influence")
    );
    assert!(
        loaded
            .country("AAA")
            .unwrap()
            .declared_estate_effects
            .unresolved
            .is_empty()
    );
}

#[test]
fn influence_initialization_resolves_owned_province_and_capital_triggers() {
    let temp = copy_fixture();
    write(
        temp.path(),
        "common/estates/test.txt",
        r#"
        guild = {
            trigger = { tag = AAA }
            base_influence = 10
            privileges = { charter }
            influence_modifier = {
                trigger = { any_owned_province = { religion = test_faith } }
                influence = 3
            }
            influence_modifier = { trigger = { capital = 1 } influence = 5 }
            influence_modifier = { trigger = { capital = 2 } influence = 7 }
        }
        "#,
    );
    write(
        temp.path(),
        "common/estate_privileges/test.txt",
        "charter = { influence = 0 is_valid = { always = yes } on_granted = {} }",
    );
    let state = VanillaExternalState {
        country_capitals: BTreeMap::from([("AAA".into(), Some(1))]),
        province_owners: BTreeMap::from([(1, Some("AAA".into()))]),
        ..Default::default()
    };

    let loaded = VanillaWorld::load_with_external_state(temp.path(), "1444.11.11", &state).unwrap();
    assert_eq!(
        loaded.country("AAA").unwrap().estates.unwrap()["guild"].influence_milli,
        Some(18_000),
        "{:?}",
        loaded.report.estates.unresolved_initialization.get("AAA")
    );
    assert!(
        !loaded
            .report
            .estates
            .unsupported_fields
            .contains_key("trigger:any_owned_province")
    );
    assert!(
        !loaded
            .report
            .estates
            .unsupported_fields
            .contains_key("trigger:capital")
    );

    let unknown_capital = VanillaExternalState {
        country_capitals: BTreeMap::from([("AAA".into(), None)]),
        ..state
    };
    let loaded =
        VanillaWorld::load_with_external_state(temp.path(), "1444.11.11", &unknown_capital)
            .unwrap();
    assert_eq!(
        loaded.country("AAA").unwrap().estates.unwrap()["guild"].influence_milli,
        None
    );
    assert!(
        loaded.report.estates.unresolved_initialization["AAA"]
            .iter()
            .any(|reason| reason.contains("guild:influence:unknown_capital"))
    );
}

#[test]
fn saved_active_influences_select_declaration_indices_without_rechecking_triggers() {
    let temp = copy_fixture();
    write(
        temp.path(),
        "common/estates/test.txt",
        r#"
        estate_guild = {
            trigger = { tag = AAA }
            base_influence = 10
            privileges = { charter }
            influence_modifier = {
                desc = unresolved_trigger
                trigger = { unknown_condition = yes }
                influence = 2
            }
            influence_modifier = {
                desc = selected_modifier
                trigger = { always = yes }
                influence = 4
            }
        }
        "#,
    );
    let state = |active_influences| VanillaExternalState {
        estate_privileges: BTreeMap::from([(
            "AAA".into(),
            BTreeMap::from([(
                "estate_guild".into(),
                Some(BTreeSet::from(["charter".into()])),
            )]),
        )]),
        estate_states: BTreeMap::from([(
            "AAA".into(),
            BTreeMap::from([(
                "estate_guild".into(),
                VanillaExternalEstateState {
                    loyalty_milli: Some(50_000),
                    land_share_micros: None,
                    active_influences: Some(active_influences),
                },
            )]),
        )]),
        ..Default::default()
    };

    let selected = VanillaWorld::load_with_external_state(
        temp.path(),
        "1444.11.11",
        &state(BTreeSet::from([1])),
    )
    .unwrap();
    assert_eq!(
        selected.country("AAA").unwrap().estates.unwrap()["estate_guild"].influence_milli,
        Some(14_000)
    );
    assert!(
        !selected
            .report
            .estates
            .unresolved_initialization
            .get("AAA")
            .is_some_and(|reasons| reasons
                .iter()
                .any(|reason| reason.contains("unknown_condition")))
    );

    let empty =
        VanillaWorld::load_with_external_state(temp.path(), "1444.11.11", &state(BTreeSet::new()))
            .unwrap();
    assert_eq!(
        empty.country("AAA").unwrap().estates.unwrap()["estate_guild"].influence_milli,
        Some(10_000)
    );
}

#[test]
fn saved_active_influence_out_of_range_preserves_unknown_initialization() {
    let temp = copy_fixture();
    write(
        temp.path(),
        "common/estates/test.txt",
        "estate_guild = { trigger = { tag = AAA } base_influence = 10 privileges = { charter } influence_modifier = { trigger = { always = yes } influence = 2 } }",
    );
    let state = VanillaExternalState {
        estate_privileges: BTreeMap::from([(
            "AAA".into(),
            BTreeMap::from([(
                "estate_guild".into(),
                Some(BTreeSet::from(["charter".into()])),
            )]),
        )]),
        estate_states: BTreeMap::from([(
            "AAA".into(),
            BTreeMap::from([(
                "estate_guild".into(),
                VanillaExternalEstateState {
                    loyalty_milli: Some(50_000),
                    land_share_micros: None,
                    active_influences: Some(BTreeSet::from([4])),
                },
            )]),
        )]),
        ..Default::default()
    };
    let loaded = VanillaWorld::load_with_external_state(temp.path(), "1444.11.11", &state).unwrap();
    assert_eq!(
        loaded.country("AAA").unwrap().estates.unwrap()["estate_guild"].influence_milli,
        None
    );
    assert!(
        loaded.report.estates.unresolved_initialization["AAA"]
            .iter()
            .any(|reason| reason.contains("unknown_modifier:4"))
    );
}

#[test]
fn dependency_conditions_macros_and_unknowns_select_without_order_guesses() {
    let temp = copy_fixture();
    write(
        temp.path(),
        "common/scripted_triggers/test.txt",
        "has_enabled_estate = { has_country_flag = allow_$estate$ } has_disabled_estate = { has_country_flag = deny_$estate$ }",
    );
    append(
        temp.path(),
        "history/countries/AAA - Alpha.txt",
        "set_country_flag = allow_favored",
    );
    append(
        temp.path(),
        "common/estates/test.txt",
        r#"
        dependent = { trigger = { has_estate = later } }
        later = { trigger = { has_dlc = "Emperor" NOT = { is_nomad = yes } } }
        conditional = { trigger = { if = { limit = { tag = BBB } always = no } } }
        favored = { trigger = { has_enabled_estate = { estate = favored } NOT = { has_disabled_estate = { estate = favored } } } }
        unbound = { trigger = { has_enabled_estate = yes } }
        missing = { trigger = { has_estate = absent } }
        cycle_a = { trigger = { has_estate = cycle_b } }
        cycle_b = { trigger = { has_estate = cycle_a } }
        false_known = { trigger = { always = no unsupported_trigger = yes } }
        true_known = { trigger = { OR = { always = yes unsupported_trigger = yes } } }
        unknown = { trigger = { unsupported_trigger = yes } }
    "#,
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let country = loaded.country("AAA").unwrap();
    let states = country.estates.unwrap();
    for id in ["dependent", "later", "conditional", "favored", "true_known"] {
        assert_eq!(states[id].active, Some(true), "{id}");
    }
    assert_eq!(states["false_known"].active, Some(false));
    for id in ["unbound", "missing", "cycle_a", "cycle_b", "unknown"] {
        assert_eq!(states[id].active, None, "{id}");
    }
    assert_eq!(
        loaded.country("BBB").unwrap().estates.unwrap()["conditional"].active,
        Some(false)
    );
    assert!(
        loaded
            .report
            .estates
            .definition_sources
            .contains_key("condition:has_enabled_estate")
    );
    append(
        temp.path(),
        "history/countries/AAA - Alpha.txt",
        "clr_country_flag = allow_favored",
    );
    assert_eq!(
        VanillaWorld::load(temp.path(), "1444.11.11")
            .unwrap()
            .country("AAA")
            .unwrap()
            .estates
            .unwrap()["favored"]
            .active,
        Some(false)
    );
}

#[test]
fn history_dates_scopes_and_land_deltas_do_not_invent_final_state() {
    let temp = copy_fixture();
    append(
        temp.path(),
        "history/countries/AAA - Alpha.txt",
        "1450.1.1 = { set_estate_privilege = charter } 1440.1.1 = { remove_estate_privilege = charter change_estate_land_share = { estate = guild share = 5 } }",
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let country = loaded.country("AAA").unwrap();
    let state = &country.estates.unwrap()["guild"];
    assert!(state.privileges.as_ref().unwrap().is_empty());
    assert_eq!(
        (
            state.loyalty_milli,
            state.influence_milli,
            state.land_share_micros
        ),
        (Some(50_000), None, None)
    );
    assert_eq!(loaded.report.estates.pending_land_changes["AAA"].len(), 1);
    assert_eq!(
        VanillaWorld::load(temp.path(), "1450.1.1")
            .unwrap()
            .country("AAA")
            .unwrap()
            .estates
            .unwrap()["guild"]
            .privileges
            .as_ref()
            .unwrap()
            .len(),
        1
    );
    append(
        temp.path(),
        "history/countries/AAA - Alpha.txt",
        "if = { limit = { unsupported_trigger = yes } set_estate_privilege = charter }",
    );
    assert!(
        VanillaWorld::load(temp.path(), "1444.11.11")
            .unwrap()
            .country("AAA")
            .unwrap()
            .estates
            .unwrap()["guild"]
            .privileges
            .is_none()
    );
}

#[test]
fn direct_privilege_history_is_reported_as_handled_but_scoped_changes_remain_unknown() {
    let temp = copy_fixture();
    append(
        temp.path(),
        "history/countries/AAA - Alpha.txt",
        "remove_estate_privilege = charter",
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(
        !loaded
            .report
            .unapplied_country_fields
            .contains_key("set_estate_privilege")
    );
    assert!(
        !loaded
            .report
            .unapplied_country_fields
            .contains_key("remove_estate_privilege")
    );
    assert!(
        loaded.country("AAA").unwrap().estates.unwrap()["guild"]
            .privileges
            .as_ref()
            .unwrap()
            .is_empty()
    );

    append(
        temp.path(),
        "history/countries/AAA - Alpha.txt",
        "if = { limit = { unsupported_trigger = yes } set_estate_privilege = charter }",
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(loaded.report.unapplied_country_fields.contains_key("if"));
    assert!(
        loaded.country("AAA").unwrap().estates.unwrap()["guild"]
            .privileges
            .is_none()
    );
}

#[test]
fn privilege_conditional_modifiers_compile_and_use_live_estate_state() {
    let temp = land_fixture();
    write(
        temp.path(),
        "common/estate_privileges/test.txt",
        r#"
        charter = {
            benefits = { global_tax_modifier = 0.1 }
            conditional_modifier = {
                trigger = { has_estate_privilege = charter }
                modifier = { global_tax_modifier = 0.2 }
            }
            loyalty_scaled_conditional_modifier = {
                trigger = { estate_loyalty = { estate = estate_guild loyalty = 40 } }
                modifier = { global_tax_modifier = 0.4 }
            }
            influence_scaled_conditional_modifier = {
                trigger = { estate_loyalty = { estate = estate_guild loyalty = 30 } }
                modifier = { global_tax_modifier = 0.6 }
            }
        }
    "#,
    );

    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let mut world = loaded.initial.world().clone();
    let mut estates = world.estates().clone();
    estates
        .countries
        .get_mut(&loaded.initial.country_entity("AAA").unwrap())
        .unwrap()
        .get_mut("estate_guild")
        .unwrap()
        .influence_milli = Some(40_000);
    world.set_estates(estates).unwrap();
    let entity = loaded.initial.country_entity("AAA").unwrap();
    let conditional_modifiers = &world.estates().privileges["charter"]
        .effect
        .conditional_modifiers;
    assert_eq!(
        conditional_modifiers.len(),
        3,
        "unsupported estate fields: {:?}",
        loaded.report.estates.unsupported_fields
    );
    assert_eq!(
        conditional_modifiers
            .iter()
            .map(|modifier| modifier.scale)
            .collect::<Vec<_>>(),
        [
            augustus_core::EstateConditionalScale::Fixed,
            augustus_core::EstateConditionalScale::Loyalty,
            augustus_core::EstateConditionalScale::Influence,
        ]
    );

    let evaluation = world
        .estates()
        .property_effects_with_world(&world, entity, "global_tax_modifier")
        .unwrap();
    assert_eq!(
        evaluation
            .contributions
            .iter()
            .filter(|contribution| contribution.source.contains(":conditional:"))
            .map(|contribution| contribution.micros)
            .collect::<Vec<_>>(),
        [200_000, 200_000, 240_000]
    );
    assert!(evaluation.unresolved.is_empty());
}

#[test]
fn country_views_evaluate_conditional_estate_modifiers_after_changes_and_restore() {
    let temp = land_fixture();
    write(
        temp.path(),
        "common/estate_privileges/test.txt",
        r#"
        charter = {
            benefits = { global_tax_modifier = 0.1 }
            conditional_modifier = {
                trigger = { has_country_flag = enhanced_charter }
                modifier = { global_tax_modifier = 0.2 }
            }
            loyalty_scaled_conditional_modifier = {
                trigger = { always = yes }
                modifier = { global_tax_modifier = 0.4 }
            }
        }
        "#,
    );
    let mut loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let entity = loaded.initial.country_entity("AAA").unwrap();
    let verify = |loaded: &VanillaWorld, expected| {
        let displayed = loaded.country("AAA").unwrap().declared_estate_effects;
        let world = loaded.initial.world();
        let calculated = world
            .estates()
            .declared_effects_with_world(world, entity)
            .unwrap();
        assert_eq!(displayed, calculated);
        assert_eq!(
            displayed.known_totals_micros["global_tax_modifier"],
            expected
        );
        assert!(
            displayed.unresolved.is_empty(),
            "{:?}",
            displayed.unresolved
        );
    };
    verify(&loaded, 300_000);
    loaded
        .initial
        .execute(
            entity,
            &[
                Action::SetMarker {
                    target: augustus_core::Target::Current,
                    name: "enhanced_charter".into(),
                    present: true,
                },
                Action::ChangeEstateMetric {
                    estate: "estate_guild".into(),
                    metric: EstateMetric::Loyalty,
                    amount: 10_000,
                    minimum: None,
                },
            ],
        )
        .unwrap();
    verify(&loaded, 540_000);
    let saved = loaded.save().unwrap();
    let mut restored = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    restored.restore(&saved).unwrap();
    verify(&restored, 540_000);

    restored
        .initial
        .execute(
            entity,
            &[Action::SetMarker {
                target: augustus_core::Target::Current,
                name: "enhanced_charter".into(),
                present: false,
            }],
        )
        .unwrap();
    verify(&restored, 340_000);
}

#[test]
fn conditional_privilege_missions_share_registered_content_and_live_completion_state() {
    let temp = land_fixture();
    write(
        temp.path(),
        "missions/test.txt",
        "sample_series={slot=1 generic=no potential={tag=AAA} sample_first={trigger={always=yes} effect={}} sample_later={position=2 trigger={always=yes} effect={}}}",
    );
    let privilege = r#"
        charter = {
            benefits = { global_tax_modifier = 0.1 }
            conditional_modifier = {
                trigger = { mission_completed = sample_first }
                modifier = { global_tax_modifier = 0.2 }
            }
            conditional_modifier = {
                trigger = { has_mission = sample_first }
                modifier = { global_tax_modifier = 0.05 }
            }
        }
    "#;
    write(temp.path(), "common/estate_privileges/test.txt", privilege);
    let mut loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let country = loaded.initial.country_entity("AAA").unwrap();
    assert!(
        loaded
            .report
            .estates
            .unsupported_conditional_triggers
            .is_empty()
    );
    assert_eq!(
        loaded.initial.world().estates().privileges["charter"]
            .effect
            .conditional_modifiers
            .len(),
        2
    );
    assert_eq!(
        loaded
            .country("AAA")
            .unwrap()
            .declared_estate_effects
            .known_totals_micros["global_tax_modifier"],
        150_000
    );
    loaded
        .initial
        .execute(
            country,
            &[Action::RecordCompletion {
                id: "sample_first".into(),
            }],
        )
        .unwrap();
    assert_eq!(
        loaded
            .country("AAA")
            .unwrap()
            .declared_estate_effects
            .known_totals_micros["global_tax_modifier"],
        350_000
    );
    let saved = loaded.save().unwrap();
    let mut restored = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    restored.restore(&saved).unwrap();
    assert_eq!(
        restored.country("AAA").unwrap().declared_estate_effects,
        loaded.country("AAA").unwrap().declared_estate_effects
    );

    let mut completions = loaded.initial.world().completions().clone();
    completions.entities.remove(&country);
    loaded.initial.set_completions(completions).unwrap();
    let unknown = loaded.country("AAA").unwrap().declared_estate_effects;
    assert!(
        unknown
            .unresolved
            .iter()
            .any(|reason| reason.contains("completion state is unknown"))
    );
    assert!(
        !unknown
            .known_totals_micros
            .contains_key("global_tax_modifier")
    );

    write(
        temp.path(),
        "common/estate_privileges/test.txt",
        &privilege.replace(
            "mission_completed = sample_first",
            "mission_completed = sample_later",
        ),
    );
    let mut changed = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(changed.restore(&saved).is_err());
}

#[test]
fn country_view_reports_conditional_estate_overflow_instead_of_panicking() {
    let temp = land_fixture();
    write(
        temp.path(),
        "common/estate_privileges/test.txt",
        r#"
        charter = {
            benefits = { global_tax_modifier = 0.1 }
            conditional_modifier = {
                trigger = { has_country_flag = enhanced_charter }
                modifier = { global_tax_modifier = 9223372036854.775807 }
            }
        }
        "#,
    );
    let mut loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert_eq!(
        loaded
            .country("AAA")
            .unwrap()
            .declared_estate_effects
            .known_totals_micros["global_tax_modifier"],
        100_000
    );
    let country = loaded.initial.country_entity("AAA").unwrap();
    loaded
        .initial
        .execute(
            country,
            &[Action::SetMarker {
                target: augustus_core::Target::Current,
                name: "enhanced_charter".into(),
                present: true,
            }],
        )
        .unwrap();
    let world = loaded.initial.world();
    assert!(matches!(
        world.estates().declared_effects_with_world(world, country),
        Err(augustus_core::Error::Overflow)
    ));
    let view = loaded.country("AAA").unwrap();
    let displayed = view.declared_estate_effects;
    assert!(displayed.known_totals_micros.is_empty());
    assert!(
        displayed
            .unresolved
            .iter()
            .any(|reason| reason.starts_with("estates:evaluation_failed:"))
    );
    let tax = &view.province_attribute_totals["tax_after_autonomy"];
    assert_eq!(tax.value_micros, None);
    assert!(
        tax.unresolved
            .iter()
            .any(|reason| reason.starts_with("attribute_aggregate:evaluation_failed:"))
    );
}

#[test]
fn conditional_privilege_branches_use_first_match_and_restore_live_state() {
    let temp = land_fixture();
    write(
        temp.path(),
        "common/estate_privileges/test.txt",
        r#"
        charter={benefits={global_tax_modifier=0.1}
            conditional_modifier={
                trigger={if={limit={has_country_flag=gate} always=no}
                    else_if={limit={tag=AAA} always=yes} else={always=no}}
                modifier={global_tax_modifier=0.2}
            }
        }
    "#,
    );
    let mut loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(
        loaded
            .report
            .estates
            .unsupported_conditional_triggers
            .is_empty()
    );
    assert_eq!(
        loaded
            .country("AAA")
            .unwrap()
            .declared_estate_effects
            .known_totals_micros["global_tax_modifier"],
        300_000
    );
    let entity = loaded.initial.country_entity("AAA").unwrap();
    loaded
        .initial
        .execute(
            entity,
            &[Action::SetMarker {
                target: augustus_core::Target::Current,
                name: "gate".into(),
                present: true,
            }],
        )
        .unwrap();
    assert_eq!(
        loaded
            .country("AAA")
            .unwrap()
            .declared_estate_effects
            .known_totals_micros["global_tax_modifier"],
        100_000
    );
    let saved = loaded.save().unwrap();
    let mut restored = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    restored.restore(&saved).unwrap();
    assert_eq!(
        restored
            .country("AAA")
            .unwrap()
            .declared_estate_effects
            .known_totals_micros["global_tax_modifier"],
        100_000
    );
    restored
        .initial
        .execute(
            entity,
            &[Action::SetMarker {
                target: augustus_core::Target::Current,
                name: "gate".into(),
                present: false,
            }],
        )
        .unwrap();
    assert_eq!(
        restored
            .country("AAA")
            .unwrap()
            .declared_estate_effects
            .known_totals_micros["global_tax_modifier"],
        300_000
    );
}

#[test]
fn conditional_privileges_bind_registered_metrics_and_read_live_scoped_values() {
    let temp = land_fixture();
    write(
        temp.path(),
        "common/estate_privileges/test.txt",
        r#"
        charter = {
            benefits = { global_tax_modifier = 0.1 }
            conditional_modifier = {
                trigger = { legitimacy = 75.125 }
                modifier = { global_tax_modifier = 0.2 }
            }
            conditional_modifier = {
                trigger = { adm_power = 5 }
                modifier = { global_tax_modifier = 0.3 }
            }
            conditional_modifier = {
                trigger = { capital_scope = { owner = { legitimacy = 75.125 } } }
                modifier = { global_manpower_modifier = 0.4 }
            }
        }
        "#,
    );
    let mut loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(
        loaded
            .report
            .estates
            .unsupported_conditional_triggers
            .is_empty()
    );
    assert_eq!(
        loaded.initial.world().estates().privileges["charter"]
            .effect
            .conditional_modifiers
            .len(),
        3
    );
    let entity = loaded.initial.country_entity("AAA").unwrap();
    let verify = |loaded: &VanillaWorld, tax, manpower| {
        let effects = loaded.country("AAA").unwrap().declared_estate_effects;
        assert!(effects.unresolved.is_empty(), "{:?}", effects.unresolved);
        for (property, expected) in [
            ("global_tax_modifier", tax),
            ("global_manpower_modifier", manpower),
        ] {
            assert_eq!(
                effects
                    .known_totals_micros
                    .get(property)
                    .copied()
                    .unwrap_or(0),
                expected
            );
        }
    };
    assert!(
        loaded
            .country("AAA")
            .unwrap()
            .declared_estate_effects
            .unresolved
            .iter()
            .any(|reason| reason.contains("adm_power"))
    );
    let mut metrics = loaded.initial.world().metrics().clone();
    let maximum = metrics.definitions["adm_power"].maximum;
    metrics
        .ceilings
        .entry(entity)
        .or_default()
        .insert("adm_power".into(), Some(maximum));
    metrics
        .entities
        .entry(entity)
        .or_default()
        .insert("adm_power".into(), Some(0));
    loaded.initial.set_metrics(metrics).unwrap();
    verify(&loaded, 300_000, 400_000);
    loaded
        .initial
        .execute(
            entity,
            &[
                Action::UpdateNumeric {
                    target: augustus_core::Target::Current,
                    slot: augustus_core::NumericSlot::Government("legitimacy".into()),
                    operation: augustus_core::NumericOperation::Set,
                    value: 75_124,
                },
                Action::UpdateNumeric {
                    target: augustus_core::Target::Current,
                    slot: augustus_core::NumericSlot::Metric("adm_power".into()),
                    operation: augustus_core::NumericOperation::Set,
                    value: 5,
                },
            ],
        )
        .unwrap();
    verify(&loaded, 400_000, 0);
    let snapshot = loaded.save().unwrap();
    let mut restored = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    restored.restore(&snapshot).unwrap();
    verify(&restored, 400_000, 0);

    let mut governments = loaded.initial.world().governments().clone();
    governments
        .countries
        .get_mut(&entity)
        .unwrap()
        .metrics
        .as_mut()
        .unwrap()
        .insert("legitimacy".into(), None);
    loaded.initial.set_governments(governments).unwrap();
    let effects = loaded.country("AAA").unwrap().declared_estate_effects;
    assert!(
        !effects
            .known_totals_micros
            .contains_key("global_tax_modifier")
    );
    assert!(
        !effects
            .known_totals_micros
            .contains_key("global_manpower_modifier")
    );
    assert!(
        effects
            .unresolved
            .iter()
            .any(|reason| reason.contains("legitimacy"))
    );
}

#[test]
fn conditional_trigger_structure_cannot_be_hidden_by_unsupported_syntax() {
    for (body, expected) in [
        (
            "trigger={unknown_trigger=yes} trigger={always=yes} modifier={global_tax_modifier=0.2}",
            "duplicate conditional modifier trigger",
        ),
        (
            "trigger={always>0} trigger={always=yes} modifier={global_tax_modifier=0.2}",
            "duplicate conditional modifier trigger",
        ),
        (
            "unknown_field=yes modifier={global_tax_modifier=0.2}",
            "conditional modifier is missing its trigger block",
        ),
    ] {
        let temp = land_fixture();
        write(
            temp.path(),
            "common/estate_privileges/test.txt",
            &format!("charter={{conditional_modifier={{{body}}}}}"),
        );
        let error = VanillaWorld::load(temp.path(), "1444.11.11")
            .unwrap_err()
            .to_string();
        assert!(error.contains(expected), "{error}");
        assert!(error.contains("estate_privileges"), "{error}");
    }
}

#[test]
fn mission_selection_uses_registered_treasury_and_refreshes_after_resource_changes() {
    let temp = land_fixture();
    write(
        temp.path(),
        "missions/test.txt",
        "funded_series={slot=3 potential={treasury=5} funded_task={}} default_series={slot=3 generic=yes potential={always=yes} default_task={}} ",
    );
    let mut loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let entity = loaded.initial.country_entity("AAA").unwrap();
    let mut conditions = serde_json::to_string(loaded.initial.world().objectives()).unwrap();
    assert!(!conditions.contains("unregistered"), "{conditions}");
    assert!(
        loaded
            .initial
            .world()
            .objective_available(entity, "funded_task")
            .is_err()
    );
    for (treasury, funded) in [(5_000, true), (4_999, false)] {
        loaded
            .initial
            .execute(
                entity,
                &[
                    Action::UpdateNumeric {
                        target: augustus_core::Target::Current,
                        slot: augustus_core::NumericSlot::Resource(
                            augustus_core::CountryResource::Treasury,
                        ),
                        operation: augustus_core::NumericOperation::Set,
                        value: treasury,
                    },
                    Action::RefreshObjectives,
                ],
            )
            .unwrap();
        assert_eq!(
            loaded
                .initial
                .world()
                .objective_available(entity, "funded_task")
                .unwrap(),
            funded
        );
        assert_eq!(
            loaded
                .initial
                .world()
                .objective_available(entity, "default_task")
                .unwrap(),
            !funded
        );
    }
    let snapshot = loaded.save().unwrap();
    let mut restored = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    restored.restore(&snapshot).unwrap();
    conditions = serde_json::to_string(restored.initial.world().objectives()).unwrap();
    assert!(!conditions.contains("unregistered"), "{conditions}");
    assert!(
        !restored
            .initial
            .world()
            .objective_available(entity, "funded_task")
            .unwrap()
    );
    assert!(
        restored
            .initial
            .world()
            .objective_available(entity, "default_task")
            .unwrap()
    );
}

#[test]
fn unsupported_conditional_privilege_triggers_remain_unresolved() {
    let temp = land_fixture();
    write(
        temp.path(),
        "common/estate_privileges/test.txt",
        r#"
        charter = {
            conditional_modifier = {
                trigger = { unknown_trigger = yes }
                modifier = { global_tax_modifier = 0.2 }
            }
        }
    "#,
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(
        loaded
            .report
            .estates
            .unsupported_fields
            .contains_key("privilege:conditional_modifier_trigger")
    );
    assert!(
        loaded
            .report
            .estates
            .unsupported_conditional_triggers
            .contains_key("unsupported trigger unknown_trigger")
    );
    assert!(
        loaded.initial.world().estates().privileges["charter"]
            .effect
            .conditional_modifiers
            .is_empty()
    );

    let world = loaded.initial.world();
    let entity = loaded.initial.country_entity("AAA").unwrap();
    let evaluation = world
        .estates()
        .property_effects_with_world(world, entity, "global_tax_modifier")
        .unwrap();
    assert!(
        evaluation
            .unresolved
            .iter()
            .any(|reason| reason.contains("global_tax_modifier"))
    );
}

#[test]
fn conditional_properties_and_unexecuted_callbacks_preserve_missing_sources() {
    let temp = copy_fixture();
    write(
        temp.path(),
        "common/estate_privileges/test.txt",
        "charter = { conditional_modifier = { trigger = { tag = AAA } modifier = { global_tax_modifier = 0.9 } } benefits = { global_tax_modifier = 0.1 global_manpower_modifier = 0.2 } }",
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let alpha = loaded.initial.country_entity("AAA").unwrap();
    let effects = loaded
        .initial
        .world()
        .estates()
        .property_effects(alpha, "global_manpower_modifier")
        .unwrap();
    assert_eq!(
        effects.known_totals_micros["global_manpower_modifier"],
        200_000
    );
    assert!(effects.unresolved.is_empty());
    let tax = loaded
        .initial
        .world()
        .estates()
        .property_effects(alpha, "global_tax_modifier")
        .unwrap();
    assert!(!tax.known_totals_micros.contains_key("global_tax_modifier"));
    assert!(
        tax.unresolved
            .iter()
            .any(|r| r.contains("global_tax_modifier"))
    );
    write(
        temp.path(),
        "common/estate_privileges/test.txt",
        "charter = { benefits = { global_tax_modifier = 0.1 global_tax_modifier = 0.2 global_manpower_modifier = 0.2 } on_granted = { add_country_modifier = { name = not_loaded duration = -1 } } is_valid = { tag = AAA } }",
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let effects = &loaded.country("AAA").unwrap().declared_estate_effects;
    assert_eq!(
        effects.known_totals_micros["global_manpower_modifier"],
        200_000
    );
    assert!(
        !effects
            .known_totals_micros
            .contains_key("global_tax_modifier")
    );
    assert!(
        effects
            .unresolved
            .iter()
            .any(|r| r.ends_with(":on_granted"))
    );
    assert!(effects.unresolved.iter().any(|r| r.ends_with(":is_valid")));
    assert_eq!(loaded.report.estates.ambiguous_properties.len(), 1);
}

#[test]
fn ambiguous_or_unbound_grants_are_unknown_and_never_counted_twice() {
    let temp = copy_fixture();
    append(
        temp.path(),
        "common/estates/test.txt",
        "other = { trigger = { always = yes } privileges = { charter } }",
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let country = loaded.country("AAA").unwrap();
    for id in ["guild", "other"] {
        assert!(country.estates.unwrap()[id].privileges.is_none());
    }
    assert_eq!(loaded.report.estates.historical_privileges, 0);
    assert!(
        !country
            .declared_estate_effects
            .known_totals_micros
            .contains_key("global_tax_modifier")
    );
    assert!(
        country
            .declared_estate_effects
            .unresolved
            .iter()
            .any(|r| r.contains("unresolved_privilege_binding"))
    );
    append(
        temp.path(),
        "history/countries/AAA - Alpha.txt",
        "set_estate_privilege = absent",
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(
        loaded.report.estates.unresolved_countries["AAA"]
            .iter()
            .any(|r| r == "unresolved_privilege_binding:absent")
    );
}

#[test]
fn invalid_catalogs_and_conditions_report_their_source() {
    for text in [
        "guild = { trigger = { always = yes } } guild = {}",
        "guild = { trigger = { always = yes } trigger = {} }",
        "guild = { privileges = { charter charter } }",
        "guild = { trigger = { has_estate = \"\" } }",
        "guild = { trigger = { if = { always = yes } } }",
        "guild = { base_influence = 1 base_influence = 2 }",
        "guild = { base_influence = invalid }",
        "guild = { influence_modifier = { trigger = { always = yes } } }",
    ] {
        let temp = copy_fixture();
        write(temp.path(), "common/estates/test.txt", text);
        assert!(
            VanillaWorld::load(temp.path(), "1444.11.11")
                .unwrap_err()
                .to_string()
                .replace(char::from(92), "/")
                .contains("common/estates/test.txt")
        );
    }
    let temp = copy_fixture();
    append(
        temp.path(),
        "common/estate_privileges/test.txt",
        "charter = {}",
    );
    assert!(
        VanillaWorld::load(temp.path(), "1444.11.11")
            .unwrap_err()
            .to_string()
            .replace(char::from(92), "/")
            .contains("common/estate_privileges/test.txt")
    );
}

fn land_fixture() -> tempfile::TempDir {
    let temp = copy_fixture();
    let path = "common/defines.lua";
    let text = fs::read_to_string(temp.path().join(path)).unwrap();
    write(temp.path(), path, &text.replace("NCountry = {", "NCountry = {\n ESTATE_START_CROWNLAND_INFLUENCE = 12,\n ESTATE_MIN_DISTRIBUTED_CROWNLAND = 40,\n"));
    write(
        temp.path(),
        "common/estates/test.txt",
        r#"
        estate_guild = { trigger = { always = yes } base_influence = 12 privileges = { charter }
            influence_modifier = { desc = test trigger = { tag = BBB } influence = -12 } }
        estate_council = { trigger = { always = yes } base_influence = 24 }
        estate_order = { trigger = { always = yes } base_influence = 12 }
    "#,
    );
    temp
}

#[test]
fn land_distribution_and_dated_history_generate_resumable_core_state() {
    let temp = land_fixture();
    append(
        temp.path(),
        "history/countries/AAA - Alpha.txt",
        r#"
        change_estate_land_share = { estate = estate_council share = 5 }
        change_estate_land_share = { estate = estate_guild share = -2 }
        change_estate_land_share = { estate = estate_order share = -3 }
        add_estate_loyalty = { estate = estate_guild loyalty = 7 }
        1450.1.1 = { add_estate_loyalty = { estate = estate_guild loyalty = -100 } }
    "#,
    );
    let mut loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let states = loaded.country("AAA").unwrap().estates.unwrap();
    assert_eq!(states["estate_guild"].land_share_micros, Some(130_000));
    assert_eq!(states["estate_council"].land_share_micros, Some(350_000));
    assert_eq!(states["estate_order"].land_share_micros, Some(120_000));
    assert_eq!(states["estate_guild"].loyalty_milli, Some(57_000));
    assert_eq!(
        loaded.country("BBB").unwrap().estates.unwrap()["estate_guild"].land_share_micros,
        None
    );
    // The republic's metric effects are not initialized by this fixture.
    assert!(
        loaded
            .report
            .estates
            .unresolved_initialization
            .contains_key("BBB")
    );
    assert_eq!(loaded.report.estates.initial_land_countries, 1);
    assert!(loaded.report.estates.pending_land_changes.is_empty());
    for field in [
        "set_estate_privilege",
        "change_estate_land_share",
        "add_estate_loyalty",
    ] {
        assert!(
            !loaded.report.unapplied_country_fields.contains_key(field),
            "{field} was already applied by estate history"
        );
    }
    let saved = loaded.save().unwrap();
    let mut restored = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    restored.restore(&saved).unwrap();
    loaded.advance(365).unwrap();
    restored.advance(365).unwrap();
    assert_eq!(loaded.save().unwrap(), restored.save().unwrap());
    let later = VanillaWorld::load(temp.path(), "1450.1.1").unwrap();
    assert_eq!(
        later.country("AAA").unwrap().estates.unwrap()["estate_guild"].loyalty_milli,
        Some(0)
    );
}

#[test]
fn uncertain_influence_scoped_history_and_startup_effects_preserve_unknowns() {
    let temp = land_fixture();
    append(
        temp.path(),
        "common/estates/test.txt",
        "uncertain = { trigger = { always = yes } base_influence = 3 influence_modifier = { trigger = { unknown_condition = yes } influence = 2 } }",
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(
        loaded
            .country("AAA")
            .unwrap()
            .estates
            .unwrap()
            .values()
            .all(|s| s.land_share_micros.is_none())
    );
    assert!(
        loaded.report.estates.unresolved_initialization["AAA"]
            .iter()
            .any(|r| r.contains("unknown_condition"))
    );
    write(
        temp.path(),
        "common/on_actions/test.txt",
        "on_startup = { effect = { add_estate_loyalty = { estate = estate_guild loyalty = 9 } } }",
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(
        loaded
            .country("AAA")
            .unwrap()
            .estates
            .unwrap()
            .values()
            .all(|s| s.loyalty_milli.is_none())
    );
    assert!(
        loaded.report.estates.unresolved_initialization["AAA"]
            .contains("unexecuted_startup_estate_effects")
    );
    assert!(
        loaded
            .country("AAA")
            .unwrap()
            .estates
            .unwrap()
            .values()
            .all(|s| s.land_share_micros.is_none())
    );
    write(temp.path(), "common/on_actions/test.txt", "on_startup = {}");
    append(
        temp.path(),
        "history/countries/AAA - Alpha.txt",
        "if = { limit = { always = yes } add_estate_loyalty = { estate = estate_guild loyalty = 9 } }",
    );
    assert!(
        VanillaWorld::load(temp.path(), "1444.11.11")
            .unwrap()
            .country("AAA")
            .unwrap()
            .estates
            .unwrap()
            .values()
            .all(|s| s.loyalty_milli.is_none())
    );
}

#[test]
fn unvalidated_land_boundaries_and_privilege_land_effects_are_reported() {
    let temp = land_fixture();
    append(
        temp.path(),
        "history/countries/AAA - Alpha.txt",
        "change_estate_land_share = { estate = estate_guild share = 100 }",
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(
        loaded
            .country("AAA")
            .unwrap()
            .estates
            .unwrap()
            .values()
            .all(|s| s.land_share_micros.is_none())
    );
    assert_eq!(loaded.report.estates.pending_land_changes["AAA"].len(), 1);
    let temp = land_fixture();
    append(
        temp.path(),
        "common/estate_privileges/test.txt",
        "other = { land_share = 5 }",
    );
    write(
        temp.path(),
        "common/estate_privileges/test.txt",
        "charter = { land_share = 5 is_valid = { always = yes } }",
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(
        loaded
            .country("AAA")
            .unwrap()
            .estates
            .unwrap()
            .values()
            .all(|s| s.land_share_micros.is_none())
    );
}

#[test]
fn government_and_attached_modifiers_contribute_before_historical_privileges() {
    let temp = land_fixture();
    append(
        temp.path(),
        "common/government_reforms/test_governments.txt",
        "estate_levy = { modifiers = { guild_influence_modifier = 0.12 } }",
    );
    append(
        temp.path(),
        "history/countries/AAA - Alpha.txt",
        "add_government_reform = estate_levy",
    );
    append(
        temp.path(),
        "common/event_modifiers/test_modifiers.txt",
        "estate_weights = { guild_exclusive_influence_modifier = 0.12 all_estate_influence_modifier = 0.12 }",
    );
    append(
        temp.path(),
        "history/countries/AAA - Alpha.txt",
        "add_country_modifier = { name = estate_weights duration = -1 }",
    );
    write(
        temp.path(),
        "common/estate_privileges/test.txt",
        "charter = { influence = 0.9 loyalty = 0.8 is_valid = { always = yes } on_granted = {} }",
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let states = loaded.country("AAA").unwrap().estates.unwrap();
    assert_eq!(states["estate_guild"].land_share_micros, Some(418_460));
    assert_eq!(states["estate_council"].land_share_micros, Some(108_920));
    assert_eq!(states["estate_order"].land_share_micros, Some(72_620));
    assert_eq!(states["estate_guild"].loyalty_milli, Some(50_000));
    assert_eq!(states["estate_guild"].influence_milli, Some(138_000));
}

#[test]
fn unknown_numeric_boundaries_and_invalid_history_remain_diagnostic() {
    let temp = land_fixture();
    append(
        temp.path(),
        "history/countries/AAA - Alpha.txt",
        "add_estate_loyalty = { estate = estate_guild loyalty = 100 }",
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(
        loaded
            .country("AAA")
            .unwrap()
            .estates
            .unwrap()
            .values()
            .all(|s| s.loyalty_milli.is_none())
    );
    assert_eq!(
        loaded.report.estates.pending_loyalty_changes["AAA"].len(),
        1
    );
    assert!(loaded.report.estates.pending_land_changes.is_empty());
    assert!(
        loaded
            .country("AAA")
            .unwrap()
            .estates
            .unwrap()
            .values()
            .all(|s| s.land_share_micros.is_some())
    );

    let temp = land_fixture();
    append(
        temp.path(),
        "common/estates/test.txt",
        "negative = { trigger = { always = yes } base_influence = -1 }",
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(
        loaded
            .country("AAA")
            .unwrap()
            .estates
            .unwrap()
            .values()
            .all(|s| s.land_share_micros.is_none())
    );
    assert!(
        loaded.report.estates.unresolved_initialization["AAA"]
            .contains("negative:unsupported_negative_initial_influence")
    );

    for history in [
        "change_estate_land_share = { estate = estate_guild share = 0.00001 }",
        "add_estate_loyalty = { estate = estate_guild loyalty = 1 loyalty = 2 }",
        "add_estate_loyalty = { loyalty = 1 }",
    ] {
        let temp = land_fixture();
        append(temp.path(), "history/countries/AAA - Alpha.txt", history);
        let message = VanillaWorld::load(temp.path(), "1444.11.11")
            .unwrap_err()
            .to_string()
            .replace(char::from(92), "/");
        assert!(
            message.contains("history/countries/AAA - Alpha.txt"),
            "{message}"
        );
    }
}

#[test]
fn removed_or_unbound_privileges_cannot_hide_unexecuted_initialization() {
    for operation in [
        "remove_estate_privilege = charter",
        "set_estate_privilege = absent",
    ] {
        let temp = land_fixture();
        write(
            temp.path(),
            "common/estate_privileges/test.txt",
            "charter = { on_granted = { add_estate_loyalty = { estate = estate_guild loyalty = 8 } } }",
        );
        append(temp.path(), "history/countries/AAA - Alpha.txt", operation);
        let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
        let states = loaded.country("AAA").unwrap().estates.unwrap();
        assert!(
            states
                .values()
                .all(|s| s.land_share_micros.is_none() && s.loyalty_milli.is_none())
        );
        assert!(
            loaded.report.estates.unresolved_initialization["AAA"]
                .contains("unresolved_privilege_initialization")
        );
        if operation.starts_with("remove") {
            assert!(
                states["estate_guild"]
                    .privileges
                    .as_ref()
                    .unwrap()
                    .is_empty()
            );
        }
    }
}

#[test]
fn estate_loyalty_target_and_decay_defines_load_as_milli_values() {
    let temp = copy_fixture();
    let path = "common/defines.lua";
    let baseline = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let baseline_estates = baseline.country("AAA").unwrap().estates.unwrap().clone();
    let defines = fs::read_to_string(temp.path().join(path)).unwrap();
    write(
        temp.path(),
        path,
        &defines.replace(
            "ESTATE_HAPPY_THRESHOLD = 60,",
            "ESTATE_HAPPY_THRESHOLD = 60,\n        ESTATE_DEFAULT_LOYALTY = 30.0,\n        ESTATE_LOYALTY_DECAY_BASE_MIN = 2.0,\n        ESTATE_LOYALTY_DECAY_BASE_MAX = 5.0,",
        ),
    );

    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert_eq!(
        loaded
            .initial
            .world()
            .estates()
            .rules
            .loyalty_target_base_milli,
        Some(30_000)
    );
    assert_eq!(
        loaded.country("AAA").unwrap().estates.unwrap(),
        &baseline_estates
    );
    assert_eq!(
        loaded.initial.world().estates().rules.loyalty_decay,
        Some(augustus_core::EstateLoyaltyDecay {
            base_min_annual_milli: 2_000,
            base_max_annual_milli: 5_000,
        })
    );
    assert!(
        loaded
            .report
            .estates
            .definition_sources
            .contains_key("define:ESTATE_LOYALTY_DECAY_BASE_MIN")
    );
    assert!(
        loaded
            .report
            .estates
            .definition_sources
            .contains_key("define:ESTATE_LOYALTY_DECAY_BASE_MAX")
    );
    assert!(
        loaded
            .report
            .estates
            .definition_sources
            .contains_key("define:ESTATE_DEFAULT_LOYALTY")
    );
}

#[test]
fn estate_loyalty_target_base_rejects_unsupported_precision_and_range() {
    for (value, expected) in [
        (
            "30.0005",
            "estate loyalty target base define exceeds supported precision",
        ),
        (
            "2147483.648",
            "estate loyalty target base define exceeds supported range",
        ),
    ] {
        let temp = copy_fixture();
        let path = "common/defines.lua";
        let defines = fs::read_to_string(temp.path().join(path)).unwrap();
        write(
            temp.path(),
            path,
            &defines.replace(
                "ESTATE_HAPPY_THRESHOLD = 60,",
                &format!("ESTATE_HAPPY_THRESHOLD = 60,\n        ESTATE_DEFAULT_LOYALTY = {value},"),
            ),
        );

        assert!(
            VanillaWorld::load(temp.path(), "1444.11.11")
                .unwrap_err()
                .to_string()
                .contains(expected)
        );
    }
}

#[test]
fn estate_loyalty_decay_defines_must_be_a_complete_valid_pair() {
    for (replacement, expected) in [
        ("ESTATE_LOYALTY_DECAY_BASE_MIN = 2.0,", None),
        (
            "ESTATE_LOYALTY_DECAY_BASE_MIN = 5.0,\n        ESTATE_LOYALTY_DECAY_BASE_MAX = 2.0,",
            Some("invalid estate loyalty decay defines"),
        ),
    ] {
        let temp = copy_fixture();
        let path = "common/defines.lua";
        let defines = fs::read_to_string(temp.path().join(path)).unwrap();
        write(
            temp.path(),
            path,
            &defines.replace(
                "ESTATE_HAPPY_THRESHOLD = 60,",
                &format!("ESTATE_HAPPY_THRESHOLD = 60,\n        {replacement}"),
            ),
        );
        let result = VanillaWorld::load(temp.path(), "1444.11.11");
        if let Some(expected) = expected {
            assert!(result.unwrap_err().to_string().contains(expected));
        } else {
            assert!(
                result
                    .unwrap_err()
                    .to_string()
                    .contains("must be provided together")
            );
        }
    }
}
