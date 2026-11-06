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
    land_forcelimit: Money,
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
            let mut land_forcelimit = Money::ZERO;
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
                            match key.read_str().as_ref() {
                                "local_tax_modifier" => {
                                    script::equal(&file, operator, "local_tax_modifier")?;
                                    tax_modifier = decimal(&value, &file)?;
                                }
                                "land_forcelimit" => {
                                    script::equal(&file, operator, "land_forcelimit")?;
                                    land_forcelimit = decimal(&value, &file)?;
                                }
                                _ => (),
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
                land_forcelimit,
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
                    land_forcelimit: pending.land_forcelimit,
                    replaces: pending.replaces,
                },
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use augustus_core::Money;

    use crate::content::ContentRoot;

    use super::load;

    #[test]
    fn reads_land_forcelimit_from_building_modifier() {
        let root = std::env::temp_dir().join("augustus-eu4-building-limit");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("common/buildings")).unwrap();
        fs::write(root.join("common/buildings/00_buildings.txt"), BUILDINGS).unwrap();
        let buildings = load(&ContentRoot::new(&root)).unwrap();
        assert_eq!(
            buildings["regimental_camp"].land_forcelimit,
            Money::from(1)
        );
        assert_eq!(
            buildings["conscription_center"].land_forcelimit,
            Money::from(2)
        );
        assert_eq!(
            buildings["conscription_center"].replaces.as_deref(),
            Some("regimental_camp")
        );
        assert_eq!(buildings["temple"].land_forcelimit, Money::ZERO);
        fs::remove_dir_all(&root).unwrap();
    }

    const BUILDINGS: &str = r#"
        temple = {
            cost = 100
            time = 12
            modifier = { local_tax_modifier = 0.4 }
        }
        regimental_camp = {
            cost = 200
            time = 12
            modifier = { land_forcelimit = 1 }
        }
        conscription_center = {
            cost = 400
            time = 12
            make_obsolete = regimental_camp
            modifier = { land_forcelimit = 2 }
        }
    "#;
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
