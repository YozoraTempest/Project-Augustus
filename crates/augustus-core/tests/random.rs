use augustus_core::{InitialWorld, Schema, World};

#[test]
fn eu4_world_snapshot_restores_seed_and_stream_position() {
    let schema = Schema::new(["unused"]).unwrap();
    let mut world = World::eu4_seeded(schema.clone(), Vec::new(), 8_351_375);
    for _ in 0..623 {
        world.draw_random_u32();
    }

    let state_before_save = world.eu4_random_state();
    let snapshot = world.save("rng-content").unwrap();
    let value: serde_json::Value = serde_json::from_str(&snapshot).unwrap();
    assert_eq!(value["format"], 2);
    assert!(value.get("eu4_random").is_some());
    assert!(value.get("random").is_none());
    assert_eq!(value["eu4_event_seed"], 8_351_375);

    let mut restored = World::restore(&snapshot, "rng-content", &schema).unwrap();
    assert_eq!(restored.eu4_random_state(), state_before_save);
    assert_eq!(restored.eu4_event_seed(), Some(8_351_375));
    for _ in 0..1_000 {
        assert_eq!(world.draw_random_u32(), restored.draw_random_u32());
    }
}

#[test]
fn generic_world_snapshots_keep_the_existing_random_format() {
    let schema = Schema::new(["unused"]).unwrap();
    let mut world = World::seeded(schema.clone(), Vec::new(), [9; 32]);
    world.draw_random_u32();
    let snapshot = world.save("generic-content").unwrap();
    let value: serde_json::Value = serde_json::from_str(&snapshot).unwrap();
    assert_eq!(value["format"], 1);
    assert!(value.get("random").is_some());
    assert!(value.get("eu4_random").is_none());
    assert!(value.get("eu4_event_seed").is_none());

    let mut restored = World::restore(&snapshot, "generic-content", &schema).unwrap();
    assert_eq!(world.draw_random_u32(), restored.draw_random_u32());
}

#[test]
fn eu4_world_rebuilds_a_saved_seed_and_count() {
    let schema = Schema::new(["unused"]).unwrap();
    let mut world = World::eu4_seeded_from_saved_count(schema, Vec::new(), 8_351_375, 1_001);
    assert_eq!(world.eu4_random_state(), Some((8_351_375, 1_001, 377)));
    assert_eq!(world.draw_random_u32(), 312_438_070);
    assert_eq!(world.eu4_random_state(), Some((8_351_375, 1_002, 378)));
}

#[test]
fn initial_world_can_restore_a_saved_eu4_random_state() {
    let mut initial = InitialWorld::new(Vec::new(), Vec::new()).unwrap();
    initial.set_eu4_random_state(8_351_375, 1_001);

    assert_eq!(
        initial.world().eu4_random_state(),
        Some((8_351_375, 1_001, 377))
    );
    assert_eq!(initial.world().eu4_event_seed(), Some(8_351_375));
}

#[test]
fn older_eu4_snapshots_without_an_event_seed_restore_as_unfixed() {
    let schema = Schema::new(["unused"]).unwrap();
    let world = World::eu4_seeded(schema.clone(), Vec::new(), 8_351_375);
    let mut snapshot: serde_json::Value =
        serde_json::from_str(&world.save("old-eu4-content").unwrap()).unwrap();
    snapshot.as_object_mut().unwrap().remove("eu4_event_seed");

    let restored = World::restore(
        &serde_json::to_string(&snapshot).unwrap(),
        "old-eu4-content",
        &schema,
    )
    .unwrap();
    assert_eq!(restored.eu4_event_seed(), None);
}

#[test]
fn eu4_world_exposes_the_native_batch_and_forbidden_paths() {
    let schema = Schema::new(["unused"]).unwrap();
    let mut world = World::eu4_seeded(schema, Vec::new(), 8_351_375);
    let batch = world.fill_eu4_random_batch(false).unwrap();
    assert_eq!(
        &batch[..10],
        &[
            2_131_175_983,
            1_432_086_561,
            1_519_919_863,
            1_488_729_639,
            674_643_279,
            856_080_159,
            1_743_027_350,
            1_693_715_639,
            1_062_965_195,
            339_709_696,
        ]
    );
    assert_eq!(world.eu4_random_state(), Some((8_351_375, 1_000, 376)));

    let state_before_forbidden = world.eu4_random_state();
    let forbidden = world.fill_eu4_random_batch(true).unwrap();
    assert!(forbidden.iter().all(|value| *value == 0));
    assert_eq!(world.eu4_random_state(), state_before_forbidden);
}
