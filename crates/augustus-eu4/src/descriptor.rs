// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use crate::{Error, ParsedText};
use jomini::text::Operator;

/// EU4 descriptor metadata. Paths are recorded, never mounted implicitly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModDescriptor {
    pub name: String,
    pub version: Option<String>,
    pub supported_version: Option<String>,
    pub path: Option<String>,
    pub archive: Option<String>,
    pub replace_paths: Vec<String>,
    pub dependencies: Vec<String>,
    pub tags: Vec<String>,
    pub other_fields: Vec<String>,
}

impl ModDescriptor {
    pub fn parse(document: &ParsedText<'_>) -> Result<Self, Error> {
        let mut name = None;
        let mut descriptor = Self {
            name: String::new(),
            version: None,
            supported_version: None,
            path: None,
            archive: None,
            replace_paths: Vec::new(),
            dependencies: Vec::new(),
            tags: Vec::new(),
            other_fields: Vec::new(),
        };
        for (key, operator, value) in document.reader().fields() {
            let key = key.read_string();
            if operator.is_some_and(|operator| operator != Operator::Equal) {
                return Err(Error::message(
                    document.source(),
                    format!("descriptor field {key} requires '='"),
                ));
            }
            match key.as_str() {
                "name" | "version" | "supported_version" | "path" | "archive" => {
                    let slot = match key.as_str() {
                        "name" => &mut name,
                        "version" => &mut descriptor.version,
                        "supported_version" => &mut descriptor.supported_version,
                        "path" => &mut descriptor.path,
                        _ => &mut descriptor.archive,
                    };
                    if slot.is_some() {
                        return Err(Error::message(
                            document.source(),
                            format!("duplicate descriptor field {key}"),
                        ));
                    }
                    *slot = Some(value.read_string().map_err(|cause| {
                        Error::message(document.source(), format!("{key}: {cause}"))
                    })?);
                }
                "replace_path" => {
                    let scalar = value.read_scalar().map_err(|cause| {
                        Error::message(document.source(), format!("replace_path: {cause}"))
                    })?;
                    descriptor
                        .replace_paths
                        .push(document.decode_path_scalar(scalar));
                }
                "dependencies" | "tags" => {
                    if value
                        .read_object()
                        .is_ok_and(|object| object.fields_len() != 0)
                    {
                        return Err(Error::message(
                            document.source(),
                            format!("{key}: expected a list of strings"),
                        ));
                    }
                    let array = value.read_array().map_err(|cause| {
                        Error::message(document.source(), format!("{key}: {cause}"))
                    })?;
                    let items = array
                        .values()
                        .map(|item| item.read_string())
                        .collect::<Result<Vec<_>, _>>()
                        .map_err(|cause| {
                            Error::message(document.source(), format!("{key}: {cause}"))
                        })?;
                    let target = if key == "dependencies" {
                        &mut descriptor.dependencies
                    } else {
                        &mut descriptor.tags
                    };
                    target.extend(items);
                }
                _ => descriptor.other_fields.push(key),
            }
        }
        descriptor.name = name.filter(|name| !name.trim().is_empty()).ok_or_else(|| {
            Error::message(document.source(), "descriptor requires a non-empty name")
        })?;
        Ok(descriptor)
    }
}
