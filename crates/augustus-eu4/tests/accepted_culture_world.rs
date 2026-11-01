// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{AttributeEvaluation, Condition, EntityId};
use augustus_eu4::{ContentRoot, ContentSet, ContentSource, TextEncoding, VanillaWorld};
use std::{collections::BTreeSet, path::Path};

const START_EVENT: &str = "accepted_cultures.start";
const FOLLOWUP_EVENT: &str = "accepted_cultures.remove_alpha";

fn roots() -> (std::path::PathBuf, std::path::PathBuf) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    (root.join("world"), root.join("accepted-cultures"))
}

fn load_with_mod() -> VanillaWorld {
    let (base, mod_root) = roots();
    let content = ContentSet::resolve(vec![
        ContentSource::base(ContentRoot::open(base).unwrap()),
        ContentSource::mod_directory(mod_root, TextEncoding::Utf8).unwrap(),
    ])
    .unwrap();
    VanillaWorld::load_from_content_with_players(
        &content,
        "1444.11.11",
        &BTreeSet::from(["BBB".into()]),
    )
    .unwrap()
}

fn tax_basis(world: &VanillaWorld) -> AttributeEvaluation {
    let province = world.initial.province_entity(1).unwrap();
    world
        .initial
        .world()
        .attributes()
        .evaluate(world.initial.world(), province, "tax_basis")
        .unwrap()
}

fn same_group_tax_contribution(evaluation: &AttributeEvaluation) -> Option<i64> {
    evaluation
        .steps
        .iter()
        .flat_map(|step| &step.contributions)
        .find(|contribution| contribution.source == "static:same_culture_group")
        .map(|contribution| contribution.micros)
}

fn accepted_culture_condition(world: &VanillaWorld) -> Condition {
    world.initial.world().event_catalog().option_conditions()[FOLLOWUP_EVENT]
        .values()
        .next()
        .unwrap()
        .clone()
}

fn choose_first_option(world: &mut VanillaWorld, event: &str, country: EntityId) {
    let pending = world.initial.world().pending_events();
    let event_id = pending
        .iter()
        .find(|pending| pending.event == event && pending.current == country)
        .unwrap_or_else(|| panic!("missing pending event {event}: {pending:?}"))
        .id;
    let option = world
        .initial
        .world()
        .available_event_options(event_id)
        .unwrap()[0]
        .id
        .clone();
    world
        .initial
        .choose_event(&augustus_core::EventChoice {
            pending: event_id,
            option,
        })
        .unwrap();
}

#[test]
fn monthly_event_acceptance_changes_live_condition_and_tax_basis_across_restore() {
    let mut world = load_with_mod();
    let bbb = world.initial.country_entity("BBB").unwrap();
    let condition = accepted_culture_condition(&world);
    let group_policy_before = world.initial.world().affiliations().0["culture"].acceptance[&bbb]
        .groups
        .clone();

    assert_eq!(
        world
            .initial
            .world()
            .affiliations()
            .accepts_member("culture", bbb, "alpha"),
        Some(false)
    );
    assert!(!condition.evaluate(world.initial.world(), bbb).unwrap());
    let initial_tax = tax_basis(&world);
    assert_eq!(initial_tax.known_subtotal_micros, Some(212_000));
    assert_eq!(initial_tax.value_micros, None);
    assert_eq!(same_group_tax_contribution(&initial_tax), Some(-150_000));

    let snapshot_before_reads = world.save().unwrap();
    assert!(!condition.evaluate(world.initial.world(), bbb).unwrap());
    let _ = tax_basis(&world);
    assert_eq!(
        world.save().unwrap(),
        snapshot_before_reads,
        "attribute and condition queries leave the core snapshot, including RNG state, unchanged"
    );

    for _ in 0..31 {
        if !world.initial.world().pending_events().is_empty() {
            break;
        }
        world.advance(1).unwrap();
    }
    let pending = world.initial.world().pending_events();
    assert_eq!(
        pending.len(),
        1,
        "only BBB's tagged start event should fire"
    );
    assert_eq!(pending[0].event, START_EVENT);
    assert_eq!(pending[0].current, bbb);
    choose_first_option(&mut world, START_EVENT, bbb);

    let accepted_tax = tax_basis(&world);
    assert_eq!(
        world
            .initial
            .world()
            .affiliations()
            .accepts_member("culture", bbb, "alpha"),
        Some(true)
    );
    assert!(condition.evaluate(world.initial.world(), bbb).unwrap());
    assert_eq!(accepted_tax.known_subtotal_micros, Some(531_000));
    assert_eq!(accepted_tax.value_micros, None);
    assert_eq!(same_group_tax_contribution(&accepted_tax), None);

    let acceptance = &world.initial.world().affiliations().0["culture"].acceptance[&bbb];
    assert!(acceptance.members.as_ref().unwrap().contains("alpha"));
    assert_eq!(acceptance.groups, group_policy_before);
    let saved_after_acceptance = world.save().unwrap();

    world.advance(1).unwrap();
    assert_eq!(
        world.initial.world().pending_events().len(),
        1,
        "the option schedules the follow-up for the next day"
    );
    assert_eq!(
        world.initial.world().pending_events()[0].event,
        FOLLOWUP_EVENT
    );
    let followup = world.initial.world().pending_events()[0].id;
    assert_eq!(
        world
            .initial
            .world()
            .available_event_options(followup)
            .unwrap()
            .len(),
        1,
        "the accepted-culture option condition is live"
    );
    choose_first_option(&mut world, FOLLOWUP_EVENT, bbb);

    let removed_tax = tax_basis(&world);
    assert_eq!(
        world
            .initial
            .world()
            .affiliations()
            .accepts_member("culture", bbb, "alpha"),
        Some(false)
    );
    assert!(!condition.evaluate(world.initial.world(), bbb).unwrap());
    assert_eq!(removed_tax.known_subtotal_micros, Some(212_000));
    assert_eq!(removed_tax.value_micros, None);
    assert_eq!(same_group_tax_contribution(&removed_tax), Some(-150_000));
    assert_eq!(
        world.initial.world().affiliations().0["culture"].acceptance[&bbb].groups,
        group_policy_before,
        "member changes preserve the existing group acceptance policy"
    );

    let mut restored = load_with_mod();
    restored.restore(&saved_after_acceptance).unwrap();
    let restored_country = restored.initial.country_entity("BBB").unwrap();
    assert!(
        condition
            .evaluate(restored.initial.world(), restored_country)
            .unwrap()
    );
    let restored_tax = tax_basis(&restored);
    assert_eq!(restored_tax.known_subtotal_micros, Some(531_000));
    assert_eq!(same_group_tax_contribution(&restored_tax), None);
    restored.advance(1).unwrap();
    choose_first_option(&mut restored, FOLLOWUP_EVENT, restored_country);

    assert_eq!(
        restored.initial.world().affiliations().accepts_member(
            "culture",
            restored_country,
            "alpha"
        ),
        Some(false)
    );
    let restored_removed_tax = tax_basis(&restored);
    assert_eq!(restored_removed_tax.known_subtotal_micros, Some(212_000));
    assert_eq!(
        same_group_tax_contribution(&restored_removed_tax),
        Some(-150_000)
    );
    assert_eq!(restored.save().unwrap(), world.save().unwrap());
}
