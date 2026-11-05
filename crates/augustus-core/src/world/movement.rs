// SPDX-License-Identifier: GPL-3.0-only

use std::collections::BTreeMap;

use pathfinding::prelude::dijkstra;

use crate::{
    Army, ArmyMovement, GameDate, LandConnection, MovementError, MovementLeg, ProvinceKind,
};

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
        let army = self.commanded_army(country, id)?;
        if army.movement.is_none() && army.province == destination {
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
        // 改道从当前路段的终点开始，原路段的出发与抵达日期保持不变。
        let (from, mut started_on) =
            army.movement
                .as_ref()
                .map_or((army.province, date), |movement| {
                    let current = movement.current_leg();
                    (current.to, current.arrives_on)
                });
        // 先计算完整报价，失败时不改变已经下达的命令。
        let (path, _) = dijkstra(
            &from,
            |id| {
                self.land_neighbors(*id)
                    .into_iter()
                    .flatten()
                    .filter(|(next, _)| self.can_enter(country, **next))
                    .map(|(&next, &days)| (next, u64::from(days)))
            },
            |&id| id == destination,
        )
        .ok_or(MovementError::NoRoute)?;
        let mut legs = army.movement.as_ref().map_or_else(Vec::new, |movement| {
            movement.legs[..=movement.leg_index].to_vec()
        });
        let leg_index = army.movement.as_ref().map_or(0, ArmyMovement::leg_index);
        legs.reserve(path.len() - 1);
        for pair in path.windows(2) {
            let (from, to) = (pair[0], pair[1]);
            let arrives_on = started_on
                .checked_add_days(self.land_routes[&from][&to])
                .ok_or(MovementError::DateOutOfRange)?;
            legs.push(MovementLeg {
                from,
                to,
                started_on,
                arrives_on,
            });
            started_on = arrives_on;
        }
        Ok(ArmyMovement { legs, leg_index })
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

    /// 取消后续路段；军队继续完成当前路段，抵达后按正常流程驻扎。
    pub fn stop_army(&mut self, country: &str, id: u32) -> Result<(), MovementError> {
        self.commanded_army(country, id)?;
        let movement = self
            .military
            .armies
            .get_mut(&id)
            .expect("已确认可指挥的军队存在")
            .movement
            .as_mut()
            .ok_or(MovementError::NotMoving)?;
        movement.legs.truncate(movement.leg_index + 1);
        Ok(())
    }

    pub(crate) fn complete_movements(&mut self, date: GameDate) {
        for army in self.military.armies.values_mut() {
            while army
                .movement
                .as_ref()
                .is_some_and(|movement| movement.current_leg().arrives_on <= date)
            {
                let movement = army.movement.as_mut().expect("行军已到期");
                army.province = movement.current_leg().to;
                if movement.leg_index + 1 == movement.legs.len() {
                    // 最终抵达才结束命令；途经省份不驻留，也不合并其他军队。
                    army.movement = None;
                } else {
                    movement.leg_index += 1;
                }
            }
        }
    }

    fn can_enter(&self, country: &str, province: u32) -> bool {
        self.active_provinces.contains(&province)
            && self.province(province).is_some_and(|province| {
                province.kind == ProvinceKind::Land
                    && province.owner.as_deref() == Some(country)
                    && province.controller.as_deref() == Some(country)
            })
    }

    fn commanded_army(&self, country: &str, id: u32) -> Result<&Army, MovementError> {
        self.country(country).ok_or(MovementError::UnknownCountry)?;
        let army = self.armies().get(&id).ok_or(MovementError::UnknownArmy)?;
        if army.country != country {
            return Err(MovementError::NotPlayerArmy);
        }
        Ok(army)
    }

    pub(super) fn valid_movement(
        &self,
        movement: &ArmyMovement,
        province: u32,
        country: &str,
        date: GameDate,
    ) -> bool {
        // 先检查游标，避免把不完整或错位的存档恢复为可执行命令。
        let Some(current) = movement.legs.get(movement.leg_index) else {
            return false;
        };
        current.from == province
            && current.started_on <= date
            && date < current.arrives_on
            && movement.legs.iter().all(|leg| {
                leg.started_on < leg.arrives_on
                    && self
                        .land_neighbors(leg.from)
                        .is_some_and(|neighbors| neighbors.contains_key(&leg.to))
                    && self.can_enter(country, leg.to)
            })
            && movement
                .legs
                .windows(2)
                .all(|pair| pair[0].to == pair[1].from && pair[0].arrives_on == pair[1].started_on)
    }
}
