// SPDX-License-Identifier: GPL-3.0-only

use augustus_core::{MilitaryRules, Money};

use crate::{Error, content::ContentRoot};

pub(super) struct MilitaryContent {
    pub rules: MilitaryRules,
    pub infantry_speed: Money,
}

pub(super) fn load(content: &ContentRoot) -> Result<MilitaryContent, Error> {
    let file = content.read("common/defines.lua")?;
    let text = String::from_utf8_lossy(&file.bytes);
    let mut cost = None;
    let mut maintenance = None;
    let mut days = None;
    let mut refund_progress = None;
    let mut speed: Option<Money> = None;
    // 基础 defines 使用数值字面量，只读取当前招募、取消、维护费和行军需要的字段。
    for line in text.lines() {
        let line = line.split("--").next().unwrap_or("");
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim().trim_end_matches(',').trim();
        match key.trim() {
            "REGAIN_COST_BEFORE_PROGRESS" => {
                refund_progress = Some(value.parse().map_err(|error| {
                    Error::invalid(
                        &file.path,
                        format!("invalid REGAIN_COST_BEFORE_PROGRESS: {error}"),
                    )
                })?)
            }
            "INFANTRY_COST" => {
                cost = Some(value.parse().map_err(|error| {
                    Error::invalid(&file.path, format!("invalid INFANTRY_COST: {error}"))
                })?)
            }
            "INFANTRY_TIME" => {
                days = Some(value.parse().map_err(|error| {
                    Error::invalid(&file.path, format!("invalid INFANTRY_TIME: {error}"))
                })?)
            }
            "LAND_MAINTENANCE_FACTOR" => {
                maintenance = Some(value.parse().map_err(|error| {
                    Error::invalid(
                        &file.path,
                        format!("invalid LAND_MAINTENANCE_FACTOR: {error}"),
                    )
                })?)
            }
            "INFANTRY_SPEED" => {
                speed = Some(value.parse().map_err(|error| {
                    Error::invalid(&file.path, format!("invalid INFANTRY_SPEED: {error}"))
                })?)
            }
            _ => (),
        }
    }
    let infantry_speed = speed
        .filter(|speed| *speed > Money::ZERO)
        .ok_or_else(|| Error::invalid(&file.path, "INFANTRY_SPEED must be positive"))?;
    Ok(MilitaryContent {
        rules: MilitaryRules {
            infantry_cost: cost
                .ok_or_else(|| Error::invalid(&file.path, "missing INFANTRY_COST"))?,
            land_maintenance_factor: maintenance
                .ok_or_else(|| Error::invalid(&file.path, "missing LAND_MAINTENANCE_FACTOR"))?,
            infantry_days: days
                .ok_or_else(|| Error::invalid(&file.path, "missing INFANTRY_TIME"))?,
            recruitment_full_refund_progress: refund_progress
                .ok_or_else(|| Error::invalid(&file.path, "missing REGAIN_COST_BEFORE_PROGRESS"))?,
            manpower: super::manpower::load(content, &text)?,
        },
        infantry_speed,
    })
}
