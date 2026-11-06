// SPDX-License-Identifier: GPL-3.0-only

use std::{fmt::Display, path::Path, str::FromStr};

use augustus_core::{MilitaryRules, Money};

use crate::{Error, content::ContentRoot};

use super::forcelimit;

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
    let mut extra_cost = None;
    // 基础 defines 使用数值字面量，只读取当前招募、取消、维护费、上限和行军需要的字段。
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
            "LAND_FORCELIMIT_EXTRA_COST_FACTOR" => extra_cost = Some(parse(value, &file.path, key)?),
            "INFANTRY_SPEED" => infantry_speed = Some(parse(value, &file.path, key)?),
            "CAVALRY_SPEED" => cavalry_speed = Some(parse(value, &file.path, key)?),
            _ => (),
        }
    }
    let speed = |value: Option<Money>, key: &str| {
        value
            .filter(|speed| *speed > Money::ZERO)
            .ok_or_else(|| Error::invalid(&file.path, format!("{key} must be positive")))
    };
    let limits = forcelimit::load(content)?;
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
        base_land_forcelimit: limits.base,
        development_land_forcelimit: limits.per_development,
        land_forcelimit_extra_cost_factor: required(
            extra_cost,
            &file.path,
            "LAND_FORCELIMIT_EXTRA_COST_FACTOR",
        )?,
        goods_land_forcelimit: limits.goods,
        manpower: super::manpower::load(content, &text)?,
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

#[cfg(test)]
mod tests {
    use std::fs;

    use augustus_core::Money;

    use crate::content::ContentRoot;

    use super::load;

    #[test]
    fn reads_force_limit_sources_from_content() {
        let root = std::env::temp_dir().join("augustus-eu4-force-limit");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("common/static_modifiers")).unwrap();
        fs::create_dir_all(root.join("common/tradegoods")).unwrap();
        fs::write(root.join("common/defines.lua"), DEFINES).unwrap();
        fs::write(
            root.join("common/static_modifiers/00_static_modifiers.txt"),
            STATICS,
        )
        .unwrap();
        fs::write(root.join("common/tradegoods/00_tradegoods.txt"), GOODS).unwrap();

        let rules = load(&ContentRoot::new(&root)).unwrap();
        assert_eq!(rules.base_land_forcelimit, Money::from(6));
        assert_eq!(rules.development_land_forcelimit, Money::new(1, 1));
        assert_eq!(rules.land_forcelimit_extra_cost_factor, Money::from(2));
        assert_eq!(
            rules.goods_land_forcelimit.get("grain").copied(),
            Some(Money::new(5, 1))
        );
        assert_eq!(
            rules.goods_land_forcelimit.get("fish").copied(),
            Some(Money::ZERO)
        );
        fs::remove_dir_all(&root).unwrap();
    }

    const DEFINES: &str = r#"
        REGAIN_COST_BEFORE_PROGRESS = 0.1,
        INFANTRY_COST = 10,
        CAVALRY_COST = 25,
        INFANTRY_TIME = 60,
        CAVALRY_TIME = 90,
        LAND_MAINTENANCE_FACTOR = 0.25,
        LAND_FORCELIMIT_EXTRA_COST_FACTOR = 2,
        INFANTRY_SPEED = 1,
        CAVALRY_SPEED = 2,
        BASE_MP_TO_MANPOWER = 0.25,
        MAX_MANPOWER = 10,
        MIN_MONTHLY_MANPOWER = 0.1,
        INF_REGIMENT_MANPOWER_COST = 1,
        CAV_REGIMENT_MANPOWER_COST = 1,
    "#;

    const STATICS: &str = r#"
        base_values = {
            global_manpower = 1
            land_forcelimit = 6
        }
        development = {
            land_forcelimit = 0.1
        }
    "#;

    const GOODS: &str = r#"
        grain = {
            modifier = { land_forcelimit_modifier = 0.20 }
            province = { land_forcelimit = 0.5 }
        }
        fish = {
            province = { local_sailors_modifier = 0.25 }
        }
    "#;
}
