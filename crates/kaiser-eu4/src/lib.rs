// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

//! Read-only EU4 content access. EU4-specific syntax and metadata stay in this crate.

#![forbid(unsafe_code)]

mod content;
mod descriptor;
mod diagnostic;
mod text;

pub use content::{ContentRoot, FileEntry};
pub use descriptor::ModDescriptor;
pub use diagnostic::{Error, SourceLocation};
pub use text::{FieldSummary, ParsedText, TextDocument, TextEncoding};
