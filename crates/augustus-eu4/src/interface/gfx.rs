// SPDX-License-Identifier: GPL-3.0-only

//! `.gfx` 只绑定精灵名和纹理路径，不解码图片。

use std::path::Path;

use jomini::TextTape;

use crate::Error;

use super::model::{ContentSource, GfxCatalog, SpriteDefinition, UiField, UiValue};
use super::value::{self, Context};

pub fn parse_gfx_bytes(path: &Path, bytes: &[u8]) -> Result<GfxCatalog, Error> {
    let tape =
        TextTape::from_slice(bytes).map_err(|error| Error::invalid(path, error.to_string()))?;
    let mut context = Context::new(path);
    let fields = value::read_root_fields(&tape, &mut context)?;
    let mut catalog = GfxCatalog::new();
    let source = ContentSource::new(path);
    for field in fields {
        collect_sprites(field, &source, &mut catalog);
    }
    Ok(catalog)
}

fn collect_sprites(field: UiField, source: &ContentSource, catalog: &mut GfxCatalog) {
    let UiValue::Object(fields) = field.value else {
        return;
    };
    if field.key == "spriteTypes" {
        for child in fields {
            if let UiValue::Object(body) = child.value {
                catalog
                    .sprites
                    .push(normalize_sprite(&child.key, body, source));
            }
        }
        return;
    }
    if field.key.ends_with("Type") || field.key.ends_with("type") {
        catalog
            .sprites
            .push(normalize_sprite(&field.key, fields, source));
    }
}

fn normalize_sprite(kind: &str, fields: Vec<UiField>, source: &ContentSource) -> SpriteDefinition {
    let mut name = None;
    let mut texture_file = None;
    let mut frames = None;
    let mut frame_duration = None;
    let mut always_transparent = None;
    let mut properties = Vec::new();
    for field in fields {
        match field.key.as_str() {
            "name" if name.is_none() => match value::scalar_text(&field.value) {
                Some(text) => name = Some(text),
                None => properties.push(field),
            },
            "texturefile" if texture_file.is_none() => match value::scalar_text(&field.value) {
                Some(text) => texture_file = Some(normalize_slashes(&text)),
                None => properties.push(field),
            },
            "noOfFrames" if frames.is_none() => match &field.value {
                UiValue::Integer(value) => match u32::try_from(*value) {
                    Ok(value) => frames = Some(value),
                    Err(_) => properties.push(field),
                },
                _ => properties.push(field),
            },
            "alwaystransparent" if always_transparent.is_none() => {
                match value::parse_flag(&field.value, false) {
                    Some(flag) => always_transparent = Some(flag),
                    None => properties.push(field),
                }
            }
            "animationtime" | "frameDuration" if frame_duration.is_none() => {
                match decimal_text(&field.value) {
                    Some(text) => frame_duration = Some(text),
                    None => properties.push(field),
                }
            }
            "animation" => {
                if frame_duration.is_none() {
                    frame_duration = animation_time(&field.value);
                }
                properties.push(field);
            }
            _ => properties.push(field),
        }
    }
    SpriteDefinition {
        kind: kind.to_owned(),
        name: name.unwrap_or_default(),
        texture_file,
        frames,
        frame_duration,
        always_transparent,
        properties,
        source: source.clone(),
    }
}

fn animation_time(value: &UiValue) -> Option<String> {
    let UiValue::Object(fields) = value else {
        return None;
    };
    fields.iter().find_map(|field| match field.key.as_str() {
        "animationtime" | "frameDuration" => decimal_text(&field.value),
        _ => None,
    })
}

fn decimal_text(value: &UiValue) -> Option<String> {
    match value {
        UiValue::Decimal(text) => Some(text.clone()),
        UiValue::Integer(value) => Some(value.to_string()),
        UiValue::String(text) | UiValue::Atom(text) => Some(text.clone()),
        _ => None,
    }
}

pub(super) fn normalize_slashes(path: &str) -> String {
    path.replace('\\', "/")
}
