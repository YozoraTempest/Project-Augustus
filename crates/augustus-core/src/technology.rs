// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use crate::{EntityId, EntityKind, Error, World};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// A level's declarations, without implying how each named property is applied.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TechnologyLevel {
    pub year: i16,
    pub values_micros: BTreeMap<String, i64>,
    pub flags: BTreeMap<String, bool>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub enabled_units: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub unresolved_properties: BTreeSet<String>,
}

/// Result of looking up a named property on a country's current technology.
///
/// `Absent` means the parsed level is known and does not declare the property.
/// `Unknown` means that the country, branch, level, or property could not be
/// resolved without making up a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TechnologyProperty {
    Unknown,
    Absent,
    Value(i64),
    Flag(bool),
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

    pub fn country_level(&self, country: EntityId, branch: &str) -> Option<u16> {
        self.countries
            .get(&country)
            .and_then(|branches| branches.get(branch).copied())
    }

    pub fn country_property(
        &self,
        country: EntityId,
        branch: &str,
        property: &str,
    ) -> TechnologyProperty {
        if property.is_empty() {
            return TechnologyProperty::Unknown;
        }
        let Some(level) = self.country_level(country, branch) else {
            return TechnologyProperty::Unknown;
        };
        let Some(level) = self
            .tables
            .get(branch)
            .and_then(|levels| levels.get(usize::from(level)))
        else {
            return TechnologyProperty::Unknown;
        };
        if level.unresolved_properties.contains(property) {
            return TechnologyProperty::Unknown;
        }
        if let Some(value) = level.values_micros.get(property) {
            return TechnologyProperty::Value(*value);
        }
        if let Some(flag) = level.flags.get(property) {
            return TechnologyProperty::Flag(*flag);
        }
        TechnologyProperty::Absent
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
                    || level.unresolved_properties.iter().any(String::is_empty)
                    || level.enabled_units.iter().any(|unit| {
                        unit.trim().is_empty()
                            || unit.bytes().any(|byte| byte.is_ascii_whitespace())
                    })
                    || level
                        .values_micros
                        .keys()
                        .any(|k| level.flags.contains_key(k))
                    || level
                        .values_micros
                        .keys()
                        .chain(level.flags.keys())
                        .any(|k| level.unresolved_properties.contains(k))
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
