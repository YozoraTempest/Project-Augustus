// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    Action, Calendar, EntityId, EntityKind, Error, Schema, World, WorldDate, YearlyPipeline,
    YearlySystem,
};

struct PipelinePhase {
    id: &'static str,
    country: EntityId,
    message: &'static str,
}

impl YearlySystem for PipelinePhase {
    fn id(&self) -> &str {
        self.id
    }

    fn validate(&self, world: &World) -> Result<(), Error> {
        if world.entity(self.country)?.kind != EntityKind::Country {
            return Err(Error::Invalid("yearly phase requires a country".into()));
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
fn yearly_pipeline_requires_a_phase() {
    assert!(YearlyPipeline::new("", None, None).is_err());
    assert!(YearlyPipeline::new("yearly-pipeline:test:v1", None, None).is_err());
}

#[test]
fn yearly_pipeline_runs_world_then_country_and_restores() {
    let (mut world, country) = world();
    let world_phase = PipelinePhase {
        id: "yearly-world:test:v1",
        country,
        message: "world",
    };
    let country_phase = PipelinePhase {
        id: "yearly-country:test:v1",
        country,
        message: "country",
    };
    let pipeline = YearlyPipeline::new(
        "yearly-pipeline:test:v1",
        Some(&world_phase),
        Some(&country_phase),
    )
    .unwrap();

    world.select_yearly_system(&pipeline).unwrap();
    world.advance_with_yearly_system(52, &pipeline).unwrap();
    assert_eq!(
        world
            .events()
            .iter()
            .map(|event| event.2.as_str())
            .collect::<Vec<_>>(),
        ["world", "country"]
    );

    let saved = world.save("yearly-pipeline:test:v1").unwrap();
    let mut resumed = World::restore(&saved, "yearly-pipeline:test:v1", world.schema()).unwrap();
    assert_eq!(resumed.yearly_system_id(), Some("yearly-pipeline:test:v1"));

    let mut continuous = world.clone();
    continuous
        .advance_with_yearly_system(417, &pipeline)
        .unwrap();
    resumed.advance_with_yearly_system(417, &pipeline).unwrap();
    assert_eq!(resumed.events(), continuous.events());
    assert_eq!(
        resumed.save("yearly-pipeline:test:v1").unwrap(),
        continuous.save("yearly-pipeline:test:v1").unwrap()
    );
}
