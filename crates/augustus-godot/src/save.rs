// SPDX-License-Identifier: GPL-3.0-only

use std::{fs, path::Path};

use augustus_core::{GameDate, SavedGame};
use serde::{Deserialize, Serialize};

const SAVE_VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
pub(crate) struct SaveFile {
    version: u32,
    pub game_directory: String,
    pub opening_date: GameDate,
    pub game: SavedGame,
}

impl SaveFile {
    pub fn new(game_directory: String, opening_date: GameDate, game: SavedGame) -> Self {
        Self {
            version: SAVE_VERSION,
            game_directory,
            opening_date,
            game,
        }
    }

    pub fn read(path: &Path) -> Result<Self, String> {
        let bytes = fs::read(path).map_err(|error| format!("无法打开存档：{error}"))?;
        // 先读取版本，让新版存档得到明确提示，而不是被当成当前格式解析。
        #[derive(Deserialize)]
        struct Header {
            version: u32,
        }
        let header: Header =
            serde_json::from_slice(&bytes).map_err(|error| format!("无法解析存档：{error}"))?;
        if header.version != SAVE_VERSION {
            return Err(format!(
                "不支持存档版本 {}，当前支持版本 {SAVE_VERSION}",
                header.version
            ));
        }
        serde_json::from_slice(&bytes).map_err(|error| format!("无法解析存档：{error}"))
    }

    pub fn write(&self, path: &Path) -> Result<(), String> {
        let directory = path.parent().ok_or("请选择有效的存档位置")?;
        // 在同一目录写完后再替换目标；失败不会截断已有存档，临时文件自动清理。
        let mut pending = tempfile::NamedTempFile::new_in(directory)
            .map_err(|error| format!("无法创建存档：{error}"))?;
        serde_json::to_writer(&mut pending, self)
            .map_err(|error| format!("无法写入存档：{error}"))?;
        pending
            .as_file()
            .sync_all()
            .map_err(|error| format!("无法保存存档：{error}"))?;
        pending
            .persist(path)
            .map_err(|error| format!("无法替换存档：{}", error.error))?;
        Ok(())
    }
}
