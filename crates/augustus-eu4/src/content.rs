// SPDX-License-Identifier: GPL-3.0-only

//! 集中处理游戏目录下的文件读取，为解析模块保留来源路径。

use std::path::{Component, Path, PathBuf};

use crate::Error;

pub(crate) struct ContentRoot {
    root: PathBuf,
}

pub(crate) struct ContentFile {
    // 来源路径与原始字节一起传递，解析失败时可以指出具体文件。
    pub path: PathBuf,
    pub bytes: Vec<u8>,
}

impl ContentRoot {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
        }
    }

    pub fn path(&self) -> &Path {
        &self.root
    }

    pub fn read(&self, relative: impl AsRef<Path>) -> Result<ContentFile, Error> {
        let relative = relative.as_ref();
        // 内容引用使用相对路径，禁止绝对路径和父目录跳转。
        if relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_) | Component::CurDir))
        {
            return Err(Error::invalid(relative, "expected a relative content path"));
        }
        let path = self.root.join(relative);
        let bytes = std::fs::read(&path).map_err(|source| Error::Read {
            path: path.clone(),
            source,
        })?;
        Ok(ContentFile { path, bytes })
    }

    /// EU4 的国家标签和省份历史按目录中的文件加载；排序使覆盖顺序固定。
    pub fn text_files(&self, relative: &str) -> Result<Vec<PathBuf>, Error> {
        self.files(relative, "txt")
    }

    pub fn files(&self, relative: &str, extension: &str) -> Result<Vec<PathBuf>, Error> {
        let path = self.root.join(relative);
        let entries = std::fs::read_dir(&path).map_err(|source| Error::Read {
            path: path.clone(),
            source,
        })?;
        let mut files = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|source| Error::Read {
                path: path.clone(),
                source,
            })?;
            if entry
                .path()
                .extension()
                .is_some_and(|candidate| candidate.eq_ignore_ascii_case(extension))
                && entry
                    .file_type()
                    .map_err(|source| Error::Read {
                        path: entry.path(),
                        source,
                    })?
                    .is_file()
            {
                files.push(PathBuf::from(relative).join(entry.file_name()));
            }
        }
        files.sort();
        Ok(files)
    }
}
