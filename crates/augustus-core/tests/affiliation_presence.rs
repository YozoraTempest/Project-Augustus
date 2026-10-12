// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{Affiliations, Condition, EntityId, EntityKind, Schema, Target, World};

#[test]
fn explicit_absence_unknown_members_and_selected_members_survive_restore_distinctly() {
    let mut world = World::new(Schema::new(["value"]).unwrap());
    for name in ["Absent", "Selected", "Unknown", "Missing"] {
        world.add_entity(name, EntityKind::Country);
    }
    world
        .set_affiliations(
            serde_json::from_str(
                r#"{"choice":{
        "groups":{"first":"group"},"members":{"1":"first","2":null},
        "absent_members":[0],"acceptance":{}}}"#,
            )
            .unwrap(),
        )
        .unwrap();
    let present = Condition::AffiliationPresent {
        target: Target::Current,
        dimension: "choice".into(),
    };
    let member = Condition::AffiliationMember {
        target: Target::Current,
        dimension: "choice".into(),
        member: "first".into(),
    };
    let group = Condition::AffiliationGroup {
        target: Target::Current,
        dimension: "choice".into(),
        group: "group".into(),
    };
    assert!(!present.evaluate(&world, EntityId(0)).unwrap());
    assert!(!member.evaluate(&world, EntityId(0)).unwrap());
    assert!(!group.evaluate(&world, EntityId(0)).unwrap());
    assert!(present.evaluate(&world, EntityId(1)).unwrap());
    assert!(member.evaluate(&world, EntityId(1)).unwrap());
    for id in [EntityId(2), EntityId(3)] {
        assert!(present.evaluate(&world, id).is_err());
        assert!(
            Condition::Not(Box::new(present.clone()))
                .evaluate(&world, id)
                .is_err()
        );
    }
    let saved = world.save("presence").unwrap();
    let restored = World::restore(&saved, "presence", world.schema()).unwrap();
    assert_eq!(world, restored);
    assert_eq!(saved, restored.save("presence").unwrap());
    let mut changed = world.affiliations().clone();
    let data = changed.0.get_mut("choice").unwrap();
    data.absent_members.remove(&EntityId(0));
    data.members.insert(EntityId(0), Some("first".into()));
    world.set_affiliations(changed).unwrap();
    assert!(present.evaluate(&world, EntityId(0)).unwrap());
    assert!(group.evaluate(&world, EntityId(0)).unwrap());
}

#[test]
fn contradictory_or_invalid_absence_is_rejected_atomically_and_old_nulls_stay_unknown() {
    let mut world = World::new(Schema::new(["value"]).unwrap());
    world.add_entity("Country", EntityKind::Country);
    let before = world.save("presence").unwrap();
    for members in [r#"{"0":"first"}"#, r#"{"0":null}"#] {
        let invalid: Affiliations = serde_json::from_str(&format!(r#"{{"choice":{{"groups":{{}},"members":{members},"absent_members":[0],"acceptance":{{}}}}}}"#)).unwrap();
        assert!(world.set_affiliations(invalid.clone()).is_err());
        assert_eq!(world.save("presence").unwrap(), before);
        let mut saved: serde_json::Value = serde_json::from_str(&before).unwrap();
        saved["affiliations"] = serde_json::to_value(invalid).unwrap();
        assert!(World::restore(&saved.to_string(), "presence", world.schema()).is_err());
    }
    let invalid = serde_json::from_str(
        r#"{"choice":{"groups":{},"members":{},"absent_members":[99],"acceptance":{}}}"#,
    )
    .unwrap();
    assert!(world.set_affiliations(invalid).is_err());
    world
        .set_affiliations(
            serde_json::from_str(
                r#"{"choice":{"groups":{},"members":{"0":null},"acceptance":{}}}"#,
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(world.affiliations().present("choice", EntityId(0)), None);
    assert!(!world.save("presence").unwrap().contains("absent_members"));
}
