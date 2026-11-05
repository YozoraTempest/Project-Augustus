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
                world.settle_monthly_taxes(date);
            }
            // 月初先支付此前月份的税收，再完成当天建设，避免新建筑追溯增加上月收入。
            world.complete_constructions(date);
        })
    }
}
