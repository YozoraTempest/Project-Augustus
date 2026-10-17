// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::path::{Path, PathBuf};

/// A one-based position; the column counts bytes, not display characters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceLocation {
    pub line: usize,
    pub column: usize,
}

impl SourceLocation {
    pub(crate) fn from_offset(bytes: &[u8], offset: usize) -> Self {
        let prefix = &bytes[..offset.min(bytes.len())];
        let line = prefix.iter().filter(|&&byte| byte == b'\n').count() + 1;
        let column = prefix
            .iter()
            .rposition(|&byte| byte == b'\n')
            .map_or(prefix.len() + 1, |newline| prefix.len() - newline);
        Self { line, column }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("{}{}: {message}", path.display(), location_suffix(.location))]
pub struct Error {
    path: PathBuf,
    location: Option<SourceLocation>,
    message: String,
    #[source]
    cause: Option<Box<dyn std::error::Error + Send + Sync>>,
}

fn location_suffix(location: &Option<SourceLocation>) -> String {
    location
        .map(|position| format!(":{}:{}", position.line, position.column))
        .unwrap_or_default()
}

impl Error {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn detail(&self) -> &str {
        &self.message
    }

    pub fn location(&self) -> Option<SourceLocation> {
        self.location
    }

    pub(crate) fn message(path: impl Into<PathBuf>, message: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            location: None,
            message: message.into(),
            cause: None,
        }
    }

    pub(crate) fn caused_by(
        path: impl Into<PathBuf>,
        cause: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        let mut error = Self::message(path, cause.to_string());
        error.cause = Some(Box::new(cause));
        error
    }

    pub(crate) fn at_offset(mut self, bytes: &[u8], offset: usize) -> Self {
        self.location = Some(SourceLocation::from_offset(bytes, offset));
        self
    }

    pub(crate) fn with_location(mut self, location: SourceLocation) -> Self {
        self.location = Some(location);
        self
    }
}
