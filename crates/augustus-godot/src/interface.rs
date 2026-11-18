// SPDX-License-Identifier: GPL-3.0-only

//! 把已解析的 EU4 界面定义交给 Godot。这里不创建控件。

use std::path::PathBuf;

use augustus_eu4::interface::{
    GfxCatalog, GuiDocument, InterfaceSource, LocalisationCatalog, document_json,
    resolve_texture_path, window_json,
};
use godot::prelude::*;

pub(crate) struct UiSession {
    root: PathBuf,
    documents: Vec<GuiDocument>,
    sprites: GfxCatalog,
    localisation: LocalisationCatalog,
}

pub(crate) fn load_document(
    sessions: &mut Vec<UiSession>,
    root: &str,
    relative: &str,
) -> VarDictionary {
    let source = InterfaceSource::open(root);
    let document = match source.load_gui(relative) {
        Ok(document) => document,
        Err(error) => return failure(error.to_string()),
    };
    let json = match session_mut(sessions, &source) {
        Ok(session) => {
            session.documents.push(document);
            let Some(stored) = session.documents.last() else {
                return failure("界面文档没有写入会话".to_owned());
            };
            document_json(stored)
        }
        Err(error) => return failure(error),
    };
    let mut result = VarDictionary::new();
    result.set("ok", true);
    result.set("error", "");
    result.set("index", document_count(sessions) - 1);
    result.set("json", json.as_str());
    result.set("gfx_errors", session_error_count(sessions, root) as i64);
    result
}

pub(crate) fn window_document(sessions: &[UiSession], index: i64, name: &str) -> GString {
    let json = document_at(sessions, index)
        .and_then(|document| window_json(document, name))
        .unwrap_or_default();
    json.as_str().into()
}

pub(crate) fn resolve_sprite(sessions: &[UiSession], name: &str) -> VarDictionary {
    let mut info = VarDictionary::new();
    if name.is_empty() {
        return info;
    }
    for session in sessions.iter().rev() {
        let Some(sprite) = session.sprites.resolve(name) else {
            continue;
        };
        info.set("name", sprite.name.as_str());
        match sprite.texture_file.as_deref() {
            Some(texture) => {
                info.set("texture_file", texture);
                match resolve_texture_path(&session.root, texture) {
                    Ok(path) => info.set(
                        "absolute_path",
                        existing_texture(path).display().to_string().as_str(),
                    ),
                    Err(error) => info.set("error", error.to_string().as_str()),
                }
            }
            None => info.set("texture_file", &Variant::nil()),
        }
        match sprite.frames {
            Some(frames) => info.set("frames", i64::from(frames)),
            None => info.set("frames", &Variant::nil()),
        }
        set_texture_slot(&mut info, &session.root, "1", sprite.texture_file_1.as_deref());
        set_texture_slot(&mut info, &session.root, "2", sprite.texture_file_2.as_deref());
        set_texture_slot(&mut info, &session.root, "3", sprite.texture_file_3.as_deref());
        return info;
    }
    info
}

pub(crate) fn localise(sessions: &[UiSession], key: &str) -> String {
    if key.is_empty() {
        return String::new();
    }
    for session in sessions.iter().rev() {
        if let Some(text) = session.localisation.format_key(key) {
            return text;
        }
    }
    String::new()
}

fn session_mut<'a>(
    sessions: &'a mut Vec<UiSession>,
    source: &InterfaceSource,
) -> Result<&'a mut UiSession, String> {
    let root = source.root().to_path_buf();
    if let Some(index) = sessions.iter().position(|session| session.root == root) {
        return Ok(&mut sessions[index]);
    }
    let sprites = source
        .load_gfx_catalog()
        .map_err(|error| error.to_string())?;
    let localisation = source
        .load_localisation()
        .map_err(|error| error.to_string())?;
    sessions.push(UiSession {
        root,
        documents: Vec::new(),
        sprites,
        localisation,
    });
    Ok(sessions.last_mut().expect("会话刚插入"))
}

/// 声明的扩展名经常和磁盘上的文件不一致，例如 `.tga` 实际是 `.dds`。
fn set_texture_slot(info: &mut VarDictionary, root: &std::path::Path, slot: &str, texture: Option<&str>) {
    let Some(texture) = texture.filter(|text| !text.is_empty()) else {
        return;
    };
    info.set(format!("texture_file_{slot}").as_str(), texture);
    match resolve_texture_path(root, texture) {
        Ok(path) => info.set(
            format!("absolute_path_{slot}").as_str(),
            existing_texture(path).display().to_string().as_str(),
        ),
        Err(error) => info.set("error", error.to_string().as_str()),
    }
}

fn existing_texture(path: PathBuf) -> PathBuf {
    if path.is_file() {
        return path;
    }
    let Some(parent) = path.parent() else {
        return path;
    };
    let Some(stem) = path.file_stem() else {
        return path;
    };
    for extension in ["dds", "tga", "png", "bmp"] {
        let candidate = parent.join(stem).with_extension(extension);
        if candidate.is_file() {
            return candidate;
        }
    }
    path
}

fn failure(message: String) -> VarDictionary {
    let mut result = VarDictionary::new();
    result.set("ok", false);
    result.set("error", message.as_str());
    result.set("index", -1i64);
    result.set("json", "");
    result.set("gfx_errors", 0i64);
    result
}

fn document_count(sessions: &[UiSession]) -> i64 {
    sessions
        .iter()
        .map(|session| session.documents.len() as i64)
        .sum()
}

fn document_at(sessions: &[UiSession], index: i64) -> Option<&GuiDocument> {
    if index < 0 {
        return None;
    }
    let mut cursor = 0i64;
    for session in sessions {
        let length = session.documents.len() as i64;
        if index < cursor + length {
            return session.documents.get((index - cursor) as usize);
        }
        cursor += length;
    }
    None
}

fn session_error_count(sessions: &[UiSession], root: &str) -> usize {
    let root = PathBuf::from(root);
    sessions
        .iter()
        .find(|session| session.root == root)
        .map_or(0, |session| session.sprites.errors.len())
}
