// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::{EntityId, EntityKind, Error, World};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ArmyId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RegimentId(pub u64);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Regiment {
    pub id: RegimentId,
    pub unit_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Army {
    pub id: ArmyId,
    pub owner: EntityId,
    pub location: EntityId,
    pub name: String,
    pub regiments: Vec<Regiment>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub(crate) struct Armies {
    next_army_id: u64,
    next_regiment_id: u64,
    armies: Vec<Army>,
}

impl Armies {
    pub(crate) fn is_empty(&self) -> bool {
        self.armies.is_empty()
    }

    pub(crate) fn validate_regiment_types(regiments: &[String]) -> Result<(), Error> {
        if regiments.is_empty()
            || regiments.iter().any(|unit_type| {
                unit_type.trim().is_empty()
                    || unit_type.bytes().any(|byte| byte.is_ascii_whitespace())
            })
        {
            return Err(Error::Invalid("invalid army regiment list".into()));
        }
        Ok(())
    }

    pub(crate) fn validate_categories(categories: &[String]) -> Result<(), Error> {
        if categories.is_empty()
            || categories.iter().any(|category| {
                category.trim().is_empty()
                    || category.bytes().any(|byte| byte.is_ascii_whitespace())
            })
        {
            return Err(Error::Invalid("invalid army unit categories".into()));
        }
        Ok(())
    }

    fn create(
        &mut self,
        owner: EntityId,
        location: EntityId,
        name: &str,
        unit_types: &[String],
    ) -> Result<ArmyId, Error> {
        Self::validate_regiment_types(unit_types)?;

        let id = ArmyId(self.next_army_id);
        let next_army_id = self.next_army_id.checked_add(1).ok_or(Error::Overflow)?;
        let next_regiment_id = self
            .next_regiment_id
            .checked_add(u64::try_from(unit_types.len()).map_err(|_| Error::Overflow)?)
            .ok_or(Error::Overflow)?;
        let regiments = unit_types
            .iter()
            .enumerate()
            .map(|(offset, unit_type)| Regiment {
                id: RegimentId(self.next_regiment_id + offset as u64),
                unit_type: unit_type.clone(),
            })
            .collect();

        self.armies.push(Army {
            id,
            owner,
            location,
            name: name.into(),
            regiments,
        });
        self.next_army_id = next_army_id;
        self.next_regiment_id = next_regiment_id;
        Ok(id)
    }

    pub(crate) fn validate(&self, entities: &[crate::Entity]) -> Result<(), Error> {
        let mut army_ids = BTreeSet::new();
        let mut regiment_ids = BTreeSet::new();
        for army in &self.armies {
            if !army_ids.insert(army.id) || army.id.0 >= self.next_army_id {
                return Err(Error::Invalid("invalid army identifier".into()));
            }
            let owner = entities
                .get(army.owner.0)
                .ok_or_else(|| Error::Invalid("army references unknown owner".into()))?;
            let location = entities
                .get(army.location.0)
                .ok_or_else(|| Error::Invalid("army references unknown location".into()))?;
            if owner.kind != EntityKind::Country || location.kind != EntityKind::Province {
                return Err(Error::Invalid("invalid army owner or location".into()));
            }
            if army.regiments.is_empty() {
                return Err(Error::Invalid("army has no regiments".into()));
            }
            for regiment in &army.regiments {
                if !regiment_ids.insert(regiment.id) || regiment.id.0 >= self.next_regiment_id {
                    return Err(Error::Invalid("invalid regiment identifier".into()));
                }
                Self::validate_regiment_types(std::slice::from_ref(&regiment.unit_type))?;
            }
        }
        Ok(())
    }
}

impl World {
    pub fn armies(&self) -> &[Army] {
        &self.armies.armies
    }

    pub fn create_army(
        &mut self,
        owner: EntityId,
        location: EntityId,
        name: &str,
        unit_types: &[String],
    ) -> Result<ArmyId, Error> {
        self.require_running()?;
        if self.entity(owner)?.kind != EntityKind::Country {
            return Err(Error::Invalid("army owner must be a country".into()));
        }
        if self.entity(location)?.kind != EntityKind::Province {
            return Err(Error::Invalid("army location must be a province".into()));
        }
        self.armies.create(owner, location, name, unit_types)
    }

    pub fn create_army_from_categories(
        &mut self,
        owner: EntityId,
        location: EntityId,
        name: &str,
        categories: &[String],
    ) -> Result<Option<ArmyId>, Error> {
        self.require_running()?;
        if self.entity(owner)?.kind != EntityKind::Country {
            return Err(Error::Invalid("army owner must be a country".into()));
        }
        if self.entity(location)?.kind != EntityKind::Province {
            return Err(Error::Invalid("army location must be a province".into()));
        }
        Armies::validate_categories(categories)?;

        let mut unit_types = Vec::with_capacity(categories.len());
        for category in categories {
            let Some(unit_type) = self.selected_unit(owner, category) else {
                return Ok(None);
            };
            unit_types.push(unit_type.to_owned());
        }

        self.create_army(owner, location, name, &unit_types)
            .map(Some)
    }
}
