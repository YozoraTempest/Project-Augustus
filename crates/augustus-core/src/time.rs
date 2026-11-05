// SPDX-License-Identifier: GPL-3.0-only

use std::{fmt, time::Duration};

use jomini::common::{Date, PdsDate};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

/// 游戏使用每年 365 天的历法；日期运算复用 jomini，不依赖显示引擎。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct GameDate(Date);

impl GameDate {
    pub fn new(year: i16, month: u8, day: u8) -> Option<Self> {
        if year < 1 {
            return None;
        }
        Date::from_ymd_opt(year, month, day).map(Self)
    }

    pub fn year(self) -> i16 {
        self.0.year()
    }

    pub fn month(self) -> u8 {
        self.0.month()
    }

    pub fn day(self) -> u8 {
        self.0.day()
    }

    pub fn days_until(self, other: Self) -> i32 {
        self.0.days_until(&other.0)
    }

    pub fn checked_add_days(self, days: u32) -> Option<Self> {
        let remaining = self.0.days_until(&Date::from_ymd(i16::MAX, 12, 31));
        (i64::from(days) <= i64::from(remaining)).then(|| self.add_days(days as i32))
    }

    /// 工期按日历月计算；目标月份没有同一天时，使用该月最后一天。
    pub fn checked_add_months(self, months: u32) -> Option<Self> {
        let index = u32::try_from(self.year()).ok()? * 12 + u32::from(self.month() - 1);
        let index = index.checked_add(months)?;
        let year = i16::try_from(index / 12).ok()?;
        let month = (index % 12 + 1) as u8;
        (1..=self.day())
            .rev()
            .find_map(|day| Self::new(year, month, day))
    }

    pub(crate) fn add_days(self, days: i32) -> Self {
        Self(self.0.add_days(days))
    }
}

impl fmt::Display for GameDate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.game_fmt().fmt(formatter)
    }
}

// 日期以游戏中使用的年月日文本保存，读取时仍经过历法检查。
impl Serialize for GameDate {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for GameDate {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        let date = Date::parse(text.as_bytes())
            .ok()
            .and_then(|date| Self::new(date.year(), date.month(), date.day()));
        date.ok_or_else(|| de::Error::custom("无效的游戏日期"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameSpeed {
    Normal,
    Double,
    Quadruple,
    Octuple,
}

impl GameSpeed {
    pub fn multiplier(self) -> u32 {
        match self {
            Self::Normal => 1,
            Self::Double => 2,
            Self::Quadruple => 4,
            Self::Octuple => 8,
        }
    }

    pub fn from_multiplier(multiplier: u32) -> Option<Self> {
        match multiplier {
            1 => Some(Self::Normal),
            2 => Some(Self::Double),
            4 => Some(Self::Quadruple),
            8 => Some(Self::Octuple),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub struct GameClock {
    date: GameDate,
    paused: bool,
    speed: GameSpeed,
    // 保留不足一天的进度，暂停、恢复和切换倍速都不会清零。
    progress: Duration,
}

impl GameClock {
    pub fn new(date: GameDate) -> Self {
        Self {
            date,
            paused: true,
            speed: GameSpeed::Normal,
            progress: Duration::ZERO,
        }
    }

    pub fn date(&self) -> GameDate {
        self.date
    }

    pub fn is_paused(&self) -> bool {
        self.paused
    }

    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused || self.date.0 == Self::last_date();
    }

    pub fn speed(&self) -> GameSpeed {
        self.speed
    }

    pub(crate) fn progress(&self) -> Duration {
        self.progress
    }

    pub(crate) fn restore(date: GameDate, speed: GameSpeed, progress: Duration) -> Self {
        Self {
            date,
            speed,
            progress,
            // 读取后先暂停，现实中关闭游戏的时间不计入模拟。
            paused: true,
        }
    }

    pub fn set_speed(&mut self, speed: GameSpeed) {
        self.speed = speed;
    }

    /// 1× 时每个现实秒推进一天；一次调用可跨过多天，不依赖渲染帧数。
    pub fn advance(&mut self, elapsed: Duration) -> u32 {
        self.advance_with(elapsed, |_| {})
    }

    pub(crate) fn advance_with(
        &mut self,
        elapsed: Duration,
        mut on_day: impl FnMut(GameDate),
    ) -> u32 {
        if self.paused {
            return 0;
        }
        self.progress += elapsed * self.speed.multiplier();
        let remaining = self.date.0.days_until(&Self::last_date()) as u64;
        let days = self.progress.as_secs().min(remaining) as u32;
        // 逐日交给模拟层处理，单帧跨月或跨年时也不会遗漏任何月初。
        for _ in 0..days {
            self.date = self.date.add_days(1);
            on_day(self.date);
        }
        self.progress = Duration::from_nanos(u64::from(self.progress.subsec_nanos()));
        // 到达日期库支持的末日后停止，不能让日期溢出或倒退。
        if u64::from(days) == remaining {
            self.paused = true;
            self.progress = Duration::ZERO;
        }
        days
    }

    fn last_date() -> Date {
        Date::from_ymd(i16::MAX, 12, 31)
    }
}
