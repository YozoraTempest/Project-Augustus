// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::{EntityId, EntityKind, Error, World};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct NavalUnitId(pub u64);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NavalUnit {
    pub id: NavalUnitId,
    pub owner: EntityId,
    pub location: EntityId,
    pub unit_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub(crate) struct NavalUnits {
    next_id: u64,
    units: Vec<NavalUnit>,
}

impl NavalUnits {
    pub(crate) fn is_empty(&self) -> bool {
        self.units.is_empty()
    }

    pub(crate) fn validate_type(unit_type: &str) -> Result<(), Error> {
        if unit_type.is_empty() || unit_type.bytes().any(|byte| byte.is_ascii_whitespace()) {
            return Err(Error::Invalid("invalid naval unit type".into()));
        }
        Ok(())
    }

    fn create(
        &mut self,
        owner: EntityId,
        location: EntityId,
        unit_type: &str,
    ) -> Result<NavalUnitId, Error> {
        Self::validate_type(unit_type)?;
        let next_id = self.next_id.checked_add(1).ok_or(Error::Overflow)?;
        let id = NavalUnitId(self.next_id);
        self.units.push(NavalUnit {
            id,
            owner,
            location,
            unit_type: unit_type.into(),
        });
        self.next_id = next_id;
        Ok(id)
    }

    pub(crate) fn validate(&self, entities: &[crate::Entity]) -> Result<(), Error> {
        let mut ids = BTreeSet::new();
        for unit in &self.units {
            if !ids.insert(unit.id) || unit.id.0 >= self.next_id {
                return Err(Error::Invalid("invalid naval unit identifier".into()));
            }
            Self::validate_type(&unit.unit_type)?;
            let owner = entities
                .get(unit.owner.0)
                .ok_or_else(|| Error::Invalid("naval unit references unknown owner".into()))?;
            let location = entities
                .get(unit.location.0)
                .ok_or_else(|| Error::Invalid("naval unit references unknown location".into()))?;
            if owner.kind != EntityKind::Country || location.kind != EntityKind::Province {
                return Err(Error::Invalid(
                    "invalid naval unit owner or location".into(),
                ));
            }
        }
        Ok(())
    }
}

impl World {
    pub fn naval_units(&self) -> &[NavalUnit] {
        &self.naval_units.units
    }

    pub fn create_naval_unit(
        &mut self,
        owner: EntityId,
        location: EntityId,
        unit_type: &str,
    ) -> Result<NavalUnitId, Error> {
        self.require_running()?;
        if self.entity(owner)?.kind != EntityKind::Country {
            return Err(Error::Invalid("naval unit owner must be a country".into()));
        }
        if self.entity(location)?.kind != EntityKind::Province {
            return Err(Error::Invalid(
                "naval unit location must be a province".into(),
            ));
        }
        if !self.province_has_port(location)? {
            return Err(Error::Invalid("naval unit requires a port province".into()));
        }
        self.naval_units.create(owner, location, unit_type)
    }
}
