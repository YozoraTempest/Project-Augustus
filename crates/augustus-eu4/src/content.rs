// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::Read,
    path::{Component, Path, PathBuf},
    sync::Mutex,
};
use walkdir::WalkDir;
use zip::ZipArchive;

use crate::{Error, TextDocument, TextEncoding};

/// One explicit directory source. This does not apply EU4 mod load-order rules.
#[derive(Debug, Clone)]
pub struct ContentRoot {
    root: PathBuf,
}

#[derive(Debug)]
pub(crate) struct ArchiveContentRoot {
    path: PathBuf,
    entries: Vec<ArchiveEntry>,
    lookup: BTreeMap<String, usize>,
    archive: Mutex<ZipArchive<File>>,
}

#[derive(Debug, Clone)]
struct ArchiveEntry {
    file: FileEntry,
    member_name: String,
    archive_index: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    pub relative_path: PathBuf,
    pub byte_size: u64,
}

/// Read-only view of logical game files with their physical source paths.
pub trait ContentView {
    fn path(&self) -> &Path;

    fn source_roots(&self) -> Vec<PathBuf>;

    fn files(&self) -> Result<Vec<FileEntry>, Error>;

    fn read_text(
        &self,
        relative_path: impl AsRef<Path>,
        encoding: TextEncoding,
    ) -> Result<TextDocument, Error>;

    fn physical_path(&self, relative_path: &Path) -> PathBuf;
}

impl<T: ContentView + ?Sized> ContentView for &T {
    fn path(&self) -> &Path {
        (**self).path()
    }

    fn source_roots(&self) -> Vec<PathBuf> {
        (**self).source_roots()
    }

    fn files(&self) -> Result<Vec<FileEntry>, Error> {
        (**self).files()
    }

    fn read_text(
        &self,
        relative_path: impl AsRef<Path>,
        encoding: TextEncoding,
    ) -> Result<TextDocument, Error> {
        (**self).read_text(relative_path, encoding)
    }

    fn physical_path(&self, relative_path: &Path) -> PathBuf {
        (**self).physical_path(relative_path)
    }
}

impl ContentRoot {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, Error> {
        let path = path.as_ref();
        let root = fs::canonicalize(path).map_err(|cause| Error::caused_by(path, cause))?;
        if !root.is_dir() {
            return Err(Error::message(path, "content root must be a directory"));
        }
        Ok(Self { root })
    }

    pub fn path(&self) -> &Path {
        &self.root
    }

    /// Returns a sorted catalogue without loading file contents into memory.
    pub fn files(&self) -> Result<Vec<FileEntry>, Error> {
        let mut files = Vec::new();
        for entry in WalkDir::new(&self.root).follow_links(false) {
            let entry = entry.map_err(|cause| {
                let path = cause.path().unwrap_or(&self.root).to_path_buf();
                Error::caused_by(path, cause)
            })?;
            if entry.file_type().is_symlink() {
                return Err(Error::message(
                    entry.path(),
                    "linked entries are not supported",
                ));
            }
            if entry.file_type().is_file() {
                let metadata = entry
                    .metadata()
                    .map_err(|cause| Error::caused_by(entry.path(), cause))?;
                let relative_path = entry
                    .path()
                    .strip_prefix(&self.root)
                    .map_err(|cause| Error::caused_by(entry.path(), cause))?
                    .to_path_buf();
                files.push(FileEntry {
                    relative_path,
                    byte_size: metadata.len(),
                });
            }
        }
        files.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
        Ok(files)
    }

    /// Reads an existing relative file, retaining its physical source path.
    pub fn read_text(
        &self,
        relative_path: impl AsRef<Path>,
        encoding: TextEncoding,
    ) -> Result<TextDocument, Error> {
        let relative_path = relative_path.as_ref();
        if relative_path.as_os_str().is_empty()
            || relative_path
                .components()
                .any(|part| !matches!(part, Component::Normal(_)))
        {
            return Err(Error::message(
                relative_path,
                "expected a relative file path without traversal",
            ));
        }
        let requested = self.root.join(relative_path);
        let resolved =
            fs::canonicalize(&requested).map_err(|cause| Error::caused_by(&requested, cause))?;
        if !resolved.starts_with(&self.root) {
            return Err(Error::message(
                &requested,
                "file resolves outside the content root",
            ));
        }
        TextDocument::read(resolved, encoding)
    }
}

