// SPDX-License-Identifier: GPL-3.0-only

//! 读取 EU4 内容并提供查询接口，文件格式处理与游戏模拟核心分离。

mod content;
pub mod map;

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{}: {source}", path.display())]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{}: {message}", path.display())]
    InvalidData { path: PathBuf, message: String },
    #[error("coordinate ({x}, {y}) is outside the {width} x {height} map")]
    OutOfBounds {
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    },
}

impl Error {
    pub(crate) fn invalid(path: impl Into<PathBuf>, message: impl Into<String>) -> Self {
        Self::InvalidData {
            path: path.into(),
            message: message.into(),
        }
    }
}
