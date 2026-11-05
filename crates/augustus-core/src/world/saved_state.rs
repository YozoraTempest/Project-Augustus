// SPDX-License-Identifier: GPL-3.0-only

use std::collections::{BTreeMap, BTreeSet};

use crate::{Country, GameDate, Province, ProvinceKind, SaveError};

use super::World;

impl World {
    pub(crate) fn restore_state(
        &self,
        countries: Vec<Country>,
        provinces: BTreeMap<u32, Province>,
        date: GameDate,
    ) -> Result<Self, SaveError> {
        let tags: BTreeSet<_> = countries.iter().map(Country::tag).collect();
        if tags != self.countries.keys().map(String::as_str).collect()
            || !provinces.keys().eq(self.provinces.keys())
            || provinces
                .iter()
                .any(|(id, province)| province.kind != self.provinces[id].kind)
        {
            return Err(SaveError::IncompatibleContent);
        }
        let restored = Self::new(
            countries,
            provinces,
            self.active_provinces.iter().copied(),
            self.buildings.values().cloned(),
        )?;
        for (&id, province) in &restored.provinces {
            if let Some(project) = &province.construction {
                // 项目已扣过费用，只恢复状态，不能重新执行建造命令。
                if !restored.active_provinces.contains(&id)
                    || province.kind != ProvinceKind::Land
                    || restored.country(&project.country).is_none()
                    || !restored.buildings.contains_key(&project.building)
                    || province.buildings.contains(&project.building)
                    || project.started_on > date
                    || project.completes_on <= date
                {
                    return Err(SaveError::InvalidConstruction(id));
                }
            }
        }
        Ok(restored)
    }
}
