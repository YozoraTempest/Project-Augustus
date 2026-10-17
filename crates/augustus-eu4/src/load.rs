// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt, fs,
    path::{Path, PathBuf},
};

use serde::{
    Deserialize, Deserializer,
    de::{self, IgnoredAny, MapAccess, Visitor},
};

use crate::{
    ContentRoot, ContentView, Error, FileEntry, ModDescriptor, TextDocument, TextEncoding,
    content::{ArchiveContentRoot, normalize_content_path},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ContentRole {
    UserDirectory,
    Mod,
    Dlc,
    Base,
}

#[derive(Debug)]
enum ContentBacking {
    Directory(ContentRoot),
    Archive(ArchiveContentRoot),
}

/// One explicitly mounted directory or local DLC archive.
#[derive(Debug)]
pub struct ContentSource {
    backing: ContentBacking,
    role: ContentRole,
    label: Option<String>,
    descriptor: Option<(PathBuf, ModDescriptor)>,
    replace_paths: BTreeSet<String>,
}

impl ContentSource {
    pub fn base(root: ContentRoot) -> Self {
        Self {
            backing: ContentBacking::Directory(root),
            role: ContentRole::Base,
            label: None,
            descriptor: None,
            replace_paths: BTreeSet::new(),
        }
    }

    /// Opens the user directory as the highest-priority content overlay.
    pub fn user_directory(path: impl AsRef<Path>) -> Result<Self, Error> {
        Ok(Self {
            backing: ContentBacking::Directory(ContentRoot::open(path)?),
            role: ContentRole::UserDirectory,
            label: None,
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
            let normalized = normalize_content_path(path).map_err(|message| {
                Error::message(
                    document.source(),
                    format!("replace_path {path:?}: {message}"),
                )
            })?;
            replace_paths.insert(normalized);
        }
        Ok(Self {
            backing: ContentBacking::Directory(root),
            role: ContentRole::Mod,
            label: None,
            descriptor: Some((document.source().to_path_buf(), descriptor)),
            replace_paths,
        })
    }

    /// Opens one DLC archive without extracting its files.
    pub fn dlc_archive(path: impl AsRef<Path>, name: impl Into<String>) -> Result<Self, Error> {
        Ok(Self {
            backing: ContentBacking::Archive(ArchiveContentRoot::open(path)?),
            role: ContentRole::Dlc,
            label: Some(name.into()),
            descriptor: None,
            replace_paths: BTreeSet::new(),
        })
    }

    /// Finds locally installed DLC archives described by the EU4 installation.
    /// Only descriptors under `dlc/` and `builtin_dlc/` are considered.
    pub fn installed_dlc_archives(game_root: impl AsRef<Path>) -> Result<Vec<Self>, Error> {
        discover_installed_dlc_archives(game_root.as_ref())
    }

    pub fn directory_root(&self) -> Option<&ContentRoot> {
        match &self.backing {
            ContentBacking::Directory(root) => Some(root),
            ContentBacking::Archive(_) => None,
        }
    }

    pub fn path(&self) -> &Path {
        match &self.backing {
            ContentBacking::Directory(root) => root.path(),
            ContentBacking::Archive(archive) => archive.path(),
        }
    }

    fn files(&self) -> Result<Vec<FileEntry>, Error> {
        match &self.backing {
            ContentBacking::Directory(root) => root.files(),
            ContentBacking::Archive(archive) => Ok(archive.files()),
        }
    }

    fn read_text(
        &self,
        relative_path: impl AsRef<Path>,
        encoding: TextEncoding,
    ) -> Result<TextDocument, Error> {
        match &self.backing {
            ContentBacking::Directory(root) => root.read_text(relative_path, encoding),
            ContentBacking::Archive(archive) => archive.read_text(relative_path, encoding),
        }
    }

    fn physical_path(&self, relative_path: &Path) -> PathBuf {
        match &self.backing {
            ContentBacking::Directory(root) => root.path().join(relative_path),
            ContentBacking::Archive(archive) => archive.physical_path(relative_path),
        }
    }

    pub fn name(&self) -> &str {
        match self.role {
            ContentRole::UserDirectory => "EU4 user directory",
            ContentRole::Base => "EU4 base",
            ContentRole::Dlc => self.label.as_deref().expect("DLC archives have labels"),
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

#[derive(Debug)]
struct LauncherModSelection {
    enabled_mods: BTreeSet<String>,
}

impl<'de> Deserialize<'de> for LauncherModSelection {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct SelectionVisitor;

        impl<'de> Visitor<'de> for SelectionVisitor {
            type Value = LauncherModSelection;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("an EU4 launcher configuration object")
            }

            fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                let mut enabled_mods = None;
                while let Some(key) = map.next_key::<String>()? {
                    if key.eq_ignore_ascii_case("enabled_mods") {
                        if enabled_mods.is_some() {
                            return Err(de::Error::custom(
                                "duplicate enabled_mods configuration field",
                            ));
                        }
                        enabled_mods = Some(map.next_value::<Vec<String>>()?);
                    } else {
                        let _: IgnoredAny = map.next_value()?;
                    }
                }
                let enabled_mods =
                    enabled_mods.ok_or_else(|| de::Error::missing_field("enabled_mods"))?;
                Ok(LauncherModSelection {
                    enabled_mods: enabled_mods.into_iter().collect(),
                })
            }
        }

        deserializer.deserialize_map(SelectionVisitor)
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
    /// Orders sources by search priority. Archive paths remain virtual and are never extracted.
    pub fn resolve(sources: Vec<ContentSource>) -> Result<Self, Error> {
        let sources = order_sources(sources)?;
        let mut files = BTreeMap::<String, FileTrace>::new();
        for (source_index, source) in sources.iter().enumerate() {
            let mut seen = BTreeSet::new();
            for entry in source.files()? {
                let physical_path = source.physical_path(&entry.relative_path);
                let path = entry.relative_path.to_str().ok_or_else(|| {
                    Error::message(&physical_path, "content path is not valid UTF-8")
                })?;
                let logical_path = normalize_content_path(path)
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

    /// Resolves the base game, locally installed DLC, and explicitly supplied directory Mods.
    pub fn from_installation(
        game_root: impl AsRef<Path>,
        mod_directories: &[PathBuf],
        user_directory: Option<&Path>,
        launcher_config: Option<&Path>,
        descriptor_encoding: TextEncoding,
    ) -> Result<Self, Error> {
        let base = ContentRoot::open(game_root)?;
        let mut sources = vec![ContentSource::base(base.clone())];
        sources.extend(ContentSource::installed_dlc_archives(base.path())?);
        for path in mod_directories {
            sources.push(ContentSource::mod_directory(path, descriptor_encoding)?);
        }
        if let Some(path) = user_directory {
            sources.push(ContentSource::user_directory(path)?);
        }
        match launcher_config {
            Some(path) => Self::resolve_with_launcher_config(sources, path),
            None => Self::resolve(sources),
        }
    }

    /// Selects explicitly provided Mods from `enabled_mods` in EU4's `dlc_load.json`.
    /// DLC sources remain governed by the fixed all-DLC ruleset.
    pub fn resolve_with_launcher_config(
        sources: Vec<ContentSource>,
        config_path: impl AsRef<Path>,
    ) -> Result<Self, Error> {
        let config_path = config_path.as_ref();
        let bytes = fs::read(config_path).map_err(|cause| Error::caused_by(config_path, cause))?;
        let selection: LauncherModSelection =
            serde_json::from_slice(&bytes).map_err(|cause| Error::caused_by(config_path, cause))?;
        let provided = sources
            .iter()
            .filter(|source| source.role == ContentRole::Mod)
            .map(|source| source.name().to_owned())
            .collect::<BTreeSet<_>>();
        if let Some(missing) = selection.enabled_mods.difference(&provided).next() {
            return Err(Error::message(
                config_path,
                format!("enabled Mod has no supplied directory source: {missing}"),
            ));
        }
        let selected = sources
            .into_iter()
            .filter(|source| {
                source.role != ContentRole::Mod || selection.enabled_mods.contains(source.name())
            })
            .collect();
        Self::resolve(selected)
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
            .directory_root()
            .expect("base source is a directory")
    }

    pub fn trace(&self, path: &str) -> Result<Option<&FileTrace>, Error> {
        let normalized =
            normalize_content_path(path).map_err(|message| Error::message(path, message))?;
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
        self.sources[origin.source_index].read_text(&origin.relative_path, encoding)
    }
}

impl ContentView for ContentSet {
    fn path(&self) -> &Path {
        self.base_root().path()
    }

    fn source_roots(&self) -> Vec<PathBuf> {
        self.sources
            .iter()
            .map(|source| source.path().to_path_buf())
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

fn discover_installed_dlc_archives(game_root: &Path) -> Result<Vec<ContentSource>, Error> {
    let game_root =
        fs::canonicalize(game_root).map_err(|cause| Error::caused_by(game_root, cause))?;
    if !game_root.is_dir() {
        return Err(Error::message(
            game_root,
            "EU4 installation must be a directory",
        ));
    }

    let mut found = Vec::<(String, ContentSource)>::new();
    for category in ["builtin_dlc", "dlc"] {
        let category_root = game_root.join(category);
        let directories = match fs::read_dir(&category_root) {
            Ok(entries) => entries
                .map(|entry| entry.map_err(|cause| Error::caused_by(&category_root, cause)))
                .collect::<Result<Vec<_>, _>>()?,
            Err(cause) if cause.kind() == std::io::ErrorKind::NotFound => continue,
            Err(cause) => return Err(Error::caused_by(&category_root, cause)),
        };
        let mut directories = directories;
        directories.sort_by_key(|entry| entry.file_name());
        for directory in directories {
            let file_type = directory
                .file_type()
                .map_err(|cause| Error::caused_by(directory.path(), cause))?;
            if file_type.is_symlink() || !file_type.is_dir() {
                continue;
            }
            let descriptor_directory = directory.path();
            let descriptors = match fs::read_dir(&descriptor_directory) {
                Ok(entries) => entries
                    .map(|entry| {
                        entry.map_err(|cause| Error::caused_by(&descriptor_directory, cause))
                    })
                    .collect::<Result<Vec<_>, _>>()?,
                Err(cause) => return Err(Error::caused_by(&descriptor_directory, cause)),
            };
            let mut descriptors = descriptors;
            descriptors.sort_by_key(|entry| entry.file_name());
            for descriptor_file in descriptors {
                let path = descriptor_file.path();
                let file_type = descriptor_file
                    .file_type()
                    .map_err(|cause| Error::caused_by(&path, cause))?;
                if file_type.is_symlink()
                    || !file_type.is_file()
                    || !path
                        .extension()
                        .is_some_and(|extension| extension.eq_ignore_ascii_case("dlc"))
                {
                    continue;
                }
                let document = TextDocument::read(&path, TextEncoding::Windows1252)?;
                let parsed = document.parse()?;
                let descriptor = ModDescriptor::parse(&parsed)?;
                let Some(archive) = descriptor.archive.as_deref() else {
                    continue;
                };
                let relative_path = archive.replace('\\', "/");
                normalize_content_path(&relative_path).map_err(|message| {
                    Error::message(
                        &path,
                        format!("invalid DLC archive path {archive:?}: {message}"),
                    )
                })?;
                let requested_archive = game_root.join(&relative_path);
                let archive_path = match fs::canonicalize(&requested_archive) {
                    Ok(path) => path,
                    Err(cause) if cause.kind() == std::io::ErrorKind::NotFound => continue,
                    Err(cause) => return Err(Error::caused_by(&requested_archive, cause)),
                };
                if !archive_path.starts_with(&game_root) {
                    return Err(Error::message(
                        &path,
                        format!("DLC archive path resolves outside the installation: {archive}"),
                    ));
                }
                if !archive_path.is_file() {
                    continue;
                }
                let name = descriptor.name;
                let source = ContentSource::dlc_archive(&archive_path, format!("DLC: {name}"))?;
                found.push((name, source));
            }
        }
    }
    found.sort_by(|left, right| right.0.as_bytes().cmp(left.0.as_bytes()));
    Ok(found.into_iter().map(|(_, source)| source).collect())
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
    let mut dlcs = Vec::new();
    for source in sources.drain(..) {
        if !roots.insert(source.path().to_path_buf()) {
            return Err(Error::message(
                source.path(),
                "content source was mounted more than once",
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
            ContentRole::Dlc => dlcs.push(source),
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
            left_priority.cmp(right_priority).then_with(|| {
                left_source
                    .name()
                    .as_bytes()
                    .cmp(right_source.name().as_bytes())
            })
        },
    );

    let mut result = Vec::with_capacity(ordered.len() + dlcs.len() + 2);
    if let Some(user_directory) = user_directory {
        result.push(user_directory);
    }
    result.extend(ordered.into_iter().map(|(source, _)| source));
    result.extend(dlcs);
    result.push(base);
    Ok(result)
}
