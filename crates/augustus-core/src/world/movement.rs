// SPDX-License-Identifier: GPL-3.0-only

use std::collections::BTreeMap;

use crate::{ArmyMovement, GameDate, LandConnection, MovementError, ProvinceKind};

use super::World;

impl World {
    pub fn land_neighbors(&self, province: u32) -> Option<&BTreeMap<u32, u32>> {
        self.land_routes.get(&province)
    }

    pub fn land_connections(&self) -> impl Iterator<Item = LandConnection> + '_ {
        self.land_routes.iter().flat_map(|(&from, neighbors)| {
            neighbors
                .iter()
                .filter(move |&(&to, _)| from < to)
                .map(move |(&to, &days)| LandConnection { from, to, days })
        })
    }

    pub fn quote_movement(
        &self,
        country: &str,
        id: u32,
        destination: u32,
        date: GameDate,
    ) -> Result<ArmyMovement, MovementError> {
        self.country(country).ok_or(MovementError::UnknownCountry)?;
        let army = self.armies().get(&id).ok_or(MovementError::UnknownArmy)?;
        if army.country != country {
            return Err(MovementError::NotPlayerArmy);
        }
        if army.movement.is_some() {
            return Err(MovementError::AlreadyMoving);
        }
        if army.province == destination {
            return Err(MovementError::SameProvince);
        }
        let target = self
            .province(destination)
            .filter(|province| {
                self.active_provinces.contains(&destination) && province.kind == ProvinceKind::Land
            })
            .ok_or(MovementError::InvalidDestination)?;
        if target.owner.as_deref() != Some(country) || target.controller.as_deref() != Some(country)
        {
            return Err(MovementError::NotOwnedAndControlled);
        }
        let &days = self
            .land_neighbors(army.province)
            .and_then(|neighbors| neighbors.get(&destination))
            .ok_or(MovementError::NotAdjacent)?;
        Ok(ArmyMovement {
            from: army.province,
            to: destination,
            started_on: date,
            arrives_on: date
                .checked_add_days(days)
                .ok_or(MovementError::DateOutOfRange)?,
        })
    }

    pub fn move_army(
        &mut self,
        country: &str,
        id: u32,
        destination: u32,
        date: GameDate,
    ) -> Result<(), MovementError> {
        let movement = self.quote_movement(country, id, destination, date)?;
        self.military
            .armies
            .get_mut(&id)
            .expect("移动军队已存在")
            .movement = Some(movement);
        Ok(())
    }

    pub(crate) fn complete_movements(&mut self, date: GameDate) {
        for army in self.military.armies.values_mut() {
            if army
                .movement
                .as_ref()
                .is_some_and(|movement| movement.arrives_on <= date)
            {
                let movement = army.movement.take().expect("行军已到期");
                // 抵达不合并军队，编号和下属部队保持不变。
                army.province = movement.to;
            }
        }
    }
}
