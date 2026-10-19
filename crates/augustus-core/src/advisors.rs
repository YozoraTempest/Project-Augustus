// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{EntityId, EntityKind, Error, RulerSkill, World};

/// One advisor currently available to a country.
///
/// The compatibility layer resolves the source game's advisor identifier to a
/// generic monarch-power category before this state reaches the core.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdvisorState {
    pub type_name: String,
    pub category: RulerSkill,
    pub skill: u8,
}

impl AdvisorState {
    pub(crate) fn validate(&self) -> Result<(), Error> {
        if self.type_name.is_empty()
            || self
                .type_name
                .bytes()
                .any(|byte| byte.is_ascii_whitespace())
        {
            return Err(Error::Invalid("invalid advisor type name".into()));
        }
        if !(1..=3).contains(&self.skill) {
            return Err(Error::Invalid("advisor skill outside 1..3".into()));
        }
        Ok(())
    }
}

/// Current advisor slots for each country.
///
/// A missing country is unknown. `Some(empty)` is an authoritative statement
/// that the country has no active advisors.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Advisors {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub countries: BTreeMap<EntityId, Option<Vec<AdvisorState>>>,
}

impl Advisors {
    pub(crate) fn is_empty(&self) -> bool {
        self.countries.is_empty()
    }

    pub(crate) fn validate(&self, world: &World) -> Result<(), Error> {
        for (&country, state) in &self.countries {
            if world.entity(country)?.kind != EntityKind::Country {
                return Err(Error::Invalid("advisors require country entities".into()));
            }
            if let Some(advisors) = state {
                for advisor in advisors {
                    advisor.validate()?;
                }
            }
        }
        Ok(())
    }
}

impl World {
    pub fn advisors(&self) -> &Advisors {
        &self.advisors
    }

    pub fn set_advisors(&mut self, advisors: Advisors) -> Result<(), Error> {
        self.require_running()?;
        advisors.validate(self)?;
        self.advisors = advisors;
        Ok(())
    }

    /// Remove the active advisor in a category when that country's advisor
    /// slots are known. The active advisor pool is owned by the compatibility
    /// layer and is not changed by this core operation.
    pub fn remove_advisor_by_category(
        &mut self,
        entity: EntityId,
        category: RulerSkill,
    ) -> Result<(), Error> {
        self.require_running()?;
        if self.entity(entity)?.kind != EntityKind::Country {
            return Err(Error::Invalid(
                "advisor removal requires country scope".into(),
            ));
        }
        let advisors = self
            .advisors
            .countries
            .get_mut(&entity)
            .and_then(Option::as_mut)
            .ok_or_else(|| Error::Invalid("advisor state is unknown".into()))?;
        advisors.retain(|advisor| advisor.category != category);
        Ok(())
    }
}
