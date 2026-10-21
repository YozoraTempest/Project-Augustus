// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    Action, Calendar, DailyEventCountry, DailyEventGroup, DailySystem, EntityId, EntityKind, Error,
    EventCatalog, EventDefinition, EventOption, MonthlySystem, Schema, World, WorldDate,
    YearlyEventRules, YearlySystem,
};

struct DailyRecorder {
    country: EntityId,
}

impl DailySystem for DailyRecorder {
    fn id(&self) -> &str {
        "test-daily:calendar-boundary"
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
fn eu4_calendar_runs_daily_then_monthly_then_yearly_at_the_new_year_boundary() {
    let mut world = World::new(Schema::new([] as [&str; 0]).unwrap());
    let country = world.add_entity("AAA", EntityKind::Country);
    world
        .set_calendar(Calendar {
            start: WorldDate {
                year: 1444,
                month: 11,
                day: 1,
            },
            month_lengths: [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31],
        })
        .unwrap();
    let daily = DailyRecorder { country };
    let monthly = MonthlyRecorder { country };
    let yearly = YearlyRecorder { country };
    world.select_daily_system(&daily).unwrap();
    world.select_monthly_system(&monthly).unwrap();
    world.select_yearly_system(&yearly).unwrap();

    world
        .advance_with_systems_and_yearly(61, Some(&daily), Some(&monthly), Some(&yearly))
        .unwrap();

    assert_eq!(
        world.date().unwrap(),
        Some(WorldDate {
            year: 1445,
            month: 1,
            day: 1,
        })
    );
    let messages = world
        .events()
        .iter()
        .map(|event| event.2.as_str())
        .collect::<Vec<_>>();
    assert_eq!(&messages[29..31], ["daily:30", "monthly:30"]);
    assert_eq!(&messages[61..64], ["daily:61", "monthly:61", "yearly:61"]);
}

#[test]
fn split_resume_matches_continuous_run_across_month_and_year_boundaries() {
    let (mut split, country) = world();
    let monthly = MonthlyRecorder { country };
    let yearly = YearlyRecorder { country };
    split.select_monthly_system(&monthly).unwrap();
    split.select_yearly_system(&yearly).unwrap();

    split
        .advance_with_systems_and_yearly(21, None, Some(&monthly), Some(&yearly))
        .unwrap();
    let saved = split.save("yearly-resume-test").unwrap();
    let mut continuous = split.clone();
    continuous
        .advance_with_systems_and_yearly(52, None, Some(&monthly), Some(&yearly))
        .unwrap();

    let mut resumed = World::restore(&saved, "yearly-resume-test", split.schema()).unwrap();
    resumed
        .advance_with_systems_and_yearly(52, None, Some(&monthly), Some(&yearly))
        .unwrap();

    assert_eq!(resumed.date().unwrap(), continuous.date().unwrap());
    assert_eq!(resumed.events(), continuous.events());
    assert_eq!(
        resumed.save("yearly-resume-test").unwrap(),
        continuous.save("yearly-resume-test").unwrap()
    );
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

#[test]
fn yearly_event_rules_dispatch_translated_candidates() {
    let (mut world, country) = world();
    let catalog = EventCatalog::new([EventDefinition {
        id: "annual".into(),
        scope: EntityKind::Country,
        immediate: vec![Action::Emit("annual".into())],
        options: vec![EventOption {
            id: "accept".into(),
            effects: Vec::new(),
        }],
        after: Vec::new(),
    }])
    .unwrap();
    world.set_event_catalog(catalog.clone()).unwrap();

    let rules = YearlyEventRules::new(
        "yearly-events:test:v1",
        vec![DailyEventCountry {
            country,
            groups: vec![DailyEventGroup::all(["annual"])],
        }],
    )
    .unwrap();
    world.select_yearly_system(&rules).unwrap();
    world.advance_with_yearly_system(52, &rules).unwrap();

    assert_eq!(world.pending_events().len(), 1);
    assert_eq!(world.pending_events()[0].event, "annual");
    assert_eq!(
        world.events().last().map(|event| event.2.as_str()),
        Some("annual")
    );

    let save = world.save("yearly-events").unwrap();
    let restored =
        World::restore_with_events(&save, "yearly-events", world.schema(), Vec::new(), catalog)
            .unwrap();
    assert_eq!(restored.yearly_system_id(), Some("yearly-events:test:v1"));
}
