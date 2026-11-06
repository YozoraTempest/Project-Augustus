// SPDX-License-Identifier: GPL-3.0-only

use std::collections::BTreeMap;

use augustus_core::Money;
use jomini::{Windows1252Encoding, text::ValueReader};

use crate::{Error, content::ContentFile, content::ContentRoot, script};

pub(super) struct ForceLimitSources {
    pub base: Money,
    pub per_development: Money,
    pub goods: BTreeMap<String, Money>,
}

pub(super) fn load(content: &ContentRoot) -> Result<ForceLimitSources, Error> {
    let (base, per_development) = static_limits(content)?;
    Ok(ForceLimitSources {
        base,
        per_development,
        goods: trade_goods(content)?,
    })
}

fn static_limits(content: &ContentRoot) -> Result<(Money, Money), Error> {
    let mut base = None;
    let mut per_development = None;
    let directory = content.path().join("common/static_modifiers");
    for path in content.text_files("common/static_modifiers")? {
        let file = content.read(path)?;
        let tape = script::parse(&file)?;
        for (key, operator, value) in tape.windows1252_reader().fields() {
            let name = key.read_str().into_owned();
            if name != "base_values" && name != "development" {
                continue;
            }
            script::equal(&file, operator, &name)?;
            let object = value
                .read_object()
                .map_err(|error| Error::invalid(&file.path, error.to_string()))?;
            for (key, operator, value) in object.fields() {
                if key.read_str() != "land_forcelimit" {
                    continue;
                }
                script::equal(&file, operator, "land_forcelimit")?;
                let amount = decimal(&value, &file)?;
                match name.as_str() {
                    "base_values" => base = Some(amount),
                    "development" => per_development = Some(amount),
                    _ => (),
                }
            }
        }
    }
    Ok((
        base.ok_or_else(|| {
            Error::invalid(directory.clone(), "missing base_values.land_forcelimit")
        })?,
        per_development
            .ok_or_else(|| Error::invalid(directory, "missing development.land_forcelimit"))?,
    ))
}

fn trade_goods(content: &ContentRoot) -> Result<BTreeMap<String, Money>, Error> {
    let mut goods = BTreeMap::new();
    for path in content.text_files("common/tradegoods")? {
        let file = content.read(path)?;
        let tape = script::parse(&file)?;
        for (key, operator, value) in tape.windows1252_reader().fields() {
            let id = key.read_str().into_owned();
            script::equal(&file, operator, &id)?;
            let object = value
                .read_object()
                .map_err(|error| Error::invalid(&file.path, error.to_string()))?;
            let mut contribution = Money::ZERO;
            for (key, operator, value) in object.fields() {
                if key.read_str() != "province" {
                    continue;
                }
                script::equal(&file, operator, "province")?;
                for (key, operator, value) in value
                    .read_object()
                    .map_err(|error| Error::invalid(&file.path, error.to_string()))?
                    .fields()
                {
                    if key.read_str() != "land_forcelimit" {
                        continue;
                    }
                    script::equal(&file, operator, "land_forcelimit")?;
                    contribution = decimal(&value, &file)?;
                }
            }
            if goods.insert(id.clone(), contribution).is_some() {
                return Err(Error::invalid(
                    &file.path,
                    format!("duplicate trade good: {id}"),
                ));
            }
        }
    }
    Ok(goods)
}

fn decimal(
    value: &ValueReader<'_, '_, Windows1252Encoding>,
    file: &ContentFile,
) -> Result<Money, Error> {
    value
        .read_string()
        .map_err(|error| Error::invalid(&file.path, error.to_string()))?
        .parse()
        .map_err(|error| Error::invalid(&file.path, format!("invalid land forcelimit: {error}")))
}
