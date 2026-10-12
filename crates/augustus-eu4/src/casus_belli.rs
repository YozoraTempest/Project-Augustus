// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::BTreeMap;

use augustus_core::{Condition, EntityKind, JustificationDefinition, Schema, Target};
use serde::Serialize;

use crate::bootstrap::{direct_txt, equal, source};
use crate::setup::{block, flag};
use crate::{
    ContentRoot, ContentSet, Error, FileEntry, LoadedSource, ScriptCompiler, ScriptLibrary,
    TextDocument, TextEncoding,
};

#[derive(Debug, Clone)]
pub struct CasusBelliType {
    pub default_months: u32,
    pub triggered_only: bool,
    pub maintenance: JustificationDefinition,
}

#[derive(Debug, Default, Serialize)]
pub struct CasusBelliReport {
    pub definitions: usize,
    pub triggered_types: usize,
    pub sources: BTreeMap<String, LoadedSource>,
    pub uncompiled_maintenance: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Default)]
pub struct CasusBelliTypes {
    pub types: BTreeMap<String, CasusBelliType>,
    pub report: CasusBelliReport,
}

impl CasusBelliTypes {
    pub fn from_content(
        content: &ContentSet,
        encoding: TextEncoding,
        schema: &Schema,
        library: &ScriptLibrary,
    ) -> Result<Self, Error> {
        let mut result = Self::default();
        for (path, _) in content
            .effective_files()
            .filter(|(path, _)| direct_txt(std::path::Path::new(path), "common/cb_types"))
        {
            result.document(&content.read_text(path, encoding)?, schema, library)?;
        }
        Ok(result)
    }

    pub(crate) fn from_root(
        root: &ContentRoot,
        files: &[FileEntry],
        schema: &Schema,
    ) -> Result<Self, Error> {
        let mut result = Self::default();
        let library = ScriptLibrary::from_root(root, files, TextEncoding::Windows1252)?;
        for file in files
            .iter()
            .filter(|file| direct_txt(&file.relative_path, "common/cb_types"))
        {
            result.document(
                &root.read_text(&file.relative_path, TextEncoding::Windows1252)?,
                schema,
                &library,
            )?;
        }
        Ok(result)
    }

    pub fn definitions(&self) -> BTreeMap<String, JustificationDefinition> {
        self.types
            .iter()
            .map(|(name, data)| (name.clone(), data.maintenance.clone()))
            .collect()
    }

    fn document(
        &mut self,
        document: &TextDocument,
        schema: &Schema,
        library: &ScriptLibrary,
    ) -> Result<(), Error> {
        let parsed = document.parse()?;
        for (key, operator, value) in parsed.reader().fields() {
            let name = key.read_string();
            if name.starts_with('@') {
                continue;
            }
            let at = parsed.field_location(&key);
            let fail = |message: &str| Error::message(parsed.source(), message).with_location(at);
            equal(operator, &fail)?;
            if name.is_empty() || self.types.contains_key(&name) {
                return Err(fail("empty or ambiguous casus belli definition"));
            }
            let body = block(&value, &fail)?;
            let mut fields = std::collections::BTreeSet::new();
            let mut data = CasusBelliType {
                default_months: 12,
                triggered_only: false,
                maintenance: JustificationDefinition {
                    issuer: Some(Condition::HasOwnedProvince {
                        target: Target::Current,
                    }),
                    target: Some(Condition::HasOwnedProvince {
                        target: Target::Current,
                    }),
                },
            };
            for (key, operator, value) in body.fields() {
                let field = key.read_string();
                if !matches!(
                    field.as_str(),
                    "months" | "is_triggered_only" | "prerequisites_self" | "prerequisites"
                ) {
                    continue;
                }
                let at = parsed.field_location(&key);
                let fail =
                    |message: &str| Error::message(parsed.source(), message).with_location(at);
                equal(operator, &fail)?;
                if !fields.insert(field.clone()) {
                    return Err(fail("duplicate casus belli parameter"));
                }
                match field.as_str() {
                    "months" => {
                        data.default_months = value
                            .read_string()
                            .ok()
                            .and_then(|v| v.parse::<i32>().ok())
                            .and_then(|v| u32::try_from(v).ok())
                            .ok_or_else(|| {
                                fail("casus belli months require a non-negative literal integer")
                            })?
                    }
                    "is_triggered_only" => {
                        data.triggered_only = value
                            .read_string()
                            .ok()
                            .and_then(|v| flag(&v))
                            .ok_or_else(|| fail("expected casus belli flag"))?
                    }
                    _ => {
                        let reader = block(&value, &fail)?;
                        if field == "prerequisites_self" {
                            let compiled = ScriptCompiler::new(schema, library, true)
                                .compile_block_condition_reader(
                                    &parsed,
                                    "AND",
                                    reader,
                                    at,
                                    EntityKind::Country,
                                );
                            Self::append_rule(
                                compiled,
                                &mut data.maintenance.issuer,
                                &name,
                                &mut self.report,
                            );
                        } else {
                            // Top-level prerequisite fields are a conjunction in
                            // issuer context; FROM switches to the opposing country.
                            // Mixed nested contexts remain explicit uncompiled rules.
                            for (key, operator, value) in reader.fields() {
                                let field = key.read_string();
                                let at = parsed.field_location(&key);
                                let fail = |message: &str| {
                                    Error::message(parsed.source(), message).with_location(at)
                                };
                                equal(operator, &fail)?;
                                let compiler = ScriptCompiler::new(schema, library, true);
                                let compiled = if field == "FROM" {
                                    compiler.compile_block_condition_reader(
                                        &parsed,
                                        "AND",
                                        block(&value, &fail)?,
                                        at,
                                        EntityKind::Country,
                                    )
                                } else if let Ok(scalar) = value.read_string() {
                                    compiler.compile_scalar_condition_reader(
                                        &parsed,
                                        &field,
                                        scalar,
                                        at,
                                        EntityKind::Country,
                                    )
                                } else {
                                    compiler.compile_block_condition_reader(
                                        &parsed,
                                        &field,
                                        block(&value, &fail)?,
                                        at,
                                        EntityKind::Country,
                                    )
                                };
                                let slot = if field == "FROM" {
                                    &mut data.maintenance.target
                                } else {
                                    &mut data.maintenance.issuer
                                };
                                Self::append_rule(compiled, slot, &name, &mut self.report);
                            }
                        }
                    }
                }
            }
            self.report.triggered_types += usize::from(data.triggered_only);
            self.report
                .sources
                .insert(name.clone(), source(&parsed, at));
            self.types.insert(name, data);
        }
        self.report.definitions = self.types.len();
        Ok(())
    }

    fn append_rule(
        compiled: Result<Condition, Error>,
        slot: &mut Option<Condition>,
        name: &str,
        report: &mut CasusBelliReport,
    ) {
        match compiled {
            Ok(condition) => {
                if let Some(previous) = slot.take() {
                    *slot = Some(Condition::All(vec![previous, condition]));
                }
            }
            Err(error) => {
                report
                    .uncompiled_maintenance
                    .entry(name.into())
                    .or_default()
                    .push(error.to_string());
                *slot = None;
            }
        }
    }
}
