// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use crate::{ContentRoot, Error, ModDescriptor, TextDocument, TextEncoding};

/// An explicitly mounted directory and its optional mod descriptor.
#[derive(Debug)]
pub struct ContentSource {
    root: ContentRoot,
    descriptor: Option<(PathBuf, ModDescriptor)>,
    replace_paths: BTreeSet<String>,
}

impl ContentSource {
    pub fn base(root: ContentRoot) -> Self {
        Self {
            root,
            descriptor: None,
            replace_paths: BTreeSet::new(),
        }
    }

    /// Reads descriptor.mod inside the selected root; declared paths are not followed.
    pub fn mod_directory(
        path: impl AsRef<Path>,
        descriptor_encoding: TextEncoding,
    ) -> Result<Self, Error> {
        let root = ContentRoot::open(path)?;
        let document = root.read_text("descriptor.mod", descriptor_encoding)?;
        let parsed = document.parse()?;
        // The current string decoder removes raw backslashes. Do not let that
        // silently change the directory excluded by a descriptor declaration.
        for (key, _, value) in parsed.reader().fields() {
            if key.read_string() == "replace_path"
                && value
                    .read_scalar()
                    .is_ok_and(|scalar| scalar.as_bytes().contains(&b'\\'))
            {
                return Err(Error::message(
                    document.source(),
                    "replace_path must use forward slashes; raw backslashes are not supported",
                )
                .with_location(parsed.field_location(&key)));
            }
        }
        let descriptor = ModDescriptor::parse(&parsed)?;
        let mut replace_paths = BTreeSet::new();
        for path in &descriptor.replace_paths {
            let normalized = normalize_path(path).map_err(|message| {
                Error::message(
                    document.source(),
                    format!("replace_path {path:?}: {message}"),
                )
            })?;
            replace_paths.insert(normalized);
        }
        Ok(Self {
            root,
            descriptor: Some((document.source().to_path_buf(), descriptor)),
            replace_paths,
        })
    }

    pub fn root(&self) -> &ContentRoot {
        &self.root
    }

    pub fn name(&self) -> &str {
        self.descriptor().map_or("EU4 base", |item| &item.name)
    }

    pub fn descriptor(&self) -> Option<&ModDescriptor> {
        self.descriptor.as_ref().map(|(_, descriptor)| descriptor)
    }

    pub fn descriptor_path(&self) -> Option<&Path> {
        self.descriptor.as_ref().map(|(path, _)| path.as_path())
    }
}

