// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use serde::{Deserialize, Serialize};

use crate::{Error, WorldDate};

/// A repeating twelve-month calendar. Simulation day zero is the supplied start
/// date; month lengths are content, independent of the input format.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Calendar {
    pub start: WorldDate,
    pub month_lengths: [u8; 12],
}

impl Calendar {
    fn ordinal(&self, date: WorldDate) -> Result<u64, Error> {
        if self.month_lengths.contains(&0)
            || date.year < 1
            || !(1..=12).contains(&date.month)
            || date.day == 0
            || date.day > self.month_lengths[usize::from(date.month - 1)]
        {
            return Err(Error::Invalid(
                "invalid calendar date or month length".into(),
            ));
        }
        let year_days: u64 = self.month_lengths.iter().map(|&v| u64::from(v)).sum();
        Ok(
            u64::try_from(date.year - 1).map_err(|_| Error::Overflow)? * year_days
                + self.month_lengths[..usize::from(date.month - 1)]
                    .iter()
                    .map(|&v| u64::from(v))
                    .sum::<u64>()
                + u64::from(date.day - 1),
        )
    }

    pub fn date(&self, day: u64) -> Result<WorldDate, Error> {
        let ordinal = self
            .ordinal(self.start)?
            .checked_add(day)
            .ok_or(Error::Overflow)?;
        let year_days: u64 = self.month_lengths.iter().map(|&v| u64::from(v)).sum();
        let year = i16::try_from(ordinal / year_days + 1).map_err(|_| Error::Overflow)?;
        let mut rest = ordinal % year_days;
        for (index, &length) in self.month_lengths.iter().enumerate() {
            if rest < u64::from(length) {
                return Ok(WorldDate {
                    year,
                    month: index as u8 + 1,
                    day: rest as u8 + 1,
                });
            }
            rest -= u64::from(length);
        }
        unreachable!("ordinal lies within a validated year")
    }

    /// The next boundary is strictly after the current day, including when a
    /// campaign starts on the first of a month.
    pub fn next_month(&self, day: u64) -> Result<u64, Error> {
        let date = self.date(day)?;
        day.checked_add(u64::from(self.month_lengths[usize::from(date.month - 1)] - date.day) + 1)
            .ok_or(Error::Overflow)
    }

    pub fn elapsed_months(&self, day: u64) -> Result<u64, Error> {
        let date = self.date(day)?;
        u64::try_from(
            i32::from(date.year - self.start.year) * 12 + i32::from(date.month)
                - i32::from(self.start.month),
        )
        .map_err(|_| Error::Overflow)
    }
}
