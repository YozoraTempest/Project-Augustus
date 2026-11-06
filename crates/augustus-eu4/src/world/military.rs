// SPDX-License-Identifier: GPL-3.0-only

use std::{fmt::Display, path::Path, str::FromStr};

use augustus_core::{MilitaryRules, Money};

use crate::{Error, content::ContentRoot};

pub(super) fn load(content: &ContentRoot) -> Result<MilitaryRules, Error> {
    let file = content.read("common/defines.lua")?;
    let text = String::from_utf8_lossy(&file.bytes);
    let mut infantry_cost = None;
    let mut cavalry_cost = None;
    let mut maintenance = None;
    let mut infantry_days = None;
    let mut cavalry_days = None;
    let mut refund_progress = None;
    let mut infantry_speed = None;
    let mut cavalry_speed = None;
    let mut supply_base = None;
    // 基础 defines 使用数值字面量，只读取当前招募、取消、维护费、行军和补给需要的字段。
    for line in text.lines() {
        let line = line.split("--").next().unwrap_or("");
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim().trim_end_matches(',').trim();
        match key {
            "REGAIN_COST_BEFORE_PROGRESS" => refund_progress = Some(parse(value, &file.path, key)?),
            "INFANTRY_COST" => infantry_cost = Some(parse(value, &file.path, key)?),
            "CAVALRY_COST" => cavalry_cost = Some(parse(value, &file.path, key)?),
            "INFANTRY_TIME" => infantry_days = Some(parse(value, &file.path, key)?),
            "CAVALRY_TIME" => cavalry_days = Some(parse(value, &file.path, key)?),
            "LAND_MAINTENANCE_FACTOR" => maintenance = Some(parse(value, &file.path, key)?),
            "INFANTRY_SPEED" => infantry_speed = Some(parse(value, &file.path, key)?),
            "CAVALRY_SPEED" => cavalry_speed = Some(parse(value, &file.path, key)?),
            "SUPPLYLIMIT_BASE_MULTIPLIER" => supply_base = Some(parse(value, &file.path, key)?),
            _ => (),
        }
    }
    let speed = |value: Option<Money>, key: &str| {
        value
            .filter(|speed| *speed > Money::ZERO)
            .ok_or_else(|| Error::invalid(&file.path, format!("{key} must be positive")))
    };
    Ok(MilitaryRules {
        infantry_cost: required(infantry_cost, &file.path, "INFANTRY_COST")?,
        cavalry_cost: required(cavalry_cost, &file.path, "CAVALRY_COST")?,
        land_maintenance_factor: required(maintenance, &file.path, "LAND_MAINTENANCE_FACTOR")?,
        infantry_days: required(infantry_days, &file.path, "INFANTRY_TIME")?,
        cavalry_days: required(cavalry_days, &file.path, "CAVALRY_TIME")?,
        infantry_speed: speed(infantry_speed, "INFANTRY_SPEED")?,
        cavalry_speed: speed(cavalry_speed, "CAVALRY_SPEED")?,
        recruitment_full_refund_progress: required(
            refund_progress,
            &file.path,
            "REGAIN_COST_BEFORE_PROGRESS",
        )?,
        supply_base: positive_whole(
            required(supply_base, &file.path, "SUPPLYLIMIT_BASE_MULTIPLIER")?,
            &file.path,
        )?,
        manpower: super::manpower::load(content, &text)?,
    })
}

/// 原版写成 6.0；只接受能放进 u32 的正整数，小数部分必须为 0。
fn positive_whole(value: Money, path: &Path) -> Result<u32, Error> {
    u32::try_from(value)
        .ok()
        .filter(|_| value.is_integer() && value > Money::ZERO)
        .ok_or_else(|| {
            Error::invalid(
                path,
                "SUPPLYLIMIT_BASE_MULTIPLIER must be a positive whole number",
            )
        })
}

fn parse<T>(value: &str, path: &Path, key: &str) -> Result<T, Error>
where
    T: FromStr,
    T::Err: Display,
{
    value
        .parse()
        .map_err(|error| Error::invalid(path, format!("invalid {key}: {error}")))
}

fn required<T>(value: Option<T>, path: &Path, key: &str) -> Result<T, Error> {
    value.ok_or_else(|| Error::invalid(path, format!("missing {key}")))
}
