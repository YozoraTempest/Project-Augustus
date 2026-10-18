// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    Action, Calendar, EntityId, EntityKind, Error, MonthlySystem, Schema, World, WorldDate,
    YearlySystem,
};

struct MonthlyRecorder {
    country: EntityId,
}

impl MonthlySystem for MonthlyRecorder {
    fn id(&self) -> &str {
        "test-monthly:1"
    }

    fn validate(&self, world: &World) -> Result<(), Error> {
        if world.entity(self.country)?.kind != EntityKind::Country {
            return Err(Error::Invalid("monthly recorder requires a country".into()));
        }
        Ok(())
    }

    fn settle(&self, world: &mut World) -> Result<(), Error> {
        world.execute(
            self.country,
            &[Action::Emit(format!("monthly:{}", world.day()))],
        )
    }
}

struct YearlyRecorder {
    country: EntityId,
}

impl YearlySystem for YearlyRecorder {
    fn id(&self) -> &str {
        "test-yearly:1"
    }

    fn validate(&self, world: &World) -> Result<(), Error> {
        if world.entity(self.country)?.kind != EntityKind::Country {
            return Err(Error::Invalid("yearly recorder requires a country".into()));
        }
        Ok(())
    }

    fn settle(&self, world: &mut World) -> Result<(), Error> {
        world.execute(
            self.country,
            &[Action::Emit(format!("yearly:{}", world.day()))],
        )
    }
}

struct AlternateYearly;

impl YearlySystem for AlternateYearly {
    fn id(&self) -> &str {
        "test-yearly:other"
    }

    fn validate(&self, _: &World) -> Result<(), Error> {
        Ok(())
    }

    fn settle(&self, _: &mut World) -> Result<(), Error> {
        Ok(())
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
fn yearly_phase_runs_after_monthly_on_the_shared_year_boundary() {
    let (mut world, country) = world();
    let monthly = MonthlyRecorder { country };
    let yearly = YearlyRecorder { country };
    world.select_monthly_system(&monthly).unwrap();
    world.select_yearly_system(&yearly).unwrap();

    world
        .advance_with_systems_and_yearly(52, None, Some(&monthly), Some(&yearly))
        .unwrap();

    assert_eq!(
        world.date().unwrap(),
        Some(WorldDate {
            year: 1445,
            month: 1,
            day: 1
        })
    );
    assert_eq!(
        world
            .events()
            .iter()
            .map(|event| event.2.as_str())
            .collect::<Vec<_>>(),
        ["monthly:21", "monthly:52", "yearly:52"]
    );

    let save = world.save("yearly-test").unwrap();
    let restored = World::restore(&save, "yearly-test", world.schema()).unwrap();
    assert_eq!(restored.monthly_system_id(), Some("test-monthly:1"));
    assert_eq!(restored.yearly_system_id(), Some("test-yearly:1"));
}

#[test]
fn yearly_provider_must_match_the_selected_system() {
    let (mut world, country) = world();
    let selected = YearlyRecorder { country };
    let other = AlternateYearly;
    world.select_yearly_system(&selected).unwrap();
    assert!(world.advance_with_yearly_system(52, &other).is_err());
    assert!(world.advance_to(52).is_err());
}
