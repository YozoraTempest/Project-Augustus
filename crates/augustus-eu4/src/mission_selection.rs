// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use jomini::{
    TextToken,
    binary::TokenResolver,
    common::Date,
    text::{ObjectReader, Operator, ValueReader},
};
use serde::Serialize;

use crate::administration_validation::{SaveTokenResolver, read_eu4_save_documents};
use crate::{Error, ParsedText, TextEncoding};

type Object<'a, 'b> = ObjectReader<'a, 'b, TextEncoding>;
type Field<'a, 'b> = ValueReader<'a, 'b, TextEncoding>;

/// Mission series saved as selected by each country in an EU4 save.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct VanillaMissionSelectionImport {
    pub date: String,
    pub countries: BTreeMap<String, BTreeSet<String>>,
}

/// Read selected mission series from an EU4 text or ZIP save.
pub fn read_vanilla_mission_selection(
    path: impl AsRef<Path>,
) -> Result<VanillaMissionSelectionImport, Error> {
    read_vanilla_mission_selection_inner_thread(path.as_ref(), None, None)
}

/// Read selected mission series using a caller-provided binary token map.
pub fn read_vanilla_mission_selection_with_token_map(
    path: impl AsRef<Path>,
    token_map_path: impl AsRef<Path>,
) -> Result<VanillaMissionSelectionImport, Error> {
    read_vanilla_mission_selection_inner_thread(path.as_ref(), Some(token_map_path.as_ref()), None)
}

/// Read selected mission series using an integration-provided token resolver.
pub fn read_vanilla_mission_selection_with_token_resolver<R>(
    path: impl AsRef<Path>,
    resolver: R,
) -> Result<VanillaMissionSelectionImport, Error>
where
    R: TokenResolver + Send + 'static,
{
    read_vanilla_mission_selection_inner_thread(path.as_ref(), None, Some(Box::new(resolver)))
}

fn read_vanilla_mission_selection_inner_thread(
    path: &Path,
    token_map_path: Option<&Path>,
    token_resolver: Option<SaveTokenResolver>,
) -> Result<VanillaMissionSelectionImport, Error> {
    let path = path.to_path_buf();
    let token_map_path = token_map_path.map(Path::to_path_buf);
    let mut parser_token_resolver = token_resolver;
    let thread_path = path.clone();
    let thread_token_map_path = token_map_path.clone();
    let handle = std::thread::Builder::new()
        .name("augustus-eu4-mission-selection-parser".into())
        .stack_size(32 * 1024 * 1024)
        .spawn(move || {
            read_vanilla_mission_selection_inner(
                &thread_path,
                thread_token_map_path.as_deref(),
                &mut parser_token_resolver,
            )
        })
        .map_err(|error| Error::caused_by(&path, error))?;
    handle
        .join()
        .map_err(|_| Error::message(&path, "EU4 mission selection parser thread panicked"))?
}

fn read_vanilla_mission_selection_inner(
    path: &Path,
    token_map_path: Option<&Path>,
    token_resolver: &mut Option<SaveTokenResolver>,
) -> Result<VanillaMissionSelectionImport, Error> {
    let documents = read_eu4_save_documents(path, token_map_path, token_resolver)?;
    let parsed = documents.game_state.parse()?;
    let root = parsed.reader();
    let meta = documents
        .meta
        .as_ref()
        .map(|document| document.parse())
        .transpose()?;
    let date = match optional_scalar(&parsed, &root, "date")? {
        Some(date) => date,
        None => meta
            .as_ref()
            .map(|metadata| optional_scalar(metadata, &metadata.reader(), "date"))
            .transpose()?
            .flatten()
            .ok_or_else(|| Error::message(parsed.source(), "missing save field date"))?,
    };
    Date::parse(&date).map_err(|error| Error::caused_by(parsed.source(), error))?;

    let countries_object = object(&parsed, &required(&parsed, &root, "countries")?)?;
    let mut countries = BTreeMap::new();
    let mut seen_countries = BTreeSet::new();
    for (key, operator, value) in countries_object.fields() {
        let tag = key.read_string();
        if !is_country_tag(&tag) {
            continue;
        }
        if operator.is_some_and(|operator| operator != Operator::Equal) {
            return Err(Error::message(
                parsed.source(),
                "invalid saved country block",
            ));
        }
        if !seen_countries.insert(tag.clone()) {
            return Err(Error::message(
                parsed.source(),
                format!("duplicate saved country {tag}"),
            ));
        }
        let country = object(&parsed, &value)?;
        if let Some(missions) = optional_object(&parsed, &country, "country_missions")? {
            countries.insert(tag, read_country_missions(&parsed, &missions)?);
        }
    }

    Ok(VanillaMissionSelectionImport { date, countries })
}

