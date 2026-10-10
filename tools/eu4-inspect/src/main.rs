// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_eu4::{ContentRoot, ModDescriptor, TextDocument, TextEncoding};
use clap::{Parser, Subcommand, ValueEnum};
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
        }
    }
    Ok(())
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
