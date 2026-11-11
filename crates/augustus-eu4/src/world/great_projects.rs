// SPDX-License-Identifier: GPL-3.0-only

//! 开局奇观的国家驻军规模。
//!
//! 只读取 `date` 不晚于 1444.11.11 的项目，取其 `starting_tier` 对应档的
//! `country_modifiers.garrison_size`。产出「奇观所在省份 → 千分比」。
//! 同一省份有多处时相加。1444 只有马里恩堡一处，加法沿用引擎写入时的相加，没有第二处实测。
//!
//! 理念、政体改革、特权、事件和省份 `local_garrison_size` 不读。
//! 这些来源当前没有加载器，1444 开局也不生效。

use std::collections::{BTreeMap, BTreeSet};

use augustus_core::Money;
use jomini::{Windows1252Encoding, common::Date, text::ValueReader};

use crate::{
    Error,
    content::{ContentFile, ContentRoot},
    script,
};

pub(super) fn load(content: &ContentRoot) -> Result<BTreeMap<u32, i32>, Error> {
    let files = match content.text_files("common/great_projects") {
        Ok(files) => files,
        Err(Error::Read { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
            return Ok(BTreeMap::new());
        }
        Err(error) => return Err(error),
    };
    let opening = Date::parse(super::START_DATE.as_bytes()).expect("固定开局日期必须有效");
    let mut seen = BTreeSet::new();
    let mut by_province = BTreeMap::new();
    for path in files {
        let file = content.read(path)?;
        let tape = script::parse(&file)?;
        for (key, operator, value) in tape.windows1252_reader().fields() {
            let id = key.read_str().into_owned();
            script::equal(&file, operator, &id)?;
            if !seen.insert(id.clone()) {
                return Err(Error::invalid(
                    &file.path,
                    format!("duplicate great project: {id}"),
                ));
            }
            let object = value
                .read_object()
                .map_err(|error| Error::invalid(&file.path, error.to_string()))?;
            let mut start = None;
            let mut built = None;
            let mut starting_tier = None;
            let mut tiers: BTreeMap<u32, Option<i32>> = BTreeMap::new();
            for (key, operator, value) in object.fields() {
                let name = key.read_str();
                if let Some(tier) = name
                    .strip_prefix("tier_")
                    .and_then(|tier| tier.parse().ok())
                {
                    script::equal(&file, operator, &name)?;
                    tiers.insert(tier, tier_garrison(&value, &file)?);
                    continue;
                }
                match name.as_ref() {
                    "start" => {
                        script::equal(&file, operator, &name)?;
                        start = Some(whole_u32(&value, &file, "start")?);
                    }
                    "date" => {
                        script::equal(&file, operator, &name)?;
                        let text = value
                            .read_string()
                            .map_err(|error| Error::invalid(&file.path, error.to_string()))?;
                        built = Some(Date::parse(text.as_bytes()).map_err(|_| {
                            Error::invalid(&file.path, format!("{id} has an invalid date"))
                        })?);
                    }
                    "starting_tier" => {
                        script::equal(&file, operator, &name)?;
                        starting_tier = Some(whole_u32(&value, &file, "starting_tier")?);
                    }
                    _ => (),
                }
            }
            let start = start
                .ok_or_else(|| Error::invalid(&file.path, format!("{id} has no start province")))?;
            let built =
                built.ok_or_else(|| Error::invalid(&file.path, format!("{id} has no date")))?;
            let starting_tier = starting_tier
                .ok_or_else(|| Error::invalid(&file.path, format!("{id} has no starting tier")))?;
            // 晚于开局才建成的项目不参与 1444 驻军。
            if built > opening {
                continue;
            }
            let garrison = tiers.get(&starting_tier).ok_or_else(|| {
                Error::invalid(&file.path, format!("{id} is missing tier_{starting_tier}"))
            })?;
            let Some(permille) = *garrison else {
                continue;
            };
            if permille == 0 {
                continue;
            }
            by_province
                .entry(start)
                .and_modify(|total: &mut i32| *total = total.saturating_add(permille))
                .or_insert(permille);
        }
    }
    Ok(by_province)
}

/// 只取该档 `country_modifiers.garrison_size`。没有这项时返回 None。
fn tier_garrison(
    value: &ValueReader<'_, '_, Windows1252Encoding>,
    file: &ContentFile,
) -> Result<Option<i32>, Error> {
    let mut garrison = None;
    for (key, operator, value) in value
        .read_object()
        .map_err(|error| Error::invalid(&file.path, error.to_string()))?
        .fields()
    {
        if key.read_str() != "country_modifiers" {
            continue;
        }
        script::equal(file, operator, "country_modifiers")?;
        for (key, operator, value) in value
            .read_object()
            .map_err(|error| Error::invalid(&file.path, error.to_string()))?
            .fields()
        {
            if key.read_str() != "garrison_size" {
                continue;
            }
            script::equal(file, operator, "garrison_size")?;
            garrison = Some(permille(&value, file)?);
        }
    }
    Ok(garrison)
}

fn permille(
    value: &ValueReader<'_, '_, Windows1252Encoding>,
    file: &ContentFile,
) -> Result<i32, Error> {
    let amount: Money = value
        .read_string()
        .map_err(|error| Error::invalid(&file.path, error.to_string()))?
        .parse()
        .map_err(|error| Error::invalid(&file.path, format!("invalid garrison_size: {error}")))?;
    // 引擎把 1.0 存成 1000。0.15 是 150。小数向零截断。
    let scaled = (amount * Money::from(1_000)).trunc();
    i32::try_from(scaled)
        .map_err(|_| Error::invalid(&file.path, "garrison_size is outside the supported range"))
}

fn whole_u32(
    value: &ValueReader<'_, '_, Windows1252Encoding>,
    file: &ContentFile,
    label: &str,
) -> Result<u32, Error> {
    value
        .read_scalar()
        .ok()
        .and_then(|value| value.to_u64().ok())
        .and_then(|value| u32::try_from(value).ok())
        .filter(|value| *value > 0 || label == "starting_tier")
        .ok_or_else(|| Error::invalid(&file.path, format!("{label} must be an integer")))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    use crate::content::ContentRoot;

    use super::load;

    fn root() -> std::path::PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        std::env::temp_dir().join(format!(
            "augustus-great-projects-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn reads_starting_tier_country_garrison_and_skips_later_dates() {
        let root = root();
        fs::create_dir_all(root.join("common/great_projects")).unwrap();
        fs::write(
            root.join("common/great_projects/01_monuments.txt"),
            "\
malbork_castle = {
    start = 1841
    date = 1300.01.01
    starting_tier = 2
    can_use_modifiers_trigger = { always = yes }
    tier_1 = {
        country_modifiers = { garrison_size = 0.10 }
    }
    tier_2 = {
        province_modifiers = {
            local_defensiveness = 1
            local_garrison_size = 0.50
        }
        country_modifiers = {
            legitimacy = 0.5
            garrison_size = 0.15
        }
    }
}
future_castle = {
    start = 126
    date = 1444.11.12
    starting_tier = 1
    tier_1 = { country_modifiers = { garrison_size = 0.25 } }
}
same_day = {
    start = 41
    date = 1444.11.11
    starting_tier = 1
    tier_1 = { country_modifiers = { garrison_size = 0.10 } }
}
late_incomplete = {
    start = 50
    date = 1500.01.01
    starting_tier = 3
}
",
        )
        .unwrap();
        let loaded = load(&ContentRoot::new(&root));
        let _ = fs::remove_dir_all(&root);
        let loaded = loaded.unwrap();
        assert_eq!(loaded.get(&1841).copied(), Some(150));
        assert_eq!(loaded.get(&41).copied(), Some(100));
        assert!(!loaded.contains_key(&126));
        assert!(!loaded.contains_key(&50));
    }

    #[test]
    fn truncates_toward_zero_and_omits_a_tier_without_garrison_size() {
        let root = root();
        fs::create_dir_all(root.join("common/great_projects")).unwrap();
        fs::write(
            root.join("common/great_projects/01_monuments.txt"),
            "\
fraction = {
    start = 12
    date = 1444.11.11
    starting_tier = 1
    tier_1 = { country_modifiers = { garrison_size = 0.1599 } }
}
negative = {
    start = 13
    date = 100.01.01
    starting_tier = 1
    tier_1 = { country_modifiers = { garrison_size = -0.1599 } }
}
zero = {
    start = 14
    date = 1444.01.01
    starting_tier = 1
    tier_1 = { country_modifiers = { garrison_size = 0 } }
}
province_only = {
    start = 15
    date = 1444.01.01
    starting_tier = 1
    tier_1 = { province_modifiers = { local_garrison_size = 0.20 } }
}
",
        )
        .unwrap();
        let loaded = load(&ContentRoot::new(&root));
        let _ = fs::remove_dir_all(&root);
        let loaded = loaded.unwrap();
        assert_eq!(loaded.get(&12).copied(), Some(159));
        assert_eq!(loaded.get(&13).copied(), Some(-159));
        assert!(!loaded.contains_key(&14));
        assert!(!loaded.contains_key(&15));
    }

    #[test]
    fn duplicate_project_ids_are_rejected() {
        let root = root();
        fs::create_dir_all(root.join("common/great_projects")).unwrap();
        fs::write(
            root.join("common/great_projects/00_projects.txt"),
            "\
shared = {
    start = 10
    date = 1300.01.01
    starting_tier = 1
    tier_1 = { country_modifiers = { garrison_size = 0.10 } }
}
",
        )
        .unwrap();
        fs::write(
            root.join("common/great_projects/01_monuments.txt"),
            "\
shared = {
    start = 11
    date = 1895.06.20
    starting_tier = 1
    tier_1 = { country_modifiers = { garrison_size = 0.20 } }
}
",
        )
        .unwrap();
        let loaded = load(&ContentRoot::new(&root));
        let _ = fs::remove_dir_all(&root);
        let message = loaded.unwrap_err().to_string();
        assert!(
            message.contains("duplicate great project: shared"),
            "{message}"
        );
    }

    #[test]
    fn missing_starting_tier_block_is_an_error() {
        let root = root();
        fs::create_dir_all(root.join("common/great_projects")).unwrap();
        fs::write(
            root.join("common/great_projects/01_monuments.txt"),
            "\
broken = {
    start = 10
    date = 1300.01.01
    starting_tier = 2
    tier_1 = { country_modifiers = { garrison_size = 0.10 } }
}
",
        )
        .unwrap();
        let loaded = load(&ContentRoot::new(&root));
        let _ = fs::remove_dir_all(&root);
        let message = loaded.unwrap_err().to_string();
        assert!(message.contains("broken is missing tier_2"), "{message}");
    }

    #[test]
    fn sums_monuments_that_share_a_province() {
        let root = root();
        fs::create_dir_all(root.join("common/great_projects")).unwrap();
        fs::write(
            root.join("common/great_projects/00_projects.txt"),
            "\
first = {
    start = 10
    date = 1300.01.01
    starting_tier = 0
    tier_0 = { country_modifiers = { garrison_size = 0.10 } }
}
second = {
    start = 10
    date = 1400.01.01
    starting_tier = 1
    tier_1 = { country_modifiers = { garrison_size = 0.05 } }
}
",
        )
        .unwrap();
        let loaded = load(&ContentRoot::new(&root));
        let _ = fs::remove_dir_all(&root);
        assert_eq!(loaded.unwrap().get(&10).copied(), Some(150));
    }

    #[test]
    fn missing_directory_is_empty() {
        let root = root();
        fs::create_dir_all(&root).unwrap();
        let loaded = load(&ContentRoot::new(&root));
        let _ = fs::remove_dir_all(&root);
        assert!(loaded.unwrap().is_empty());
    }
}
