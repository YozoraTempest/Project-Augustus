// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    Action, AffiliationDimension, Affiliations, Condition, EntityId, EntityKind, FieldId, Schema,
    SchoolOpinionTarget, Target, World,
};
use std::collections::BTreeMap;

fn world() -> World {
    let mut world = World::new(Schema::new(["eligible"]).unwrap());
    world.add_entity("Source", EntityKind::Country);
    world.add_entity("Selected", EntityKind::Country);
    world.add_entity("Other", EntityKind::Country);
    world
        .set_affiliations(Affiliations(BTreeMap::from([(
            "religious_school".into(),
            AffiliationDimension {
                groups: BTreeMap::from([
                    ("source_school".into(), Some("islam".into())),
                    ("selected_school".into(), Some("islam".into())),
                    ("other_school".into(), Some("islam".into())),
                ]),
                members: BTreeMap::from([
                    (EntityId(0), Some("source_school".into())),
                    (EntityId(1), Some("selected_school".into())),
                    (EntityId(2), Some("other_school".into())),
                ]),
                ..Default::default()
            },
        )])))
        .unwrap();
    world.set(EntityId(1), FieldId(0), 1).unwrap();
    world
}

#[test]
fn random_country_filters_candidates_and_prev_targets_the_parent_school() {
    let mut world = world();
    world
        .execute(
            EntityId(0),
            &[Action::RandomCountry {
                condition: Condition::AtLeast {
                    target: Target::Current,
                    field: FieldId(0),
                    value: 1,
                },
                plan: vec![Action::SetSchoolOpinion {
                    target: SchoolOpinionTarget::Previous,
                    opinion: -1,
                }],
            }],
        )
        .unwrap();

    assert_eq!(
        world.school_opinion(EntityId(1), "source_school").unwrap(),
        -1
    );
    assert_eq!(
        world.school_opinion(EntityId(2), "source_school").unwrap(),
        0
    );
    assert_eq!(
        world
            .school_opinion(EntityId(1), "selected_school")
            .unwrap(),
        0
    );
}

#[test]
fn school_opinions_survive_snapshot_restore() {
    let mut world = world();
    world
        .set_school_opinion(EntityId(1), "source_school", -1)
        .unwrap();
    let saved = world.save("school-opinions").unwrap();
    let restored = World::restore(&saved, "school-opinions", world.schema()).unwrap();

    assert_eq!(restored, world);
    assert_eq!(restored.save("school-opinions").unwrap(), saved);
}
