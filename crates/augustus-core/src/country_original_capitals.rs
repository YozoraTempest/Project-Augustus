// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::BTreeMap;

use crate::{EntityId, EntityKind, Error, World};

/// A country original-capital entry distinguishes known absence from unknown
/// save history: a missing key is unknown, while `None` is explicit absence.
pub type CountryOriginalCapitalStates = BTreeMap<EntityId, Option<EntityId>>;

pub(crate) fn validate(states: &CountryOriginalCapitalStates, world: &World) -> Result<(), Error> {
    for (&country, capital) in states {
        if world.entity(country)?.kind != EntityKind::Country {
            return Err(Error::Invalid(
                "country original capital state requires a country".into(),
            ));
        }
        if let Some(capital) = capital
            && world.entity(*capital)?.kind != EntityKind::Province
        {
            return Err(Error::Invalid(
                "country original capital state requires a province".into(),
            ));
        }
    }
    Ok(())
}

impl World {
    /// Returns `None` for unknown history, `Some(None)` for known absence, and
    /// `Some(Some(province))` for a known original capital.
    pub fn country_original_capital_state(
        &self,
        country: EntityId,
    ) -> Result<Option<Option<EntityId>>, Error> {
        if self.entity(country)?.kind != EntityKind::Country {
            return Err(Error::Invalid(
                "country original capital state requires a country".into(),
            ));
        }
        Ok(self.country_original_capitals.get(&country).copied())
    }

    /// Stores an explicit original-capital state. `None` is known absence;
    /// remove the entry through `clear_country_original_capital_state` to
    /// restore unknown history.
    pub fn set_country_original_capital(
        &mut self,
        country: EntityId,
        capital: Option<EntityId>,
    ) -> Result<(), Error> {
        if self.entity(country)?.kind != EntityKind::Country {
            return Err(Error::Invalid(
                "country original capital state requires a country".into(),
            ));
        }
        if let Some(capital) = capital
            && self.entity(capital)?.kind != EntityKind::Province
        {
            return Err(Error::Invalid(
                "country original capital state requires a province".into(),
            ));
        }
        self.country_original_capitals.insert(country, capital);
        Ok(())
    }

    pub fn clear_country_original_capital_state(&mut self, country: EntityId) -> Result<(), Error> {
        if self.entity(country)?.kind != EntityKind::Country {
            return Err(Error::Invalid(
                "country original capital state requires a country".into(),
            ));
        }
        self.country_original_capitals.remove(&country);
        Ok(())
    }
}
