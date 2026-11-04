// SPDX-License-Identifier: GPL-3.0-only

use std::{path::PathBuf, process::ExitCode};

use augustus_eu4::map::ProvinceMap;
use clap::{CommandFactory, Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "augustus",
    version,
    about = "Project Augustus — 独立实现 EU4 游戏引擎"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    #[command(about = "读取 EU4 省份地图")]
    Map {
        #[arg(long, value_name = "DIRECTORY", help = "EU4 游戏安装目录")]
        game_dir: PathBuf,
        #[arg(long, num_args = 2, value_names = ["X", "Y"], help = "查询像素坐标；左上角为 (0, 0)，向右、向下递增")]
        at: Option<Vec<u32>>,
    },
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    match cli.command {
        None => Cli::command().print_help()?,
        Some(Commands::Map { game_dir, at }) => {
            let map = ProvinceMap::load(game_dir)?;
            // 先完成可选的坐标查询，成功后统一输出，避免失败时留下部分摘要。
            let province = at
                .as_ref()
                .map(|point| map.province_at(point[0], point[1]))
                .transpose()?;
            let summary = map.summary();
            println!("Map size: {} x {}", summary.width, summary.height);
            println!("Defined provinces: {}", summary.defined_provinces);
            println!("Mapped provinces: {}", summary.mapped_provinces);
            println!("Unassigned pixels: {}", summary.unassigned_pixels);
            if let Some(province) = province {
                match province {
                    Some(id) => println!("Province: {id}"),
                    None => println!("Province: none"),
                }
            }
        }
    }
    Ok(())
}
