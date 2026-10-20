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
    /// Script-supplied multiplier retained for later maintenance-cost rules.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_multiplier_milli: Option<i64>,
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
    /// Category slots updated independently when the rest of a country's
    /// advisor roster is not known.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub category_slots: BTreeMap<EntityId, BTreeMap<RulerSkill, Option<AdvisorState>>>,
}

impl Advisors {
    pub(crate) fn is_empty(&self) -> bool {
        self.countries.is_empty() && self.category_slots.is_empty()
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
        for (&country, slots) in &self.category_slots {
            if world.entity(country)?.kind != EntityKind::Country {
                return Err(Error::Invalid(
                    "advisor slots require country entities".into(),
                ));
            }
            for (&category, advisor) in slots {
                if advisor
                    .as_ref()
                    .is_some_and(|advisor| advisor.category != category)
                {
                    return Err(Error::Invalid(
                        "advisor slot category does not match its advisor".into(),
                    ));
                }
                if let Some(Some(roster)) = self.countries.get(&country) {
                    let roster_advisor = roster.iter().find(|advisor| advisor.category == category);
                    if roster_advisor != advisor.as_ref() {
                        return Err(Error::Invalid(
                            "advisor slot conflicts with the known country roster".into(),
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    /// Returns the known advisor in a category, `Some(None)` for a known empty
    /// slot, or `None` when that slot has not been observed or updated.
    pub fn advisor_in_category(
        &self,
        country: EntityId,
        category: RulerSkill,
    ) -> Option<Option<&AdvisorState>> {
        if let Some(advisor) = self
            .category_slots
            .get(&country)
            .and_then(|slots| slots.get(&category))
        {
            return Some(advisor.as_ref());
        }
        self.countries.get(&country).map(|state| {
            state
                .as_ref()
                .and_then(|advisors| advisors.iter().find(|advisor| advisor.category == category))
        })
    }

    pub(crate) fn set_category_slot(
        &mut self,
        country: EntityId,
        category: RulerSkill,
        advisor: Option<AdvisorState>,
    ) {
        let roster_known = self.countries.get(&country).is_some_and(Option::is_some);
        if let Some(Some(roster)) = self.countries.get_mut(&country) {
            roster.retain(|current| current.category != category);
            if let Some(advisor) = advisor.clone() {
                roster.push(advisor);
            }
        }
        if roster_known {
            let mut remove_country_slots = false;
            if let Some(slots) = self.category_slots.get_mut(&country) {
                slots.remove(&category);
                remove_country_slots = slots.is_empty();
            }
            if remove_country_slots {
                self.category_slots.remove(&country);
            }
        } else {
            self.category_slots
                .entry(country)
                .or_default()
                .insert(category, advisor);
        }
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
        self.advisors.set_category_slot(entity, category, None);
        Ok(())
    }

    pub fn hire_advisor(&mut self, entity: EntityId, advisor: AdvisorState) -> Result<(), Error> {
        self.require_running()?;
        if self.entity(entity)?.kind != EntityKind::Country {
            return Err(Error::Invalid(
                "advisor hiring requires country scope".into(),
            ));
        }
        advisor.validate()?;
        self.advisors
            .set_category_slot(entity, advisor.category, Some(advisor));
        Ok(())
    }
}
