// SPDX-License-Identifier: GPL-3.0-only

use clap::Command;

fn main() -> std::io::Result<()> {
    let mut command = Command::new("augustus")
        .version(env!("CARGO_PKG_VERSION"))
        .about("Project Augustus — 独立实现 EU4 游戏引擎")
        .after_help("当前为新工程初始化阶段，尚未实现游戏功能。");

    command.get_matches_mut();
    command.print_help()
}
