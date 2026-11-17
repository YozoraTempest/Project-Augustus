// SPDX-License-Identifier: GPL-3.0-only

use std::collections::{BTreeMap, BTreeSet};

use pathfinding::prelude::{dijkstra, dijkstra_all};

use crate::{
    Army, ArmyMovement, GameDate, LandConnection, MilitaryRules, Money, MovementError, MovementLeg,
    ProvinceKind,
};

use super::World;

/// 行军报价沿用的已走路段，以及是否按流放返回计算可通行省份。
/// `blocked` 中的省不能进入。集结在到齐前用它避开已经有敌军的省。
pub(crate) struct RouteRequest<'a> {
    pub kept: Option<&'a ArmyMovement>,
    pub exiled: bool,
    pub blocked: &'a BTreeSet<u32>,
}

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
        self.quote_movement_avoiding(country, id, destination, date, &BTreeSet::new())
    }

    pub(crate) fn quote_movement_avoiding(
        &self,
        country: &str,
        id: u32,
        destination: u32,
        date: GameDate,
        blocked: &BTreeSet<u32>,
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
            RouteRequest {
                kept: army.movement.as_ref(),
                exiled: army.exiled,
                blocked,
            },
        )
    }

    /// 计算可进入陆地上的路线。不检查交战或撤退，撤退命令由此绕过玩家指令限制。
    /// 流放军队可以经过任何有主或无主的有效陆地，但目的地必须是本国拥有的省份。
    pub(crate) fn land_route(
        &self,
        country: &str,
        from: u32,
        destination: u32,
        speed: Money,
        mut started_on: GameDate,
        request: RouteRequest<'_>,
    ) -> Result<ArmyMovement, MovementError> {
        if !self.is_active_land(destination) {
            return Err(MovementError::InvalidDestination);
        }
        if request.exiled {
            if self
                .province(destination)
                .and_then(|province| province.owner.as_deref())
                != Some(country)
            {
                return Err(MovementError::ExileDestination);
            }
        } else if !self.can_enter(country, destination) {
            return Err(MovementError::NoAccess);
        }
        let (path, _) = dijkstra(
            &from,
            |id| {
                self.land_neighbors(*id)
                    .into_iter()
                    .flatten()
                    .filter(|(next, _)| {
                        !request.blocked.contains(*next)
                            && self.may_transit(country, **next, request.exiled)
                    })
                    .map(|(&next, &distance)| {
                        (next, u64::from(MilitaryRules::travel_days(distance, speed)))
                    })
            },
            |&id| id == destination,
        )
        .ok_or(MovementError::NoRoute)?;
        let mut legs = request.kept.map_or_else(Vec::new, |movement| {
            movement.legs[..=movement.leg_index].to_vec()
        });
        let leg_index = request.kept.map_or(0, ArmyMovement::leg_index);
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
        self.move_army_avoiding(country, id, destination, date, &BTreeSet::new())
    }

    /// 与 `move_army` 相同，但路线不进入 `blocked`。目的地本身若在其中，则没有路。
    pub(crate) fn move_army_avoiding(
        &mut self,
        country: &str,
        id: u32,
        destination: u32,
        date: GameDate,
        blocked: &BTreeSet<u32>,
    ) -> Result<(), MovementError> {
        let movement = self.quote_movement_avoiding(country, id, destination, date, blocked)?;
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
        if army.exiled {
            return Err(MovementError::Exiled);
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
        let exiled = army.exiled;
        let arrives_home = exiled
            && self
                .province(to)
                .and_then(|province| province.owner.as_deref())
                == Some(country.as_str());
        // 流放返回可以穿过停战后不能进入的土地，抵达本国省份后清除流放。
        if exiled {
            if !self.is_active_land(to) {
                self.clear_movement(id, false);
                return true;
            }
        } else if !self.can_enter(&country, to)
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
                if arrives_home {
                    army.exiled = false;
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
        if exiled {
            return true;
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

    /// 所有者始终可以进入自己拥有的陆地，包括被占领之后，以便行军回家并收复。
    /// 其他国家只能进入交战国拥有的陆地。中立国和无主土地不可进入。
    pub fn can_enter(&self, country: &str, province: u32) -> bool {
        let Some(province_state) = self.province(province).filter(|province_state| {
            self.active_provinces.contains(&province) && province_state.kind == ProvinceKind::Land
        }) else {
            return false;
        };
        let Some(owner) = province_state.owner.as_deref() else {
            return false;
        };
        owner == country || self.at_war(country, owner)
    }

    pub(crate) fn is_active_land(&self, province: u32) -> bool {
        self.province(province).is_some_and(|province_state| {
            self.active_provinces.contains(&province) && province_state.kind == ProvinceKind::Land
        })
    }

    fn may_transit(&self, country: &str, province: u32, exiled: bool) -> bool {
        if exiled {
            self.is_active_land(province)
        } else {
            self.can_enter(country, province)
        }
    }

    /// 沿当前军队速度可进入的陆地，给出每个省份的最短行军天数。起点为 0。
    /// 不能进入的中立国和无主土地不会出现，调用方因此也不会把它们选成目的地。
    pub(crate) fn land_travel_days(
        &self,
        country: &str,
        from: u32,
        speed: Money,
        exiled: bool,
        blocked: &BTreeSet<u32>,
    ) -> BTreeMap<u32, u32> {
        if !self.is_active_land(from) || speed <= Money::ZERO {
            return BTreeMap::new();
        }
        let mut days = BTreeMap::from([(from, 0)]);
        for (province, (_, cost)) in dijkstra_all(&from, |id| {
            self.land_neighbors(*id)
                .into_iter()
                .flatten()
                .filter(|(next, _)| {
                    !blocked.contains(*next) && self.may_transit(country, **next, exiled)
                })
                .map(|(&next, &distance)| (next, MilitaryRules::travel_days(distance, speed)))
        }) {
            days.insert(province, cost);
        }
        days
    }

    /// 从当前位置沿任意有效陆地走到最近的本国省份。已经站在本国时返回该省。
    pub(crate) fn nearest_owned_land(&self, country: &str, from: u32) -> Option<u32> {
        if !self.is_active_land(from) {
            return None;
        }
        let (path, _) = dijkstra(
            &from,
            |id| {
                self.land_neighbors(*id)
                    .into_iter()
                    .flatten()
                    .filter(|(next, _)| self.is_active_land(**next))
                    .map(|(&next, &distance)| (next, u64::from(distance)))
            },
            |id| {
                self.province(*id)
                    .and_then(|province| province.owner.as_deref())
                    == Some(country)
            },
        )?;
        path.last().copied()
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
        exiled: bool,
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
                    && self.may_transit(country, leg.to, exiled)
            })
            && (!exiled
                || movement.legs.last().is_some_and(|leg| {
                    self.province(leg.to)
                        .and_then(|province| province.owner.as_deref())
                        == Some(country)
                }))
            && movement
                .legs
                .windows(2)
                .all(|pair| pair[0].to == pair[1].from && pair[0].arrives_on == pair[1].started_on)
    }
}
