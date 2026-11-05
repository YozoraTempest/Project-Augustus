// SPDX-License-Identifier: GPL-3.0-only

use std::{collections::BTreeMap, path::PathBuf};

use augustus_core::{BuildingDefinition, Money};
use jomini::{Windows1252Encoding, text::ValueReader};

use crate::{
    Error,
    content::{ContentFile, ContentRoot},
    script,
};

struct PendingBuilding {
    cost: Option<Money>,
    months: Option<u32>,
    tax_modifier: Money,
    replaces: Option<String>,
    manufactory: bool,
    path: PathBuf,
}

pub(super) fn load(content: &ContentRoot) -> Result<BTreeMap<String, BuildingDefinition>, Error> {
    let files = match content.text_files("common/buildings") {
        Ok(files) => files,
        Err(Error::Read { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
            return Ok(BTreeMap::new());
        }
        Err(error) => return Err(error),
    };
    let mut buildings = BTreeMap::new();
    for path in files {
        let file = content.read(path)?;
        let tape = script::parse(&file)?;
        for (key, operator, value) in tape.windows1252_reader().fields() {
            let id = key.read_str().into_owned();
            script::equal(&file, operator, &id)?;
            let object = value
                .read_object()
                .map_err(|error| Error::invalid(&file.path, error.to_string()))?;
            let mut cost = None;
            let mut months = None;
            let mut tax_modifier = Money::ZERO;
            let mut replaces = None;
            let mut manufactory = false;
            for (key, operator, value) in object.fields() {
                let name = key.read_str();
                match name.as_ref() {
                    "cost" => {
                        script::equal(&file, operator, &name)?;
                        cost = Some(decimal(&value, &file)?);
                    }
                    "time" => {
                        script::equal(&file, operator, &name)?;
                        months = Some(
                            value
                                .read_scalar()
                                .ok()
                                .and_then(|value| value.to_u64().ok())
                                .and_then(|months| u32::try_from(months).ok())
                                .filter(|months| *months > 0)
                                .ok_or_else(|| {
                                    Error::invalid(
                                        &file.path,
                                        "building time must be a positive number of months",
                                    )
                                })?,
                        );
                    }
                    "make_obsolete" => {
                        script::equal(&file, operator, &name)?;
                        replaces = Some(
                            value
                                .read_string()
                                .map_err(|error| Error::invalid(&file.path, error.to_string()))?,
                        );
                    }
                    "manufactory" => {
                        script::equal(&file, operator, &name)?;
                        manufactory = true;
                    }
                    "modifier" => {
                        script::equal(&file, operator, &name)?;
                        for (key, operator, value) in value
                            .read_object()
                            .map_err(|error| Error::invalid(&file.path, error.to_string()))?
                            .fields()
                        {
                            if key.read_str() == "local_tax_modifier" {
                                script::equal(&file, operator, "local_tax_modifier")?;
                                tax_modifier = decimal(&value, &file)?;
                            }
                        }
                    }
                    _ => (),
                }
            }
            let definition = PendingBuilding {
                cost,
                months,
                tax_modifier,
                replaces,
                manufactory,
                path: file.path.clone(),
            };
            if buildings.insert(id.clone(), definition).is_some() {
                return Err(Error::invalid(
                    &file.path,
                    format!("duplicate building: {id}"),
                ));
            }
        }
    }
    // 各类工场共用 manufactory 定义的费用和工期，先读完全部文件再解析继承。
    let prototype_cost = buildings
        .get("manufactory")
        .and_then(|building| building.cost);
    let prototype_months = buildings
        .get("manufactory")
        .and_then(|building| building.months);
    buildings
        .into_iter()
        .map(|(id, pending)| {
            let cost = pending
                .cost
                .or_else(|| pending.manufactory.then_some(prototype_cost).flatten())
                .ok_or_else(|| {
                    Error::invalid(&pending.path, format!("building {id} has no cost"))
                })?;
            let months = pending
                .months
                .or_else(|| pending.manufactory.then_some(prototype_months).flatten())
                .ok_or_else(|| {
                    Error::invalid(
                        &pending.path,
                        format!("building {id} has no construction time"),
                    )
                })?;
            Ok((
                id.clone(),
                BuildingDefinition {
                    // 首个开放新建的类型是税收建筑；其费用、工期和税收系数来自内容。
                    constructible: id == "temple",
                    id,
                    cost,
                    months,
                    tax_modifier: pending.tax_modifier,
                    replaces: pending.replaces,
                },
            ))
        })
        .collect()
}

fn decimal(
    value: &ValueReader<'_, '_, Windows1252Encoding>,
    file: &ContentFile,
) -> Result<Money, Error> {
    value
        .read_string()
        .map_err(|error| Error::invalid(&file.path, error.to_string()))?
        .parse()
        .map_err(|error| Error::invalid(&file.path, format!("invalid building decimal: {error}")))
}
