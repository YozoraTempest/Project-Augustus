// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use crate::{CoreType, EntityId, EntityKind, Error, World};

impl World {
    /// Read a country's core on a province independently of ownership.
    /// None denotes confirmed absence; incomplete membership remains an error.
    pub fn province_core(
        &self,
        province: EntityId,
        country: EntityId,
    ) -> Result<Option<CoreType>, Error> {
        if self.entity(province)?.kind != EntityKind::Province
            || self.entity(country)?.kind != EntityKind::Country
        {
            return Err(Error::Invalid(
                "core relation requires a province and a country".into(),
            ));
        }
        let state = self
            .administration()
            .provinces
            .get(&province)
            .ok_or_else(|| Error::Invalid("missing province administration".into()))?;
        if !state.cores_known {
            return Err(Error::Invalid("unknown province core membership".into()));
        }
        Ok(state.cores.get(&country).copied())
    }

    /// Set or remove one core in the authoritative administrative relation.
    /// This does not change ownership, country area status or raw autonomy.
    pub fn set_province_core(
        &mut self,
        province: EntityId,
        country: EntityId,
        core: Option<CoreType>,
    ) -> Result<(), Error> {
        self.require_running()?;
        if self.province_core(province, country)? == core {
            return Ok(());
        }
        let state = std::sync::Arc::make_mut(&mut self.administration)
            .provinces
            .get_mut(&province)
            .expect("validated province administration");
        match core {
            Some(core) => {
                state.cores.insert(country, core);
            }
            None => {
                state.cores.remove(&country);
            }
        }
        Ok(())
    }
}
