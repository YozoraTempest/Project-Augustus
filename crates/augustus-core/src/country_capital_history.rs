// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::BTreeMap;

use crate::{EntityId, EntityKind, Error, World, WorldDate};

/// Dated country capital events. A missing country key means unknown history;
/// an empty present map means known history with no supported capital events.
pub type CountryCapitalEvents = Vec<(WorldDate, Option<EntityId>)>;
pub type CountryCapitalHistory = BTreeMap<EntityId, CountryCapitalEvents>;

pub(crate) fn validate(history: &CountryCapitalHistory, world: &World) -> Result<(), Error> {
    for (&country, entries) in history {
        if world.entity(country)?.kind != EntityKind::Country {
            return Err(Error::Invalid(
                "country capital history requires a country".into(),
            ));
        }
        for (_, capital) in entries {
            if let Some(capital) = capital
                && world.entity(*capital)?.kind != EntityKind::Province
            {
                return Err(Error::Invalid(
                    "country capital history requires a province".into(),
                ));
            }
        }
    }
    Ok(())
}

impl World {
    /// Returns None for unknown history and Some(entries) for known history.
    pub fn country_capital_history(
        &self,
        country: EntityId,
    ) -> Result<Option<&CountryCapitalEvents>, Error> {
        if self.entity(country)?.kind != EntityKind::Country {
            return Err(Error::Invalid(
                "country capital history requires a country".into(),
            ));
        }
        Ok(self.country_capital_history.get(&country))
    }

    /// Replaces a country's complete dated capital history after validating all
    /// entity references. An empty map records known empty history.
    pub fn set_country_capital_history(
        &mut self,
        country: EntityId,
        history: Vec<(WorldDate, Option<EntityId>)>,
    ) -> Result<(), Error> {
        if self.entity(country)?.kind != EntityKind::Country {
            return Err(Error::Invalid(
                "country capital history requires a country".into(),
            ));
        }
        for (_, capital) in &history {
            if let Some(capital) = capital
                && self.entity(*capital)?.kind != EntityKind::Province
            {
                return Err(Error::Invalid(
                    "country capital history requires a province".into(),
                ));
            }
        }
        self.country_capital_history.insert(country, history);
        Ok(())
    }

    pub fn clear_country_capital_history(&mut self, country: EntityId) -> Result<(), Error> {
        if self.entity(country)?.kind != EntityKind::Country {
            return Err(Error::Invalid(
                "country capital history requires a country".into(),
            ));
        }
        self.country_capital_history.remove(&country);
        Ok(())
    }
}
