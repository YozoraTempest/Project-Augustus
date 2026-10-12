// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use serde::{Deserialize, Serialize};

use crate::{EntityId, EntityKind, Error, World};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CountryController {
    Human,
    Ai,
}

impl World {
    /// An absent controller means that the session has not supplied this fact.
    pub fn country_controller(
        &self,
        country: EntityId,
    ) -> Result<Option<CountryController>, Error> {
        if self.entity(country)?.kind != EntityKind::Country {
            return Err(Error::Invalid(
                "country control requires a country entity".into(),
            ));
        }
        Ok(self.country_controllers.get(&country).copied())
    }

    pub fn country_controllers(&self) -> &std::collections::BTreeMap<EntityId, CountryController> {
        &self.country_controllers
    }

    /// Update control without changing the country's state or pending event identities.
    pub fn set_country_controller(
        &mut self,
        country: EntityId,
        controller: CountryController,
    ) -> Result<(), Error> {
        self.require_running()?;
        self.country_controller(country)?;
        self.country_controllers.insert(country, controller);
        Ok(())
    }
}
