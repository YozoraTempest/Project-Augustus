// SPDX-License-Identifier: GPL-3.0-only

use std::collections::{BTreeMap, BTreeSet};

use augustus_core::Province;
use jomini::{Windows1252Encoding, common::Date, text::ObjectReader};

use crate::{
    Error,
    content::{ContentFile, ContentRoot},
    script,
};

pub(super) fn apply(
    content: &ContentRoot,
    countries: &BTreeMap<String, [u8; 3]>,
    provinces: &mut BTreeMap<u32, Province>,
) -> Result<BTreeSet<u32>, Error> {
    let mut loaded = BTreeSet::new();
    let start = Date::parse(super::START_DATE).expect("固定开局日期必须有效");
    for path in content.text_files("history/provinces")? {
        let file = content.read(path)?;
        let id = file
            .path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .and_then(|stem| stem.split([' ', '-']).next())
            .and_then(|id| id.parse::<u32>().ok())
            .ok_or_else(|| Error::invalid(&file.path, "invalid province history filename"))?;
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
        let root = tape.windows1252_reader();
        // 顶层是日期之前的初始值，不能按文件中日期块出现的位置顺序直接覆盖。
        apply_owner(&file, &root, province, countries)?;
        let mut dated = Vec::new();
        for (key, operator, value) in root.fields() {
            let name = key.read_str();
            if !name.as_bytes().first().is_some_and(u8::is_ascii_digit) || !name.contains('.') {
                continue;
            }
            script::equal(&file, operator, &name)?;
            let date = Date::parse(name.as_bytes())
                .map_err(|_| Error::invalid(&file.path, format!("invalid history date: {name}")))?;
            if date <= start {
                let block = value
                    .read_object()
                    .map_err(|error| Error::invalid(&file.path, error.to_string()))?;
                dated.push((date, block));
            }
        }
        // 稳定排序保留同一天的原有顺序，开局当天的变化也必须生效。
        dated.sort_by_key(|(date, _)| *date);
        for (_, block) in dated {
            apply_owner(&file, &block, province, countries)?;
        }
    }
    Ok(loaded)
}

fn apply_owner(
    file: &ContentFile,
    block: &ObjectReader<'_, '_, Windows1252Encoding>,
    province: &mut Province,
    countries: &BTreeMap<String, [u8; 3]>,
) -> Result<(), Error> {
    for (key, operator, value) in block.fields() {
        if key.read_str() != "owner" {
            continue;
        }
        script::equal(file, operator, "owner")?;
        let tag = value
            .read_string()
            .map_err(|error| Error::invalid(&file.path, error.to_string()))?;
        // EU4 用 --- 清除所有者；controller 是占领状态，不覆盖法理归属。
        province.owner = if tag == "---" {
            None
        } else if countries.contains_key(&tag) {
            Some(tag)
        } else {
            return Err(Error::invalid(
                &file.path,
                format!("unknown province owner: {tag}"),
            ));
        };
    }
    Ok(())
}
