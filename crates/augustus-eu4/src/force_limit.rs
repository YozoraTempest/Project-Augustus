// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::{BTreeMap, BTreeSet};

use jomini::text::ValueReader;

use crate::bootstrap::{direct_txt, equal};
use crate::setup::{block, micros};
use crate::{ContentView, Error, FileEntry, TextEncoding};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct SubjectForceLimitTransfer {
    pub(super) land_micros: i64,
    pub(super) naval_micros: i64,
}

#[derive(Debug)]
enum Operation {
    CopyFrom(String),
    Set {
        land_micros: Option<i64>,
        naval_micros: Option<i64>,
    },
}

#[derive(Debug, Default)]
pub(super) struct SubjectForceLimitTransfers {
    types: BTreeMap<String, Option<SubjectForceLimitTransfer>>,
}

impl SubjectForceLimitTransfers {
    pub(super) fn load(root: &impl ContentView, files: &[FileEntry]) -> Result<Self, Error> {
        let mut definitions = BTreeMap::<String, Option<Vec<Operation>>>::new();
        for file in files
            .iter()
            .filter(|file| direct_txt(&file.relative_path, "common/subject_types"))
        {
            let document = root.read_text(&file.relative_path, TextEncoding::Windows1252)?;
            let parsed = document.parse()?;
            for (key, op, value) in parsed.reader().fields() {
                let name = key.read_string();
                let at = parsed.field_location(&key);
                let fail =
                    |message: &str| Error::message(parsed.source(), message).with_location(at);
                equal(op, &fail)?;
                let body = block(&value, &fail)?;
                let mut operations = Vec::new();
                let mut land_micros = None;
                let mut naval_micros = None;
                let mut complete = true;
                for (field, field_op, field_value) in body.fields() {
                    let field_name = field.read_string();
                    if !matches!(
                        field_name.as_str(),
                        "copy_from" | "forcelimit_to_overlord" | "naval_forcelimit_to_overlord"
                    ) {
                        continue;
                    }
                    equal(field_op, &fail)?;
                    match field_name.as_str() {
                        "copy_from" => match field_value.read_string() {
                            Ok(parent) => operations.push(Operation::CopyFrom(parent)),
                            Err(_) => complete = false,
                        },
                        "forcelimit_to_overlord" => {
                            if land_micros.is_some() {
                                complete = false;
                                continue;
                            }
                            land_micros = read_micros(&field_value);
                            complete &= land_micros.is_some();
                        }
                        "naval_forcelimit_to_overlord" => {
                            if naval_micros.is_some() {
                                complete = false;
                                continue;
                            }
                            naval_micros = read_micros(&field_value);
                            complete &= naval_micros.is_some();
                        }
                        _ => unreachable!(),
                    }
                }
                if land_micros.is_some() || naval_micros.is_some() {
                    operations.push(Operation::Set {
                        land_micros,
                        naval_micros,
                    });
                }
                let definition = complete.then_some(operations);
                definitions
                    .entry(name)
                    .and_modify(|existing| *existing = None)
                    .or_insert(definition);
            }
        }
        let types = definitions
            .keys()
            .map(|name| {
                (
                    name.clone(),
                    resolve_type(name, &definitions, &mut BTreeSet::new()),
                )
            })
            .collect();
        Ok(Self { types })
    }

    pub(super) fn for_type(&self, name: &str) -> Option<&Option<SubjectForceLimitTransfer>> {
        self.types.get(name)
    }
}

fn read_micros(value: &ValueReader<'_, '_, TextEncoding>) -> Option<i64> {
    micros(&value.read_string().ok()?)
}

fn resolve_type(
    name: &str,
    definitions: &BTreeMap<String, Option<Vec<Operation>>>,
    stack: &mut BTreeSet<String>,
) -> Option<SubjectForceLimitTransfer> {
    if stack.len() >= 64 || !stack.insert(name.to_owned()) {
        return None;
    }
    let resolved = (|| {
        let mut transfer = if name == "default" {
            SubjectForceLimitTransfer::default()
        } else if definitions.contains_key("default") {
            resolve_type("default", definitions, stack)?
        } else {
            SubjectForceLimitTransfer::default()
        };
        let operations = definitions.get(name)?.as_ref()?;
        for operation in operations {
            match operation {
                Operation::CopyFrom(parent) => {
                    transfer = resolve_type(parent, definitions, stack)?;
                }
                Operation::Set {
                    land_micros,
                    naval_micros,
                } => {
                    if let Some(value) = land_micros {
                        transfer.land_micros = *value;
                    }
                    if let Some(value) = naval_micros {
                        transfer.naval_micros = *value;
                    }
                }
            }
        }
        Some(transfer)
    })();
    stack.remove(name);
    resolved
}
