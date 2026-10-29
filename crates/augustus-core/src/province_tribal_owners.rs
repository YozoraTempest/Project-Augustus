// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::BTreeMap;

use crate::{EntityId, EntityKind, Error, World};

/// A tribal-owner entry distinguishes known absence from an unknown relation:
/// a missing key is unknown, while a present key with `None` is explicit
/// known absence.
pub type ProvinceTribalOwnerStates = BTreeMap<EntityId, Option<EntityId>>;

pub(crate) fn validate(states: &ProvinceTribalOwnerStates, world: &World) -> Result<(), Error> {
    for (&province, owner) in states {
        if world.entity(province)?.kind != EntityKind::Province {
            return Err(Error::Invalid(
                "province tribal owner state requires a province".into(),
            ));
        }
        if let Some(owner) = owner
            && world.entity(*owner)?.kind != EntityKind::Country
        {
            return Err(Error::Invalid(
                "province tribal owner state requires a country".into(),
            ));
        }
    }
    Ok(())
}

impl World {
    /// Returns `None` for unknown history, `Some(None)` for known absence, and
    /// `Some(Some(country))` for a known tribal owner.
    pub fn province_tribal_owner_state(
        &self,
        province: EntityId,
    ) -> Result<Option<Option<EntityId>>, Error> {
        if self.entity(province)?.kind != EntityKind::Province {
            return Err(Error::Invalid(
                "province tribal owner state requires a province".into(),
            ));
        }
        Ok(self.province_tribal_owners.get(&province).copied())
    }

    /// Stores an explicit tribal-owner state. `None` is known absence; remove
    /// the entry through `clear_province_tribal_owner_state` to restore unknown.
    pub fn set_province_tribal_owner(
        &mut self,
        province: EntityId,
        owner: Option<EntityId>,
    ) -> Result<(), Error> {
        if self.entity(province)?.kind != EntityKind::Province {
            return Err(Error::Invalid(
                "province tribal owner state requires a province".into(),
            ));
        }
        if let Some(owner) = owner
            && self.entity(owner)?.kind != EntityKind::Country
        {
            return Err(Error::Invalid(
                "province tribal owner state requires a country".into(),
            ));
        }
        self.province_tribal_owners.insert(province, owner);
        Ok(())
    }

    pub fn clear_province_tribal_owner_state(&mut self, province: EntityId) -> Result<(), Error> {
        if self.entity(province)?.kind != EntityKind::Province {
            return Err(Error::Invalid(
                "province tribal owner state requires a province".into(),
            ));
        }
        self.province_tribal_owners.remove(&province);
        Ok(())
    }
}
