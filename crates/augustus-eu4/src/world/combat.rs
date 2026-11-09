// SPDX-License-Identifier: GPL-3.0-only

//! 从 defines、军事科技和兵种文件读取基础陆战数值。
//!
//! 数值只累加第一个 technology 块。应用之前战术是 0.5，其余数值是 0；
//! 块里没写的键加 0。陆战士气取该块的 `land_morale`，不读 `base_values`。
//! 科技 0 没有 `enable` 时，兵种改取其后第一个写出 `enable` 的科技级，不累加那一级的数值。
//! 步兵和骑兵按兵种组筛选：`unit_type` 与该组相同，没写 `unit_type` 则各组都能用。
//! 同一兵种有多个时，用历史兵种名单里最后一项已经启用的；名单没有就用该级最后一项 `enable`。

use std::path::Path;

use augustus_core::{CombatRules, Money, UnitCombat, UnitKind};

use crate::{Error, content::ContentRoot, script};

pub(super) fn load(
    content: &ContentRoot,
    defines: &str,
    defines_path: &Path,
    unit_group: &str,
    historical_units: &[impl AsRef<str>],
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
    let technology = technology(content)?;
    let units = load_units(content, &technology.enabled)?;
    let infantry = choose(
        &units,
        UnitKind::Infantry,
        &technology,
        unit_group,
        historical_units,
    )?;
    let cavalry = choose(
        &units,
        UnitKind::Cavalry,
        &technology,
        unit_group,
        historical_units,
    )?;
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
    /// 第一个写出 `enable` 的 technology 块序号。科技 0 是 0。
    level: u32,
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
    /// 没写时各组都能用。
    group: Option<String>,
    kind: UnitKind,
    combat: UnitCombat,
}

impl LoadedUnit {
    fn available_to(&self, kind: UnitKind, group: &str) -> bool {
        self.kind == kind
            && self
                .group
                .as_deref()
                .is_none_or(|candidate| candidate == group)
    }
}

