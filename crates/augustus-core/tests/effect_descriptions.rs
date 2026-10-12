// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::*;

fn world() -> World {
    let mut world = World::new(Schema::new(["value"]).unwrap());
    world.add_entity("AAA", EntityKind::Country);
    world
}

fn description(key: &str, hidden: bool) -> Action {
    Action::Description {
        key: key.into(),
        hidden,
    }
}

#[test]
fn descriptions_preserve_randomness_and_delayed_content_without_state_changes() {
    let mut state = world();
    let mut reference = state.clone();
    let text = vec![
        description("first", false),
        description("second", true),
        description("first", false),
    ];
    state.execute(EntityId(0), &text).unwrap();
    assert_eq!(state, reference);
    let roll = Action::RandomBranch {
        then: vec![Action::Set {
            target: Target::Current,
            field: FieldId(0),
            value: 1,
        }],
        otherwise: vec![Action::Set {
            target: Target::Current,
            field: FieldId(0),
            value: 2,
        }],
    };
    for _ in 0..32 {
        state.execute(EntityId(0), &text).unwrap();
        state
            .execute(EntityId(0), std::slice::from_ref(&roll))
            .unwrap();
        reference
            .execute(EntityId(0), std::slice::from_ref(&roll))
            .unwrap();
        assert_eq!(state, reference);
    }
    state.schedule(EntityId(0), 1, text).unwrap();
    let saved = state.save("descriptions:v1").unwrap();
    let json: serde_json::Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(
        json["tasks"][0]["plan"][0],
        serde_json::json!({"Description":{"key":"first"}})
    );
    assert_eq!(
        json["tasks"][0]["plan"][1],
        serde_json::json!({"Description":{"key":"second","hidden":true}})
    );
    let mut resumed = World::restore(&saved, "descriptions:v1", state.schema()).unwrap();
    state.advance_to(1).unwrap();
    resumed.advance_to(1).unwrap();
    reference.advance_to(1).unwrap();
    assert_eq!(state, resumed);
    assert!(state.events().is_empty());
    assert_eq!(state.entities(), reference.entities());
}

#[test]
fn empty_description_keys_are_rejected_in_queues_catalogs_restore_and_execution() {
    let mut state = world();
    for key in ["", " \t"] {
        let before = state.clone();
        assert!(
            state
                .schedule(
                    EntityId(0),
                    1,
                    vec![Action::Preview {
                        plan: vec![description(key, true)]
                    }]
                )
                .is_err()
        );
        assert_eq!(state, before);
        let catalog = EventCatalog::new([EventDefinition {
            id: "bad".into(),
            scope: EntityKind::Country,
            immediate: vec![description(key, false)],
            options: vec![EventOption {
                id: "0".into(),
                effects: vec![],
            }],
            after: vec![],
        }])
        .unwrap();
        assert!(state.set_event_catalog(catalog).is_err());
        assert_eq!(state, before);
    }
    state
        .schedule(EntityId(0), 1, vec![description("valid", false)])
        .unwrap();
    let mut json: serde_json::Value =
        serde_json::from_str(&state.save("descriptions:v1").unwrap()).unwrap();
    json["tasks"][0]["plan"][0]["Description"]["key"] = "".into();
    assert!(World::restore(&json.to_string(), "descriptions:v1", state.schema()).is_err());
    assert!(
        state
            .execute(EntityId(0), &[description("", false)])
            .is_err()
    );
}
