// SPDX-License-Identifier: GPL-3.0-only

//! 建立颜色到省份编号的索引，保留重复颜色的信息供位图加载时判断。

use std::{
    collections::{BTreeSet, HashMap},
    path::PathBuf,
};

use crate::{Error, content::ContentFile};

pub(super) struct Definitions {
    source: PathBuf,
    pub(super) ids: BTreeSet<u32>,
    // Some(id) 表示唯一编号，None 表示多个定义共享颜色；缺失的键表示未定义。
    by_color: HashMap<[u8; 3], Option<u32>>,
}

impl Definitions {
    pub fn parse(file: &ContentFile, max_provinces: u32) -> Result<Self, Error> {
        let mut reader = csv::ReaderBuilder::new()
            .delimiter(b';')
            .flexible(true)
            .trim(csv::Trim::All)
            .from_reader(file.bytes.as_slice());
        let mut ids = BTreeSet::new();
        let mut by_color = HashMap::new();
        // 名称等附加列可能包含 Windows-1252 字符，按字节读取，只解码数值列。
        for row in reader.byte_records() {
            let row = row.map_err(|error| Error::invalid(&file.path, error.to_string()))?;
            let line = row.position().map_or(0, |position| position.line());
            let fail = |message| Error::invalid(&file.path, format!("line {line}: {message}"));
            let integer = |index, name: &str| {
                row.get(index)
                    .and_then(|bytes| std::str::from_utf8(bytes).ok())
                    .and_then(|text| text.parse::<u32>().ok())
                    .ok_or_else(|| fail(format!("invalid {name}")))
            };
            let id = integer(0, "province ID")?;
            // 排除保留编号和配置范围之外的记录。
            if id == 0 || id >= max_provinces {
                continue;
            }
            if !ids.insert(id) {
                return Err(fail(format!("duplicate province ID: {id}")));
            }
            let mut color = [0; 3];
            for (index, channel) in color.iter_mut().enumerate() {
                *channel = u8::try_from(integer(index + 1, "RGB component")?)
                    .map_err(|_| fail("RGB component is outside 0..255".to_owned()))?;
            }
            // 原版存在未使用的重复颜色；仅在位图实际引用该颜色时报告歧义。
            by_color
                .entry(color)
                .and_modify(|province| *province = None)
                .or_insert(Some(id));
        }
        if ids.is_empty() {
            return Err(Error::invalid(&file.path, "no province definitions"));
        }
        Ok(Self {
            source: file.path.clone(),
            ids,
            by_color,
        })
    }

    pub fn len(&self) -> usize {
        self.ids.len()
    }

    pub fn province_for_color(&self, color: [u8; 3]) -> Result<Option<u32>, Error> {
        match self.by_color.get(&color) {
            Some(Some(id)) => Ok(Some(*id)),
            Some(None) => Err(Error::invalid(
                &self.source,
                format!("multiple provinces use map color {color:?}"),
            )),
            None => Ok(None),
        }
    }
}