fn technology(content: &ContentRoot) -> Result<Technology, Error> {
    let file = content.read("common/technologies/mil.txt")?;
    let tape = script::parse(&file)?;
    // 文件和 defines 都不写基础战术。科技 0 只把写出的键加到这个起点上。
    let mut technology = Technology {
        level: 0,
        enabled: Vec::new(),
        infantry_fire: Money::ZERO,
        infantry_shock: Money::ZERO,
        cavalry_fire: Money::ZERO,
        cavalry_shock: Money::ZERO,
        land_morale: Money::ZERO,
        tactics: Money::new(5, 1),
    };
    let mut found = false;
    let mut index = 0_u32;
    for (key, operator, value) in tape.windows1252_reader().fields() {
        if key.read_str() != "technology" {
            continue;
        }
        script::equal(&file, operator, "technology")?;
        let object = value
            .read_object()
            .map_err(|error| Error::invalid(&file.path, error.to_string()))?;
        found = true;
        let mut enabled = Vec::new();
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
                | "military_tactics" | "land_morale"
                    if index == 0 =>
                {
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
        if !enabled.is_empty() {
            technology.level = index;
            technology.enabled = enabled;
            break;
        }
        index += 1;
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
        return Err(Error::invalid(&file.path, "no technology enables a unit"));
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
        let mut group = None;
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
                "unit_type" => {
                    script::equal(&file, operator, "unit_type")?;
                    group = Some(
                        value
                            .read_string()
                            .map_err(|error| Error::invalid(&file.path, error.to_string()))?
                            .to_string(),
                    );
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
            group,
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

/// 历史名单从后往前，取第一项已经启用且属于该兵种组的。名单没有就用启用顺序的最后一项。
fn choose<'a>(
    units: &'a [LoadedUnit],
    kind: UnitKind,
    technology: &Technology,
    unit_group: &str,
    historical_units: &[impl AsRef<str>],
) -> Result<&'a UnitCombat, Error> {
    historical_units
        .iter()
        .rev()
        .find_map(|name| {
            let name = name.as_ref();
            units
                .iter()
                .find(|unit| unit.name == name && unit.available_to(kind, unit_group))
        })
        .or_else(|| {
            units
                .iter()
                .rev()
                .find(|unit| unit.available_to(kind, unit_group))
        })
        .map(|unit| &unit.combat)
        .ok_or_else(|| {
            Error::invalid(
                Path::new("common/technologies/mil.txt"),
                format!(
                    "technology {} does not enable a {} unit for {unit_group}",
                    technology.level,
                    kind.id()
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

        let rules = load(
            &ContentRoot::new(&root),
            DEFINES,
            Path::new("defines.lua"),
            "western",
            NO_HISTORY,
        )
        .unwrap();
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
            unit_text("infantry", "western", 1),
        )
        .unwrap();

        let error = load(
            &ContentRoot::new(&root),
            DEFINES,
            Path::new("defines.lua"),
            "western",
            NO_HISTORY,
        )
        .unwrap_err();
        let _ = fs::remove_dir_all(&root);
        let message = error.to_string();
        assert!(
            message.contains("technology 1 does not enable a cavalry unit for western"),
            "{message}"
        );
        assert!(
            !message.contains("technology 0 does not enable a unit"),
            "{message}"
        );
        assert!(!message.contains("military_tactics"), "{message}");
        assert!(!message.contains("land_morale"), "{message}");
        assert!(!message.contains("base_values"), "{message}");
    }

    #[test]
    fn first_enable_level_filters_by_unit_type_and_keeps_technology_zero_values() {
        let root = temp_root("mil-group");
        fs::write(
            root.join("common/static_modifiers/00_static_modifiers.txt"),
            "base_values = { land_morale = 9 }\n",
        )
        .unwrap();
        fs::write(
            root.join("common/technologies/mil.txt"),
            r#"
                technology = {
                    infantry_fire = 0.25
                    infantry_shock = 0.20
                    cavalry_shock = 0.8
                    land_morale = 2.0
                }
                technology = {
                    infantry_fire = 1
                    military_tactics = 0.5
                    enable = western_medieval_infantry
                    enable = halberd_infantry
                    enable = western_medieval_knights
                    enable = chevauchee
                    enable = eastern_medieval_infantry
                }
                technology = {
                    enable = later_infantry
                }
            "#,
        )
        .unwrap();
        fs::write(
            root.join("common/units/western_medieval_infantry.txt"),
            unit_text("infantry", "western", 1),
        )
        .unwrap();
        fs::write(
            root.join("common/units/halberd_infantry.txt"),
            unit_text("infantry", "western", 3),
        )
        .unwrap();
        fs::write(
            root.join("common/units/western_medieval_knights.txt"),
            unit_text("cavalry", "western", 4),
        )
        .unwrap();
        fs::write(
            root.join("common/units/chevauchee.txt"),
            unit_text("cavalry", "western", 7),
        )
        .unwrap();
        fs::write(
            root.join("common/units/eastern_medieval_infantry.txt"),
            unit_text("infantry", "eastern", 9),
        )
        .unwrap();
        fs::write(
            root.join("common/units/later_infantry.txt"),
            unit_text("infantry", "western", 8),
        )
        .unwrap();

        let content = ContentRoot::new(&root);
        let rules = load(
            &content,
            DEFINES,
            Path::new("defines.lua"),
            "western",
            &[
                "western_medieval_knights",
                "western_medieval_infantry",
                "chevauchee",
            ],
        )
        .unwrap();
        assert_eq!(rules.tactics, Money::new(5, 1));
        assert_eq!(rules.max_morale, Money::from(2));
        assert_eq!(rules.infantry.fire_damage, Money::new(25, 2));
        assert_eq!(rules.infantry.offensive_shock, 1);
        assert_eq!(rules.cavalry.offensive_shock, 7);
        assert_eq!(rules.cavalry.shock_damage, Money::new(8, 1));

        let fallback = load(
            &content,
            DEFINES,
            Path::new("defines.lua"),
            "western",
            NO_HISTORY,
        )
        .unwrap();
        assert_eq!(fallback.infantry.offensive_shock, 3);
        assert_eq!(fallback.cavalry.offensive_shock, 7);

        let error = load(
            &content,
            DEFINES,
            Path::new("defines.lua"),
            "eastern",
            NO_HISTORY,
        )
        .unwrap_err();
        let _ = fs::remove_dir_all(&root);
        let message = error.to_string();
        assert!(
            message.contains("technology 1 does not enable a cavalry unit for eastern"),
            "{message}"
        );
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

        let rules = load(
            &ContentRoot::new(&root),
            DEFINES,
            Path::new("defines.lua"),
            "western",
            NO_HISTORY,
        )
        .unwrap();
        let _ = fs::remove_dir_all(&root);
        assert_eq!(rules.max_morale, Money::ZERO);
    }

    fn unit_text(kind: &str, group: &str, shock: u32) -> String {
        format!(
            "type = {kind}\nunit_type = {group}\nmaneuver = 1\noffensive_morale = 0\ndefensive_morale = 0\noffensive_fire = 0\ndefensive_fire = 0\noffensive_shock = {shock}\ndefensive_shock = 0\n"
        )
    }

    const NO_HISTORY: &[&str] = &[];

    const DEFINES: &str = r#"
        BASE_COMBAT_WIDTH = 15,
        MORALE_RECOVERY_SPEED = 0.15,
        MORALE_RECOVERY_SPEED_OWN_TERRITORY = 0.05,
        SHATTERED_RETREAT_SPEED_MODIFIER = 0.5,
        WAR_LENGTH_DAMAGE_MODIFIER = 0.01,
    "#;
}
