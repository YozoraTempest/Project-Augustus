// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use crate::{EntityId, EntityKind, Error, World};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A level's declarations, without implying how each named property is applied.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TechnologyLevel {
    pub year: i16,
    pub values_micros: BTreeMap<String, i64>,
    pub flags: BTreeMap<String, bool>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Technology {
    pub tables: BTreeMap<String, Vec<TechnologyLevel>>,
    pub countries: BTreeMap<EntityId, BTreeMap<String, u16>>,
}

impl Technology {
    pub fn is_empty(&self) -> bool {
        self.tables.is_empty() && self.countries.is_empty()
    }

    pub(crate) fn validate(&self, world: &World) -> Result<(), Error> {
        for (name, levels) in &self.tables {
            if name.is_empty() || levels.is_empty() || levels.len() > usize::from(u16::MAX) + 1 {
                return Err(Error::Invalid("invalid technology table".into()));
            }
            for level in levels {
                if level
                    .values_micros
                    .keys()
                    .chain(level.flags.keys())
                    .any(|k| k.is_empty())
                    || level
                        .values_micros
                        .keys()
                        .any(|k| level.flags.contains_key(k))
                {
                    return Err(Error::Invalid(
                        "invalid technology property identifier".into(),
                    ));
                }
            }
        }
        for (&country, levels) in &self.countries {
            if world.entity(country)?.kind != EntityKind::Country {
                return Err(Error::Invalid(
                    "technology requires a country entity".into(),
                ));
            }
            for (branch, &level) in levels {
                if !self
                    .tables
                    .get(branch)
                    .is_some_and(|t| usize::from(level) < t.len())
                {
                    return Err(Error::Invalid("unknown technology branch or level".into()));
                }
            }
        }
        Ok(())
    }
}
