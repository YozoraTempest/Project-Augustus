// SPDX-License-Identifier: GPL-3.0-only

use std::{collections::BTreeMap, path::Path};

use augustus_core::{ManpowerRules, Money};

use crate::{Error, content::ContentRoot, script};

pub(super) fn load(content: &ContentRoot, defines: &str) -> Result<ManpowerRules, Error> {
    let path = content.path().join("common/defines.lua");
    let mut values = BTreeMap::new();
    for line in defines.lines() {
        let line = line.split("--").next().unwrap_or("");
        if let Some((key, value)) = line.split_once('=') {
            values.insert(key.trim(), value.trim().trim_end_matches(',').trim());
        }
    }
    let required = |key| {
        values
            .get(key)
            .copied()
            .ok_or_else(|| Error::invalid(&path, format!("missing {key}")))
    };
    let mut base = None;
    for path in content.text_files("common/static_modifiers")? {
        let file = content.read(path)?;
        let tape = script::parse(&file)?;
        for (key, operator, value) in tape.windows1252_reader().fields() {
            if key.read_str() != "base_values" {
                continue;
            }
            script::equal(&file, operator, "base_values")?;
            let object = value
                .read_object()
                .map_err(|error| Error::invalid(&file.path, error.to_string()))?;
            for (key, operator, value) in object.fields() {
                if key.read_str() == "global_manpower" {
                    script::equal(&file, operator, "global_manpower")?;
                    let value = value
                        .read_string()
                        .map_err(|error| Error::invalid(&file.path, error.to_string()))?;
                    base = Some(scaled(&value, 1_000, &file.path)?);
                }
            }
        }
    }
    Ok(ManpowerRules {
        base_capacity: base.ok_or_else(|| {
            Error::invalid(
                content.path().join("common/static_modifiers"),
                "missing base_values.global_manpower",
            )
        })?,
        // 原版人力以千人为单位；核心统一使用人数，避免浮点误差。
        per_development: scaled(required("BASE_MP_TO_MANPOWER")?, 1_000, &path)?,
        recovery_months: u32::try_from(scaled(required("MAX_MANPOWER")?, 12, &path)?)
            .map_err(|error| Error::invalid(&path, error.to_string()))?,
        minimum_monthly: scaled(required("MIN_MONTHLY_MANPOWER")?, 1_000, &path)?,
        infantry_cost: scaled(required("INF_REGIMENT_MANPOWER_COST")?, 1_000, &path)?,
        cavalry_cost: scaled(required("CAV_REGIMENT_MANPOWER_COST")?, 1_000, &path)?,
    })
}

fn scaled(value: &str, factor: u32, path: &Path) -> Result<u64, Error> {
    let value: Money = value
        .parse()
        .map_err(|error| Error::invalid(path, format!("invalid manpower value: {error}")))?;
    (value * Money::from(factor))
        .normalize()
        .to_string()
        .parse()
        .map_err(|error| {
            Error::invalid(
                path,
                format!(
                    "manpower must be a non-negative integer number of people or months: {error}"
                ),
            )
        })
}
