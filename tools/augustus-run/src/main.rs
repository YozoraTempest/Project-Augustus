// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::{path::PathBuf, process::ExitCode};

use augustus_demo::{Demo, Input, System};
use clap::{Parser, ValueEnum};

#[derive(Clone, Copy, ValueEnum)]
enum InputChoice {
    Eu4,
    Native,
}
#[derive(Clone, Copy, ValueEnum)]
enum SystemChoice {
    Fixed,
    Capacity,
}

#[derive(Parser)]
#[command(version, about = "Run the self-authored construction demonstration")]
struct Cli {
    #[arg(long)]
    content: Option<PathBuf>,
    #[arg(long, value_enum, default_value = "eu4")]
    input: InputChoice,
    #[arg(long, value_enum, default_value = "fixed")]
    system: SystemChoice,
    #[arg(long)]
    no_optimise: bool,
    #[arg(long)]
    load: Option<PathBuf>,
    /// Start construction in Greenfield (market) and Riverside (workshop).
    #[arg(long)]
    build: bool,
    #[arg(long, default_value_t = 0)]
    days: u64,
    #[arg(long)]
    save: Option<PathBuf>,
}

fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    let input = match cli.input {
        InputChoice::Eu4 => Input::Eu4,
        InputChoice::Native => Input::Native,
    };
    let system = match cli.system {
        SystemChoice::Fixed => System::Fixed,
        SystemChoice::Capacity => System::Capacity,
    };
    if cli.no_optimise && cli.content.is_none() {
        return Err("--no-optimise requires --content".into());
    }
    let mut demo = match cli.content {
        Some(root) => Demo::from_directory(&root, input, system, !cli.no_optimise)?,
        None => Demo::embedded(input, system)?,
    };
    if let Some(path) = cli.load {
        demo.restore(&std::fs::read_to_string(path)?)?;
    }
    if cli.build {
        demo.build(0, 0)?;
        demo.build(1, 1)?;
    }
    demo.advance(cli.days)?;
    if let Some(path) = cli.save {
        if path.exists() {
            return Err(format!("save destination already exists: {}", path.display()).into());
        }
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;
        file.write_all(demo.save()?.as_bytes())?;
    }
    println!("{}", serde_json::to_string_pretty(&demo.view()?)?);
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
