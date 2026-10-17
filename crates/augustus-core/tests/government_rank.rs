// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{Action, Condition, EntityKind, Schema, Target, World};

#[test]
fn government_rank_is_country_state_and_round_trips_without_changing_unknown_shape() {
    let mut world = World::new(Schema::new(std::iter::empty::<String>()).unwrap());
    let country = world.add_entity("AAA", EntityKind::Country);
    let province = world.add_entity("1", EntityKind::Province);

    world.set_government_rank(country, Some(3)).unwrap();
    assert_eq!(world.governments().countries[&country].rank, Some(3));
    assert!(world.set_government_rank(country, Some(0)).is_err());
    assert!(world.set_government_rank(country, Some(4)).is_err());
    assert!(world.set_government_rank(province, Some(2)).is_err());
    assert_eq!(world.governments().countries[&country].rank, Some(3));

    let saved = world.save("government-rank:1").unwrap();
    let restored = World::restore(&saved, "government-rank:1", world.schema()).unwrap();
    assert_eq!(restored.governments().countries[&country].rank, Some(3));

    let mut legacy: serde_json::Value = serde_json::from_str(&saved).unwrap();
    legacy["governments"]["countries"][country.0.to_string()]
        .as_object_mut()
        .unwrap()
        .remove("rank");
    let restored =
        World::restore(&legacy.to_string(), "government-rank:1", world.schema()).unwrap();
    assert_eq!(restored.governments().countries[&country].rank, None);
}

#[test]
fn rank_conditions_and_effects_read_and_update_live_country_state() {
    let mut world = World::new(Schema::new(std::iter::empty::<String>()).unwrap());
    let country = world.add_entity("AAA", EntityKind::Country);
    let province = world.add_entity("1", EntityKind::Province);
    world.set_government_rank(country, Some(2)).unwrap();

    let rank_two = Condition::GovernmentRank {
        target: Target::Current,
        rank: 2,
    };
    assert!(rank_two.evaluate(&world, country).unwrap());
    assert!(rank_two.evaluate(&world, province).is_err());
    let invalid_condition = Condition::GovernmentRank {
        target: Target::Current,
        rank: 4,
    };
    assert!(invalid_condition.evaluate(&world, country).is_err());
    assert!(world.validate_condition(&invalid_condition).is_err());

    world
        .execute(
            country,
            &[Action::SetGovernmentRank {
                target: Target::Current,
                rank: 3,
            }],
        )
        .unwrap();
    assert!(!rank_two.evaluate(&world, country).unwrap());
    assert_eq!(world.government_rank(country).unwrap(), 3);

    let invalid = [Action::SetGovernmentRank {
        target: Target::Current,
        rank: 4,
    }];
    assert!(world.execute(country, &invalid).is_err());

    let unknown = world.add_entity("BBB", EntityKind::Country);
    let rank_one = Condition::GovernmentRank {
        target: Target::Current,
        rank: 1,
    };
    assert!(rank_one.evaluate(&world, unknown).is_err());
}
