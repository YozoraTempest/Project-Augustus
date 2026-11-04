// SPDX-License-Identifier: GPL-3.0-only

use std::{collections::BTreeMap, path::Path};

use augustus_core::{Province, ProvinceKind};

use crate::{
    Error,
    content::{ContentFile, ContentRoot},
    script,
};

pub(super) fn apply(
    content: &ContentRoot,
    provinces: &mut BTreeMap<u32, Province>,
) -> Result<(), Error> {
    let file = content.read("map/default.map")?;
    let tape = script::parse(&file)?;
    let mut climate = None;
    for (key, operator, value) in tape.windows1252_reader().fields() {
        let name = key.read_str();
        let kind = match name.as_ref() {
            "sea_starts" => ProvinceKind::Sea,
            "lakes" => ProvinceKind::Lake,
            "climate" => {
                script::equal(&file, operator, &name)?;
                if climate
                    .replace(script::content_path(&value, &file)?)
                    .is_some()
                {
                    return Err(Error::invalid(&file.path, "duplicate climate reference"));
                }
                continue;
            }
            _ => continue,
        };
        script::equal(&file, operator, &name)?;
        classify(
            &file,
            script::province_list(&value, &file)?,
            kind,
            provinces,
        )?;
    }
    if let Some(climate) = climate {
        let file = content.read(Path::new("map").join(climate))?;
        let tape = script::parse(&file)?;
        for (key, operator, value) in tape.windows1252_reader().fields() {
            if key.read_str() == "impassable" {
                script::equal(&file, operator, "impassable")?;
                classify(
                    &file,
                    script::province_list(&value, &file)?,
                    ProvinceKind::Wasteland,
                    provinces,
                )?;
            }
        }
    }
    Ok(())
}

fn classify(
    file: &ContentFile,
    ids: Vec<u32>,
    kind: ProvinceKind,
    provinces: &mut BTreeMap<u32, Province>,
) -> Result<(), Error> {
    for id in ids {
        let province = provinces
            .get_mut(&id)
            .ok_or_else(|| Error::invalid(&file.path, format!("unknown province: {id}")))?;
        // impassable 描述不可通行性，原版也列入了湖泊；水域分类优先于陆地荒地。
        if kind == ProvinceKind::Wasteland
            && matches!(province.kind, ProvinceKind::Sea | ProvinceKind::Lake)
        {
            continue;
        }
        if province.kind != ProvinceKind::Land && province.kind != kind {
            return Err(Error::invalid(
                &file.path,
                format!("conflicting geography for province {id}"),
            ));
        }
        province.kind = kind;
    }
    Ok(())
}
