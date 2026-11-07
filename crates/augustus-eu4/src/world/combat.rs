// SPDX-License-Identifier: GPL-3.0-only

//! 从 defines、静态修正、军事科技 0 级和兵种文件读取基础陆战数值。
//!
//! 国家科技组和后续科技等级尚未实现，因此使用 `mil.txt` 的第一个 technology 块，
//! 而不是 1444 年各国已经研究的等级。

use std::path::Path;

use augustus_core::{CombatRules, Money, UnitCombat, UnitKind};

use crate::{Error, content::ContentRoot, script};

pub(super) fn load(
    content: &ContentRoot,
    defines: &str,
    defines_path: &Path,
) -> Result<CombatRules, Error> {
    let mut width = None;
    let mut recovery = None;
    let mut recovery_own = None;
    let mut shattered = None;
    let mut length_damage = None;
    for line in defines.lines() {
        let line = line.split("--").next().unwrap_or("");
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim().trim_end_matches(',').trim();
        let parsed = match key {
            "BASE_COMBAT_WIDTH"
            | "MORALE_RECOVERY_SPEED"
            | "MORALE_RECOVERY_SPEED_OWN_TERRITORY"
            | "SHATTERED_RETREAT_SPEED_MODIFIER"
            | "WAR_LENGTH_DAMAGE_MODIFIER" => Some(parse_money(value, defines_path, key)?),
            _ => None,
        };
        match key {
            "BASE_COMBAT_WIDTH" => width = parsed,
            "MORALE_RECOVERY_SPEED" => recovery = parsed,
            "MORALE_RECOVERY_SPEED_OWN_TERRITORY" => recovery_own = parsed,
            "SHATTERED_RETREAT_SPEED_MODIFIER" => shattered = parsed,
            "WAR_LENGTH_DAMAGE_MODIFIER" => length_damage = parsed,
            _ => {}
        }
    }
    let technology = first_technology(content)?;
    let units = load_units(content, &technology.enabled)?;
    let infantry = choose(&units, UnitKind::Infantry, &technology)?;
    let cavalry = choose(&units, UnitKind::Cavalry, &technology)?;
    Ok(CombatRules {
        width: positive_whole(
            required(width, defines_path, "BASE_COMBAT_WIDTH")?,
            defines_path,
        )?,
        max_morale: land_morale(content)?,
        tactics: technology.tactics,
        recovery: required(recovery, defines_path, "MORALE_RECOVERY_SPEED")?,
        recovery_own_territory: required(
            recovery_own,
            defines_path,
            "MORALE_RECOVERY_SPEED_OWN_TERRITORY",
        )?,
        shattered_retreat_speed: required(
            shattered,
            defines_path,
            "SHATTERED_RETREAT_SPEED_MODIFIER",
        )?,
        battle_length_damage: required(length_damage, defines_path, "WAR_LENGTH_DAMAGE_MODIFIER")?,
        infantry: with_damage(
            infantry,
            technology.infantry_fire,
            technology.infantry_shock,
        ),
        cavalry: with_damage(cavalry, technology.cavalry_fire, technology.cavalry_shock),
    })
}

struct Technology {
    enabled: Vec<String>,
    infantry_fire: Money,
    infantry_shock: Money,
    cavalry_fire: Money,
    cavalry_shock: Money,
    tactics: Money,
}

struct LoadedUnit {
    name: String,
    kind: UnitKind,
    combat: UnitCombat,
}

