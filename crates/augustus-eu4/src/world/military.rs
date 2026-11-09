// SPDX-License-Identifier: GPL-3.0-only

use std::{fmt::Display, path::Path, str::FromStr};

use augustus_core::{MilitaryRules, Money};

use crate::{Error, content::ContentRoot};

use super::forcelimit;

pub(super) fn load(
    content: &ContentRoot,
    unit_group: &str,
    historical_units: &[impl AsRef<str>],
) -> Result<MilitaryRules, Error> {
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
    let mut extra_cost = None;
    let mut monthly_reinforce = None;
    let mut reinforce_cost = None;
    let mut starting_army = None;
    let mut starting_army_at_war = None;
    // 基础 defines 使用数值字面量，只读取当前招募、取消、维护费、补员、上限、行军和补给需要的字段。
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
            "LAND_FORCELIMIT_EXTRA_COST_FACTOR" => {
                extra_cost = Some(parse(value, &file.path, key)?)
            }
            "MONTHLY_REINFORCE" => monthly_reinforce = Some(parse(value, &file.path, key)?),
            "EXTRA_LAND_REINFORCE_COST" => reinforce_cost = Some(parse(value, &file.path, key)?),
            "INFANTRY_SPEED" => infantry_speed = Some(parse(value, &file.path, key)?),
            "CAVALRY_SPEED" => cavalry_speed = Some(parse(value, &file.path, key)?),
            "SUPPLYLIMIT_BASE_MULTIPLIER" => supply_base = Some(parse(value, &file.path, key)?),
            "STARTING_ARMY_SIZE" => starting_army = Some(parse(value, &file.path, key)?),
            "STARTING_ARMY_SIZE_AT_WAR" => {
                starting_army_at_war = Some(parse(value, &file.path, key)?)
            }
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
        monthly_reinforce: required(monthly_reinforce, &file.path, "MONTHLY_REINFORCE")?,
        extra_land_reinforce_cost: required(
            reinforce_cost,
            &file.path,
            "EXTRA_LAND_REINFORCE_COST",
        )?,
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
        base_land_forcelimit: limits.base,
        development_land_forcelimit: limits.per_development,
        land_forcelimit_extra_cost_factor: required(
            extra_cost,
            &file.path,
            "LAND_FORCELIMIT_EXTRA_COST_FACTOR",
        )?,
        goods_land_forcelimit: limits.goods,
        tax_recruitment_time: tax_recruitment_time(content)?,
        starting_army_size: nonnegative(
            required(starting_army, &file.path, "STARTING_ARMY_SIZE")?,
            &file.path,
            "STARTING_ARMY_SIZE",
        )?,
        starting_army_size_at_war: nonnegative(
            required(
                starting_army_at_war,
                &file.path,
                "STARTING_ARMY_SIZE_AT_WAR",
            )?,
            &file.path,
            "STARTING_ARMY_SIZE_AT_WAR",
        )?,
        manpower: super::manpower::load(content, &text)?,
        combat: super::combat::load(content, &text, &file.path, unit_group, historical_units)?,
    })
}

