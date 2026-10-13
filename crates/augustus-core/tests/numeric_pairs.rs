// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::*;

fn named(name: &str) -> EntityRef {
    EntityRef::Selected(ScopeSelector::Named {
        kind: EntityKind::Country,
        name: name.into(),
    })
}
fn slot() -> NumericSlot {
    NumericSlot::Resource(CountryResource::Treasury)
}
fn pair(first: EntityRef, second: EntityRef) -> Condition {
    Condition::NumericCompareEntities {
        first,
        second,
        slot: slot(),
        comparison: NumericComparison::AtLeast,
    }
}
fn fixture() -> World {
    let mut world = World::new(Schema::new(["unused"]).unwrap());
    world.add_entity("AAA", EntityKind::Country);
    world.add_entity("BBB", EntityKind::Country);
    world
        .set_country_resources(
            [
                (
                    EntityId(0),
                    CountryResources {
                        treasury_milli: Some(999),
                        ..Default::default()
                    },
                ),
                (
                    EntityId(1),
                    CountryResources {
                        treasury_milli: Some(500),
                        ..Default::default()
                    },
                ),
            ]
            .into(),
        )
        .unwrap();
    world
}

#[test]
fn comparisons_read_live_precision_and_keep_unknown_and_absent_references_distinct() {
    let mut world = fixture();
    assert!(
        pair(named("AAA"), named("BBB"))
            .evaluate(&world, EntityId(0))
            .unwrap()
    );
    assert!(
        !pair(named("BBB"), named("AAA"))
            .evaluate(&world, EntityId(0))
            .unwrap()
    );
    assert!(
        !pair(
            named("AAA"),
            EntityRef::Selected(ScopeSelector::Global("absent".into()))
        )
        .evaluate(&world, EntityId(0))
        .unwrap()
    );
    world
        .set_country_resources(
            [(
                EntityId(0),
                CountryResources {
                    treasury_milli: Some(999),
                    ..Default::default()
                },
            )]
            .into(),
        )
        .unwrap();
    assert!(
        pair(named("AAA"), named("BBB"))
            .evaluate(&world, EntityId(0))
            .is_err()
    );
}

#[test]
fn delayed_root_comparisons_restore_the_same_entities_and_read_later_writes() {
    let mut whole = fixture();
    let condition = pair(
        EntityRef::Relative(Target::Current),
        EntityRef::Selected(ScopeSelector::Root),
    );
    whole
        .execute(
            EntityId(0),
            &[Action::InScope {
                selector: ScopeSelector::Named {
                    kind: EntityKind::Country,
                    name: "BBB".into(),
                },
                kind: EntityKind::Country,
                plan: vec![Action::Schedule {
                    delay: 2,
                    plan: vec![Action::If {
                        condition,
                        then: vec![Action::Emit("later".into())],
                        otherwise: vec![],
                    }],
                }],
            }],
        )
        .unwrap();
    whole.advance_to(1).unwrap();
    let saved = whole.save("pairs:1").unwrap();
    let mut resumed = World::restore(&saved, "pairs:1", whole.schema()).unwrap();
    for world in [&mut whole, &mut resumed] {
        world
            .update_numeric(EntityId(0), &slot(), NumericOperation::Set, 250)
            .unwrap();
        world.advance_to(2).unwrap();
    }
    assert_eq!(
        whole.save("pairs:1").unwrap(),
        resumed.save("pairs:1").unwrap()
    );
    assert_eq!(whole.events()[0].1, EntityId(1));
    assert_eq!(whole.events()[0].2, "later");
    let mut corrupt: serde_json::Value = serde_json::from_str(&saved).unwrap();
    corrupt["tasks"][0]["plan"][0]["If"]["condition"]["NumericCompareEntities"]["slot"] =
        serde_json::json!({"Field":99});
    assert!(World::restore(&corrupt.to_string(), "pairs:1", whole.schema()).is_err());
}
