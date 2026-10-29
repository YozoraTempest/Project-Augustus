// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::BTreeMap;

use crate::{EntityId, EntityKind, Error, World};

/// A previous-controller entry distinguishes known absence from an unknown
/// history value: a missing key is unknown, while a present key with `None`
/// records that no previous controller is known for the province.
pub type ProvincePreviousControllerStates = BTreeMap<EntityId, Option<EntityId>>;

pub(crate) fn validate(
    states: &ProvincePreviousControllerStates,
    world: &World,
) -> Result<(), Error> {
    for (&province, controller) in states {
        if world.entity(province)?.kind != EntityKind::Province {
            return Err(Error::Invalid(
                "province previous controller state requires a province".into(),
            ));
        }
        if let Some(controller) = controller
            && world.entity(*controller)?.kind != EntityKind::Country
        {
            return Err(Error::Invalid(
                "province previous controller state requires a country".into(),
            ));
        }
    }
    Ok(())
}

impl World {
    /// Returns `None` for unknown history, `Some(None)` for known absence, and
    /// `Some(Some(country))` for a known previous controller.
    pub fn province_previous_controller_state(
        &self,
        province: EntityId,
    ) -> Result<Option<Option<EntityId>>, Error> {
        if self.entity(province)?.kind != EntityKind::Province {
            return Err(Error::Invalid(
                "province previous controller state requires a province".into(),
            ));
        }
        Ok(self.province_previous_controllers.get(&province).copied())
    }

    /// Stores an explicit previous-controller state. `None` is a known
    /// absence; remove the entry through `clear_province_previous_controller_state`
    /// to restore unknown history.
    pub fn set_province_previous_controller(
        &mut self,
        province: EntityId,
        controller: Option<EntityId>,
    ) -> Result<(), Error> {
        if self.entity(province)?.kind != EntityKind::Province {
            return Err(Error::Invalid(
                "province previous controller state requires a province".into(),
            ));
        }
        if let Some(controller) = controller
            && self.entity(controller)?.kind != EntityKind::Country
        {
            return Err(Error::Invalid(
                "province previous controller state requires a country".into(),
            ));
        }
        self.province_previous_controllers
            .insert(province, controller);
        Ok(())
    }

    pub fn clear_province_previous_controller_state(
        &mut self,
        province: EntityId,
    ) -> Result<(), Error> {
        if self.entity(province)?.kind != EntityKind::Province {
            return Err(Error::Invalid(
                "province previous controller state requires a province".into(),
            ));
        }
        self.province_previous_controllers.remove(&province);
        Ok(())
    }
}
