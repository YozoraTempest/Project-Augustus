// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    Action, Calendar, DailySystem, EntityId, EntityKind, Error, Schema, World, WorldDate,
};

struct Recorder {
    country: EntityId,
}

impl DailySystem for Recorder {
    fn id(&self) -> &str {
        "test-daily:1"
    }

    fn validate(&self, world: &World) -> Result<(), Error> {
        if world.entity(self.country)?.kind != EntityKind::Country {
            return Err(Error::Invalid("daily recorder requires a country".into()));
        }
        Ok(())
    }

    fn tick(&self, world: &mut World) -> Result<(), Error> {
        world.execute(
            self.country,
            &[Action::Emit(format!("daily:{}", world.day()))],
        )
    }
}

struct Failing;

impl DailySystem for Failing {
    fn id(&self) -> &str {
        "failing-daily:1"
    }

    fn validate(&self, _: &World) -> Result<(), Error> {
        Ok(())
    }

    fn tick(&self, _: &mut World) -> Result<(), Error> {
        Err(Error::Invalid("daily phase failed".into()))
    }
}

fn calendar() -> Calendar {
    Calendar {
        start: WorldDate {
            year: 1444,
            month: 11,
            day: 11,
        },
        month_lengths: [31; 12],
    }
}

fn world() -> (World, EntityId) {
    let mut world = World::new(Schema::new([] as [&str; 0]).unwrap());
    let country = world.add_entity("AAA", EntityKind::Country);
    world.set_calendar(calendar()).unwrap();
    (world, country)
}

#[test]
fn daily_phase_runs_before_due_tasks_and_restores_by_provider_id() {
    let (mut world, country) = world();
    let recorder = Recorder { country };
    world.select_daily_system(&recorder).unwrap();
    world
        .schedule(country, 1, vec![Action::Emit("task:1".into())])
        .unwrap();

    world.advance_with_daily_system(1, &recorder).unwrap();
    assert_eq!(world.day(), 1);
    assert_eq!(world.daily_system_id(), Some("test-daily:1"));
    assert_eq!(
        world
            .events()
            .iter()
            .map(|event| event.2.as_str())
            .collect::<Vec<_>>(),
        ["daily:1", "task:1"]
    );

    let save = world.save("daily-test").unwrap();
    let mut restored = World::restore(&save, "daily-test", world.schema()).unwrap();
    assert_eq!(restored.daily_system_id(), Some("test-daily:1"));
    assert!(restored.advance_to(2).is_err());
    restored.advance_with_daily_system(2, &recorder).unwrap();
    assert_eq!(
        restored.events().last().map(|event| event.2.as_str()),
        Some("daily:2")
    );
}

#[test]
fn daily_provider_must_match_the_selected_system() {
    let (mut world, _) = world();
    let recorder = Recorder {
        country: EntityId(0),
    };
    world.select_daily_system(&recorder).unwrap();
    assert!(world.advance_with_daily_system(1, &Failing).is_err());
}

#[test]
fn daily_provider_failure_stops_the_session() {
    let (mut world, _) = world();
    let failing = Failing;
    world.select_daily_system(&failing).unwrap();
    assert!(world.advance_with_daily_system(1, &failing).is_err());
    assert!(world.advance_with_daily_system(2, &failing).is_err());
}
