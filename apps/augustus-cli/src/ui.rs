// SPDX-License-Identifier: GPL-3.0-only

//! 只读检查 EU4 界面。不把游戏文件写回仓库。

use std::path::PathBuf;

use augustus_eu4::interface::{
    InterfaceSource, document_json, inspect_gfx, inspect_gui, inspect_window, window_json,
};
use clap::Subcommand;

#[derive(Subcommand)]
pub enum UiCommand {
    /// 读取一个 `.gui` 或 `.gfx`，输出窗口、精灵和未知项。
    Inspect {
        #[arg(long, value_name = "DIRECTORY")]
        game_dir: PathBuf,
        #[arg(long, value_name = "FILE")]
        file: PathBuf,
        /// 输出 JSON，而不是缩进文本。
        #[arg(long)]
        json: bool,
    },
    /// 只输出指定名称的窗口。
    Window {
        #[arg(long, value_name = "DIRECTORY")]
        game_dir: PathBuf,
        #[arg(long, value_name = "FILE")]
        file: PathBuf,
        #[arg(long)]
        name: String,
        #[arg(long)]
        json: bool,
    },
}

pub fn run(command: UiCommand) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        UiCommand::Inspect {
            game_dir,
            file,
            json,
        } => inspect(game_dir, file, json),
        UiCommand::Window {
            game_dir,
            file,
            name,
            json,
        } => window(game_dir, file, &name, json),
    }
}

fn inspect(game_dir: PathBuf, file: PathBuf, json: bool) -> Result<(), Box<dyn std::error::Error>> {
    let source = InterfaceSource::open(game_dir);
    if is_gfx(&file) {
        let catalog = source.load_gfx(&file)?;
        if json {
            println!("{}", serde_json::to_string(&gfx_json(&catalog))?);
        } else {
            println!("{}", inspect_gfx(&catalog));
        }
        return Ok(());
    }
    let document = source.load_gui(&file)?;
    let sprites = source.load_gfx_catalog()?;
    let localisation = source.load_localisation()?;
    if json {
        println!("{}", document_json(&document));
    } else {
        println!(
            "{}",
            inspect_gui(&document, Some(&sprites), Some(&localisation))
        );
    }
    Ok(())
}

fn window(
    game_dir: PathBuf,
    file: PathBuf,
    name: &str,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let source = InterfaceSource::open(game_dir);
    let document = source.load_gui(&file)?;
    let localisation = source.load_localisation()?;
    if json {
        let Some(json) = window_json(&document, name) else {
            return Err(format!(
                "{}: window `{name}` was not found",
                document.source.path.display()
            )
            .into());
        };
        println!("{json}");
        return Ok(());
    }
    println!("{}", inspect_window(&document, name, Some(&localisation))?);
    Ok(())
}

fn is_gfx(path: &std::path::Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("gfx"))
}

fn gfx_json(catalog: &augustus_eu4::interface::GfxCatalog) -> serde_json::Value {
    serde_json::json!({
        "sprites": catalog.sprites.iter().map(|sprite| {
            serde_json::json!({
                "kind": sprite.kind,
                "name": sprite.name,
                "texture_file": sprite.texture_file,
                "frames": sprite.frames,
                "frame_duration": sprite.frame_duration,
            })
        }).collect::<Vec<_>>(),
        "errors": catalog.errors,
    })
}
