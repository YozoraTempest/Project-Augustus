// SPDX-License-Identifier: GPL-3.0-only

//! 复用 jomini 的脚本读取器，只解释当前功能需要的字段。

use jomini::{
    TextTape, Windows1252Encoding,
    text::{Operator, ValueReader},
};

use crate::{Error, content::ContentFile};

pub(crate) fn parse(file: &ContentFile) -> Result<TextTape<'_>, Error> {
    TextTape::from_slice(&file.bytes).map_err(|error| Error::invalid(&file.path, error.to_string()))
}

pub(crate) fn equal(
    file: &ContentFile,
    operator: Option<Operator>,
    field: &str,
) -> Result<(), Error> {
    if operator.is_some_and(|operator| operator != Operator::Equal) {
        return Err(Error::invalid(&file.path, format!("{field}: expected '='")));
    }
    Ok(())
}

pub(crate) fn content_path(
    value: &ValueReader<'_, '_, Windows1252Encoding>,
    file: &ContentFile,
) -> Result<String, Error> {
    let scalar = value
        .read_scalar()
        .map_err(|error| Error::invalid(&file.path, error.to_string()))?;
    // 解码器会移除反斜杠；路径在解码之前统一分隔符，兼容 Windows 格式引用。
    let bytes: Vec<_> = scalar
        .as_bytes()
        .iter()
        .map(|&b| if b == b'\\' { b'/' } else { b })
        .collect();
    Ok(Windows1252Encoding::decode(&bytes).into_owned())
}

pub(crate) fn province_list(
    value: &ValueReader<'_, '_, Windows1252Encoding>,
    file: &ContentFile,
) -> Result<Vec<u32>, Error> {
    let array = value
        .read_array()
        .map_err(|error| Error::invalid(&file.path, error.to_string()))?;
    array
        .values()
        .map(|value| {
            let scalar = value
                .read_scalar()
                .map_err(|error| Error::invalid(&file.path, error.to_string()))?;
            scalar
                .to_u64()
                .ok()
                .and_then(|id| u32::try_from(id).ok())
                .filter(|&id| id > 0)
                .ok_or_else(|| Error::invalid(&file.path, "expected a positive province ID"))
        })
        .collect()
}
