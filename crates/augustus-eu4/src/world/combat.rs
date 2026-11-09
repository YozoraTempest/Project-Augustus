// SPDX-License-Identifier: GPL-3.0-only

//! 从 defines、军事科技 0 级和兵种文件读取基础陆战数值。
//!
//! 国家科技组和后续科技等级尚未实现，因此只应用 `mil.txt` 的第一个 technology 块，
//! 而不是 1444 年各国已经研究的等级。应用之前战术是 0.5，其余数值是 0；
//! 块里没写的键加 0。陆战士气取该块的 `land_morale`，不读 `base_values`。

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
        max_morale: technology.land_morale,
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
        // 国家骑兵比例尚未分国读取。500 是基础允许比例，不是临时惩罚。
        allowed_cavalry_ratio: 500,
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
    land_morale: Money,
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
    // 文件和 defines 都不写基础战术。第一个 technology 块只把写出的键加到这个起点上。
    let mut technology = Technology {
        enabled: Vec::new(),
        infantry_fire: Money::ZERO,
        infantry_shock: Money::ZERO,
        cavalry_fire: Money::ZERO,
        cavalry_shock: Money::ZERO,
        land_morale: Money::ZERO,
        tactics: Money::new(5, 1),
    };
    let mut found = false;
    for (key, operator, value) in tape.windows1252_reader().fields() {
        if key.read_str() != "technology" {
            continue;
        }
        script::equal(&file, operator, "technology")?;
        let object = value
            .read_object()
            .map_err(|error| Error::invalid(&file.path, error.to_string()))?;
        found = true;
        for (key, operator, value) in object.fields() {
            let name = key.read_str().to_string();
            match name.as_str() {
                "enable" => {
                    script::equal(&file, operator, "enable")?;
                    technology.enabled.push(
                        value
                            .read_string()
                            .map_err(|error| Error::invalid(&file.path, error.to_string()))?
                            .to_string(),
                    );
                }
                "infantry_fire" | "infantry_shock" | "cavalry_fire" | "cavalry_shock"
                | "military_tactics" | "land_morale" => {
                    script::equal(&file, operator, &name)?;
                    let amount = decimal(&value, &file, &name)?;
                    match name.as_str() {
                        "infantry_fire" => technology.infantry_fire += amount,
                        "infantry_shock" => technology.infantry_shock += amount,
                        "cavalry_fire" => technology.cavalry_fire += amount,
                        "cavalry_shock" => technology.cavalry_shock += amount,
                        "military_tactics" => technology.tactics += amount,
                        "land_morale" => technology.land_morale += amount,
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        break;
    }
    if !found {
        return Err(Error::invalid(&file.path, "missing technology"));
    }
    if technology.tactics <= Money::ZERO {
        return Err(Error::invalid(
            &file.path,
            "military_tactics must be positive",
        ));
    }
    if technology.enabled.is_empty() {
        return Err(Error::invalid(
            &file.path,
            "technology 0 does not enable a unit",
        ));
    }
    Ok(technology)
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
                    "technology 0 does not enable a {} unit ({})",
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

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;
    use std::sync::atomic::{AtomicU64, Ordering};

    use augustus_core::Money;

    use crate::content::ContentRoot;

    use super::load;

    fn temp_root(name: &str) -> std::path::PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "augustus-{name}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("common/technologies")).unwrap();
        fs::create_dir_all(root.join("common/units")).unwrap();
        fs::create_dir_all(root.join("common/static_modifiers")).unwrap();
        root
    }

    #[test]
    fn technology_zero_adds_written_keys_to_the_base() {
        let root = temp_root("mil-base");
        fs::write(
            root.join("common/static_modifiers/00_static_modifiers.txt"),
            "base_values = { land_morale = 9 }\n",
        )
        .unwrap();
        fs::write(
            root.join("common/technologies/mil.txt"),
            r#"
                technology = {
                    enable = western_medieval_infantry
                    enable = eastern_medieval_cavalry
                    infantry_fire = 0.25
                    infantry_shock = 0.20
                    cavalry_shock = 0.8
                    land_morale = 2.0
                    military_tactics = 0.25
                }
                technology = {
                    enable = western_medieval_infantry
                    infantry_fire = 1
                    cavalry_fire = 1
                    military_tactics = 1
                    land_morale = 5
                }
            "#,
        )
        .unwrap();
        fs::write(
            root.join("common/units/western_medieval_infantry.txt"),
            r#"
                type = infantry
                maneuver = 1
                offensive_morale = 0
                defensive_morale = 1
                offensive_fire = 0
                defensive_fire = 1
                offensive_shock = 1
                defensive_shock = 1
            "#,
        )
        .unwrap();
        fs::write(
            root.join("common/units/eastern_medieval_cavalry.txt"),
            r#"
                type = cavalry
                maneuver = 2
                offensive_morale = 0
                defensive_morale = 0
                offensive_fire = 0
                defensive_fire = 0
                offensive_shock = 0
                defensive_shock = 0
            "#,
        )
        .unwrap();

        let rules = load(&ContentRoot::new(&root), DEFINES, Path::new("defines.lua")).unwrap();
        let _ = fs::remove_dir_all(&root);
        assert_eq!(rules.tactics, Money::new(5, 1) + Money::new(25, 2));
        assert_eq!(rules.max_morale, Money::from(2));
        assert_eq!(rules.infantry.fire_damage, Money::new(25, 2));
        assert_eq!(rules.infantry.shock_damage, Money::new(2, 1));
        assert_eq!(rules.cavalry.fire_damage, Money::ZERO);
        assert_eq!(rules.cavalry.shock_damage, Money::new(8, 1));
        assert_eq!(rules.infantry.offensive_shock, 1);
        assert_eq!(rules.cavalry.maneuver, 2);
    }

    #[test]
    fn technology_zero_without_units_does_not_complain_about_tactics() {
        let root = temp_root("mil-no-unit");
        fs::write(
            root.join("common/static_modifiers/00_static_modifiers.txt"),
            "base_values = { land_morale = 4 }\n",
        )
        .unwrap();
        fs::write(
            root.join("common/technologies/mil.txt"),
            r#"
                technology = {
                    infantry_fire = 0.25
                    infantry_shock = 0.20
                    cavalry_shock = 0.8
                }
                technology = {
                    enable = western_medieval_infantry
                    military_tactics = 0.25
                    cavalry_fire = 0.5
                }
            "#,
        )
        .unwrap();
        fs::write(
            root.join("common/units/western_medieval_infantry.txt"),
            "type = infantry\n",
        )
        .unwrap();

        let error = load(&ContentRoot::new(&root), DEFINES, Path::new("defines.lua")).unwrap_err();
        let _ = fs::remove_dir_all(&root);
        let message = error.to_string();
        assert!(
            message.contains("technology 0 does not enable a unit"),
            "{message}"
        );
        assert!(!message.contains("military_tactics"), "{message}");
        assert!(!message.contains("land_morale"), "{message}");
        assert!(!message.contains("base_values"), "{message}");
    }

    #[test]
    fn omitted_land_morale_stays_zero_when_base_values_has_another() {
        let root = temp_root("mil-no-morale");
        fs::write(
            root.join("common/static_modifiers/00_static_modifiers.txt"),
            "base_values = { land_morale = 9 }\n",
        )
        .unwrap();
        fs::write(
            root.join("common/technologies/mil.txt"),
            r#"
                technology = {
                    enable = western_medieval_infantry
                    enable = eastern_medieval_cavalry
                    infantry_fire = 0.25
                    infantry_shock = 0.20
                    cavalry_shock = 0.8
                }
            "#,
        )
        .unwrap();
        fs::write(
            root.join("common/units/western_medieval_infantry.txt"),
            r#"
                type = infantry
                maneuver = 1
                offensive_morale = 0
                defensive_morale = 1
                offensive_fire = 0
                defensive_fire = 1
                offensive_shock = 1
                defensive_shock = 1
            "#,
        )
        .unwrap();
        fs::write(
            root.join("common/units/eastern_medieval_cavalry.txt"),
            r#"
                type = cavalry
                maneuver = 2
                offensive_morale = 0
                defensive_morale = 0
                offensive_fire = 0
                defensive_fire = 0
                offensive_shock = 0
                defensive_shock = 0
            "#,
        )
        .unwrap();

        let rules = load(&ContentRoot::new(&root), DEFINES, Path::new("defines.lua")).unwrap();
        let _ = fs::remove_dir_all(&root);
        assert_eq!(rules.max_morale, Money::ZERO);
    }

    const DEFINES: &str = r#"
        BASE_COMBAT_WIDTH = 15,
        MORALE_RECOVERY_SPEED = 0.15,
        MORALE_RECOVERY_SPEED_OWN_TERRITORY = 0.05,
        SHATTERED_RETREAT_SPEED_MODIFIER = 0.5,
        WAR_LENGTH_DAMAGE_MODIFIER = 0.01,
    "#;
}
