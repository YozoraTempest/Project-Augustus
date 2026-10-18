// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    Action, Calendar, EntityId, EntityKind, Error, MonthlyPipeline, MonthlySystem, Schema, World,
    WorldDate,
};

struct Recorder {
    id: &'static str,
    country: EntityId,
    message: &'static str,
}

impl MonthlySystem for Recorder {
    fn id(&self) -> &str {
        self.id
    }

    fn validate(&self, world: &World) -> Result<(), Error> {
        if world.entity(self.country)?.kind != EntityKind::Country {
            return Err(Error::Invalid("monthly recorder requires a country".into()));
        }
        Ok(())
    }

    fn settle(&self, world: &mut World) -> Result<(), Error> {
        world.execute(self.country, &[Action::Emit(self.message.into())])
    }
}

fn world() -> (World, EntityId) {
    let mut world = World::new(Schema::new([] as [&str; 0]).unwrap());
    let country = world.add_entity("AAA", EntityKind::Country);
    world
        .set_calendar(Calendar {
            start: WorldDate {
                year: 1444,
                month: 11,
                day: 11,
            },
            month_lengths: [31; 12],
        })
        .unwrap();
    (world, country)
}

#[test]
fn monthly_pipeline_runs_world_parallel_and_serial_phases_in_order() {
    let (mut world, country) = world();
    let world_phase = Recorder {
        id: "world:v1",
        country,
        message: "world",
    };
    let parallel_phase = Recorder {
        id: "parallel:v1",
        country,
        message: "parallel",
    };
    let serial_phase = Recorder {
        id: "serial:v1",
        country,
        message: "serial",
    };
    let pipeline = MonthlyPipeline::new(
        "eu4-monthly-chain:test:v1",
        Some(&world_phase),
        Some(&parallel_phase),
        Some(&serial_phase),
    )
    .unwrap();

    world.select_monthly_system(&pipeline).unwrap();
    world.advance_with_monthly_system(21, &pipeline).unwrap();
    assert_eq!(
        world
            .events()
            .iter()
            .map(|event| event.2.as_str())
            .collect::<Vec<_>>(),
        ["world", "parallel", "serial"]
    );

    let save = world.save("monthly-pipeline").unwrap();
    let restored = World::restore(&save, "monthly-pipeline", world.schema()).unwrap();
    assert_eq!(
        restored.monthly_system_id(),
        Some("eu4-monthly-chain:test:v1")
    );
}

#[test]
fn monthly_pipeline_requires_a_phase() {
    assert!(MonthlyPipeline::new("empty:v1", None, None, None).is_err());
    assert!(MonthlyPipeline::new(" ", None, None, None).is_err());
}
