// SPDX-License-Identifier: GPL-3.0-only

//! 从单个游戏目录读取界面文件。纹理本身留在玩家目录，只返回安全的相对路径。

use std::path::{Component, Path, PathBuf};

use crate::{Error, content::ContentRoot};

use super::colors::{InterfaceColors, absorb_text_colors, parse_colors_bytes};
use super::gfx::parse_gfx_bytes;
use super::gui::parse_gui_bytes;
use super::localisation::{self, LocalisationCatalog};
use super::messages::{MessageType, parse_message_types_bytes};
use super::model::{ContentStack, GfxCatalog, GuiDocument};

/// 一次读完安装目录里的界面定义。纹理仍留在原处。
pub struct InterfaceLoad {
    pub documents: Vec<GuiDocument>,
    pub sprites: GfxCatalog,
    pub localisation: LocalisationCatalog,
    pub colors: InterfaceColors,
    pub message_types: Vec<MessageType>,
    pub gui_files: Vec<std::path::PathBuf>,
    pub gfx_files: Vec<std::path::PathBuf>,
    /// 单个文件解析失败时保留路径和原因，其余文件继续读。
    pub failures: Vec<String>,
}

pub struct InterfaceSource {
    content: ContentRoot,
}

impl InterfaceSource {
    pub fn open(root: impl AsRef<Path>) -> Self {
        Self {
            content: ContentRoot::new(root),
        }
    }

    pub fn root(&self) -> &Path {
        self.content.path()
    }

    pub fn content_stack(&self) -> ContentStack {
        ContentStack::single(self.content.path())
    }

    pub fn load_gui(&self, relative: impl AsRef<Path>) -> Result<GuiDocument, Error> {
        let file = self.content.read(relative)?;
        parse_gui_bytes(&file.path, &file.bytes)
    }

    pub fn load_gfx(&self, relative: impl AsRef<Path>) -> Result<GfxCatalog, Error> {
        let file = self.content.read(relative)?;
        parse_gfx_bytes(&file.path, &file.bytes)
    }

    pub fn load_gfx_catalog(&self) -> Result<GfxCatalog, Error> {
        let mut catalog = GfxCatalog::new();
        let files = match self.content.files_under("interface", "gfx") {
            Ok(files) => files,
            Err(Error::Read { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
                return Ok(catalog);
            }
            Err(error) => return Err(error),
        };
        for relative in files {
            let file = match self.content.read(&relative) {
                Ok(file) => file,
                Err(error) => {
                    catalog.errors.push(error.to_string());
                    continue;
                }
            };
            match parse_gfx_bytes(&file.path, &file.bytes) {
                Ok(parsed) => catalog.sprites.extend(parsed.sprites),
                Err(error) => catalog.errors.push(error.to_string()),
            }
        }
        Ok(catalog)
    }

    /// 读取 `interface` 下全部 `.gui` 和 `.gfx`，再读英文界面文本。
    pub fn load_interface(&self) -> Result<InterfaceLoad, Error> {
        let gui_files = match self.content.files_under("interface", "gui") {
            Ok(files) => files,
            Err(Error::Read { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
                Vec::new()
            }
            Err(error) => return Err(error),
        };
        let gfx_files = match self.content.files_under("interface", "gfx") {
            Ok(files) => files,
            Err(Error::Read { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
                Vec::new()
            }
            Err(error) => return Err(error),
        };
        let mut failures = Vec::new();
        let mut documents = Vec::new();
        for relative in &gui_files {
            match self.load_gui(relative) {
                Ok(document) => documents.push(document),
                Err(error) => failures.push(error.to_string()),
            }
        }
        let sprites = self.load_gfx_catalog()?;
        failures.extend(sprites.errors.iter().cloned());
        let mut colors = self.load_colors();
        failures.extend(colors.1);
        absorb_text_colors(&sprites, &mut colors.0);
        let messages = self.load_message_types();
        failures.extend(messages.1);
        let localisation = self.load_localisation()?;
        failures.extend(localisation.errors.iter().cloned());
        Ok(InterfaceLoad {
            documents,
            sprites,
            localisation,
            colors: colors.0,
            message_types: messages.0,
            gui_files,
            gfx_files,
            failures,
        })
    }

    /// 只读 `interface/colors.txt`。没有这个文件时得到空表，不读 `credits.txt`。
    pub fn load_colors(&self) -> (InterfaceColors, Vec<String>) {
        self.read_optional("interface/colors.txt", parse_colors_bytes)
    }

    /// 只读 `interface/messagetypes.txt`。没有这个文件时得到空表。
    pub fn load_message_types(&self) -> (Vec<MessageType>, Vec<String>) {
        self.read_optional("interface/messagetypes.txt", parse_message_types_bytes)
    }

    fn read_optional<T>(
        &self,
        relative: &str,
        parse: fn(&std::path::Path, &[u8]) -> Result<T, Error>,
    ) -> (T, Vec<String>)
    where
        T: Default,
    {
        match self.content.read(relative) {
            Ok(file) => match parse(&file.path, &file.bytes) {
                Ok(value) => (value, Vec::new()),
                Err(error) => (T::default(), vec![error.to_string()]),
            },
            Err(Error::Read { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
                (T::default(), Vec::new())
            }
            Err(error) => (T::default(), vec![error.to_string()]),
        }
    }

    pub fn load_localisation(&self) -> Result<LocalisationCatalog, Error> {
        localisation::load(&self.content)
    }

    pub fn resolve_texture(&self, texture_file: &str) -> Result<PathBuf, Error> {
        resolve_texture_path(self.content.path(), texture_file)
    }
}

pub fn resolve_texture_path(root: &Path, texture_file: &str) -> Result<PathBuf, Error> {
    let normalized = texture_file.replace('\\', "/");
    let relative = Path::new(&normalized);
    if relative.as_os_str().is_empty()
        || relative.is_absolute()
        || relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_) | Component::CurDir))
    {
        return Err(Error::invalid(
            texture_file,
            "expected a relative content path",
        ));
    }
    Ok(root.join(relative))
}