impl ArchiveContentRoot {
    pub(crate) fn open(path: impl AsRef<Path>) -> Result<Self, Error> {
        let path = path.as_ref();
        let canonical_path =
            fs::canonicalize(path).map_err(|cause| Error::caused_by(path, cause))?;
        if !canonical_path.is_file() {
            return Err(Error::message(path, "content archive must be a file"));
        }
        let file = File::open(&canonical_path)
            .map_err(|cause| Error::caused_by(&canonical_path, cause))?;
        let mut archive =
            ZipArchive::new(file).map_err(|cause| Error::caused_by(&canonical_path, cause))?;
        let mut entries = Vec::new();
        for archive_index in 0..archive.len() {
            let entry = archive
                .by_index(archive_index)
                .map_err(|cause| Error::caused_by(&canonical_path, cause))?;
            if entry.is_dir() {
                continue;
            }
            let member_name = entry
                .name()
                .map_err(|cause| Error::caused_by(&canonical_path, cause))?
                .into_owned();
            normalize_content_path(&member_name)
                .map_err(|message| Error::message(&canonical_path, message))?;
            let relative_path = PathBuf::from(member_name.replace('\\', "/"));
            entries.push(ArchiveEntry {
                file: FileEntry {
                    relative_path,
                    byte_size: entry.size(),
                },
                member_name,
                archive_index,
            });
        }
        entries.sort_by(|left, right| left.file.relative_path.cmp(&right.file.relative_path));
        let mut lookup = BTreeMap::new();
        for (entry_index, entry) in entries.iter().enumerate() {
            let member_key = normalize_content_path(&entry.member_name)
                .map_err(|message| Error::message(&canonical_path, message))?;
            lookup.insert(member_key, entry_index);
        }
        Ok(Self {
            path: canonical_path,
            entries,
            lookup,
            archive: Mutex::new(archive),
        })
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn files(&self) -> Vec<FileEntry> {
        self.entries
            .iter()
            .map(|entry| entry.file.clone())
            .collect()
    }

    pub(crate) fn read_text(
        &self,
        relative_path: impl AsRef<Path>,
        encoding: TextEncoding,
    ) -> Result<TextDocument, Error> {
        let relative_path = relative_path.as_ref();
        let raw_path = relative_path
            .to_str()
            .ok_or_else(|| Error::message(relative_path, "content path is not valid UTF-8"))?;
        let logical_path = normalize_content_path(raw_path)
            .map_err(|message| Error::message(relative_path, message))?;
        let entry = self
            .lookup
            .get(&logical_path)
            .and_then(|index| self.entries.get(*index))
            .ok_or_else(|| {
                Error::message(self.path.join(relative_path), "archive entry was not found")
            })?;
        let source = self.path.join(&entry.file.relative_path);
        let mut archive = self
            .archive
            .lock()
            .map_err(|_| Error::message(&self.path, "archive reader lock is poisoned"))?;
        let mut file = archive
            .by_index(entry.archive_index)
            .map_err(|cause| Error::caused_by(&source, cause))?;
        let mut bytes = Vec::with_capacity(entry.file.byte_size.min(1_048_576) as usize);
        file.read_to_end(&mut bytes)
            .map_err(|cause| Error::caused_by(&source, cause))?;
        Ok(TextDocument::from_bytes(source, bytes, encoding))
    }

    pub(crate) fn physical_path(&self, relative_path: &Path) -> PathBuf {
        self.path.join(relative_path)
    }
}

pub(crate) fn normalize_content_path(path: &str) -> Result<String, &'static str> {
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

impl ContentView for ContentRoot {
    fn path(&self) -> &Path {
        ContentRoot::path(self)
    }

    fn source_roots(&self) -> Vec<PathBuf> {
        vec![self.path().to_path_buf()]
    }

    fn files(&self) -> Result<Vec<FileEntry>, Error> {
        ContentRoot::files(self)
    }

    fn read_text(
        &self,
        relative_path: impl AsRef<Path>,
        encoding: TextEncoding,
    ) -> Result<TextDocument, Error> {
        ContentRoot::read_text(self, relative_path, encoding)
    }

    fn physical_path(&self, relative_path: &Path) -> PathBuf {
        self.path().join(relative_path)
    }
}
