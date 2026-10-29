// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{EntityId, EntityKind, Error, World};

/// Caller-provided raw native migration history for one country.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeMigrationCountryState {
    pub provinces: Option<Vec<EntityId>>,
    pub migration_steps: Option<u32>,
}

/// Native migration inputs preserved as runtime state without inferred rules.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeMigrationStates {
    pub countries: BTreeMap<EntityId, NativeMigrationCountryState>,
}

impl NativeMigrationStates {
    pub fn is_empty(&self) -> bool {
        self.countries.is_empty()
    }

    pub(crate) fn validate(&self, world: &World) -> Result<(), Error> {
        for (&country, state) in &self.countries {
            if world.entity(country)?.kind != EntityKind::Country {
                return Err(Error::Invalid(
                    "native migration state requires country entities".into(),
                ));
            }
            let Some(provinces) = &state.provinces else {
                continue;
            };
            let mut seen = BTreeSet::new();
            for &province in provinces {
                if world.entity(province)?.kind != EntityKind::Province {
                    return Err(Error::Invalid(
                        "native migration province lists require province entities".into(),
                    ));
                }
                if !seen.insert(province) {
                    return Err(Error::Invalid(
                        "native migration province list contains duplicates".into(),
                    ));
                }
            }
        }
        Ok(())
    }
}

impl World {
    pub fn native_migration_states(&self) -> &NativeMigrationStates {
        &self.native_migration_states
    }

    pub fn set_native_migration_states(
        &mut self,
        states: NativeMigrationStates,
    ) -> Result<(), Error> {
        self.require_running()?;
        states.validate(self)?;
        self.native_migration_states = states;
        Ok(())
    }
}
