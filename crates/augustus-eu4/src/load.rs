// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use crate::{
    ContentRoot, ContentView, Error, FileEntry, ModDescriptor, TextDocument, TextEncoding,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ContentRole {
    UserDirectory,
    Mod,
    Base,
}

/// An explicitly mounted directory and its optional mod descriptor.
#[derive(Debug)]
pub struct ContentSource {
    root: ContentRoot,
    role: ContentRole,
    descriptor: Option<(PathBuf, ModDescriptor)>,
    replace_paths: BTreeSet<String>,
}

impl ContentSource {
    pub fn base(root: ContentRoot) -> Self {
        Self {
            root,
            role: ContentRole::Base,
            descriptor: None,
            replace_paths: BTreeSet::new(),
        }
    }

    /// Opens the user directory as the highest-priority content overlay.
    pub fn user_directory(path: impl AsRef<Path>) -> Result<Self, Error> {
        Ok(Self {
            root: ContentRoot::open(path)?,
            role: ContentRole::UserDirectory,
            descriptor: None,
            replace_paths: BTreeSet::new(),
        })
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
            role: ContentRole::Mod,
            descriptor: Some((document.source().to_path_buf(), descriptor)),
            replace_paths,
        })
    }

    pub fn root(&self) -> &ContentRoot {
        &self.root
    }

    pub fn name(&self) -> &str {
        match self.role {
            ContentRole::UserDirectory => "EU4 user directory",
            ContentRole::Base => "EU4 base",
            ContentRole::Mod => &self.descriptor().expect("mods have descriptors").name,
        }
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
    /// Candidates in resolved VFS priority order, retaining their original spelling.
    pub candidates: Vec<FileCandidate>,
}

impl FileTrace {
    pub fn effective(&self) -> Option<&FileOrigin> {
        self.candidates
            .iter()
            .find(|item| item.suppressed_by.is_none())
            .map(|item| &item.origin)
    }
}

/// A read-only file manifest. Object-definition priority is deliberately separate.
#[derive(Debug)]
pub struct ContentSet {
    sources: Vec<ContentSource>,
    files: BTreeMap<String, FileTrace>,
}

