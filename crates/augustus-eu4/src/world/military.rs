// SPDX-License-Identifier: GPL-3.0-only

use augustus_core::RecruitmentRules;

use crate::{Error, content::ContentRoot};

pub(super) fn load(content: &ContentRoot) -> Result<RecruitmentRules, Error> {
    let file = content.read("common/defines.lua")?;
    let text = String::from_utf8_lossy(&file.bytes);
    let mut cost = None;
    let mut days = None;
    // 基础 defines 使用数值字面量；只读取首版步兵招募需要的两项规则。
    for line in text.lines() {
        let line = line.split("--").next().unwrap_or("");
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim().trim_end_matches(',').trim();
        match key.trim() {
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
            _ => (),
        }
    }
    Ok(RecruitmentRules {
        infantry_cost: cost.ok_or_else(|| Error::invalid(&file.path, "missing INFANTRY_COST"))?,
        infantry_days: days.ok_or_else(|| Error::invalid(&file.path, "missing INFANTRY_TIME"))?,
    })
}
