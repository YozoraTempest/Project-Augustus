// SPDX-License-Identifier: GPL-3.0-only

//! 读取 default.map 中省份地图所需的字段，其他配置留给对应功能处理。

use std::{collections::HashMap, path::PathBuf};

use jomini::{TextTape, Windows1252Encoding, text::Operator};

use crate::{Error, content::ContentFile};

pub(super) struct MapConfig {
    pub width: u32,
    pub height: u32,
    // 省份编号的上界，不包含该值；编号 0 为保留值。
    pub max_provinces: u32,
    pub definitions: PathBuf,
    pub provinces: PathBuf,
    pub adjacencies: Option<PathBuf>,
}

impl MapConfig {
    pub fn parse(file: &ContentFile) -> Result<Self, Error> {
        let fail = |message| Error::invalid(&file.path, message);
        let tape = TextTape::from_slice(&file.bytes).map_err(|error| fail(error.to_string()))?;
        let mut fields = HashMap::new();
        for (key, operator, value) in tape.windows1252_reader().fields() {
            let key = key.read_str();
            if !matches!(
                key.as_ref(),
                "width" | "height" | "max_provinces" | "definitions" | "provinces" | "adjacencies"
            ) {
                continue;
            }
            if operator.is_some_and(|operator| operator != Operator::Equal) {
                return Err(fail(format!("{key}: expected '='")));
            }
            let scalar = value
                .read_scalar()
                .map_err(|error| fail(format!("{key}: {error}")))?;
            // jomini 的字符串解码会移除反斜杠，先将原始路径分隔符转换为正斜杠。
            let bytes: Vec<_> = scalar
                .as_bytes()
                .iter()
                .map(|&byte| if byte == b'\\' { b'/' } else { byte })
                .collect();
            let value = Windows1252Encoding::decode(&bytes).into_owned();
            if fields.insert(key.to_string(), value).is_some() {
                return Err(fail(format!("duplicate map field: {key}")));
            }
        }

        let required = |name: &str| {
            fields
                .get(name)
                .ok_or_else(|| fail(format!("missing map field: {name}")))
        };
        let positive_integer = |name: &str| {
            required(name)?
                .parse::<u32>()
                .ok()
                .filter(|value| *value > 0)
                .ok_or_else(|| fail(format!("{name}: expected a positive integer")))
        };
        let map_path = |name: &str| {
            let value = required(name)?;
            if value.is_empty() {
                return Err(fail(format!("{name}: expected a filename")));
            }
            // 配置引用的文件位于 map 目录下，也可以包含子目录。
            Ok(PathBuf::from("map").join(value))
        };
        let max_provinces = positive_integer("max_provinces")?;
        if max_provinces < 2 {
            return Err(fail("max_provinces must be at least 2".to_owned()));
        }

        Ok(Self {
            width: positive_integer("width")?,
            height: positive_integer("height")?,
            max_provinces,
            definitions: map_path("definitions")?,
            provinces: map_path("provinces")?,
            adjacencies: fields
                .get("adjacencies")
                .map(|_| map_path("adjacencies"))
                .transpose()?,
        })
    }
}