impl ContentSet {
    /// Orders supported directory sources by VFS priority; DLC archives are not represented here.
    pub fn resolve(sources: Vec<ContentSource>) -> Result<Self, Error> {
        let sources = order_sources(sources)?;
        let mut files = BTreeMap::<String, FileTrace>::new();
        for (source_index, source) in sources.iter().enumerate() {
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
        for trace in files.values_mut() {
            let replacement = sources.iter().enumerate().find_map(|(index, source)| {
                source
                    .replace_paths
                    .iter()
                    .filter(|path| trace.logical_path.starts_with(&format!("{path}/")))
                    .max_by_key(|path| path.len())
                    .map(|path| (index, path.clone()))
            });
            let mut winner = None;
            for candidate in &mut trace.candidates {
                if let Some((by_source, replace_path)) = &replacement
                    && candidate.origin.source_index > *by_source
                {
                    candidate.suppressed_by = Some(Suppression::DirectoryReplacement {
                        by_source: *by_source,
                        replace_path: replace_path.clone(),
                    });
                } else if let Some(by_source) = winner {
                    candidate.suppressed_by = Some(Suppression::FileOverride { by_source });
                } else {
                    winner = Some(candidate.origin.source_index);
                }
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

    pub fn base_root(&self) -> &ContentRoot {
        self.sources
            .iter()
            .find(|source| source.role == ContentRole::Base)
            .expect("resolved content always has a base source")
            .root()
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

impl ContentView for ContentSet {
    fn path(&self) -> &Path {
        self.base_root().path()
    }

    fn source_roots(&self) -> Vec<PathBuf> {
        self.sources
            .iter()
            .map(|source| source.root.path().to_path_buf())
            .collect()
    }

    fn files(&self) -> Result<Vec<FileEntry>, Error> {
        Ok(self
            .effective_files()
            .map(|(_, origin)| FileEntry {
                relative_path: origin.relative_path.clone(),
                byte_size: origin.byte_size,
            })
            .collect())
    }

    fn read_text(
        &self,
        relative_path: impl AsRef<Path>,
        encoding: TextEncoding,
    ) -> Result<TextDocument, Error> {
        let relative_path = relative_path.as_ref().to_str().ok_or_else(|| {
            Error::message(relative_path.as_ref(), "content path is not valid UTF-8")
        })?;
        ContentSet::read_text(self, relative_path, encoding)
    }

    fn physical_path(&self, relative_path: &Path) -> PathBuf {
        let fallback = || self.path().join(relative_path);
        let Some(logical_path) = relative_path.to_str() else {
            return fallback();
        };
        self.trace(logical_path)
            .ok()
            .flatten()
            .and_then(FileTrace::effective)
            .map_or_else(fallback, |origin| origin.physical_path.clone())
    }
}

fn order_sources(mut sources: Vec<ContentSource>) -> Result<Vec<ContentSource>, Error> {
    if sources.is_empty() {
        return Err(Error::message(
            "content sources",
            "a base directory is required",
        ));
    }

    let mut roots = BTreeSet::<PathBuf>::new();
    let mut user_directory = None;
    let mut base = None;
    let mut mods = Vec::new();
    for source in sources.drain(..) {
        if !roots.insert(source.root.path().to_path_buf()) {
            return Err(Error::message(
                source.root.path(),
                "content directory was mounted more than once",
            ));
        }
        match source.role {
            ContentRole::UserDirectory => {
                if user_directory.replace(source).is_some() {
                    return Err(Error::message(
                        "content sources",
                        "only one user directory can be mounted",
                    ));
                }
            }
            ContentRole::Base => {
                if base.replace(source).is_some() {
                    return Err(Error::message(
                        "content sources",
                        "only one base directory can be mounted",
                    ));
                }
            }
            ContentRole::Mod => mods.push(source),
        }
    }

    let Some(base) = base else {
        return Err(Error::message(
            "content sources",
            "a base directory is required",
        ));
    };
    let mut names = BTreeMap::new();
    for (index, source) in mods.iter().enumerate() {
        let descriptor = source.descriptor().expect("mods have descriptors");
        if let Some((path, _)) = source
            .descriptor
            .as_ref()
            .and_then(|(path, _)| names.insert(descriptor.name.clone(), (path.clone(), index)))
        {
            return Err(Error::message(
                path,
                format!("duplicate mod name: {}", descriptor.name),
            ));
        }
    }

    let name_indices = names
        .iter()
        .map(|(name, (_, index))| (name.as_str(), *index))
        .collect::<BTreeMap<_, _>>();
    let mut priorities = vec![-1000_i64; mods.len()];
    let mut converged = mods.is_empty();
    for _ in 0..=mods.len() {
        let mut changed = false;
        for (current_index, source) in mods.iter().enumerate() {
            let descriptor = source.descriptor().expect("mods have descriptors");
            for dependency in &descriptor.dependencies {
                let Some(&dependency_index) = name_indices.get(dependency.as_str()) else {
                    continue;
                };
                if priorities[dependency_index] <= priorities[current_index] {
                    priorities[dependency_index] = priorities[current_index] + 1;
                    changed = true;
                }
            }
        }
        if !changed {
            converged = true;
            break;
        }
    }
    if !converged {
        return Err(Error::message(
            "content sources",
            "cyclic mod dependencies do not produce a stable priority order",
        ));
    }

    let mut ordered = mods.into_iter().zip(priorities).collect::<Vec<_>>();
    ordered.sort_by(
        |(left_source, left_priority), (right_source, right_priority)| {
            right_priority.cmp(left_priority).then_with(|| {
                right_source
                    .name()
                    .as_bytes()
                    .cmp(left_source.name().as_bytes())
            })
        },
    );

    let mut result = Vec::with_capacity(ordered.len() + 2);
    if let Some(user_directory) = user_directory {
        result.push(user_directory);
    }
    result.extend(ordered.into_iter().map(|(source, _)| source));
    result.push(base);
    Ok(result)
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
