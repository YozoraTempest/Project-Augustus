// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::BTreeMap;

use augustus_core::{UnitTemplate, UnitTemplateProperty, UnitTemplates};

use crate::bootstrap::{direct_txt, equal, source};
use crate::{ContentView, Error, FileEntry, LoadedSource, SetupReport, TextEncoding};

const ENCODING: TextEncoding = TextEncoding::Windows1252;

pub(super) fn load(
    root: &impl ContentView,
    files: &[FileEntry],
    report: &mut SetupReport,
) -> Result<UnitTemplates, Error> {
    let mut definitions = BTreeMap::new();

    for file in files
        .iter()
        .filter(|file| direct_txt(&file.relative_path, "common/units"))
    {
        let path = &file.relative_path;
        let name = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .filter(|name| !name.trim().is_empty())
            .ok_or_else(|| {
                Error::message(root.physical_path(path), "invalid unit template name")
            })?;
        let document = root.read_text(path, ENCODING)?;
        let parsed = document.parse()?;
        let mut category = None;
        let mut category_source: Option<LoadedSource> = None;
        let mut properties = Vec::new();

        for (key, op, value) in parsed.reader().fields() {
            let property_name = key.read_string();
            let at = parsed.field_location(&key);
            let fail = |message: &str| Error::message(parsed.source(), message).with_location(at);
            equal(op, &fail)?;
            let value = value
                .read_string()
                .map_err(|_| fail("unit template properties must be scalar values"))?;

            if property_name == "type" {
                if category.is_some() {
                    return Err(fail("duplicate unit template category"));
                }
                category = Some(value);
                category_source = Some(source(&parsed, at));
            } else {
                properties.push(UnitTemplateProperty {
                    name: property_name,
                    value,
                });
            }
        }

        let category = category.ok_or_else(|| {
            Error::message(
                parsed.source(),
                "unit template is missing its type property",
            )
        })?;
        if definitions
            .insert(
                name.into(),
                UnitTemplate {
                    category,
                    properties,
                },
            )
            .is_some()
        {
            return Err(Error::message(
                parsed.source(),
                format!("duplicate unit template identifier: {name}"),
            ));
        }
        report.unit_template_sources.insert(
            name.into(),
            category_source.expect("category source recorded with category"),
        );
    }

    report.unit_templates = definitions.len();
    Ok(UnitTemplates { definitions })
}
