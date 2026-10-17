// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{EntityId, EntityKind, Error, World};

/// Country opinions indexed by the religious school they address.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SchoolOpinions(pub BTreeMap<EntityId, BTreeMap<String, i64>>);

impl SchoolOpinions {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub(crate) fn validate(&self, world: &World) -> Result<(), Error> {
        if self.0.is_empty() {
            return Ok(());
        }
        let schools = world
            .affiliations()
            .0
            .get("religious_school")
            .ok_or_else(|| Error::Invalid("religious school dimension is missing".into()))?;
        for (&country, opinions) in &self.0 {
            if world.entity(country)?.kind != EntityKind::Country {
                return Err(Error::Invalid(
                    "school opinion owner must be a country".into(),
                ));
            }
            if opinions
                .keys()
                .any(|school| school.is_empty() || !schools.groups.contains_key(school))
            {
                return Err(Error::Invalid(
                    "unknown religious school opinion key".into(),
                ));
            }
        }
        Ok(())
    }
}

impl World {
    pub fn school_opinions(&self) -> &SchoolOpinions {
        &self.school_opinions
    }

    pub fn school_opinion(&self, country: EntityId, school: &str) -> Result<i64, Error> {
        if self.entity(country)?.kind != EntityKind::Country {
            return Err(Error::Invalid(
                "school opinion owner must be a country".into(),
            ));
        }
        let schools = self
            .affiliations()
            .0
            .get("religious_school")
            .ok_or_else(|| Error::Invalid("religious school dimension is missing".into()))?;
        if !schools.groups.contains_key(school) {
            return Err(Error::Invalid(format!("unknown religious school {school}")));
        }
        Ok(self
            .school_opinions
            .0
            .get(&country)
            .and_then(|opinions| opinions.get(school))
            .copied()
            .unwrap_or_default())
    }

    pub fn set_school_opinion(
        &mut self,
        country: EntityId,
        school: &str,
        opinion: i64,
    ) -> Result<(), Error> {
        self.require_running()?;
        if self.entity(country)?.kind != EntityKind::Country {
            return Err(Error::Invalid(
                "school opinion owner must be a country".into(),
            ));
        }
        let schools = self
            .affiliations()
            .0
            .get("religious_school")
            .ok_or_else(|| Error::Invalid("religious school dimension is missing".into()))?;
        if !schools.groups.contains_key(school) {
            return Err(Error::Invalid(format!("unknown religious school {school}")));
        }
        let opinions = self.school_opinions.0.entry(country).or_default();
        if opinion == 0 {
            opinions.remove(school);
            if opinions.is_empty() {
                self.school_opinions.0.remove(&country);
            }
        } else {
            opinions.insert(school.into(), opinion);
        }
        Ok(())
    }
}
