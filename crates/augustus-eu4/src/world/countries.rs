// SPDX-License-Identifier: GPL-3.0-only

use std::{collections::BTreeMap, path::Path};

use augustus_core::Country;

use crate::{
    Error,
    content::{ContentFile, ContentRoot},
    script,
};

pub(super) struct CountryDefinitions {
    pub states: BTreeMap<String, Country>,
    pub colors: BTreeMap<String, [u8; 3]>,
}

pub(super) fn load(content: &ContentRoot) -> Result<CountryDefinitions, Error> {
    let mut countries = CountryDefinitions {
        states: BTreeMap::new(),
        colors: BTreeMap::new(),
    };
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
            if countries.colors.insert(tag.clone(), color).is_some() {
                return Err(Error::invalid(
                    &file.path,
                    format!("duplicate country tag: {tag}"),
                ));
            }
            countries.states.insert(tag.clone(), Country::new(tag));
        }
    }
    if countries.states.is_empty() {
        return Err(Error::invalid(
            content.path().join("common/country_tags"),
            "no country definitions",
        ));
    }
    Ok(countries)
}

/// 国家定义里的历史兵种，按文件顺序。没有该字段时为空。
pub(super) fn historical_units(content: &ContentRoot, tag: &str) -> Result<Vec<String>, Error> {
    let mut found = None;
    for path in content.text_files("common/country_tags")? {
        let file = content.read(path)?;
        let tape = script::parse(&file)?;
        for (key, operator, value) in tape.windows1252_reader().fields() {
            let name = key.read_string();
            if name != tag {
                continue;
            }
            script::equal(&file, operator, &name)?;
            if found.is_some() {
                return Err(Error::invalid(
                    &file.path,
                    format!("duplicate country tag: {tag}"),
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
            found = Some(read_historical_units(&definition)?);
        }
    }
    found.ok_or_else(|| {
        Error::invalid(
            content.path().join("common/country_tags"),
            format!("unknown country {tag}"),
        )
    })
}

fn read_historical_units(file: &ContentFile) -> Result<Vec<String>, Error> {
    let tape = script::parse(file)?;
    let mut units = None;
    for (key, operator, value) in tape.windows1252_reader().fields() {
        if key.read_str() != "historical_units" {
            continue;
        }
        script::equal(file, operator, "historical_units")?;
        if units.is_some() {
            return Err(Error::invalid(&file.path, "duplicate historical_units"));
        }
        let array = value
            .read_array()
            .map_err(|error| Error::invalid(&file.path, error.to_string()))?;
        units = Some(
            array
                .values()
                .map(|value| {
                    value
                        .read_string()
                        .map(|text| text.to_string())
                        .map_err(|error| Error::invalid(&file.path, error.to_string()))
                })
                .collect::<Result<Vec<_>, _>>()?,
        );
    }
    Ok(units.unwrap_or_default())
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

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    use crate::content::ContentRoot;

    use super::historical_units;

    #[test]
    fn historical_units_keep_file_order() {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "augustus-historical-units-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("common/country_tags")).unwrap();
        fs::create_dir_all(root.join("common/countries")).unwrap();
        fs::write(
            root.join("common/country_tags/00_countries.txt"),
            "FRA = \"countries/France.txt\"\n",
        )
        .unwrap();
        fs::write(
            root.join("common/countries/France.txt"),
            "historical_units = { western_medieval_infantry chevauchee }\n",
        )
        .unwrap();
        let units = historical_units(&ContentRoot::new(&root), "FRA").unwrap();
        let _ = fs::remove_dir_all(&root);
        assert_eq!(units, ["western_medieval_infantry", "chevauchee"]);
    }
}
