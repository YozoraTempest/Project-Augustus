// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use serde::{Deserialize, Serialize};

/// Uninitialized stocks remain unknown until an initialization rule supplies them.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
#[serde(deny_unknown_fields)]
pub struct CountryResources {
    pub treasury_milli: Option<i64>,
    pub manpower_milli: Option<i64>,
    pub mercantilism_milli: Option<i64>,
}

/// A selected stock in the authoritative country resource state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum CountryResource {
    Treasury,
    Manpower,
    Mercantilism,
}

impl CountryResource {
    pub(crate) fn get(self, state: &CountryResources) -> Option<i64> {
        match self {
            Self::Treasury => state.treasury_milli,
            Self::Manpower => state.manpower_milli,
            Self::Mercantilism => state.mercantilism_milli,
        }
    }

    pub(crate) fn set(self, state: &mut CountryResources, value: i64) {
        self.set_optional(state, Some(value));
    }

    pub(crate) fn set_optional(self, state: &mut CountryResources, value: Option<i64>) {
        match self {
            Self::Treasury => state.treasury_milli = value,
            Self::Manpower => state.manpower_milli = value,
            Self::Mercantilism => state.mercantilism_milli = value,
        }
    }

    pub(crate) fn bounded(self, value: i128) -> i128 {
        match self {
            Self::Treasury => value,
            Self::Manpower | Self::Mercantilism => value.max(0),
        }
    }
}

impl crate::World {
    pub(crate) fn resource_country(&self, entity: crate::EntityId) -> Result<(), crate::Error> {
        if self.entity(entity)?.kind != crate::EntityKind::Country {
            return Err(crate::Error::Invalid(
                "country resource requires country scope".into(),
            ));
        }
        Ok(())
    }

    pub fn resource_value(
        &self,
        entity: crate::EntityId,
        resource: CountryResource,
    ) -> Result<i64, crate::Error> {
        self.resource_country(entity)?;
        self.country_resources()
            .get(&entity)
            .and_then(|state| resource.get(state))
            .ok_or_else(|| {
                crate::Error::Invalid(format!(
                    "country resource {resource:?} is unknown for entity {}",
                    entity.0
                ))
            })
    }

    pub(crate) fn write_resource_value(
        &mut self,
        entity: crate::EntityId,
        resource: CountryResource,
        value: i64,
    ) -> Result<(), crate::Error> {
        self.require_running()?;
        self.resource_country(entity)?;
        resource.set(self.country_resources.entry(entity).or_default(), value);
        Ok(())
    }
}
