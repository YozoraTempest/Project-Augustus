// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::{collections::BTreeMap, path::Path, sync::Arc};

use jomini::{TextToken, text::Operator};

use crate::{ContentSet, Error, SourceLocation, TextEncoding, bootstrap::direct_txt};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefinitionKind {
    Buildings,
    ScriptedEffects,
    ScriptedTriggers,
}

impl DefinitionKind {
    pub fn directory(self) -> &'static str {
        match self {
            Self::Buildings => "common/buildings",
            Self::ScriptedEffects => "common/scripted_effects",
            Self::ScriptedTriggers => "common/scripted_triggers",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefinitionOrigin {
    pub source_index: usize,
    pub logical_path: Arc<str>,
    pub physical_path: Arc<Path>,
    pub location: SourceLocation,
    /// Zero-based position among the root fields, including local constants.
    pub field_index: usize,
}

/// Declaration locations in effective files, without choosing a same-name winner.
#[derive(Debug)]
pub struct DefinitionIndex {
    kind: DefinitionKind,
    definitions: BTreeMap<String, Vec<DefinitionOrigin>>,
}

impl DefinitionIndex {
    pub fn build(
        content: &ContentSet,
        kind: DefinitionKind,
        encoding: TextEncoding,
    ) -> Result<Self, Error> {
        let mut definitions = BTreeMap::<String, Vec<DefinitionOrigin>>::new();
        for (path, origin) in content.effective_files() {
            if !direct_txt(Path::new(path), kind.directory()) {
                continue;
            }
            let document = content.read_text(path, encoding)?;
            let parsed = document.parse()?;
            let logical_path: Arc<str> = Arc::from(path);
            let physical_path: Arc<Path> = Arc::from(document.source());
            for (field_index, (key, operator, value)) in parsed.reader().fields().enumerate() {
                let name = key.read_string();
                if name.starts_with('@') {
                    continue;
                }
                let location = parsed.field_location(&key);
                let block = match value.token() {
                    TextToken::Object { mixed: false, .. } => true,
                    TextToken::Array { mixed: false, .. } => value
                        .read_array()
                        .is_ok_and(|array| array.values().next().is_none()),
                    _ => false,
                };
                if operator.is_some_and(|operator| operator != Operator::Equal) || !block {
                    return Err(Error::message(
                        document.source(),
                        format!("definition {name} requires an '=' block"),
                    )
                    .with_location(location));
                }
                definitions.entry(name).or_default().push(DefinitionOrigin {
                    source_index: origin.source_index,
                    logical_path: Arc::clone(&logical_path),
                    physical_path: Arc::clone(&physical_path),
                    location,
                    field_index,
                });
            }
        }
        Ok(Self { kind, definitions })
    }

    pub fn kind(&self) -> DefinitionKind {
        self.kind
    }

    /// Names remain case-sensitive; file-path normalization does not change script identifiers.
    pub fn get(&self, name: &str) -> Option<&[DefinitionOrigin]> {
        self.definitions.get(name).map(Vec::as_slice)
    }

    /// Sorted by name. Locations use manifest path order, then source field order.
    pub fn definitions(&self) -> impl Iterator<Item = (&str, &[DefinitionOrigin])> {
        self.definitions
            .iter()
            .map(|(name, origins)| (name.as_str(), origins.as_slice()))
    }
}
