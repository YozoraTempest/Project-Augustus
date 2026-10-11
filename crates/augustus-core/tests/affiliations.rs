// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    AffiliationRelation, Affiliations, Attributes, EntityId, EntityKind, Schema, World,
};

fn world() -> World {
    let mut world = World::new(Schema::new(["base"]).unwrap());
    world.add_entity("A", EntityKind::Country);
    world.add_entity("B", EntityKind::Country);
    world.add_entity("P", EntityKind::Province);
    world.set_owner(EntityId(2), EntityId(0)).unwrap();
    world
        .set_affiliations(
            serde_json::from_str(
                r#"{
        "culture": {
            "groups":{"alpha":"near","beta":"near","gamma":"far","unknown":null},
            "members":{"0":"beta","1":"gamma","2":"alpha"},
            "acceptance":{"0":{"members":[],"groups":[]},"1":{"members":[],"groups":[]}}
        },
        "faith": {
            "groups":{"first":"group","second":"group"},
            "members":{"0":"first","1":"second","2":"first"},"acceptance":{}
        }
    }"#,
            )
            .unwrap(),
        )
        .unwrap();
    world.set_attributes(serde_json::from_str::<Attributes>(r#"{
        "static_modifiers":{
            "related":{"values_micros":{"yield":-150000},"flags":{}},
            "foreign":{"values_micros":{"yield":-330000},"flags":{}}
        },"building_modifiers":{},"trade_good_modifiers":{},
        "affiliation_modifiers":{"faith":{"national":{
            "first":{"modifier":{"values_micros":{"yield":100000},"flags":{}},"unresolved_properties":[]},
            "second":{"modifier":{"values_micros":{"yield":300000},"flags":{}},"unresolved_properties":[]}
        }}},
        "formulas":{"2":{"yield":{
            "base":{"kind":"constant","micros":1000000},
            "channels":[{"flat":[],"rates":[
                {"kind":"affiliation_relation","dimension":"culture","target":"Owner","definitions":{"same_group":"related","other_group":"foreign"},"property":"yield"},
                {"kind":"affiliation_modifiers","dimension":"faith","target":"Owner","scope":"national","property":"yield"}
            ]}],"minimum_micros":null,"maximum_micros":null,"unresolved":[]
        }}}
    }"#).unwrap()).unwrap();
    world
}

fn value(world: &World) -> augustus_core::AttributeEvaluation {
    world
        .attributes()
        .evaluate(world, EntityId(2), "yield")
        .unwrap()
}

#[test]
fn ownership_membership_and_acceptance_changes_recalculate_without_cached_totals() {
    let mut world = world();
    assert_eq!(value(&world).value_micros, Some(950_000));
    world.set_owner(EntityId(2), EntityId(1)).unwrap();
    assert_eq!(value(&world).value_micros, Some(970_000));
    let mut affiliations = world.affiliations().clone();
    affiliations
        .0
        .get_mut("culture")
        .unwrap()
        .acceptance
        .get_mut(&EntityId(1))
        .unwrap()
        .members
        .as_mut()
        .unwrap()
        .insert("alpha".into());
    world.set_affiliations(affiliations).unwrap();
    assert_eq!(value(&world).value_micros, Some(1_300_000));
    let mut affiliations = world.affiliations().clone();
    affiliations
        .0
        .get_mut("faith")
        .unwrap()
        .members
        .insert(EntityId(1), Some("first".into()));
    world.set_affiliations(affiliations).unwrap();
    assert_eq!(value(&world).value_micros, Some(1_100_000));
    let saved = world.save("affiliations").unwrap();
    let restored = World::restore(&saved, "affiliations", world.schema()).unwrap();
    assert_eq!(restored, world);
    assert_eq!(restored.save("affiliations").unwrap(), saved);
}

#[test]
fn accepted_groups_and_unknown_inputs_keep_distinct_meanings() {
    let mut world = world();
    let mut affiliations = world.affiliations().clone();
    let culture = affiliations.0.get_mut("culture").unwrap();
    culture.acceptance.get_mut(&EntityId(0)).unwrap().groups =
        Some(["near".into()].into_iter().collect());
    assert_eq!(
        affiliations.relation("culture", EntityId(2), EntityId(0)),
        Some(AffiliationRelation::Accepted)
    );
    world.set_affiliations(affiliations.clone()).unwrap();
    assert_eq!(value(&world).value_micros, Some(1_100_000));
    affiliations
        .0
        .get_mut("culture")
        .unwrap()
        .acceptance
        .get_mut(&EntityId(0))
        .unwrap()
        .members = None;
    assert_eq!(
        affiliations.relation("culture", EntityId(2), EntityId(0)),
        Some(AffiliationRelation::Accepted)
    );
    affiliations
        .0
        .get_mut("culture")
        .unwrap()
        .acceptance
        .get_mut(&EntityId(0))
        .unwrap()
        .groups = Some(Default::default());
    world.set_affiliations(affiliations.clone()).unwrap();
    assert_eq!(value(&world).known_subtotal_micros, Some(1_100_000));
    assert_eq!(value(&world).value_micros, None);
    assert!(
        value(&world)
            .unresolved
            .contains("affiliation:culture:unknown_relation")
    );
    affiliations
        .0
        .get_mut("culture")
        .unwrap()
        .members
        .insert(EntityId(0), Some("alpha".into()));
    world.set_affiliations(affiliations).unwrap();
    assert_eq!(value(&world).value_micros, Some(1_100_000));
    let mut attributes = world.attributes().clone();
    let definition = attributes
        .affiliation_modifiers
        .get_mut("faith")
        .unwrap()
        .get_mut("national")
        .unwrap()
        .get_mut("first")
        .unwrap();
    definition.modifier.values_micros.remove("yield");
    definition.unresolved_properties.insert("yield".into());
    world.set_attributes(attributes).unwrap();
    assert_eq!(value(&world).known_subtotal_micros, Some(1_000_000));
    assert_eq!(value(&world).value_micros, None);
    assert!(
        value(&world)
            .unresolved
            .contains("affiliation:faith:national:first:ambiguous:yield")
    );
}

#[test]
fn validation_is_atomic_and_old_snapshots_still_restore() {
    let mut world = world();
    let saved = world.save("affiliations").unwrap();
    for invalid in [
        r#"{"culture":{"groups":{},"members":{"99":"alpha"},"acceptance":{}}}"#,
        r#"{"culture":{"groups":{"alpha":""},"members":{},"acceptance":{}}}"#,
        r#"{"culture":{"groups":{},"members":{"2":""},"acceptance":{}}}"#,
        r#"{"culture":{"groups":{},"members":{},"acceptance":{"2":{"members":[],"groups":[]}}}}"#,
    ] {
        assert!(
            world
                .set_affiliations(serde_json::from_str::<Affiliations>(invalid).unwrap())
                .is_err()
        );
        assert_eq!(world.save("affiliations").unwrap(), saved);
        let mut tampered: serde_json::Value = serde_json::from_str(&saved).unwrap();
        tampered["affiliations"] = serde_json::from_str(invalid).unwrap();
        assert!(World::restore(&tampered.to_string(), "affiliations", world.schema()).is_err());
    }
    let old = World::new(Schema::new(["base"]).unwrap());
    let old_save = old.save("old").unwrap();
    assert!(!old_save.contains("affiliations"));
    assert_eq!(World::restore(&old_save, "old", old.schema()).unwrap(), old);
}
