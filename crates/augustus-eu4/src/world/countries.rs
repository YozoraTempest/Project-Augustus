// SPDX-License-Identifier: GPL-3.0-only

use std::{collections::BTreeMap, path::Path};

use crate::{
    Error,
    content::{ContentFile, ContentRoot},
    script,
};

pub(super) fn load(content: &ContentRoot) -> Result<BTreeMap<String, [u8; 3]>, Error> {
    let mut countries = BTreeMap::new();
    for path in content.text_files("common/country_tags")? {
        let file = content.read(path)?;
        let tape = script::parse(&file)?;
        for (key, operator, value) in tape.windows1252_reader().fields() {
            let tag = key.read_string();
            script::equal(&file, operator, &tag)?;
            if tag.len() != 3
                || !tag
                    .bytes()
                    .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
            {
                return Err(Error::invalid(
                    &file.path,
                    format!("invalid country tag: {tag}"),
                ));
            }
            let relative = script::content_path(&value, &file)?;
            if !Path::new(&relative).starts_with("countries") {
                return Err(Error::invalid(
                    &file.path,
                    "expected a countries/ definition path",
                ));
            }
            let definition = content.read(Path::new("common").join(relative))?;
            let color = read_color(&definition)?;
            if countries.insert(tag.clone(), color).is_some() {
                return Err(Error::invalid(
                    &file.path,
                    format!("duplicate country tag: {tag}"),
                ));
            }
        }
    }
    if countries.is_empty() {
        return Err(Error::invalid(
            content.path().join("common/country_tags"),
            "no country definitions",
        ));
    }
    Ok(countries)
}

fn read_color(file: &ContentFile) -> Result<[u8; 3], Error> {
    let tape = script::parse(file)?;
    let mut color = None;
    for (key, operator, value) in tape.windows1252_reader().fields() {
        if key.read_str() != "color" {
            continue;
        }
        script::equal(file, operator, "color")?;
        if color.is_some() {
            return Err(Error::invalid(&file.path, "duplicate country color"));
        }
        let values = value
            .read_array()
            .map_err(|error| Error::invalid(&file.path, error.to_string()))?;
        let channels: Vec<_> = values
            .values()
            .map(|channel| {
                channel
                    .read_scalar()
                    .ok()
                    .and_then(|scalar| scalar.to_u64().ok())
                    .and_then(|value| u8::try_from(value).ok())
                    .ok_or_else(|| {
                        Error::invalid(&file.path, "country color component must be in 0..255")
                    })
            })
            .collect::<Result<_, _>>()?;
        color =
            Some(channels.try_into().map_err(|_| {
                Error::invalid(&file.path, "country color requires three components")
            })?);
    }
    color.ok_or_else(|| Error::invalid(&file.path, "missing country color"))
}
