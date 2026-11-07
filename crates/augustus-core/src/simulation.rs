// SPDX-License-Identifier: GPL-3.0-only

use std::time::Duration;

use crate::{GameClock, GameDate, GameSpeed, World};

#[derive(Debug)]
pub struct Simulation {
    clock: GameClock,
}

impl Simulation {
    pub fn new(start_date: GameDate) -> Self {
        Self {
            clock: GameClock::new(start_date),
        }
    }

    pub fn clock(&self) -> &GameClock {
        &self.clock
    }

    pub(crate) fn from_clock(clock: GameClock) -> Self {
        Self { clock }
    }

    pub fn set_paused(&mut self, paused: bool) {
        self.clock.set_paused(paused);
    }

    pub fn set_speed(&mut self, speed: GameSpeed) {
        self.clock.set_speed(speed);
    }

    /// 时间与世界变化一起推进；只有真正进入新月份时才为所有国家结算一次。
    pub fn advance(&mut self, world: &mut World, elapsed: Duration) -> u32 {
        self.clock.advance_with(elapsed, |date| {
            if date.day() == 1 {
                world.settle_monthly_finances(date);
                world.recover_monthly_manpower();
            }
            // 月初先结算上月收支，再完成当天建设，避免新项目影响上月账目。
            world.complete_constructions(date);
            // 先抵达再结算补给，当天完成的招募不计入这次损耗。
            world.complete_movements(date);
            if date.day() == 1 {
                // 损耗先按抵达后的省份扣减兵力，再用本月恢复的人力补员。
                world.settle_attrition(date);
                // 交战中的军队不恢复。当天才会接战的军队先恢复再开战。
                world.recover_monthly_morale();
                world.reinforce_armies();
            }
            world.complete_recruitments(date);
            // 行军、损耗和招募落地后，同一日期只结算一次。
            world.resolve_battles(date);
            // 交战结算之后才推进占领：撤退离开的军队不再计进度，当天仍在进行的战斗也不会完成占领。
            world.tick_occupations(date);
        })
    }
}
