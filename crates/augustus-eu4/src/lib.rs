// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

//! Read-only EU4 content access. EU4-specific syntax and metadata stay in this crate.

#![forbid(unsafe_code)]

mod bootstrap;
mod content;
mod definitions;
mod descriptor;
mod diagnostic;
mod load;
mod script;
mod text;

pub use bootstrap::{BootstrapReport, CountryView, LoadedSource, MapEntry, VanillaWorld};
pub use content::{ContentRoot, FileEntry};
pub use definitions::{DefinitionIndex, DefinitionKind, DefinitionOrigin};
pub use descriptor::ModDescriptor;
pub use diagnostic::{Error, SourceLocation};
pub use load::{ContentSet, ContentSource, FileCandidate, FileOrigin, FileTrace, Suppression};
pub use script::{CompiledScript, ScriptCompiler, ScriptLibrary};
pub use text::{FieldSummary, ParsedText, TextDocument, TextEncoding};