/// One physical file, including candidates that no longer contribute content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileOrigin {
    pub source_index: usize,
    pub relative_path: PathBuf,
    pub physical_path: PathBuf,
    pub byte_size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Suppression {
    FileOverride {
        by_source: usize,
    },
    DirectoryReplacement {
        by_source: usize,
        replace_path: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileCandidate {
    pub origin: FileOrigin,
    pub suppressed_by: Option<Suppression>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileTrace {
    /// Slash-separated, ASCII case-insensitive lookup key.
    pub logical_path: String,
    /// Candidates in the explicit source order, retaining their original spelling.
    pub candidates: Vec<FileCandidate>,
}

impl FileTrace {
    pub fn effective(&self) -> Option<&FileOrigin> {
        self.candidates
            .last()
            .filter(|item| item.suppressed_by.is_none())
            .map(|item| &item.origin)
    }

    fn suppress(&mut self, reason: Suppression) {
        if let Some(candidate) = self.candidates.last_mut()
            && candidate.suppressed_by.is_none()
        {
            candidate.suppressed_by = Some(reason);
        }
    }
}

/// A read-only file manifest. Object-definition priority is deliberately separate.
#[derive(Debug)]
pub struct ContentSet {
    sources: Vec<ContentSource>,
    files: BTreeMap<String, FileTrace>,
}

impl ContentSet {
    /// The first source must be a base directory, followed by mods in caller order.
    pub fn resolve(sources: Vec<ContentSource>) -> Result<Self, Error> {
        validate_sources(&sources)?;
        let mut files = BTreeMap::<String, FileTrace>::new();
        for (source_index, source) in sources.iter().enumerate() {
            for replacement in &source.replace_paths {
                let prefix = format!("{replacement}/");
                for (_, trace) in files
                    .range_mut(prefix.clone()..)
                    .take_while(|(path, _)| path.starts_with(&prefix))
                {
                    trace.suppress(Suppression::DirectoryReplacement {
                        by_source: source_index,
                        replace_path: replacement.clone(),
                    });
                }
            }
            let mut seen = BTreeSet::new();
            for entry in source.root.files()? {
                let physical_path = source.root.path().join(&entry.relative_path);
                let path = entry.relative_path.to_str().ok_or_else(|| {
                    Error::message(&physical_path, "content path is not valid UTF-8")
                })?;
                let logical_path = normalize_path(path)
                    .map_err(|message| Error::message(&physical_path, message))?;
                if !seen.insert(logical_path.clone()) {
                    return Err(Error::message(
                        &physical_path,
                        format!("ambiguous content path within source: {logical_path}"),
                    ));
                }
                let trace = files
                    .entry(logical_path.clone())
                    .or_insert_with(|| FileTrace {
                        logical_path,
                        candidates: Vec::new(),
                    });
                trace.suppress(Suppression::FileOverride {
                    by_source: source_index,
                });
                trace.candidates.push(FileCandidate {
                    origin: FileOrigin {
                        source_index,
                        relative_path: entry.relative_path,
                        physical_path,
                        byte_size: entry.byte_size,
                    },
                    suppressed_by: None,
                });
            }
        }
        Ok(Self { sources, files })
    }

    pub fn sources(&self) -> &[ContentSource] {
        &self.sources
    }

    /// All paths, including those removed without a replacement file.
    pub fn traces(&self) -> impl Iterator<Item = &FileTrace> {
        self.files.values()
    }

    pub fn effective_files(&self) -> impl Iterator<Item = (&str, &FileOrigin)> {
        self.files
            .iter()
            .filter_map(|(path, trace)| trace.effective().map(|origin| (path.as_str(), origin)))
    }

    pub fn trace(&self, path: &str) -> Result<Option<&FileTrace>, Error> {
        let normalized = normalize_path(path).map_err(|message| Error::message(path, message))?;
        Ok(self.files.get(&normalized))
    }

    /// Reads only the selected candidate; missing content is never recovered from a lower layer.
    pub fn read_text(&self, path: &str, encoding: TextEncoding) -> Result<TextDocument, Error> {
        let trace = self.trace(path)?.ok_or_else(|| {
            Error::message(path, "file was not found in the selected content sources")
        })?;
        let origin = trace
            .effective()
            .ok_or_else(|| Error::message(path, "file was removed by a directory replacement"))?;
        self.sources[origin.source_index]
            .root
            .read_text(&origin.relative_path, encoding)
    }
}

fn validate_sources(sources: &[ContentSource]) -> Result<(), Error> {
    let Some(base) = sources.first() else {
        return Err(Error::message(
            "content sources",
            "a base directory is required",
        ));
    };
    if base.descriptor.is_some() {
        return Err(Error::message(
            base.root.path(),
            "the first source must be a base directory",
        ));
    }
    let mut roots = BTreeSet::new();
    let mut names = BTreeMap::new();
    for (index, source) in sources.iter().enumerate() {
        if !roots.insert(source.root.path()) {
            return Err(Error::message(
                source.root.path(),
                "content directory was mounted more than once",
            ));
        }
        if index == 0 {
            continue;
        }
        let Some((path, descriptor)) = &source.descriptor else {
            return Err(Error::message(
                source.root.path(),
                "only the first source can be a base directory",
            ));
        };
        if names.insert(descriptor.name.as_str(), index).is_some() {
            return Err(Error::message(
                path,
                format!("duplicate mod name: {}", descriptor.name),
            ));
        }
    }
    for (index, source) in sources.iter().enumerate().skip(1) {
        if let Some((path, descriptor)) = &source.descriptor {
            for dependency in &descriptor.dependencies {
                let Some(&dependency_index) = names.get(dependency.as_str()) else {
                    return Err(Error::message(
                        path,
                        format!("missing mod dependency: {dependency}"),
                    ));
                };
                if dependency_index >= index {
                    return Err(Error::message(
                        path,
                        format!(
                            "mod dependency must precede {}: {dependency}",
                            descriptor.name
                        ),
                    ));
                }
            }
        }
    }
    Ok(())
}

fn normalize_path(path: &str) -> Result<String, &'static str> {
    let normalized = path.replace('\\', "/");
    let normalized = normalized.trim_end_matches('/');
    if normalized.is_empty()
        || normalized.contains(':')
        || normalized.chars().any(char::is_control)
        || normalized
            .split('/')
            .any(|part| matches!(part, "" | "." | ".."))
    {
        return Err("expected a relative content path without traversal");
    }
    Ok(normalized.to_ascii_lowercase())
}
