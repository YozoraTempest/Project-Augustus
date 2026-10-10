// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_eu4::{
    ContentRoot, ContentSet, ContentSource, DefinitionIndex, DefinitionKind, FileTrace,
    ModDescriptor, Suppression, TextDocument, TextEncoding,
};
use clap::{Args, Parser, Subcommand, ValueEnum};
use std::{collections::BTreeMap, path::PathBuf, process::ExitCode};

#[derive(Parser)]
#[command(version, about = "Read-only inspection of local EU4 and mod files")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Catalogue an explicitly selected game or mod directory.
    Catalog {
        root: PathBuf,
        #[arg(long)]
        list: bool,
    },
    /// Read a .mod file; descriptor paths are reported rather than followed.
    Descriptor {
        file: PathBuf,
        #[arg(long, value_enum, default_value_t = EncodingArg::Utf8)]
        encoding: EncodingArg,
    },
    /// Parse a relative text file within one selected directory.
    Parse {
        root: PathBuf,
        file: PathBuf,
        #[arg(long, value_enum, default_value_t = EncodingArg::Windows1252)]
        encoding: EncodingArg,
    },
    /// Resolve explicitly ordered directories and explain file provenance.
    Resolve {
        #[command(flatten)]
        sources: SourcesArgs,
        #[arg(long, conflicts_with = "list")]
        file: Option<String>,
        #[arg(long)]
        list: bool,
        /// Parse the selected effective file after showing its provenance.
        #[arg(long, requires = "file")]
        parse: bool,
        #[arg(long, value_enum, default_value_t = EncodingArg::Windows1252)]
        encoding: EncodingArg,
    },
    /// Index root declarations in effective files without choosing duplicate winners.
    Definitions {
        #[command(flatten)]
        sources: SourcesArgs,
        #[arg(value_enum)]
        category: CategoryArg,
        #[arg(long, conflicts_with = "list")]
        name: Option<String>,
        #[arg(long)]
        list: bool,
        /// Show only names declared more than once.
        #[arg(long, conflicts_with_all = ["name", "list"])]
        duplicates: bool,
        #[arg(long, value_enum, default_value_t = EncodingArg::Windows1252)]
        encoding: EncodingArg,
    },
}

#[derive(Args)]
struct SourcesArgs {
    /// Explicit base game directory.
    base: PathBuf,
    /// Mod directory containing descriptor.mod; repeat in the desired load order.
    #[arg(long = "mod", value_name = "DIRECTORY")]
    mods: Vec<PathBuf>,
    #[arg(long, value_enum, default_value_t = EncodingArg::Utf8)]
    descriptor_encoding: EncodingArg,
}

impl SourcesArgs {
    fn resolve(self) -> Result<ContentSet, augustus_eu4::Error> {
        let mut sources = vec![ContentSource::base(ContentRoot::open(self.base)?)];
        for root in self.mods {
            sources.push(ContentSource::mod_directory(
                root,
                self.descriptor_encoding.into(),
            )?);
        }
        ContentSet::resolve(sources)
    }
}

#[derive(Clone, Copy, ValueEnum)]
enum CategoryArg {
    Buildings,
    ScriptedEffects,
    ScriptedTriggers,
}

impl From<CategoryArg> for DefinitionKind {
    fn from(category: CategoryArg) -> Self {
        match category {
            CategoryArg::Buildings => Self::Buildings,
            CategoryArg::ScriptedEffects => Self::ScriptedEffects,
            CategoryArg::ScriptedTriggers => Self::ScriptedTriggers,
        }
    }
}

#[derive(Clone, Copy, ValueEnum)]
enum EncodingArg {
    Utf8,
    Windows1252,
}

impl From<EncodingArg> for TextEncoding {
    fn from(encoding: EncodingArg) -> Self {
        match encoding {
            EncodingArg::Utf8 => Self::Utf8,
            EncodingArg::Windows1252 => Self::Windows1252,
        }
    }
}

fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    match cli.command {
        Command::Catalog { root, list } => {
            let root = ContentRoot::open(root)?;
            let files = root.files()?;
            println!("Source: {}", root.path().display());
            println!("Files: {}", files.len());
            let mut groups = BTreeMap::<String, usize>::new();
            for file in &files {
                let category = file
                    .relative_path
                    .components()
                    .next()
                    .map(|part| part.as_os_str().to_string_lossy().into_owned())
                    .unwrap_or_default();
                *groups.entry(category).or_default() += 1;
                if list {
                    println!("{}\t{} bytes", file.relative_path.display(), file.byte_size);
                }
            }
            if !list {
                for (category, count) in groups {
                    println!("{category}: {count}");
                }
            }
        }
        Command::Descriptor { file, encoding } => {
            let document = TextDocument::read(file, encoding.into())?;
            let parsed = document.parse()?;
            let descriptor = ModDescriptor::parse(&parsed)?;
            println!("Source: {}", document.source().display());
            println!("Name: {}", descriptor.name);
            println!("Version: {:?}", descriptor.version);
            println!("Supported version: {:?}", descriptor.supported_version);
            println!("Declared path: {:?}", descriptor.path);
            println!("Declared archive: {:?}", descriptor.archive);
            println!("Dependencies: {:?}", descriptor.dependencies);
            println!("Tags: {:?}", descriptor.tags);
            println!("Replace paths: {}", descriptor.replace_paths.len());
            for path in descriptor.replace_paths {
                println!("  {path}");
            }
            println!("Other fields: {:?}", descriptor.other_fields);
        }
        Command::Parse {
            root,
            file,
            encoding,
        } => {
            let root = ContentRoot::open(root)?;
            let document = root.read_text(file, encoding.into())?;
            print_document(&document)?;
        }
        Command::Resolve {
            sources,
            file,
            list,
            parse,
            encoding,
        } => {
            let content = sources.resolve()?;
            print_sources(&content);
            println!("Effective files: {}", content.effective_files().count());
            println!("Tracked paths: {}", content.traces().count());
            if let Some(path) = file {
                let trace = content
                    .trace(&path)?
                    .ok_or_else(|| format!("file not found: {path}"))?;
                print_trace(&content, trace);
                if parse {
                    print_document(&content.read_text(&path, encoding.into())?)?;
                }
            } else if list {
                for trace in content.traces() {
                    print_trace(&content, trace);
                }
            } else {
                let mut groups = BTreeMap::<&str, usize>::new();
                for (path, _) in content.effective_files() {
                    *groups
                        .entry(path.split('/').next().unwrap_or(path))
                        .or_default() += 1;
                }
                for (category, count) in groups {
                    println!("{category}: {count}");
                }
            }
        }
        Command::Definitions {
            sources,
            category,
            name,
            list,
            duplicates,
            encoding,
        } => {
            let content = sources.resolve()?;
            let index = DefinitionIndex::build(&content, category.into(), encoding.into())?;
            print_sources(&content);
            println!("Category: {}", index.kind().directory());
            println!("Definition names: {}", index.definitions().count());
            println!(
                "Declarations: {}",
                index
                    .definitions()
                    .map(|(_, items)| items.len())
                    .sum::<usize>()
            );
            println!(
                "Duplicate names: {}",
                index
                    .definitions()
                    .filter(|(_, items)| items.len() > 1)
                    .count()
            );
            println!("Same-name definition precedence is not resolved.");
            if let Some(name) = name {
                let origins = index
                    .get(&name)
                    .ok_or_else(|| format!("definition not found: {name}"))?;
                print_definition(&content, &name, origins);
            } else if list || duplicates {
                for (name, origins) in index.definitions() {
                    if list || origins.len() > 1 {
                        print_definition(&content, name, origins);
                    }
                }
            }
        }
    }
    Ok(())
}

fn print_document(document: &TextDocument) -> Result<(), augustus_eu4::Error> {
    let parsed = document.parse()?;
    let fields = parsed.root_fields();
    println!("Source: {}", document.source().display());
    println!("Bytes: {}", document.byte_len());
    println!("Tokens: {}", parsed.token_count());
    println!("Root fields: {}", fields.len());
    for field in fields.iter().take(20) {
        println!(
            "{} {} {}",
            field.name,
            field.operator,
            field.scalar.as_deref().unwrap_or("{...}")
        );
    }
    if fields.len() > 20 {
        println!("... {} more fields", fields.len() - 20);
    }
    Ok(())
}

fn print_sources(content: &ContentSet) {
    for (index, source) in content.sources().iter().enumerate() {
        println!(
            "Source {index}: {} ({})",
            source.name(),
            source.root().path().display()
        );
    }
}

fn print_trace(content: &ContentSet, trace: &FileTrace) {
    println!("File: {}", trace.logical_path);
    if let Some(origin) = trace.effective() {
        println!(
            "  Effective source: {}",
            content.sources()[origin.source_index].name()
        );
    } else {
        println!("  Effective source: none");
    }
    for candidate in &trace.candidates {
        let origin = &candidate.origin;
        println!(
            "  {}: {} ({} bytes)",
            content.sources()[origin.source_index].name(),
            origin.physical_path.display(),
            origin.byte_size
        );
        match &candidate.suppressed_by {
            None => println!("    selected"),
            Some(Suppression::FileOverride { by_source }) => {
                println!(
                    "    overridden by file from {}",
                    content.sources()[*by_source].name()
                );
            }
            Some(Suppression::DirectoryReplacement {
                by_source,
                replace_path,
            }) => {
                let source = &content.sources()[*by_source];
                println!(
                    "    excluded by {}: replace_path={replace_path}",
                    source.name()
                );
                if let Some(path) = source.descriptor_path() {
                    println!("    descriptor: {}", path.display());
                }
            }
        }
    }
}

fn print_definition(content: &ContentSet, name: &str, origins: &[augustus_eu4::DefinitionOrigin]) {
    println!("Definition: {name} ({} declarations)", origins.len());
    for origin in origins {
        println!(
            "  {}: {}:{}:{} (root field {})",
            content.sources()[origin.source_index].name(),
            origin.physical_path.display(),
            origin.location.line,
            origin.location.column,
            origin.field_index
        );
    }
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