fn read_country_missions(
    parsed: &ParsedText<'_>,
    missions: &Object<'_, '_>,
) -> Result<BTreeSet<String>, Error> {
    let mut series = BTreeSet::new();
    for (key, operator, slot) in missions.fields() {
        let subfield = key.read_string();
        if subfield != "mission_slot" {
            return Err(Error::message(
                parsed.source(),
                format!("unknown country_missions subfield {subfield}"),
            ));
        }
        if operator.is_some_and(|operator| operator != Operator::Equal) {
            return Err(Error::message(
                parsed.source(),
                "invalid country_missions mission_slot operator",
            ));
        }
        if !matches!(slot.token(), TextToken::Array { mixed: false, .. }) {
            return Err(Error::message(
                parsed.source(),
                "country_missions mission_slot must be a scalar array",
            ));
        }
        let slot = slot
            .read_array()
            .map_err(|error| Error::caused_by(parsed.source(), error))?;
        for value in slot.values() {
            let series_id = value
                .read_string()
                .map_err(|error| Error::caused_by(parsed.source(), error))?;
            if series_id.trim().is_empty() {
                return Err(Error::message(
                    parsed.source(),
                    "country_missions contains an empty mission series id",
                ));
            }
            if !series.insert(series_id.clone()) {
                return Err(Error::message(
                    parsed.source(),
                    format!("duplicate mission series id {series_id}"),
                ));
            }
        }
    }
    Ok(series)
}

fn optional_scalar(
    parsed: &ParsedText<'_>,
    reader: &Object<'_, '_>,
    name: &str,
) -> Result<Option<String>, Error> {
    let mut result = None;
    for (_, operator, value) in reader
        .fields()
        .filter(|(key, _, _)| key.read_string() == name)
    {
        if operator.is_some_and(|operator| operator != Operator::Equal) {
            return Err(Error::message(
                parsed.source(),
                format!("invalid save field {name}"),
            ));
        }
        let value = value
            .read_string()
            .map_err(|error| Error::caused_by(parsed.source(), error))?;
        if result.replace(value).is_some() {
            return Err(Error::message(
                parsed.source(),
                format!("duplicate save field {name}"),
            ));
        }
    }
    Ok(result)
}

fn required<'a, 'b>(
    parsed: &ParsedText<'_>,
    reader: &Object<'a, 'b>,
    name: &str,
) -> Result<Field<'a, 'b>, Error> {
    let mut result = None;
    for (_, operator, value) in reader
        .fields()
        .filter(|(key, _, _)| key.read_string() == name)
    {
        if operator.is_some_and(|operator| operator != Operator::Equal) {
            return Err(Error::message(
                parsed.source(),
                format!("invalid save field {name}"),
            ));
        }
        if result.replace(value).is_some() {
            return Err(Error::message(
                parsed.source(),
                format!("duplicate save field {name}"),
            ));
        }
    }
    result.ok_or_else(|| Error::message(parsed.source(), format!("missing save field {name}")))
}

fn optional_object<'a, 'b>(
    parsed: &ParsedText<'_>,
    reader: &Object<'a, 'b>,
    name: &str,
) -> Result<Option<Object<'a, 'b>>, Error> {
    let mut result = None;
    for (_, operator, value) in reader
        .fields()
        .filter(|(key, _, _)| key.read_string() == name)
    {
        if operator.is_some_and(|operator| operator != Operator::Equal) {
            return Err(Error::message(
                parsed.source(),
                format!("invalid save field {name}"),
            ));
        }
        if result.is_some() {
            return Err(Error::message(
                parsed.source(),
                format!("duplicate save field {name}"),
            ));
        }
        result = Some(object(parsed, &value)?);
    }
    Ok(result)
}

fn object<'a, 'b>(parsed: &ParsedText<'_>, value: &Field<'a, 'b>) -> Result<Object<'a, 'b>, Error> {
    if !matches!(value.token(), TextToken::Object { mixed: false, .. })
        && !(matches!(value.token(), TextToken::Array { mixed: false, .. })
            && value.read_array().is_ok_and(|array| array.is_empty()))
    {
        return Err(Error::message(parsed.source(), "expected save object"));
    }
    value
        .read_object()
        .map_err(|error| Error::caused_by(parsed.source(), error))
}

fn is_country_tag(value: &str) -> bool {
    value.len() == 3
        && value
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
}
