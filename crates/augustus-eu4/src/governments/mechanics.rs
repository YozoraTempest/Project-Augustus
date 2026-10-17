// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::BTreeMap;

use super::{ENCODING, GovernmentReport, Reform, mechanic_feature_name};
use crate::autonomy::constant_condition;
use crate::bootstrap::{direct_txt, equal, source};
use crate::setup::{PropertyDiagnostics, block};
use crate::{ContentView, Error, FileEntry, ParsedText, SourceLocation, TextEncoding};

pub(super) fn load(
    root: &impl ContentView,
    files: &[FileEntry],
    report: &mut GovernmentReport,
) -> Result<BTreeMap<String, Option<bool>>, Error> {
    let mut availability = BTreeMap::new();
    for file in files
        .iter()
        .filter(|f| direct_txt(&f.relative_path, "common/government_mechanics"))
    {
        let doc = root.read_text(&file.relative_path, ENCODING)?;
        let parsed = doc.parse()?;
        for (key, op, value) in parsed.reader().fields() {
            let name = key.read_string();
            let at = parsed.field_location(&key);
            let fail = |m: &str| Error::message(parsed.source(), m).with_location(at);
            equal(op, &fail)?;
            let body = block(&value, &fail)?;
            let id = format!("mechanic:{name}");
            if let Some(origins) = report.ambiguous_definitions.get_mut(&id) {
                origins.push(source(&parsed, at));
                availability.insert(name, None);
                continue;
            }
            if let Some(first) = report.definition_sources.remove(&id) {
                report
                    .ambiguous_definitions
                    .insert(id, vec![first, source(&parsed, at)]);
                availability.insert(name, None);
                continue;
            }
            let count = body
                .fields()
                .filter(|(k, _, _)| k.read_string() == "available")
                .count();
            let available = if count > 1 {
                report.unsupported(&parsed, "mechanic", "duplicate_available", at);
                None
            } else if count == 0 {
                Some(true)
            } else {
                let result = constant_condition(&parsed, body.clone(), "available")?;
                if result.is_none() {
                    report.unsupported(&parsed, "mechanic", "dynamic_available", at);
                }
                result
            };
            for (key, _, _) in body.fields() {
                let field = key.read_string();
                if !matches!(
                    field.as_str(),
                    "available" | "alert_icon_gfx" | "alert_icon_index"
                ) {
                    report.unsupported(&parsed, "mechanic", &field, parsed.field_location(&key));
                }
            }
            availability.insert(name, available);
            report.definition_sources.insert(id, source(&parsed, at));
        }
    }
    report.mechanic_definitions = availability.len();
    Ok(availability)
}

pub(super) fn apply(
    parsed: &ParsedText<'_>,
    value: &jomini::text::ValueReader<'_, '_, TextEncoding>,
    availability: &BTreeMap<String, Option<bool>>,
    reform: &mut Reform,
    pending: bool,
    report: &mut GovernmentReport,
    at: SourceLocation,
) -> Result<(), Error> {
    let fail = |m: &str| Error::message(parsed.source(), m).with_location(at);
    for value in value
        .read_array()
        .map_err(|_| fail("expected government mechanic list"))?
        .values()
    {
        let id = value
            .read_string()
            .map_err(|_| fail("expected government mechanic identifier"))?;
        crate::script::government_condition("has_government_mechanic", &id).map_err(fail)?;
        let available = availability.get(&id).copied().flatten();
        if !availability.contains_key(&id) {
            report.unsupported(parsed, "mechanic", &format!("undefined:{id}"), at);
        }
        // A constant unavailable mechanic cannot be enabled by any reform.
        // Country-dependent availability needs its own refresh lifecycle.
        if available == Some(false) {
            continue;
        }
        let enabled = if pending { None } else { available };
        reform
            .effect
            .features
            .entry(mechanic_feature_name(&id))
            .and_modify(|old| {
                if *old != enabled {
                    *old = None;
                }
            })
            .or_insert(enabled);
    }
    Ok(())
}
