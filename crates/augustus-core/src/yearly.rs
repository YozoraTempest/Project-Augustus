// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use crate::{DailyEventCountry, DailyEventRules, Error, World};

/// A replaceable yearly simulation phase.
///
/// The provider owns any ruleset-specific state in [`World`]. The selected
/// provider identifier is saved with the session, so a resumed campaign must
/// supply the same provider before the next year boundary.
pub trait YearlySystem {
    fn id(&self) -> &str;
    fn validate(&self, world: &World) -> Result<(), Error>;
    fn settle(&self, world: &mut World) -> Result<(), Error>;
}

/// A fixed-order composition of yearly providers.
///
/// The slots mirror the confirmed yearly engine boundary: world work runs
/// before country work. Providers own their state in [`World`]; this type only
/// gives them one saved yearly-system identity and deterministic ordering.
/// It does not infer or implement EU4 yearly business rules.
pub struct YearlyPipeline<'a> {
    id: String,
    world: Option<&'a dyn YearlySystem>,
    country: Option<&'a dyn YearlySystem>,
}

impl<'a> YearlyPipeline<'a> {
    pub fn new(
        id: impl Into<String>,
        world: Option<&'a dyn YearlySystem>,
        country: Option<&'a dyn YearlySystem>,
    ) -> Result<Self, Error> {
        let pipeline = Self {
            id: id.into(),
            world,
            country,
        };
        if pipeline.id.trim().is_empty() {
            return Err(Error::Invalid("yearly pipeline requires an id".into()));
        }
        if pipeline.phases().next().is_none() {
            return Err(Error::Invalid(
                "yearly pipeline requires at least one phase".into(),
            ));
        }
        Ok(pipeline)
    }

    fn phases(&self) -> impl Iterator<Item = &'a dyn YearlySystem> + '_ {
        [self.world, self.country].into_iter().flatten()
    }
}

impl YearlySystem for YearlyPipeline<'_> {
    fn id(&self) -> &str {
        &self.id
    }

    fn validate(&self, world: &World) -> Result<(), Error> {
        for phase in self.phases() {
            phase.validate(world)?;
        }
        Ok(())
    }

    fn settle(&self, world: &mut World) -> Result<(), Error> {
        for phase in self.phases() {
            phase.settle(world)?;
        }
        Ok(())
    }
}

/// A generic yearly event provider for already-translated content.
///
/// It uses the same frozen-eligibility and weighted-candidate rules as
/// [`crate::DailyEventRules`]. The compatibility layer decides which events
/// belong to the yearly phase; this type does not infer EU4 on-actions.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct YearlyEventRules {
    pub id: String,
    pub countries: Vec<DailyEventCountry>,
}

impl YearlyEventRules {
    pub fn new(id: impl Into<String>, countries: Vec<DailyEventCountry>) -> Result<Self, Error> {
        let rules = Self {
            id: id.into(),
            countries,
        };
        if rules.id.trim().is_empty() {
            return Err(Error::Invalid("yearly event rules require an id".into()));
        }
        Ok(rules)
    }
}

impl YearlySystem for YearlyEventRules {
    fn id(&self) -> &str {
        &self.id
    }

    fn validate(&self, world: &World) -> Result<(), Error> {
        DailyEventRules {
            id: self.id.clone(),
            countries: self.countries.clone(),
        }
        .validate_for_schedule(world)
    }

    fn settle(&self, world: &mut World) -> Result<(), Error> {
        let rules = DailyEventRules {
            id: self.id.clone(),
            countries: self.countries.clone(),
        };
        let mut budget = 100_000;
        for entry in &rules.countries {
            for group in &entry.groups {
                rules.dispatch_group_for_schedule(world, entry.country, group, &mut budget)?;
            }
        }
        Ok(())
    }
}
