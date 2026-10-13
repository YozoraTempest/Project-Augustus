// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::{
    fs,
    path::{Component, Path, PathBuf},
};
use walkdir::WalkDir;

use crate::{Error, TextDocument, TextEncoding};

/// One explicit directory source. This does not apply EU4 mod load-order rules.
#[derive(Debug, Clone)]
pub struct ContentRoot {
    root: PathBuf,
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
