// SPDX-License-Identifier: GPL-3.0-only

use std::collections::BTreeMap;

use pathfinding::prelude::dijkstra;

use crate::{
    Army, ArmyMovement, GameDate, LandConnection, MilitaryRules, Money, MovementError, MovementLeg,
    ProvinceKind,
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
                .map(move |(&to, &distance)| LandConnection { from, to, distance })
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
        if army.battle_id().is_some() {
            return Err(MovementError::InBattle);
        }
        if army.is_retreating() {
            return Err(MovementError::Retreating);
        }
        if army.movement.is_none() && army.province == destination {
            return Err(MovementError::SameProvince);
        }
        // 改道从当前路段的终点开始，原路段的出发与抵达日期保持不变。
        let (from, started_on) = army
            .movement
            .as_ref()
            .map_or((army.province, date), |movement| {
                let current = movement.current_leg();
                (current.to, current.arrives_on)
            });
        // 先计算完整报价，失败时不改变已经下达的命令。
        // 整支军队使用当前最慢兵种的速度；途中不会更换兵种。
        let speed = army.march_speed(&self.military_rules);
        self.land_route(
            country,
            from,
            destination,
            speed,
            started_on,
            army.movement.as_ref(),
        )
    }

    /// 计算可进入陆地上的路线。不检查交战或撤退，撤退命令由此绕过玩家指令限制。
    pub(crate) fn land_route(
        &self,
        country: &str,
        from: u32,
        destination: u32,
        speed: Money,
        mut started_on: GameDate,
        kept: Option<&ArmyMovement>,
    ) -> Result<ArmyMovement, MovementError> {
        if !self.province(destination).is_some_and(|province| {
            self.active_provinces.contains(&destination) && province.kind == ProvinceKind::Land
        }) {
            return Err(MovementError::InvalidDestination);
        }
        if !self.can_enter(country, destination) {
            return Err(MovementError::NoAccess);
        }
        let (path, _) = dijkstra(
            &from,
            |id| {
                self.land_neighbors(*id)
                    .into_iter()
                    .flatten()
                    .filter(|(next, _)| self.can_enter(country, **next))
                    .map(|(&next, &distance)| {
                        (next, u64::from(MilitaryRules::travel_days(distance, speed)))
                    })
            },
            |&id| id == destination,
        )
        .ok_or(MovementError::NoRoute)?;
        let mut legs = kept.map_or_else(Vec::new, |movement| {
            movement.legs[..=movement.leg_index].to_vec()
        });
        let leg_index = kept.map_or(0, ArmyMovement::leg_index);
        legs.reserve(path.len() - 1);
        for pair in path.windows(2) {
            let (from, to) = (pair[0], pair[1]);
            let arrives_on = started_on
                .checked_add_days(MilitaryRules::travel_days(
                    self.land_routes[&from][&to],
                    speed,
                ))
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
        let army = self.commanded_army(country, id)?;
        if army.battle_id().is_some() {
            return Err(MovementError::InBattle);
        }
        if army.is_retreating() {
            return Err(MovementError::Retreating);
        }
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

    /// 按军队编号逐支走完今天到期的路段，抵达后立刻接战。
    /// 先到的一方会锁住同省敌军，使它本次移动停止。撤退军队不接战。
    pub(crate) fn complete_movements(&mut self, date: GameDate) {
        let ids: Vec<u32> = self.military.armies.keys().copied().collect();
        for id in ids {
            while self.step_army(id, date) {}
        }
    }

    fn step_army(&mut self, id: u32, date: GameDate) -> bool {
        let Some(army) = self.military.armies.get(&id) else {
            return false;
        };
        if army.battle_id().is_some() {
            return false;
        }
        let Some(movement) = army.movement.as_ref() else {
            return false;
        };
        if movement.current_leg().arrives_on > date {
            return false;
        }
        let to = movement.current_leg().to;
        let finished = movement.leg_index + 1 == movement.legs.len();
        let country = army.country.clone();
        let province = army.province;
        let retreating = army.retreating;
        // 还没进入下一省就停住。这种停路没有额外的一次士气恢复。
        if !self.can_enter(&country, to)
            || (!retreating && self.hostile_present(&country, province))
        {
            self.clear_movement(id, retreating);
            if !retreating {
                self.engage_province(province, date, Some(id));
            }
            return true;
        }
        {
            let army = self.military.armies.get_mut(&id).expect("行军军队存在");
            army.province = to;
            if finished {
                army.movement = None;
                if retreating {
                    army.retreating = false;
                    army.retreat_shattered = Some(false);
                }
            } else {
                army.movement.as_mut().expect("行军尚未结束").leg_index += 1;
            }
        }
        // 已经走进当前省，后面还有路，但下一段不能进入：先按当前省恢复一次，再清旗。
        if retreating && !finished {
            let next = self.military.armies[&id]
                .movement
                .as_ref()
                .expect("余路还在")
                .current_leg()
                .to;
            if !self.can_enter(&country, next) && self.military.armies[&id].is_shattered() {
                self.recover_army_now(id);
                self.clear_movement(id, true);
                return true;
            }
        }
        if self.military.armies[&id].retreating {
            self.maybe_clear_shattered(id);
        }
        if !self.military.armies[&id].retreating {
            if self.hostile_present(&country, to) {
                self.clear_movement(id, false);
            }
            self.engage_province(to, date, Some(id));
        }
        true
    }

    fn clear_movement(&mut self, id: u32, clear_retreat: bool) {
        let Some(army) = self.military.armies.get_mut(&id) else {
            return;
        };
        army.movement = None;
        if clear_retreat {
            army.retreating = false;
            army.retreat_shattered = Some(false);
        }
    }

    /// 该省是否有尚未撤退、且与该国交战的军队。行军中的敌军也算在当前省份。
    pub(crate) fn hostile_present(&self, country: &str, province: u32) -> bool {
        self.military.armies.values().any(|other| {
            other.province == province && !other.retreating && self.at_war(country, &other.country)
        })
    }

    /// 本国当前控制的陆地，以及交战国拥有的陆地。中立国和无主土地不可进入。
    pub fn can_enter(&self, country: &str, province: u32) -> bool {
        let Some(province_state) = self.province(province).filter(|province_state| {
            self.active_provinces.contains(&province) && province_state.kind == ProvinceKind::Land
        }) else {
            return false;
        };
        let Some(owner) = province_state.owner.as_deref() else {
            return false;
        };
        if owner == country {
            return province_state.controller.as_deref() == Some(country);
        }
        self.at_war(country, owner)
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
