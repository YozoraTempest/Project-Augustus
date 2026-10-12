// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::*;

fn world() -> World {
    let mut world = World::new(Schema::new(["unused"]).unwrap());
    world.add_entity("AAA", EntityKind::Country);
    world.add_entity("BBB", EntityKind::Country);
    let province = world.add_entity("Port", EntityKind::Province);
    world.set_owner(province, EntityId(0)).unwrap();
    world.set_capital(EntityId(0), province).unwrap();
    let modifiers: Modifiers = serde_json::from_str(r#"{
      "definitions":{"boost":{"values_micros":{"rate":125000},"flags":{}},"guard":{"values_micros":{},"flags":{}}},"active":{}
    }"#).unwrap();
    world.set_modifiers(modifiers).unwrap();
    let attributes: Attributes = serde_json::from_str(r#"{"formulas":{"0":{"yield":{"base":{"kind":"constant","micros":1000000},"channels":[{"flat":[],"rates":[{"kind":"modifiers","target":"Current","property":"rate"}]}],"minimum_micros":null,"maximum_micros":null,"unresolved":[]}}},"static_modifiers":{},"building_modifiers":{},"trade_good_modifiers":{}}"#).unwrap();
    world.set_attributes(attributes).unwrap();
    world
}

fn attach(name: &str, duration: Option<u64>, renewal: ModifierRenewal) -> Action {
    Action::AttachModifier {
        target: Target::Current,
        name: name.into(),
        duration,
        hidden: Some(true),
        renewal,
    }
}

fn has(name: &str) -> Condition {
    Condition::HasModifier {
        target: Target::Current,
        name: name.into(),
    }
}

#[test]
fn expiration_precedes_due_conditions_and_updates_attribute_sources_after_restore() {
    let mut world = world();
    world
        .execute(
            EntityId(0),
            &[attach("boost", Some(2), ModifierRenewal::Replace)],
        )
        .unwrap();
    assert_eq!(
        world
            .attributes()
            .evaluate(&world, EntityId(0), "yield")
            .unwrap()
            .value_micros,
        Some(1125000)
    );
    assert!(!world.has_modifier(EntityId(1), "boost").unwrap());
    world
        .attach_modifier(EntityId(2), "boost", None, None, ModifierRenewal::Replace)
        .unwrap();
    world
        .schedule(
            EntityId(0),
            2,
            vec![Action::If {
                condition: has("boost"),
                then: vec![Action::Emit("stale".into())],
                otherwise: vec![Action::Emit("expired".into())],
            }],
        )
        .unwrap();
    world.advance_to(1).unwrap();
    let snapshot = world.save("modifiers").unwrap();
    let mut resumed = World::restore(&snapshot, "modifiers", world.schema()).unwrap();
    for state in [&mut world, &mut resumed] {
        state.advance_to(2).unwrap();
        assert_eq!(state.events()[0].2, "expired");
        assert_eq!(
            state
                .attributes()
                .evaluate(state, EntityId(0), "yield")
                .unwrap()
                .value_micros,
            Some(1000000)
        );
        assert!(state.has_modifier(EntityId(2), "boost").unwrap());
        assert!(
            !state
                .save("modifiers")
                .unwrap()
                .contains("modifier_expirations")
        );
    }
    assert_eq!(
        world.save("modifiers").unwrap(),
        resumed.save("modifiers").unwrap()
    );
}

#[test]
fn renewal_uses_one_attachment_and_removal_cancels_old_expiration() {
    for renewal in [ModifierRenewal::Replace, ModifierRenewal::KeepLatestDated] {
        let mut world = world();
        world
            .attach_modifier(EntityId(0), "boost", Some(4), None, renewal)
            .unwrap();
        world
            .attach_modifier(EntityId(0), "boost", Some(2), Some(true), renewal)
            .unwrap();
        assert_eq!(world.modifiers().active[&EntityId(0)].len(), 1);
        assert_eq!(
            world.modifiers().declared_totals(EntityId(0)).unwrap()["rate"],
            125000
        );
        world.advance_to(2).unwrap();
        assert_eq!(
            world.has_modifier(EntityId(0), "boost").unwrap(),
            renewal == ModifierRenewal::KeepLatestDated
        );
        world.remove_modifier(EntityId(0), "boost").unwrap();
        world.remove_modifier(EntityId(0), "boost").unwrap();
        world
            .attach_modifier(EntityId(0), "boost", None, None, renewal)
            .unwrap();
        world.advance_to(5).unwrap();
        assert!(world.has_modifier(EntityId(0), "boost").unwrap());
    }
    for first in [Some(2), None] {
        let mut world = world();
        for duration in [first, if first.is_some() { None } else { Some(2) }] {
            world
                .attach_modifier(
                    EntityId(0),
                    "boost",
                    duration,
                    None,
                    ModifierRenewal::KeepLatestDated,
                )
                .unwrap();
        }
        assert_eq!(
            world.modifiers().active[&EntityId(0)]["boost"].expires_on,
            Some(2)
        );
        world.advance_to(2).unwrap();
        assert!(!world.has_modifier(EntityId(0), "boost").unwrap());
    }
}

#[test]
fn unknown_attachments_and_properties_remain_unknown_until_resolved() {
    let mut world = world();
    let mut modifiers = world.modifiers().clone();
    modifiers
        .unresolved
        .insert(EntityId(0), ["boost".into()].into());
    world.set_modifiers(modifiers).unwrap();
    assert!(world.has_modifier(EntityId(0), "boost").is_err());
    assert!(
        world
            .attributes()
            .evaluate(&world, EntityId(0), "yield")
            .unwrap()
            .value_micros
            .is_none()
    );
    let before = world.save("modifiers").unwrap();
    assert!(
        world
            .attach_modifier(
                EntityId(0),
                "boost",
                Some(2),
                None,
                ModifierRenewal::KeepLatestDated
            )
            .is_err()
    );
    assert_eq!(world.save("modifiers").unwrap(), before);
    world
        .attach_modifier(
            EntityId(0),
            "boost",
            Some(2),
            None,
            ModifierRenewal::Replace,
        )
        .unwrap();
    assert_eq!(
        world
            .attributes()
            .evaluate(&world, EntityId(0), "yield")
            .unwrap()
            .value_micros,
        Some(1125000)
    );
    let mut modifiers = world.modifiers().clone();
    modifiers
        .unresolved_properties
        .insert("boost".into(), ["rate".into()].into());
    world.set_modifiers(modifiers).unwrap();
    assert!(world.has_modifier(EntityId(0), "boost").unwrap());
    assert!(
        world
            .attributes()
            .evaluate(&world, EntityId(0), "yield")
            .unwrap()
            .value_micros
            .is_none()
    );
    world.remove_modifier(EntityId(0), "boost").unwrap();
    assert_eq!(
        world
            .attributes()
            .evaluate(&world, EntityId(0), "yield")
            .unwrap()
            .value_micros,
        Some(1000000)
    );
    let mut modifiers = world.modifiers().clone();
    modifiers.unknown_entities.insert(EntityId(0));
    world.set_modifiers(modifiers).unwrap();
    assert!(world.has_modifier(EntityId(0), "guard").is_err());
    assert!(world.remove_modifier(EntityId(0), "guard").is_err());
    assert!(
        world
            .attach_modifier(EntityId(0), "guard", None, None, ModifierRenewal::Replace)
            .is_err()
    );
    let saved = world.save("modifiers").unwrap();
    assert_eq!(
        World::restore(&saved, "modifiers", world.schema())
            .unwrap()
            .save("modifiers")
            .unwrap(),
        saved
    );
}

#[test]
fn invalid_plans_snapshots_and_catalog_replacement_fail_before_writes() {
    let mut world = world();
    let definition = EventDefinition {
        id: "event".into(),
        scope: EntityKind::Country,
        immediate: vec![attach("guard", None, ModifierRenewal::Replace)],
        options: vec![EventOption {
            id: "accept".into(),
            effects: vec![],
        }],
        after: vec![],
    };
    world
        .set_event_catalog(EventCatalog::new([definition]).unwrap())
        .unwrap();
    let before = world.save("modifiers").unwrap();
    assert!(world.set_modifiers(Modifiers::default()).is_err());
    assert_eq!(world.save("modifiers").unwrap(), before);
    for invalid in [
        attach("missing", None, ModifierRenewal::Replace),
        attach("boost", Some(0), ModifierRenewal::Replace),
    ] {
        let mut candidate = world.clone();
        assert!(
            candidate
                .schedule(
                    EntityId(0),
                    1,
                    vec![Action::If {
                        condition: Condition::Always(false),
                        then: vec![invalid],
                        otherwise: vec![]
                    }]
                )
                .is_err()
        );
        assert_eq!(candidate, world);
    }
    for edit in ["expired", "entity", "property"] {
        let mut saved: serde_json::Value = serde_json::from_str(&before).unwrap();
        match edit {
            "expired" => {
                saved["modifiers"]["active"]["0"] =
                    serde_json::json!({"boost":{"hidden":null,"expires_on":0}})
            }
            "entity" => saved["modifiers"]["unknown_entities"] = serde_json::json!([99]),
            _ => {
                saved["modifiers"]["unresolved_properties"] =
                    serde_json::json!({"missing":["rate"]})
            }
        }
        assert!(
            World::restore_with_events(
                &saved.to_string(),
                "modifiers",
                world.schema(),
                vec![],
                world.event_catalog().clone()
            )
            .is_err()
        );
    }
    world.advance_to(u64::MAX).unwrap();
    let before = world.save("modifiers").unwrap();
    assert!(
        world
            .attach_modifier(
                EntityId(0),
                "boost",
                Some(1),
                None,
                ModifierRenewal::Replace
            )
            .is_err()
    );
    assert_eq!(world.save("modifiers").unwrap(), before);
}
