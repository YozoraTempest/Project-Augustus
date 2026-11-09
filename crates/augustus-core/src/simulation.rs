// SPDX-License-Identifier: GPL-3.0-only

use std::time::Duration;

use crate::{GameClock, GameDate, GameSpeed, WarAiDecision, World};

/// 按游戏日推进的结果。决定只供调试读取，不进入世界或存档。
#[derive(Debug)]
pub struct DayAdvance {
    pub days: u32,
    pub decisions: Vec<WarAiDecision>,
}

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
    /// 不指定玩家时不为任何国家代下命令，既有测试和工具因此保持原样。
    pub fn advance(&mut self, world: &mut World, elapsed: Duration) -> u32 {
        self.advance_for(world, elapsed, None)
    }

    /// 与 [`Self::advance`] 相同，并在每个新游戏日结算后让非玩家交战国各决策一次。
    /// 暂停不进入新日期；倍速只改变同一段现实时间跨过的日期数，不会把同一天结算两次。
    pub fn advance_for(
        &mut self,
        world: &mut World,
        elapsed: Duration,
        player: Option<&str>,
    ) -> u32 {
        self.clock.advance_with(elapsed, |date| {
            run_day(world, date, player);
        })
    }

    /// 连续推进指定数量的游戏日。暂停、倍速和日内进度都保持原样。
    /// 每日结算顺序与 [`Self::advance_for`] 相同。
    pub fn advance_days(
        &mut self,
        world: &mut World,
        days: u32,
        player: Option<&str>,
    ) -> DayAdvance {
        let mut decisions = Vec::new();
        let mut advanced = 0;
        for _ in 0..days {
            if !self.clock.step_calendar_day() {
                break;
            }
            decisions.extend(run_day(world, self.clock.date(), player));
            advanced += 1;
        }
        DayAdvance {
            days: advanced,
            decisions,
        }
    }
}

/// 进入新游戏日之后的结算。返回值是这一天非玩家交战国的决定。
fn run_day(world: &mut World, date: GameDate, player: Option<&str>) -> Vec<WarAiDecision> {
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
        world.reinforce_garrisons();
    }
    world.complete_recruitments(date);
    // 行军、损耗和招募落地后，同一日期只结算一次。
    world.resolve_battles(date);
    // 交战结算之后才推进占领：撤退离开的军队不再计进度，当天仍在进行的战斗也不会完成占领。
    world.tick_occupations(date);
    world.tick_sieges(date);
    world.direct_war_ai(player, date)
}
