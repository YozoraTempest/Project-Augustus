// SPDX-License-Identifier: GPL-3.0-only

use std::collections::{BTreeMap, BTreeSet};

use augustus_core::{BuildingDefinition, Country, CourtPerson, DiplomaticTie, Province};
use jomini::{Windows1252Encoding, common::Date, text::ValueReader};

use crate::{
    Error,
    content::{ContentFile, ContentRoot},
    script,
};

pub(super) struct CountryHistory {
    pub conflicts: BTreeSet<String>,
    /// 开局日期及之前最后一次写出的科技组和兵种组。没有历史文件的国家不在这里。
    pub groups: BTreeMap<String, CountryGroups>,
    /// 开局日期及之前仍然立着的国家旗。历史没写过的旗不在这里。
    pub flags: BTreeMap<String, BTreeSet<String>>,
}

pub(super) struct CountryGroups {
    pub technology_group: Option<String>,
    pub unit_type: Option<String>,
}

/// 顾问类型和顾问池。不是已经雇上的人。
#[derive(Debug)]
pub(super) struct AdvisorBook {
    types: BTreeMap<String, AdvisorPower>,
    pool: Vec<PooledAdvisor>,
}

impl AdvisorBook {
    pub(super) fn stored(&self) -> usize {
        self.types.len() + self.pool.len()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AdvisorPower {
    Adm,
    Dip,
    Mil,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct PooledAdvisor {
    pub name: String,
    pub skill: u32,
    pub kind: String,
    pub power: AdvisorPower,
}

pub(super) fn apply_countries(
    content: &ContentRoot,
    countries: &mut BTreeMap<String, Country>,
) -> Result<CountryHistory, Error> {
    let files = match content.text_files("history/countries") {
        Ok(files) => files,
        // 特殊国家和自制内容可以没有国家历史，未提供的属性保留 None。
        Err(Error::Read { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
            return Ok(CountryHistory {
                conflicts: BTreeSet::new(),
                groups: BTreeMap::new(),
                flags: BTreeMap::new(),
            });
        }
        Err(error) => return Err(error),
    };
    let mut loaded = BTreeSet::new();
    let mut conflicts = BTreeSet::new();
    let mut groups = BTreeMap::new();
    let mut flags = BTreeMap::new();
    let at = Date::parse(super::START_DATE).expect("固定开局日期必须有效");
    for path in files {
        let file = content.read(path)?;
        let tag = history_id(&file)?;
        let target = countries.get_mut(&tag).ok_or_else(|| {
            Error::invalid(
                &file.path,
                format!("history references unknown country {tag}"),
            )
        })?;
        let mut country = Country::new(&tag);
        let mut technology_group = None;
        let mut unit_type = None;
        let tape = script::parse(&file)?;
        let country_flags = country_flags(&file, &tape, at)?;
        for block in script::history_blocks(&file, &tape, at)? {
            for (key, operator, value) in block.fields() {
                let name = key.read_str();
                script::equal(&file, operator, &name)?;
                if name == "monarch" || name == "heir" {
                    let slot = if name == "monarch" {
                        &mut country.monarch
                    } else {
                        &mut country.heir
                    };
                    apply_court_person(&file, &value, slot)?;
                    continue;
                }
                if !matches!(
                    name.as_ref(),
                    "capital"
                        | "primary_culture"
                        | "religion"
                        | "secondary_religion"
                        | "technology_group"
                        | "unit_type"
                        | "government"
                        | "add_government_reform"
                        | "historical_rival"
                        | "historical_friend"
                        | "add_historical_rival"
                        | "add_historical_friend"
                ) {
                    continue;
                }
                if name == "capital" {
                    country.capital = Some(
                        u32::try_from(integer(&value, &file)?)
                            .ok()
                            .filter(|id| *id > 0)
                            .ok_or_else(|| {
                                Error::invalid(&file.path, "capital must be a positive province ID")
                            })?,
                    );
                } else {
                    let text = optional_text(&value, &file)?;
                    match name.as_ref() {
                        "primary_culture" => country.primary_culture = text,
                        "religion" => country.religion = text,
                        "secondary_religion" => country.secondary_religion = text,
                        "technology_group" => technology_group = text,
                        "unit_type" => unit_type = text,
                        "government" => country.government = text,
                        "add_government_reform" => {
                            if let Some(reform) = text {
                                country.government_reforms.push(reform);
                            }
                        }
                        "historical_rival" | "add_historical_rival" => {
                            if let Some(other) = text {
                                country.diplomacy.push(DiplomaticTie {
                                    label: "historical_rival".to_owned(),
                                    other,
                                    icon: "GFX_rival_indication".to_owned(),
                                });
                            }
                        }
                        "historical_friend" | "add_historical_friend" => {
                            if let Some(other) = text {
                                country.diplomacy.push(DiplomaticTie {
                                    label: "historical_friend".to_owned(),
                                    other,
                                    icon: "GFX_diplomacy_action_relation".to_owned(),
                                });
                            }
                        }
                        _ => unreachable!(),
                    }
                }
            }
        }
        let origin = CountryGroups {
            technology_group,
            unit_type,
        };
        if loaded.insert(tag.clone()) {
            target.capital = country.capital;
            target.primary_culture = country.primary_culture;
            target.religion = country.religion;
            target.secondary_religion = country.secondary_religion;
            target.monarch = country.monarch;
            target.heir = country.heir;
            target.government = country.government;
            target.government_reforms = country.government_reforms;
            target.diplomacy = country.diplomacy;
            groups.insert(tag.clone(), origin);
            flags.insert(tag.clone(), country_flags);
        } else {
            let previous = groups.get(&tag).expect("重复历史仍有第一次的科技组");
            if previous.technology_group != origin.technology_group
                || previous.unit_type != origin.unit_type
            {
                return Err(Error::invalid(
                    &file.path,
                    format!("conflicting technology group or unit type for {tag}"),
                ));
            }
            // 原版 SOF 存在两份不同历史；仅保留所有候选一致的字段，不猜测加载优先级。
            if target.capital != country.capital {
                target.capital = None;
                conflicts.insert(tag.clone());
            }
            if target.primary_culture != country.primary_culture {
                target.primary_culture = None;
                conflicts.insert(tag.clone());
            }
            if target.religion != country.religion {
                target.religion = None;
                conflicts.insert(tag.clone());
            }
            if target.secondary_religion != country.secondary_religion {
                target.secondary_religion = None;
                conflicts.insert(tag.clone());
            }
            if target.monarch != country.monarch {
                target.monarch = None;
                conflicts.insert(tag.clone());
            }
            if target.heir != country.heir {
                target.heir = None;
                conflicts.insert(tag.clone());
            }
            if target.government != country.government {
                target.government = None;
                conflicts.insert(tag.clone());
            }
            if target.government_reforms != country.government_reforms {
                target.government_reforms.clear();
                conflicts.insert(tag.clone());
            }
            if target.diplomacy != country.diplomacy {
                target.diplomacy.clear();
                conflicts.insert(tag);
            }
        }
    }
    Ok(CountryHistory {
        conflicts,
        groups,
        flags,
    })
}

pub(super) fn apply_provinces(
    content: &ContentRoot,
    countries: &BTreeMap<String, [u8; 3]>,
    provinces: &mut BTreeMap<u32, Province>,
    buildings: &BTreeMap<String, BuildingDefinition>,
) -> Result<BTreeSet<u32>, Error> {
    let mut loaded = BTreeSet::new();
    let at = Date::parse(super::START_DATE).expect("固定开局日期必须有效");
    for path in content.text_files("history/provinces")? {
        let file = content.read(path)?;
        let id = history_id(&file)?
            .parse::<u32>()
            .map_err(|_| Error::invalid(&file.path, "invalid province history filename"))?;
        let province = provinces.get_mut(&id).ok_or_else(|| {
            Error::invalid(
                &file.path,
                format!("history references unknown province {id}"),
            )
        })?;
        if !loaded.insert(id) {
            return Err(Error::invalid(
                &file.path,
                format!("duplicate province history: {id}"),
            ));
        }
        let tape = script::parse(&file)?;
        let mut controller_supplied = false;
        for block in script::history_blocks(&file, &tape, at)? {
            for (key, operator, value) in block.fields() {
                let name = key.read_str();
                match name.as_ref() {
                    "owner" | "controller" => {
                        script::equal(&file, operator, &name)?;
                        let tag = optional_text(&value, &file)?;
                        if let Some(tag) = &tag
                            && !countries.contains_key(tag)
                        {
                            return Err(Error::invalid(
                                &file.path,
                                format!("unknown province {name}: {tag}"),
                            ));
                        }
                        if name == "owner" {
                            province.owner = tag;
                        } else {
                            province.controller = tag;
                            controller_supplied = true;
                        }
                    }
                    "culture" | "religion" | "trade_goods" => {
                        script::equal(&file, operator, &name)?;
                        let slot = match name.as_ref() {
                            "culture" => &mut province.culture,
                            "religion" => &mut province.religion,
                            _ => &mut province.trade_good,
                        };
                        *slot = optional_text(&value, &file)?;
                    }
                    "base_tax"
                    | "add_base_tax"
                    | "base_production"
                    | "add_base_production"
                    | "base_manpower"
                    | "add_base_manpower" => {
                        script::equal(&file, operator, &name)?;
                        let slot = match name.as_ref() {
                            "base_tax" | "add_base_tax" => &mut province.development.tax,
                            "base_production" | "add_base_production" => {
                                &mut province.development.production
                            }
                            _ => &mut province.development.manpower,
                        };
                        let value = i128::from(integer(&value, &file)?);
                        let value = if name.starts_with("add_") {
                            i128::from(slot.ok_or_else(|| {
                                Error::invalid(
                                    &file.path,
                                    format!("{name} requires an initial value"),
                                )
                            })?) + value
                        } else {
                            value
                        };
                        *slot = Some(u32::try_from(value).map_err(|_| {
                            Error::invalid(
                                &file.path,
                                format!(
                                    "{name}: development must be a nonnegative integer in range"
                                ),
                            )
                        })?);
                    }
                    _ if buildings.contains_key(name.as_ref()) => {
                        script::equal(&file, operator, &name)?;
                        let present = value
                            .read_scalar()
                            .map_err(|error| Error::invalid(&file.path, error.to_string()))?
                            .to_bool()
                            .map_err(|error| Error::invalid(&file.path, error.to_string()))?;
                        if present {
                            province.buildings.insert(name.into_owned());
                        } else {
                            province.buildings.remove(name.as_ref());
                        }
                    }
                    _ => (),
                }
            }
        }
        // 没有单独指定控制者时，初始控制权随所有者；显式 --- 不会被覆盖。
        if !controller_supplied {
            province.controller = province.owner.clone();
        }
    }
    Ok(loaded)
}

pub(super) fn load_advisors(content: &ContentRoot) -> Result<AdvisorBook, Error> {
    let types = advisor_types(content)?;
    let mut pool = Vec::new();
    for path in content.text_files("history/advisors")? {
        let file = content.read(path)?;
        let tape = script::parse(&file)?;
        for (key, operator, value) in tape.windows1252_reader().fields() {
            if key.read_str() != "advisor" {
                continue;
            }
            script::equal(&file, operator, "advisor")?;
            let object = value
                .read_object()
                .map_err(|error| Error::invalid(&file.path, error.to_string()))?;
            let mut name = None;
            let mut skill = None;
            let mut kind = None;
            for (key, operator, value) in object.fields() {
                let field = key.read_str();
                match field.as_ref() {
                    "name" | "skill" | "type" => script::equal(&file, operator, &field)?,
                    _ => continue,
                }
                match field.as_ref() {
                    "name" => name = Some(text_value(&file, &value, "name")?),
                    "skill" => skill = Some(whole(&file, &value, "skill")?),
                    "type" => kind = Some(text_value(&file, &value, "type")?),
                    _ => unreachable!(),
                }
            }
            let name =
                name.ok_or_else(|| Error::invalid(&file.path, "advisor is missing a name"))?;
            let skill = skill
                .ok_or_else(|| Error::invalid(&file.path, format!("{name} is missing skill")))?;
            let kind = kind
                .ok_or_else(|| Error::invalid(&file.path, format!("{name} is missing a type")))?;
            let power = types.get(&kind).copied().ok_or_else(|| {
                Error::invalid(
                    &file.path,
                    format!("{name} has unknown advisor type {kind}"),
                )
            })?;
            pool.push(PooledAdvisor {
                name,
                skill,
                kind,
                power,
            });
        }
    }
    Ok(AdvisorBook { types, pool })
}

fn advisor_types(content: &ContentRoot) -> Result<BTreeMap<String, AdvisorPower>, Error> {
    let mut types = BTreeMap::new();
    for path in content.text_files("common/advisortypes")? {
        let file = content.read(path)?;
        let tape = script::parse(&file)?;
        for (key, operator, value) in tape.windows1252_reader().fields() {
            let name = key.read_str().into_owned();
            script::equal(&file, operator, &name)?;
            let object = value
                .read_object()
                .map_err(|error| Error::invalid(&file.path, error.to_string()))?;
            let mut power = None;
            for (key, operator, value) in object.fields() {
                if key.read_str() != "monarch_power" {
                    continue;
                }
                script::equal(&file, operator, "monarch_power")?;
                if power.is_some() {
                    return Err(Error::invalid(
                        &file.path,
                        format!("{name} specifies monarch_power twice"),
                    ));
                }
                let text = text_value(&file, &value, "monarch_power")?;
                power = Some(match text.as_str() {
                    "ADM" => AdvisorPower::Adm,
                    "DIP" => AdvisorPower::Dip,
                    "MIL" => AdvisorPower::Mil,
                    _ => {
                        return Err(Error::invalid(
                            &file.path,
                            format!("{name} has unknown monarch_power {text}"),
                        ));
                    }
                });
            }
            let power = power.ok_or_else(|| {
                Error::invalid(&file.path, format!("{name} is missing monarch_power"))
            })?;
            if types.insert(name.clone(), power).is_some() {
                return Err(Error::invalid(
                    &file.path,
                    format!("duplicate advisor type {name}"),
                ));
            }
        }
    }
    Ok(types)
}

fn apply_court_person(
    file: &ContentFile,
    value: &jomini::text::ValueReader<'_, '_, jomini::Windows1252Encoding>,
    slot: &mut Option<CourtPerson>,
) -> Result<(), Error> {
    let object = value
        .read_object()
        .map_err(|error| Error::invalid(&file.path, error.to_string()))?;
    let mut name = None;
    let mut adm = None;
    let mut dip = None;
    let mut mil = None;
    for (key, operator, value) in object.fields() {
        let field = key.read_str();
        let kind = field.to_ascii_lowercase();
        match kind.as_str() {
            "name" | "adm" | "dip" | "mil" => script::equal(file, operator, &field)?,
            _ => continue,
        }
        match kind.as_str() {
            "name" => name = Some(text_value(file, &value, "name")?),
            "adm" => adm = Some(whole(file, &value, "adm")?),
            "dip" => dip = Some(whole(file, &value, "dip")?),
            "mil" => mil = Some(whole(file, &value, "mil")?),
            _ => unreachable!(),
        }
    }
    if let Some(name) = name {
        *slot = Some(CourtPerson {
            name,
            adm: adm.ok_or_else(|| Error::invalid(&file.path, "person is missing adm"))?,
            dip: dip.ok_or_else(|| Error::invalid(&file.path, "person is missing dip"))?,
            mil: mil.ok_or_else(|| Error::invalid(&file.path, "person is missing mil"))?,
        });
        return Ok(());
    }
    // 摄政只改能力，不另造一个没有名字的人。
    if let Some(person) = slot.as_mut() {
        if let Some(value) = adm {
            person.adm = value;
        }
        if let Some(value) = dip {
            person.dip = value;
        }
        if let Some(value) = mil {
            person.mil = value;
        }
    }
    Ok(())
}

fn text_value(
    file: &ContentFile,
    value: &jomini::text::ValueReader<'_, '_, jomini::Windows1252Encoding>,
    field: &str,
) -> Result<String, Error> {
    value
        .read_string()
        .map_err(|error| Error::invalid(&file.path, format!("{field}: {error}")))
}

fn whole(
    file: &ContentFile,
    value: &jomini::text::ValueReader<'_, '_, jomini::Windows1252Encoding>,
    field: &str,
) -> Result<u32, Error> {
    let scalar = value
        .read_scalar()
        .map_err(|error| Error::invalid(&file.path, error.to_string()))?;
    scalar
        .to_u64()
        .ok()
        .and_then(|number| u32::try_from(number).ok())
        .ok_or_else(|| Error::invalid(&file.path, format!("{field} must be a nonnegative integer")))
}

fn country_flags(
    file: &ContentFile,
    tape: &jomini::TextTape<'_>,
    at: Date,
) -> Result<BTreeSet<String>, Error> {
    let mut flags = BTreeSet::new();
    for block in script::history_blocks(file, tape, at)? {
        for (key, operator, value) in block.fields() {
            let name = key.read_str();
            if name != "set_country_flag" && name != "clr_country_flag" {
                continue;
            }
            script::equal(file, operator, &name)?;
            let Some(flag) = optional_text(&value, file)? else {
                continue;
            };
            if name == "set_country_flag" {
                flags.insert(flag);
            } else {
                flags.remove(&flag);
            }
        }
    }
    Ok(flags)
}

fn history_id(file: &ContentFile) -> Result<String, Error> {
    file.path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .and_then(|stem| stem.split([' ', '-']).next())
        .filter(|id| !id.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| Error::invalid(&file.path, "invalid history filename"))
}

fn optional_text(
    value: &ValueReader<'_, '_, Windows1252Encoding>,
    file: &ContentFile,
) -> Result<Option<String>, Error> {
    let text = value
        .read_string()
        .map_err(|error| Error::invalid(&file.path, error.to_string()))?;
    Ok((text != "---" && !text.is_empty()).then_some(text))
}

fn integer(
    value: &ValueReader<'_, '_, Windows1252Encoding>,
    file: &ContentFile,
) -> Result<i64, Error> {
    value
        .read_scalar()
        .map_err(|error| Error::invalid(&file.path, error.to_string()))?
        .to_i64()
        .map_err(|error| Error::invalid(&file.path, error.to_string()))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    use augustus_core::Country;

    use crate::content::ContentRoot;

    use super::apply_countries;

    #[test]
    fn unit_type_overrides_technology_group_until_the_start_date() {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "augustus-unit-group-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("history/countries")).unwrap();
        fs::write(
            root.join("history/countries/FRA - France.txt"),
            r#"
                technology_group = eastern
                unit_type = western
                1400.1.1 = { unit_type = muslim }
                1500.1.1 = { unit_type = ottoman }
            "#,
        )
        .unwrap();
        let mut countries = BTreeMap::from([("FRA".to_owned(), Country::new("FRA"))]);
        let history = apply_countries(&ContentRoot::new(&root), &mut countries).unwrap();
        let _ = fs::remove_dir_all(&root);
        let groups = history.groups.get("FRA").unwrap();
        assert_eq!(groups.technology_group.as_deref(), Some("eastern"));
        // 1444 之前最后一次 unit_type 覆盖科技组；开局之后的日期不生效。
        assert_eq!(groups.unit_type.as_deref(), Some("muslim"));
    }

    #[test]
    fn secondary_religion_stops_at_the_start_date() {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "augustus-secondary-religion-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("history/countries")).unwrap();
        fs::write(
            root.join("history/countries/HMI - Kara Del.txt"),
            r#"
                religion = tengri_pagan_reformed
                secondary_religion = orthodox
                1400.1.1 = { secondary_religion = vajrayana }
                1500.1.1 = { secondary_religion = shiite }
            "#,
        )
        .unwrap();
        let mut countries = BTreeMap::from([("HMI".to_owned(), Country::new("HMI"))]);
        apply_countries(&ContentRoot::new(&root), &mut countries).unwrap();
        let _ = fs::remove_dir_all(&root);
        let country = countries.get("HMI").unwrap();
        assert_eq!(country.religion.as_deref(), Some("tengri_pagan_reformed"));
        assert_eq!(country.secondary_religion.as_deref(), Some("vajrayana"));
    }

    #[test]
    fn monarch_and_heir_keep_the_last_entry_on_or_before_the_start_date() {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "augustus-court-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("history/countries")).unwrap();
        fs::write(
            root.join("history/countries/TUR - Ottomans.txt"),
            r#"
                1440.1.1 = {
                    monarch = { name = "Murad II" adm = 3 dip = 2 mil = 1 }
                    heir = { name = "Mehmed" adm = 6 dip = 4 mil = 6 }
                }
                1444.1.1 = {
                    monarch = { name = "Mehmed II Fatih" adm = 6 dip = 4 mil = 6 }
                }
                1446.1.1 = {
                    monarch = { name = "Later" adm = 0 dip = 0 mil = 0 }
                    heir = { name = "Later Heir" adm = 1 dip = 1 mil = 1 }
                }
            "#,
        )
        .unwrap();
        let mut countries = BTreeMap::from([("TUR".to_owned(), Country::new("TUR"))]);
        apply_countries(&ContentRoot::new(&root), &mut countries).unwrap();
        let _ = fs::remove_dir_all(&root);
        let country = countries.get("TUR").unwrap();
        let monarch = country.monarch.as_ref().unwrap();
        assert_eq!(monarch.name, "Mehmed II Fatih");
        assert_eq!((monarch.adm, monarch.dip, monarch.mil), (6, 4, 6));
        let heir = country.heir.as_ref().unwrap();
        assert_eq!(heir.name, "Mehmed");
        assert_eq!((heir.adm, heir.dip, heir.mil), (6, 4, 6));
    }

    #[test]
    fn government_keeps_reforms_on_or_before_the_start_date() {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "augustus-government-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("history/countries")).unwrap();
        fs::write(
            root.join("history/countries/TUR - Ottomans.txt"),
            r#"
                government = monarchy
                add_government_reform = ottoman_government
                1400.1.1 = { add_government_reform = feudalism_reform }
                1459.1.1 = { government_rank = 3 government = republic }
            "#,
        )
        .unwrap();
        let mut countries = BTreeMap::from([("TUR".to_owned(), Country::new("TUR"))]);
        apply_countries(&ContentRoot::new(&root), &mut countries).unwrap();
        let _ = fs::remove_dir_all(&root);
        let country = countries.get("TUR").unwrap();
        assert_eq!(country.government.as_deref(), Some("monarchy"));
        assert_eq!(
            country.government_reforms,
            [
                "ottoman_government".to_owned(),
                "feudalism_reform".to_owned()
            ]
        );
    }

    #[test]
    fn advisors_stay_in_the_pool() {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "augustus-advisors-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("common/advisortypes")).unwrap();
        fs::create_dir_all(root.join("history/advisors")).unwrap();
        fs::write(
            root.join("common/advisortypes/00_advisortypes.txt"),
            "philosopher = { monarch_power = ADM }\nstatesman = { monarch_power = DIP }\n",
        )
        .unwrap();
        fs::write(
            root.join("history/advisors/Europe.txt"),
            r#"
                advisor = { name = "Kosem" skill = 1 type = statesman date = 1623.1.1 }
            "#,
        )
        .unwrap();
        let book = super::load_advisors(&ContentRoot::new(&root)).unwrap();
        let _ = fs::remove_dir_all(&root);
        assert_eq!(book.pool.len(), 1);
        assert_eq!(book.pool[0].name, "Kosem");
        assert_eq!(book.pool[0].skill, 1);
        assert_eq!(book.pool[0].power, super::AdvisorPower::Dip);
        assert!(book.types.contains_key("philosopher"));
    }
}
