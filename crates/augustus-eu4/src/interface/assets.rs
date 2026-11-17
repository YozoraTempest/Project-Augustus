// SPDX-License-Identifier: GPL-3.0-only

//! 从单个游戏目录读取界面文件。纹理本身留在玩家目录，只返回安全的相对路径。

use std::path::{Component, Path, PathBuf};

use crate::{Error, content::ContentRoot};

use super::gfx::parse_gfx_bytes;
use super::gui::parse_gui_bytes;
use super::localisation::{self, LocalisationCatalog};
use super::model::{ContentStack, GfxCatalog, GuiDocument};

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
        let files = match self.content.files("interface", "gfx") {
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
