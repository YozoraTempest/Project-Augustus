// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

//! Read-only EU4 content access. EU4-specific syntax and metadata stay in this crate.

#![forbid(unsafe_code)]

mod administration;
mod administration_validation;
mod attributes;
mod autonomy;
mod bootstrap;
mod content;
mod country_conditions;
mod country_status;
mod defines;
mod definitions;
mod descriptor;
mod diagnostic;
mod economy;
mod estates;
mod geography;
mod governments;
mod ideas;
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
pub use estates::EstateReport;
pub use governments::GovernmentReport;
pub use ideas::IdeaReport;
pub use load::{ContentSet, ContentSource, FileCandidate, FileOrigin, FileTrace, Suppression};
pub use politics::{RelationSource, RelationView};
pub use script::{CompiledEvents, CompiledScript, ScriptCompiler, ScriptLibrary};
pub use setup::SetupReport;
pub use text::{FieldSummary, ParsedText, TextDocument, TextEncoding};

pub(crate) const EXPANSIONS: [&str; 22] = [
    "Conquest of Paradise",
    "Wealth of Nations",
    "Res Publica",
    "Art of War",
    "El Dorado",
    "Common Sense",
    "The Cossacks",
    "Mare Nostrum",
    "Rights of Man",
    "Mandate of Heaven",
    "Third Rome",
    "Cradle of Civilization",
    "Rule Britannia",
    "Dharma",
    "Golden Century",
    "Emperor",
    "Leviathan",
    "Origins",
    "Lions of the North",
    "Domination",
    "King of Kings",
    "Winds of Change",
];

/// Recognized DLC queries under the fixed final-edition, all-DLC ruleset.
/// Mechanism support remains separate from content availability.
pub(crate) fn is_known_dlc(name: &str) -> bool {
    EXPANSIONS.contains(&name)
        || [
            "American Dream",
            "Purple Phoenix",
            "Star and Crescent",
            "Women in History",
            "Dharma - Hindu Unit Pack",
            "Dharma - Indian Sultanate Pack",
        ]
        .contains(&name)
}