fn first_technology(content: &ContentRoot) -> Result<Technology, Error> {
    let file = content.read("common/technologies/mil.txt")?;
    let tape = script::parse(&file)?;
    for (key, operator, value) in tape.windows1252_reader().fields() {
        if key.read_str() != "technology" {
            continue;
        }
        script::equal(&file, operator, "technology")?;
        let object = value
            .read_object()
            .map_err(|error| Error::invalid(&file.path, error.to_string()))?;
        let mut enabled = Vec::new();
        let mut infantry_fire = None;
        let mut infantry_shock = None;
        let mut cavalry_fire = None;
        let mut cavalry_shock = None;
        let mut tactics = None;
        for (key, operator, value) in object.fields() {
            let name = key.read_str().to_string();
            match name.as_str() {
                "enable" => {
                    script::equal(&file, operator, "enable")?;
                    enabled.push(
                        value
                            .read_string()
                            .map_err(|error| Error::invalid(&file.path, error.to_string()))?
                            .to_string(),
                    );
                }
                "infantry_fire" | "infantry_shock" | "cavalry_fire" | "cavalry_shock"
                | "military_tactics" => {
                    script::equal(&file, operator, &name)?;
                    let amount = decimal(&value, &file, &name)?;
                    match name.as_str() {
                        "infantry_fire" => infantry_fire = Some(amount),
                        "infantry_shock" => infantry_shock = Some(amount),
                        "cavalry_fire" => cavalry_fire = Some(amount),
                        "cavalry_shock" => cavalry_shock = Some(amount),
                        "military_tactics" => tactics = Some(amount),
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        let tactics = required_in(&file.path, tactics, "military_tactics")?;
        if tactics <= Money::ZERO {
            return Err(Error::invalid(
                &file.path,
                "military_tactics must be positive",
            ));
        }
        return Ok(Technology {
            enabled,
            infantry_fire: required_in(&file.path, infantry_fire, "infantry_fire")?,
            infantry_shock: required_in(&file.path, infantry_shock, "infantry_shock")?,
            cavalry_fire: required_in(&file.path, cavalry_fire, "cavalry_fire")?,
            cavalry_shock: required_in(&file.path, cavalry_shock, "cavalry_shock")?,
            tactics,
        });
    }
    Err(Error::invalid(&file.path, "missing technology"))
}

fn load_units(content: &ContentRoot, names: &[String]) -> Result<Vec<LoadedUnit>, Error> {
    let mut units = Vec::new();
    for name in names {
        let relative = format!("common/units/{name}.txt");
        let file = content.read(&relative)?;
        let tape = script::parse(&file)?;
        let mut kind = None;
        let mut other_type = false;
        let mut pips = [None; 7];
        for (key, operator, value) in tape.windows1252_reader().fields() {
            let field = key.read_str().to_string();
            let index = match field.as_str() {
                "type" => {
                    script::equal(&file, operator, "type")?;
                    let text = value
                        .read_string()
                        .map_err(|error| Error::invalid(&file.path, error.to_string()))?;
                    match text.as_ref() {
                        "infantry" => kind = Some(UnitKind::Infantry),
                        "cavalry" => kind = Some(UnitKind::Cavalry),
                        // 火炮和后排不在本阶段。启用了也不当作步兵或骑兵。
                        _ => other_type = true,
                    }
                    continue;
                }
                "offensive_fire" => 0,
                "defensive_fire" => 1,
                "offensive_shock" => 2,
                "defensive_shock" => 3,
                "offensive_morale" => 4,
                "defensive_morale" => 5,
                "maneuver" => 6,
                _ => continue,
            };
            script::equal(&file, operator, &field)?;
            let text = value
                .read_string()
                .map_err(|error| Error::invalid(&file.path, error.to_string()))?;
            pips[index] = Some(text.parse::<u32>().map_err(|error| {
                Error::invalid(&file.path, format!("invalid {field}: {error}"))
            })?);
        }
        if other_type && kind.is_none() {
            continue;
        }
        let Some(kind) = kind else {
            return Err(Error::invalid(&file.path, "missing unit type"));
        };
        let pip = |index: usize, field: &str| {
            pips[index].ok_or_else(|| Error::invalid(&file.path, format!("missing {field}")))
        };
        units.push(LoadedUnit {
            name: name.clone(),
            kind,
            combat: UnitCombat {
                offensive_fire: pip(0, "offensive_fire")?,
                defensive_fire: pip(1, "defensive_fire")?,
                offensive_shock: pip(2, "offensive_shock")?,
                defensive_shock: pip(3, "defensive_shock")?,
                offensive_morale: pip(4, "offensive_morale")?,
                defensive_morale: pip(5, "defensive_morale")?,
                maneuver: pip(6, "maneuver")?,
                fire_damage: Money::ZERO,
                shock_damage: Money::ZERO,
            },
        });
    }
    Ok(units)
}

/// 同一兵种有多个单位时优先名称以 `western_` 开头的，否则取名称排序的第一个。
fn choose<'a>(
    units: &'a [LoadedUnit],
    kind: UnitKind,
    technology: &Technology,
) -> Result<&'a UnitCombat, Error> {
    let mut matching: Vec<_> = units.iter().filter(|unit| unit.kind == kind).collect();
    matching.sort_by(|left, right| left.name.cmp(&right.name));
    matching
        .iter()
        .find(|unit| unit.name.starts_with("western_"))
        .or_else(|| matching.first())
        .map(|unit| &unit.combat)
        .ok_or_else(|| {
            Error::invalid(
                Path::new("common/technologies/mil.txt"),
                format!(
                    "technology does not enable a {} unit ({})",
                    kind.id(),
                    technology.enabled.join(", ")
                ),
            )
        })
}

fn with_damage(unit: &UnitCombat, fire: Money, shock: Money) -> UnitCombat {
    let mut unit = unit.clone();
    unit.fire_damage = fire;
    unit.shock_damage = shock;
    unit
}

fn land_morale(content: &ContentRoot) -> Result<Money, Error> {
    let mut found = None;
    for path in content.text_files("common/static_modifiers")? {
        let file = content.read(path)?;
        let tape = script::parse(&file)?;
        for (key, operator, value) in tape.windows1252_reader().fields() {
            if key.read_str() != "base_values" {
                continue;
            }
            script::equal(&file, operator, "base_values")?;
            let object = value
                .read_object()
                .map_err(|error| Error::invalid(&file.path, error.to_string()))?;
            for (key, operator, value) in object.fields() {
                if key.read_str() != "land_morale" {
                    continue;
                }
                script::equal(&file, operator, "land_morale")?;
                found = Some(decimal(&value, &file, "land_morale")?);
            }
        }
    }
    let morale = found.ok_or_else(|| {
        Error::invalid(
            Path::new("common/static_modifiers"),
            "missing base_values.land_morale",
        )
    })?;
    if morale <= Money::ZERO {
        return Err(Error::invalid(
            Path::new("common/static_modifiers"),
            "land_morale must be positive",
        ));
    }
    Ok(morale)
}

fn positive_whole(value: Money, path: &Path) -> Result<u32, Error> {
    u32::try_from(value)
        .ok()
        .filter(|_| value.is_integer() && value >= Money::ONE)
        .ok_or_else(|| Error::invalid(path, "BASE_COMBAT_WIDTH must be a positive whole number"))
}

fn parse_money(value: &str, path: &Path, key: &str) -> Result<Money, Error> {
    value
        .parse()
        .map_err(|error| Error::invalid(path, format!("invalid {key}: {error}")))
}

fn decimal(
    value: &jomini::text::ValueReader<'_, '_, jomini::Windows1252Encoding>,
    file: &crate::content::ContentFile,
    field: &str,
) -> Result<Money, Error> {
    value
        .read_string()
        .map_err(|error| Error::invalid(&file.path, error.to_string()))?
        .parse()
        .map_err(|error| Error::invalid(&file.path, format!("invalid {field}: {error}")))
}

fn required(value: Option<Money>, path: &Path, key: &str) -> Result<Money, Error> {
    value.ok_or_else(|| Error::invalid(path, format!("missing {key}")))
}

fn required_in(path: &Path, value: Option<Money>, key: &str) -> Result<Money, Error> {
    value.ok_or_else(|| Error::invalid(path, format!("missing {key}")))
}
