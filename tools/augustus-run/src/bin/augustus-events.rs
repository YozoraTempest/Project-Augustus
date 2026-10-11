// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::{io::Write, path::PathBuf, process::ExitCode};

use augustus_core::{EventChoice, PendingEventId};
use augustus_demo::{EventDemo, Input};
use clap::{Parser, ValueEnum};

#[derive(Clone, Copy, ValueEnum)]
enum InputChoice {
    Eu4,
    Native,
}

#[derive(Parser)]
#[command(version, about = "Run the self-authored event scenario")]
struct Cli {
    #[arg(long)]
    content: Option<PathBuf>,
    #[arg(long, value_enum, default_value = "eu4")]
    input: InputChoice,
    #[arg(long)]
    load: Option<PathBuf>,
    #[arg(long)]
    trigger: Option<String>,
    #[arg(long, default_value_t = 0)]
    entity: usize,
    /// Select pending-id:option-id, for example 0:1. Choices run after advancing.
    #[arg(long)]
    choose: Vec<String>,
    #[arg(long, default_value_t = 0)]
    days: u64,
    #[arg(long)]
    save: Option<PathBuf>,
}

fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    let choices = cli
        .choose
        .iter()
        .map(|choice| {
            let (pending, option) = choice
                .split_once(':')
                .ok_or("expected --choose pending-id:option-id")?;
            if option.is_empty() {
                return Err("empty option id".into());
            }
            Ok(EventChoice {
                pending: PendingEventId(pending.parse()?),
                option: option.into(),
            })
        })
        .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
    let input = match cli.input {
        InputChoice::Eu4 => Input::Eu4,
        InputChoice::Native => Input::Native,
    };
    let mut demo = match cli.content {
        Some(root) => EventDemo::from_directory(&root, input)?,
        None => EventDemo::embedded(input)?,
    };
    if let Some(path) = cli.load {
        demo.restore(&std::fs::read_to_string(path)?)?;
    }
    if let Some(event) = cli.trigger {
        demo.trigger(&event, cli.entity)?;
    }
    demo.advance(cli.days)?;
    for choice in choices {
        demo.choose(&choice)?;
    }
    if let Some(path) = cli.save {
        let save = demo.save()?;
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?
            .write_all(save.as_bytes())?;
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
