// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

//! Read-only EU4 content access. EU4-specific syntax and metadata stay in this crate.

#![forbid(unsafe_code)]

mod administration;
mod administration_validation;
mod attributes;
mod bootstrap;
mod content;
mod defines;
mod definitions;
mod descriptor;
mod diagnostic;
mod economy;
mod load;
mod politics;
mod script;
mod setup;
mod text;

pub use administration::AdministrationReport;
pub use administration_validation::{
    AdministrationCheckCount, AdministrationComparison, AdministrationDifference,
    compare_administration_save,
};
pub use attributes::AttributeReport;
pub use bootstrap::{
    BootstrapReport, CountryView, LoadedSource, MapEntry, ProvinceView, VanillaWorld,
};
pub use content::{ContentRoot, FileEntry};
pub use definitions::{DefinitionIndex, DefinitionKind, DefinitionOrigin};
pub use descriptor::ModDescriptor;
pub use diagnostic::{Error, SourceLocation};
pub use economy::EconomyReport;
pub use load::{ContentSet, ContentSource, FileCandidate, FileOrigin, FileTrace, Suppression};
pub use politics::{RelationSource, RelationView};
pub use script::{CompiledScript, ScriptCompiler, ScriptLibrary};
pub use setup::SetupReport;
pub use text::{FieldSummary, ParsedText, TextDocument, TextEncoding};
