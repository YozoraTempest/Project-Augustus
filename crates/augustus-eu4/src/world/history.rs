// SPDX-License-Identifier: GPL-3.0-only

use std::collections::{BTreeMap, BTreeSet};

use augustus_core::{Country, Province};
use jomini::{Windows1252Encoding, common::Date, text::ValueReader};

use crate::{
    Error,
    content::{ContentFile, ContentRoot},
    script,
};

pub(super) fn apply_countries(
    content: &ContentRoot,
    countries: &mut BTreeMap<String, Country>,
) -> Result<BTreeSet<String>, Error> {
    let files = match content.text_files("history/countries") {
        Ok(files) => files,
        // 特殊国家和自制内容可以没有国家历史，未提供的属性保留 None。
        Err(Error::Read { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
            return Ok(BTreeSet::new());
        }
        Err(error) => return Err(error),
    };
    let mut loaded = BTreeSet::new();
    let mut conflicts = BTreeSet::new();
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
        let tape = script::parse(&file)?;
        for block in script::history_blocks(&file, &tape, at)? {
            for (key, operator, value) in block.fields() {
                let name = key.read_str();
                if !matches!(name.as_ref(), "capital" | "primary_culture" | "religion") {
                    continue;
                }
                script::equal(&file, operator, &name)?;
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
                        _ => unreachable!(),
                    }
                }
            }
        }
        if loaded.insert(tag.clone()) {
            target.capital = country.capital;
            target.primary_culture = country.primary_culture;
            target.religion = country.religion;
        } else {
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
                conflicts.insert(tag);
            }
        }
    }
    Ok(conflicts)
}

pub(super) fn apply_provinces(
    content: &ContentRoot,
    countries: &BTreeMap<String, [u8; 3]>,
    provinces: &mut BTreeMap<u32, Province>,
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
