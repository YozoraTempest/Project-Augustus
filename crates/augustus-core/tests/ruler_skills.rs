// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::*;

fn world() -> World {
    let mut world = World::new(Schema::new(["unused"]).unwrap());
    world.add_entity("AAA", EntityKind::Country);
    world.add_entity("BBB", EntityKind::Country);
    world.add_entity("Port", EntityKind::Province);
    let ruler = PersonState {
        name: Some("Iris".into()),
        administration: Some(5),
        diplomacy: Some(3),
        military: Some(1),
        ..Default::default()
    };
    world
        .set_politics(Politics {
            ruler_skill_bounds: Some(RulerSkillBounds {
                minimum: 2,
                maximum: 4,
            }),
            countries: [(
                EntityId(0),
                CountryPolitics {
                    ruler: Some(ruler.clone()),
                    heir: Some(ruler.clone()),
                    consort: Some(ruler),
                    ..Default::default()
                },
            )]
            .into(),
            ..Default::default()
        })
        .unwrap();
    world
}

#[test]
fn live_skill_changes_preserve_other_person_state_across_delayed_save_resume() {
    let mut world = world();
    let before = world.politics().countries[&EntityId(0)].clone();
    world
        .schedule(
            EntityId(0),
            1,
            vec![Action::ChangeRulerSkill {
                target: Target::Current,
                skill: RulerSkill::Administration,
                delta: 1,
            }],
        )
        .unwrap();
    let saved = world.save("fixture").unwrap();
    let mut restored = World::restore(&saved, "fixture", world.schema()).unwrap();
    restored.advance_to(1).unwrap();
    let state = &restored.politics().countries[&EntityId(0)];
    assert_eq!(state.heir, before.heir);
    assert_eq!(state.consort, before.consort);
    assert_eq!(state.ruler.as_ref().unwrap().diplomacy, Some(3));
    assert_eq!(state.ruler.as_ref().unwrap().military, Some(1));
    assert_eq!(
        restored
            .numeric_value(
                EntityId(0),
                &NumericSlot::RulerSkill(RulerSkill::Administration)
            )
            .unwrap(),
        4
    );
    assert!(
        Condition::NumericCompare {
            target: Target::Current,
            slot: NumericSlot::RulerSkill(RulerSkill::Administration),
            comparison: NumericComparison::Equal,
            value: 4,
        }
        .evaluate(&restored, EntityId(0))
        .unwrap()
    );
    restored
        .change_ruler_skill(EntityId(0), RulerSkill::Administration, i64::MIN)
        .unwrap();
    assert_eq!(
        restored
            .ruler_skill(EntityId(0), RulerSkill::Administration)
            .unwrap(),
        2
    );
    restored
        .change_ruler_skill(EntityId(0), RulerSkill::Administration, i64::MAX)
        .unwrap();
    assert_eq!(
        restored
            .ruler_skill(EntityId(0), RulerSkill::Administration)
            .unwrap(),
        4
    );
}

#[test]
fn missing_rules_people_and_skills_never_invent_zero_or_mutate_state() {
    let mut world = world();
    let before = world.save("fixture").unwrap();
    for entity in [EntityId(1), EntityId(2)] {
        assert!(
            world
                .ruler_skill(entity, RulerSkill::Administration)
                .is_err()
        );
        assert!(
            world
                .change_ruler_skill(entity, RulerSkill::Administration, 1)
                .is_err()
        );
        assert_eq!(world.save("fixture").unwrap(), before);
    }
    let mut politics = world.politics().clone();
    politics
        .countries
        .get_mut(&EntityId(0))
        .unwrap()
        .ruler
        .as_mut()
        .unwrap()
        .military = None;
    world.set_politics(politics.clone()).unwrap();
    let before = world.save("fixture").unwrap();
    assert!(
        world
            .change_ruler_skill(EntityId(0), RulerSkill::Military, 0)
            .is_err()
    );
    assert_eq!(world.save("fixture").unwrap(), before);
    politics.ruler_skill_bounds = None;
    world.set_politics(politics).unwrap();
    assert_eq!(
        world
            .ruler_skill(EntityId(0), RulerSkill::Administration)
            .unwrap(),
        5
    );
    let before = world.save("fixture").unwrap();
    assert!(
        world
            .change_ruler_skill(EntityId(0), RulerSkill::Administration, 1)
            .is_err()
    );
    assert_eq!(world.save("fixture").unwrap(), before);
}

#[test]
fn invalid_saved_rules_and_unbounded_numeric_writes_are_rejected() {
    let mut world = world();
    let before = world.save("fixture").unwrap();
    assert!(
        world
            .update_numeric(
                EntityId(0),
                &NumericSlot::RulerSkill(RulerSkill::Administration),
                NumericOperation::Set,
                0
            )
            .is_err()
    );
    let mut politics = world.politics().clone();
    politics.ruler_skill_bounds = Some(RulerSkillBounds {
        minimum: 4,
        maximum: 2,
    });
    assert!(world.set_politics(politics).is_err());
    assert_eq!(world.save("fixture").unwrap(), before);
    let mut forged: serde_json::Value = serde_json::from_str(&before).unwrap();
    forged["politics"]["ruler_skill_bounds"]["minimum"] = 5.into();
    assert!(World::restore(&forged.to_string(), "fixture", world.schema()).is_err());
}