/// defines 里的开局比例按字面量使用，0.75 就是上限的 75%，不是 75 倍。
fn nonnegative(value: Money, path: &Path, key: &str) -> Result<Money, Error> {
    if value < Money::ZERO {
        return Err(Error::invalid(path, format!("{key} must be nonnegative")));
    }
    Ok(value)
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

/// 税收发展度的招募时间系数。内容未写该键时为零。
fn tax_recruitment_time(content: &ContentRoot) -> Result<Money, Error> {
    let mut found = None;
    for path in content.text_files("common/static_modifiers")? {
        let file = content.read(path)?;
        let tape = crate::script::parse(&file)?;
        for (key, operator, value) in tape.windows1252_reader().fields() {
            if key.read_str() != "provincial_tax_income" {
                continue;
            }
            crate::script::equal(&file, operator, "provincial_tax_income")?;
            let object = value
                .read_object()
                .map_err(|error| Error::invalid(&file.path, error.to_string()))?;
            for (key, operator, value) in object.fields() {
                if key.read_str() != "regiment_recruit_speed" {
                    continue;
                }
                crate::script::equal(&file, operator, "regiment_recruit_speed")?;
                let text = value
                    .read_string()
                    .map_err(|error| Error::invalid(&file.path, error.to_string()))?;
                found = Some(text.parse().map_err(|error| {
                    Error::invalid(
                        &file.path,
                        format!("invalid regiment_recruit_speed: {error}"),
                    )
                })?);
            }
        }
    }
    Ok(found.unwrap_or(Money::ZERO))
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
    use std::sync::atomic::{AtomicU64, Ordering};

    use augustus_core::Money;

    use crate::content::ContentRoot;

    use super::load;

    #[test]
    fn tax_recruitment_time_comes_from_provincial_tax_income() {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "augustus-tax-time-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("common/static_modifiers")).unwrap();
        let file = root.join("common/static_modifiers/00_static_modifiers.txt");
        fs::write(&file, "base_values = { global_manpower = 10 }\n").unwrap();
        let missing = super::tax_recruitment_time(&ContentRoot::new(&root)).unwrap();
        fs::write(
            &file,
            "provincial_tax_income = { regiment_recruit_speed = -0.02 }\n",
        )
        .unwrap();
        let found = super::tax_recruitment_time(&ContentRoot::new(&root)).unwrap();
        let _ = fs::remove_dir_all(&root);
        assert_eq!(missing, Money::ZERO);
        assert_eq!(found, Money::new(-2, 2));
    }

    #[test]
    fn reads_force_limit_sources_from_content() {
        let root = std::env::temp_dir().join("augustus-eu4-force-limit");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("common/static_modifiers")).unwrap();
        fs::create_dir_all(root.join("common/tradegoods")).unwrap();
        fs::create_dir_all(root.join("common/technologies")).unwrap();
        fs::create_dir_all(root.join("common/units")).unwrap();
        fs::write(root.join("common/defines.lua"), DEFINES).unwrap();
        fs::write(
            root.join("common/static_modifiers/00_static_modifiers.txt"),
            STATICS,
        )
        .unwrap();
        fs::write(root.join("common/tradegoods/00_tradegoods.txt"), GOODS).unwrap();
        fs::write(root.join("common/technologies/mil.txt"), MIL).unwrap();
        fs::write(
            root.join("common/units/eastern_medieval_infantry.txt"),
            EASTERN_INFANTRY,
        )
        .unwrap();
        fs::write(
            root.join("common/units/western_medieval_infantry.txt"),
            WESTERN_INFANTRY,
        )
        .unwrap();
        fs::write(
            root.join("common/units/eastern_medieval_cavalry.txt"),
            EASTERN_CAVALRY,
        )
        .unwrap();

        let rules = load(&ContentRoot::new(&root), "western", &[] as &[&str]).unwrap();
        assert_eq!(rules.supply_base, 6);
        assert_eq!(rules.base_land_forcelimit, Money::from(6));
        assert_eq!(rules.development_land_forcelimit, Money::new(1, 1));
        assert_eq!(rules.land_forcelimit_extra_cost_factor, Money::from(2));
        assert_eq!(rules.monthly_reinforce, Money::new(1, 1));
        assert_eq!(rules.extra_land_reinforce_cost, Money::from(2));
        assert_eq!(
            rules.goods_land_forcelimit.get("grain").copied(),
            Some(Money::new(5, 1))
        );
        assert_eq!(
            rules.goods_land_forcelimit.get("fish").copied(),
            Some(Money::ZERO)
        );
        assert_eq!(rules.starting_army_size, Money::new(75, 2));
        assert_eq!(rules.starting_army_size_at_war, Money::from(1));
        assert_eq!(rules.combat.width, 15);
        assert_eq!(rules.combat.max_morale, Money::from(2));
        assert_eq!(rules.combat.tactics, Money::new(5, 1));
        assert_eq!(rules.combat.recovery, Money::new(15, 2));
        assert_eq!(rules.combat.recovery_own_territory, Money::new(5, 2));
        assert_eq!(rules.combat.shattered_retreat_speed, Money::new(5, 1));
        assert_eq!(rules.combat.battle_length_damage, Money::new(1, 2));
        assert_eq!(rules.combat.infantry.offensive_shock, 1);
        assert_eq!(rules.combat.infantry.fire_damage, Money::new(25, 2));
        assert_eq!(rules.combat.infantry.shock_damage, Money::new(2, 1));
        assert_eq!(rules.combat.cavalry.maneuver, 2);
        assert_eq!(rules.combat.cavalry.fire_damage, Money::ZERO);
        assert_eq!(rules.combat.cavalry.shock_damage, Money::new(8, 1));
        fs::remove_dir_all(&root).unwrap();
    }

    const DEFINES: &str = r#"
        REGAIN_COST_BEFORE_PROGRESS = 0.1,
        INFANTRY_COST = 10,
        CAVALRY_COST = 25,
        INFANTRY_TIME = 60,
        CAVALRY_TIME = 90,
        LAND_MAINTENANCE_FACTOR = 0.25,
        MONTHLY_REINFORCE = 0.1,
        EXTRA_LAND_REINFORCE_COST = 2,
        LAND_FORCELIMIT_EXTRA_COST_FACTOR = 2,
        SUPPLYLIMIT_BASE_MULTIPLIER = 6,
        STARTING_ARMY_SIZE = 0.75,
        STARTING_ARMY_SIZE_AT_WAR = 1,
        INFANTRY_SPEED = 1,
        CAVALRY_SPEED = 2,
        BASE_COMBAT_WIDTH = 15,
        MORALE_RECOVERY_SPEED = 0.15,
        MORALE_RECOVERY_SPEED_OWN_TERRITORY = 0.05,
        SHATTERED_RETREAT_SPEED_MODIFIER = 0.5,
        WAR_LENGTH_DAMAGE_MODIFIER = 0.01,
        BASE_MP_TO_MANPOWER = 0.25,
        MAX_MANPOWER = 10,
        MIN_MONTHLY_MANPOWER = 0.1,
        INF_REGIMENT_MANPOWER_COST = 1,
        CAV_REGIMENT_MANPOWER_COST = 1,
    "#;

    const STATICS: &str = r#"
        base_values = {
            global_manpower = 1
        }
        all_nations = {
            global_tax_income = 12
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

    const MIL: &str = r#"
        technology = {
            enable = eastern_medieval_infantry
            enable = western_medieval_infantry
            enable = eastern_medieval_cavalry
            infantry_fire = 0.25
            infantry_shock = 0.20
            cavalry_shock = 0.8
            land_morale = 2.0
        }
        technology = {
            enable = eastern_medieval_infantry
            infantry_fire = 1
            cavalry_fire = 1
            military_tactics = 0.25
            land_morale = 5
        }
    "#;

    const EASTERN_INFANTRY: &str = r#"
        type = infantry
        maneuver = 1
        offensive_morale = 0
        defensive_morale = 0
        offensive_fire = 0
        defensive_fire = 0
        offensive_shock = 9
        defensive_shock = 0
    "#;

    const WESTERN_INFANTRY: &str = r#"
        type = infantry
        maneuver = 1
        offensive_morale = 0
        defensive_morale = 1
        offensive_fire = 0
        defensive_fire = 1
        offensive_shock = 1
        defensive_shock = 1
    "#;

    const EASTERN_CAVALRY: &str = r#"
        type = cavalry
        maneuver = 2
        offensive_morale = 0
        defensive_morale = 0
        offensive_fire = 0
        defensive_fire = 0
        offensive_shock = 0
        defensive_shock = 0
    "#;
}
