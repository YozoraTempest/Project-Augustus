// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use augustus_core::{Calendar, InitialWorld, NavalConstructionInput, WorldDate};
use jomini::{
    TextToken,
    binary::TokenResolver,
    common::{Date, PdsDate},
    text::{ObjectReader, Operator, ValueReader},
};
use serde::Serialize;

use crate::administration_validation::{SaveTokenResolver, read_eu4_save_documents};
use crate::{Error, ParsedText, TextEncoding};

type Object<'a, 'b> = ObjectReader<'a, 'b, TextEncoding>;
type Field<'a, 'b> = ValueReader<'a, 'b, TextEncoding>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VanillaNavalConstructionRecord {
    pub province_id: u32,
    pub country: String,
    pub envoy: Option<i32>,
    pub unit_type: String,
    pub start_date: String,
    pub total: u64,
    pub original_total: u64,
    /// Preserved as decimal text for diagnostics; it is not charged or replayed.
    pub progress: String,
    pub date: String,
    /// Preserved as decimal text for diagnostics; the order cost was already paid.
    pub cost: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VanillaNavalConstructionUnresolved {
    pub province_id: Option<u32>,
    pub reason: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct VanillaNavalConstructionReport {
    pub date: String,
    pub records: Vec<VanillaNavalConstructionRecord>,
    pub unresolved: Vec<VanillaNavalConstructionUnresolved>,
    /// Counts all saved military-construction blocks, including records that
    /// could not be parsed, so a malformed sibling cannot be mistaken for a
    /// single-order province queue.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub military_order_counts_by_province: BTreeMap<u32, usize>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct VanillaNavalConstructionImportReport {
    pub imported_count: usize,
    pub unresolved: Vec<VanillaNavalConstructionUnresolved>,
}

/// Read military construction observations from an EU4 text or ZIP save.
pub fn read_vanilla_naval_constructions(
    path: impl AsRef<Path>,
) -> Result<VanillaNavalConstructionReport, Error> {
    read_vanilla_naval_constructions_inner_thread(path.as_ref(), None, None)
}

/// Read military construction observations using a caller-provided binary token map.
pub fn read_vanilla_naval_constructions_with_token_map(
    path: impl AsRef<Path>,
    token_map_path: impl AsRef<Path>,
) -> Result<VanillaNavalConstructionReport, Error> {
    read_vanilla_naval_constructions_inner_thread(
        path.as_ref(),
        Some(token_map_path.as_ref()),
        None,
    )
}

/// Read military construction observations using an integration-provided token resolver.
pub fn read_vanilla_naval_constructions_with_token_resolver<R>(
    path: impl AsRef<Path>,
    resolver: R,
) -> Result<VanillaNavalConstructionReport, Error>
where
    R: TokenResolver + Send + 'static,
{
    read_vanilla_naval_constructions_inner_thread(path.as_ref(), None, Some(Box::new(resolver)))
}

fn read_vanilla_naval_constructions_inner_thread(
    path: &Path,
    token_map_path: Option<&Path>,
    token_resolver: Option<SaveTokenResolver>,
) -> Result<VanillaNavalConstructionReport, Error> {
    let path = path.to_path_buf();
    let token_map_path = token_map_path.map(Path::to_path_buf);
    let mut parser_token_resolver = token_resolver;
    let thread_path = path.clone();
    let thread_token_map_path = token_map_path.clone();
    let handle = std::thread::Builder::new()
        .name("augustus-eu4-naval-construction-parser".into())
        .stack_size(32 * 1024 * 1024)
        .spawn(move || {
            read_vanilla_naval_constructions_inner(
                &thread_path,
                thread_token_map_path.as_deref(),
                &mut parser_token_resolver,
            )
        })
        .map_err(|error| Error::caused_by(&path, error))?;
    handle
        .join()
        .map_err(|_| Error::message(&path, "EU4 naval construction parser thread panicked"))?
}

fn read_vanilla_naval_constructions_inner(
    path: &Path,
    token_map_path: Option<&Path>,
    token_resolver: &mut Option<SaveTokenResolver>,
) -> Result<VanillaNavalConstructionReport, Error> {
    let documents = read_eu4_save_documents(path, token_map_path, token_resolver)?;
    let parsed = documents.game_state.parse()?;
    let root = parsed.reader();
    let meta = documents
        .meta
        .as_ref()
        .map(|document| document.parse())
        .transpose()?;
    let date = match optional_scalar(&parsed, &root, "date")? {
        Some(date) => Some(date),
        None => meta
            .as_ref()
            .map(|metadata| optional_scalar(metadata, &metadata.reader(), "date"))
            .transpose()?
            .flatten(),
    };
    let date = date.ok_or_else(|| Error::message(parsed.source(), "missing save field date"))?;
    Date::parse(&date).map_err(|error| Error::caused_by(parsed.source(), error))?;

    let provinces = object(&parsed, &required(&parsed, &root, "provinces")?)?;
    let (siege_provinces, unknown_siege_location) = read_siege_combat_locations(&parsed, &root)?;
    let mut report = VanillaNavalConstructionReport {
        date,
        ..VanillaNavalConstructionReport::default()
    };
    for (key, operator, value) in provinces.fields() {
        if operator.is_some_and(|operator| operator != Operator::Equal) {
            return Err(Error::message(
                parsed.source(),
                "invalid saved province block",
            ));
        }
        let Some(province_id) = key
            .read_string()
            .strip_prefix('-')
            .and_then(|text| text.parse::<u32>().ok())
            .filter(|id| *id > 0)
        else {
            return Err(Error::message(
                parsed.source(),
                "expected negative save province identifier",
            ));
        };
        let province = object(&parsed, &value)?;
        for (_, operator, value) in province
            .fields()
            .filter(|(key, _, _)| key.read_string() == "military_construction")
        {
            *report
                .military_order_counts_by_province
                .entry(province_id)
                .or_default() += 1;
            let unresolved = |reason: String| VanillaNavalConstructionUnresolved {
                province_id: Some(province_id),
                reason,
            };
            if operator.is_some_and(|operator| operator != Operator::Equal) {
                report.unresolved.push(unresolved(
                    "military_construction uses an unsupported operator".into(),
                ));
                continue;
            }
            match parse_record(&value, province_id) {
                Ok((record, unsupported_fields)) => {
                    report.records.push(record);
                    if !unsupported_fields.is_empty() {
                        report.unresolved.push(unresolved(format!(
                            "unsupported military_construction fields: {}",
                            unsupported_fields.join(", ")
                        )));
                    }
                }
                Err(reason) => report.unresolved.push(unresolved(reason)),
            }
        }
    }
    for &province_id in report.military_order_counts_by_province.keys() {
        if unknown_siege_location || siege_provinces.contains(&province_id) {
            report
                .unresolved
                .push(VanillaNavalConstructionUnresolved {
                    province_id: Some(province_id),
                    reason: if unknown_siege_location {
                        "a siege combat has an unresolved location, so construction pause state is unknown".into()
                    } else {
                        "province has a siege combat; construction pause timing is unsupported".into()
                    },
                });
        }
    }
    Ok(report)
}

fn read_siege_combat_locations(
    parsed: &ParsedText<'_>,
    root: &Object<'_, '_>,
) -> Result<(BTreeSet<u32>, bool), Error> {
    let Some(combat) = optional_object(parsed, root, "combat")? else {
        return Ok((BTreeSet::new(), false));
    };
    let mut locations = BTreeSet::new();
    let mut unknown_location = false;
    for (_, operator, value) in combat
        .fields()
        .filter(|(key, _, _)| key.read_string() == "siege_combat")
    {
        if operator.is_some_and(|operator| operator != Operator::Equal) {
            unknown_location = true;
            continue;
        }
        let Ok(siege) = object(parsed, &value) else {
            unknown_location = true;
            continue;
        };
        match optional_scalar(parsed, &siege, "location") {
            Ok(Some(value)) => match parse_province_id(&value) {
                Some(province_id) => {
                    locations.insert(province_id);
                }
                None => unknown_location = true,
            },
            Ok(None) | Err(_) => unknown_location = true,
        }
    }
    Ok((locations, unknown_location))
}

fn parse_record(
    value: &Field<'_, '_>,
    province_id: u32,
) -> Result<(VanillaNavalConstructionRecord, Vec<String>), String> {
    if !matches!(value.token(), TextToken::Object { mixed: false, .. }) {
        return Err("military_construction must be an object".into());
    }
    let object = value
        .read_object()
        .map_err(|_| "military_construction must be an object".to_string())?;
    let expected = [
        "country",
        "envoy",
        "type",
        "start_date",
        "total",
        "original_total",
        "progress",
        "date",
        "cost",
    ];
    let mut fields = BTreeMap::new();
    let mut unsupported_fields = BTreeSet::new();
    for (key, operator, value) in object.fields() {
        let name = key.read_string();
        if operator.is_some_and(|operator| operator != Operator::Equal) {
            return Err(format!(
                "military_construction field {name} uses an unsupported operator"
            ));
        }
        if !expected.contains(&name.as_str()) {
            unsupported_fields.insert(name);
            continue;
        }
        let value = value
            .read_string()
            .map_err(|_| format!("military_construction field {name} must be scalar"))?;
        if fields.insert(name.clone(), value).is_some() {
            return Err(format!("duplicate military_construction field {name}"));
        }
    }
    let get = |name: &str| {
        fields
            .get(name)
            .cloned()
            .ok_or_else(|| format!("missing military_construction field {name}"))
    };
    let country = get("country")?;
    if !is_country_tag(&country) {
        return Err("military_construction has an invalid country tag".into());
    }
    let envoy = fields
        .get("envoy")
        .map(|value| value.parse::<i32>())
        .transpose()
        .map_err(|_| "military_construction has an invalid envoy".to_string())?;
    let unit_type = get("type")?;
    if unit_type.is_empty() || unit_type.bytes().any(|byte| byte.is_ascii_whitespace()) {
        return Err("military_construction has an invalid unit type".into());
    }
    let start_date = get("start_date")?;
    parse_world_date(&start_date)
        .ok_or_else(|| "military_construction has an invalid start_date".to_string())?;
    let date = get("date")?;
    parse_world_date(&date)
        .ok_or_else(|| "military_construction has an invalid date".to_string())?;
    let total = parse_days(&get("total")?)
        .ok_or_else(|| "military_construction has an invalid total".to_string())?;
    let original_total = parse_days(&get("original_total")?)
        .ok_or_else(|| "military_construction has an invalid original_total".to_string())?;
    let progress = get("progress")?;
    if !is_nonnegative_decimal(&progress) {
        return Err("military_construction has an invalid progress".into());
    }
    let cost = get("cost")?;
    if !is_nonnegative_decimal(&cost) {
        return Err("military_construction has an invalid cost".into());
    }
    Ok((
        VanillaNavalConstructionRecord {
            province_id,
            country,
            envoy,
            unit_type,
            start_date,
            total,
            original_total,
            progress,
            date,
            cost,
        },
        unsupported_fields.into_iter().collect(),
    ))
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

fn parse_days(value: &str) -> Option<u64> {
    (!value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()))
        .then(|| value.parse().ok())
        .flatten()
}

fn parse_province_id(value: &str) -> Option<u32> {
    let id = value.parse::<i64>().ok()?.unsigned_abs();
    u32::try_from(id).ok().filter(|id| *id > 0)
}

fn is_nonnegative_decimal(value: &str) -> bool {
    let mut parts = value.split('.');
    let integer = parts.next().unwrap_or_default();
    let fraction = parts.next();
    !integer.is_empty()
        && integer.bytes().all(|byte| byte.is_ascii_digit())
        && fraction.is_none_or(|fraction| {
            !fraction.is_empty() && fraction.bytes().all(|byte| byte.is_ascii_digit())
        })
        && parts.next().is_none()
}

fn is_country_tag(value: &str) -> bool {
    value.len() == 3
        && value
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
}

fn parse_world_date(value: &str) -> Option<WorldDate> {
    let date = Date::parse(value).ok()?;
    Some(WorldDate {
        year: date.year(),
        month: date.month(),
        day: date.day(),
    })
}

pub(crate) fn bind_naval_constructions(
    report: &VanillaNavalConstructionReport,
    initial: &InitialWorld,
    loaded_date: &str,
) -> (
    Vec<NavalConstructionInput>,
    VanillaNavalConstructionImportReport,
) {
    let mut imported = VanillaNavalConstructionImportReport {
        unresolved: report.unresolved.clone(),
        ..VanillaNavalConstructionImportReport::default()
    };
    let mut inputs = Vec::new();
    let unresolved_provinces = report
        .unresolved
        .iter()
        .filter_map(|entry| entry.province_id)
        .collect::<BTreeSet<_>>();
    let mut record_counts = BTreeMap::new();
    for record in &report.records {
        *record_counts.entry(record.province_id).or_insert(0usize) += 1;
    }
    let mut duplicate_provinces = report
        .military_order_counts_by_province
        .iter()
        .filter_map(|(&province, &count)| (count > 1).then_some(province))
        .collect::<BTreeSet<_>>();
    duplicate_provinces.extend(
        record_counts
            .into_iter()
            .filter_map(|(province, count)| (count > 1).then_some(province)),
    );
    let unresolved_duplicates = duplicate_provinces.clone();
    for province in unresolved_duplicates {
        imported
            .unresolved
            .push(VanillaNavalConstructionUnresolved {
                province_id: Some(province),
                reason: "multiple military_construction orders in one province are unsupported"
                    .into(),
            });
    }

    let Some(save_date) = parse_world_date(&report.date) else {
        imported
            .unresolved
            .push(VanillaNavalConstructionUnresolved {
                province_id: None,
                reason: "invalid construction report save date".into(),
            });
        return (inputs, imported);
    };
    if parse_world_date(loaded_date) != Some(save_date) {
        imported
            .unresolved
            .push(VanillaNavalConstructionUnresolved {
                province_id: None,
                reason: "construction report date does not match the loaded save date".into(),
            });
        return (inputs, imported);
    }

    let Some(calendar) = initial.world().calendar() else {
        imported
            .unresolved
            .push(VanillaNavalConstructionUnresolved {
                province_id: None,
                reason: "loaded world has no calendar".into(),
            });
        return (inputs, imported);
    };
    let current_day = initial.world().day();
    if calendar.date(current_day).ok() != Some(save_date) {
        imported
            .unresolved
            .push(VanillaNavalConstructionUnresolved {
                province_id: None,
                reason: "loaded world date does not match the construction save date".into(),
            });
        return (inputs, imported);
    }
    let epoch = Calendar {
        start: WorldDate {
            year: 1,
            month: 1,
            day: 1,
        },
        month_lengths: calendar.month_lengths,
    };
    let Some(save_ordinal) = epoch.day_offset(save_date).ok() else {
        imported
            .unresolved
            .push(VanillaNavalConstructionUnresolved {
                province_id: None,
                reason: "construction save date is outside the loaded calendar".into(),
            });
        return (inputs, imported);
    };

    for record in &report.records {
        let unresolved = |reason: String| VanillaNavalConstructionUnresolved {
            province_id: Some(record.province_id),
            reason,
        };
        if duplicate_provinces.contains(&record.province_id) {
            continue;
        }
        if unresolved_provinces.contains(&record.province_id) {
            continue;
        }
        if !is_country_tag(&record.country) {
            imported
                .unresolved
                .push(unresolved("construction has an invalid country tag".into()));
            continue;
        }
        if record.envoy.is_some_and(|envoy| envoy != -1) {
            imported.unresolved.push(unresolved(
                "military_construction envoy value other than -1 is unsupported".into(),
            ));
            continue;
        }
        if record.unit_type.is_empty()
            || record
                .unit_type
                .bytes()
                .any(|byte| byte.is_ascii_whitespace())
        {
            imported
                .unresolved
                .push(unresolved("construction has an invalid unit type".into()));
            continue;
        }
        if !is_nonnegative_decimal(&record.progress) || !is_nonnegative_decimal(&record.cost) {
            imported.unresolved.push(unresolved(
                "construction progress or cost is not a supported decimal".into(),
            ));
            continue;
        }
        let Some(template) = initial
            .world()
            .unit_templates()
            .definitions
            .get(&record.unit_type)
        else {
            imported.unresolved.push(unresolved(format!(
                "unknown naval construction unit type {}",
                record.unit_type
            )));
            continue;
        };
        if !matches!(
            template.category.as_str(),
            "heavy_ship" | "light_ship" | "galley" | "transport"
        ) {
            imported.unresolved.push(unresolved(format!(
                "unit type {} is not a loaded naval template",
                record.unit_type
            )));
            continue;
        }
        let Some(province) = initial.province(record.province_id) else {
            imported
                .unresolved
                .push(unresolved("construction province is not loaded".into()));
            continue;
        };
        if province.owner.as_deref() != Some(record.country.as_str())
            || province.controller.as_deref() != Some(record.country.as_str())
        {
            imported.unresolved.push(unresolved(
                "construction country, current owner, and controller must match".into(),
            ));
            continue;
        }
        let Some(owner) = initial.country_entity(&record.country) else {
            imported.unresolved.push(unresolved(format!(
                "construction country {} is not loaded",
                record.country
            )));
            continue;
        };
        let Some(location) = initial.province_entity(record.province_id) else {
            imported
                .unresolved
                .push(unresolved("construction province is not loaded".into()));
            continue;
        };
        if !initial.world().province_has_port(location).unwrap_or(false) {
            imported.unresolved.push(unresolved(
                "construction province is not a known port".into(),
            ));
            continue;
        }
        if record.total == 0 || record.total != record.original_total {
            imported.unresolved.push(unresolved(
                "construction total must be positive and equal original_total".into(),
            ));
            continue;
        }
        let Some(start_date) = parse_world_date(&record.start_date) else {
            imported
                .unresolved
                .push(unresolved("construction start_date is invalid".into()));
            continue;
        };
        let Some(planned_date) = parse_world_date(&record.date) else {
            imported
                .unresolved
                .push(unresolved("construction date is invalid".into()));
            continue;
        };
        let Some(start_ordinal) = epoch.day_offset(start_date).ok() else {
            imported.unresolved.push(unresolved(
                "construction start_date is outside the calendar".into(),
            ));
            continue;
        };
        if start_ordinal > save_ordinal {
            imported.unresolved.push(unresolved(
                "construction start_date is after the saved world date".into(),
            ));
            continue;
        }
        let Some(planned_ordinal) = epoch.day_offset(planned_date).ok() else {
            imported.unresolved.push(unresolved(
                "construction date is outside the calendar".into(),
            ));
            continue;
        };
        if start_ordinal.checked_add(record.total) != Some(planned_ordinal) {
            imported.unresolved.push(unresolved(
                "start_date plus total does not equal the planned construction date".into(),
            ));
            continue;
        }
        let Some(completion_ordinal) = planned_ordinal.checked_add(1) else {
            imported.unresolved.push(unresolved(
                "construction completion day exceeds the core calendar range".into(),
            ));
            continue;
        };
        if completion_ordinal <= save_ordinal {
            imported.unresolved.push(unresolved(
                "construction completion is overdue in the saved world".into(),
            ));
            continue;
        }
        let Some(completion_offset) = completion_ordinal.checked_sub(save_ordinal) else {
            imported.unresolved.push(unresolved(
                "construction completion precedes the saved world date".into(),
            ));
            continue;
        };
        let Some(completion_day) = current_day.checked_add(completion_offset) else {
            imported.unresolved.push(unresolved(
                "construction completion day exceeds the core calendar range".into(),
            ));
            continue;
        };
        let Some(expected_completion_date) = epoch.date(completion_ordinal).ok() else {
            imported.unresolved.push(unresolved(
                "construction completion date exceeds the supported calendar range".into(),
            ));
            continue;
        };
        if calendar.date(completion_day).ok() != Some(expected_completion_date) {
            imported.unresolved.push(unresolved(
                "construction completion date is outside the loaded calendar".into(),
            ));
            continue;
        }
        inputs.push(NavalConstructionInput {
            owner,
            location,
            unit_type: record.unit_type.clone(),
            completion_day,
        });
    }
    (inputs, imported)
}
