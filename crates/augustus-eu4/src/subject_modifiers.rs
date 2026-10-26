// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::{BTreeMap, BTreeSet};

use augustus_core::{Condition, EntityKind, Schema};

use crate::bootstrap::{direct_txt, equal};
use crate::script::{ScriptCompiler, ScriptLibrary};
use crate::setup::block;
use crate::{ContentView, Error, FileEntry, TextEncoding};

#[derive(Debug, Clone)]
pub(super) enum SubjectModifierTrigger {
    Condition(Condition),
    Unresolved,
}

#[derive(Debug, Clone)]
pub(super) struct SubjectModifier {
    pub(super) definition: String,
    pub(super) trigger: Option<SubjectModifierTrigger>,
}

#[derive(Debug)]
enum Operation {
    CopyFrom(String),
    Clear,
    Add(SubjectModifier),
}

#[derive(Debug, Default)]
pub(super) struct SubjectModifierDraft {
    types: BTreeMap<String, Option<Vec<SubjectModifier>>>,
    referenced_modifiers: BTreeSet<String>,
}

impl SubjectModifierDraft {
    pub(super) fn load(root: &impl ContentView, files: &[FileEntry]) -> Result<Self, Error> {
        let mut definitions = BTreeMap::<String, Option<Vec<Operation>>>::new();
        let mut referenced_modifiers = BTreeSet::new();
        let schema = Schema::new(Vec::<String>::new())
            .map_err(|error| Error::caused_by(root.path(), error))?;
        let library = ScriptLibrary::new();
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
                if body.fields().next().is_none() {
                    continue;
                }

                let mut complete = true;
                let mut operations = Vec::new();
                for (field, field_op, field_value) in body.fields() {
                    let field = field.read_string();
                    if !matches!(field.as_str(), "copy_from" | "modifier_subject") {
                        continue;
                    }
                    equal(field_op, &fail)?;
                    match field.as_str() {
                        "copy_from" => match field_value.read_string() {
                            Ok(parent) => operations.push(Operation::CopyFrom(parent)),
                            Err(_) => complete = false,
                        },
                        "modifier_subject" => {
                            if field_value
                                .read_string()
                                .is_ok_and(|value| value == "clear")
                            {
                                operations.push(Operation::Clear);
                                continue;
                            }
                            let Ok(modifier_block) = field_value.read_object() else {
                                complete = false;
                                continue;
                            };
                            let mut definition = None;
                            let mut trigger = None;
                            for (key, op, value) in modifier_block.fields() {
                                equal(op, &fail)?;
                                match key.read_string().as_str() {
                                    "modifier" => match value.read_string() {
                                        Ok(value) if definition.is_none() => {
                                            referenced_modifiers.insert(value.clone());
                                            definition = Some(value);
                                        }
                                        _ => complete = false,
                                    },
                                    "trigger" => {
                                        if trigger.is_some() {
                                            trigger = Some(SubjectModifierTrigger::Unresolved);
                                            continue;
                                        }
                                        let Ok(trigger_reader) = value.read_object() else {
                                            complete = false;
                                            continue;
                                        };
                                        let compiled = ScriptCompiler::new(&schema, &library, true)
                                            .with_unbound_subject_types()
                                            .compile_condition_reader(
                                                &parsed,
                                                trigger_reader,
                                                EntityKind::Country,
                                            );
                                        trigger = Some(match compiled {
                                            Ok(condition) => {
                                                SubjectModifierTrigger::Condition(condition)
                                            }
                                            Err(_) => SubjectModifierTrigger::Unresolved,
                                        });
                                    }
                                    "expiration_message_subject"
                                    | "expiration_message_overlord" => {}
                                    _ => trigger = Some(SubjectModifierTrigger::Unresolved),
                                }
                            }
                            if let Some(definition) = definition {
                                operations.push(Operation::Add(SubjectModifier {
                                    definition,
                                    trigger,
                                }));
                            } else {
                                complete = false;
                            }
                        }
                        _ => unreachable!(),
                    }
                }
                let definition = complete.then_some(operations);
                definitions
                    .entry(name)
                    .and_modify(|existing| *existing = None)
                    .or_insert(definition);
            }
        }

        let mut types = BTreeMap::new();
        for name in definitions.keys() {
            types.insert(
                name.clone(),
                resolve_type(name, &definitions, &mut BTreeSet::new()),
            );
        }
        Ok(Self {
            types,
            referenced_modifiers,
        })
    }

    pub(super) fn for_type(&self, name: &str) -> Option<&[SubjectModifier]> {
        self.types.get(name)?.as_deref()
    }

    pub(super) fn references(&self, definition: &str) -> bool {
        self.referenced_modifiers.contains(definition)
    }
}

fn resolve_type(
    name: &str,
    definitions: &BTreeMap<String, Option<Vec<Operation>>>,
    stack: &mut BTreeSet<String>,
) -> Option<Vec<SubjectModifier>> {
    if stack.len() >= 64 || !stack.insert(name.to_owned()) {
        return None;
    }
    let resolved = (|| {
        let operations = definitions.get(name)?.as_ref()?;
        let mut modifiers = if name == "default" {
            Vec::new()
        } else {
            resolve_type("default", definitions, stack)?
        };
        for operation in operations {
            match operation {
                Operation::CopyFrom(parent) => {
                    modifiers = resolve_type(parent, definitions, stack)?;
                }
                Operation::Clear => modifiers.clear(),
                Operation::Add(modifier) => modifiers.push(modifier.clone()),
            }
        }
        Some(modifiers)
    })();
    stack.remove(name);
    resolved
}
