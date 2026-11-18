// SPDX-License-Identifier: GPL-3.0-only

//! 把 `.gui` 归一化成控件树。未知控件保留为节点，未知字段留在 properties。

use std::path::Path;

use jomini::TextTape;

use crate::Error;

use super::model::{
    ContentSource, GuiDocument, GuiNode, GuiNodeKind, UiField, UiOrientation, UiScalar, UiValue,
    UiVec2,
};
use super::value::{self, Context};

pub fn parse_gui_bytes(path: &Path, bytes: &[u8]) -> Result<GuiDocument, Error> {
    let tape =
        TextTape::from_slice(bytes).map_err(|error| Error::invalid(path, error.to_string()))?;
    let mut context = Context::new(path);
    let fields = value::read_root_fields(&tape, &mut context)?;
    let source = ContentSource::new(path);
    let mut document = GuiDocument {
        source: source.clone(),
        roots: Vec::new(),
        properties: Vec::new(),
    };
    for field in fields {
        absorb_root(field, &source, &mut document);
    }
    Ok(document)
}

fn absorb_root(field: UiField, source: &ContentSource, document: &mut GuiDocument) {
    let UiValue::Object(fields) = &field.value else {
        document.properties.push(field);
        return;
    };
    if is_widget_key(&field.key) {
        let UiValue::Object(fields) = field.value else {
            return;
        };
        document
            .roots
            .push(normalize_widget(&field.key, fields, source));
        return;
    }
    let (children, rest) = split_widgets(fields.clone(), source);
    // `guiTypes` 以及任何包着控件的顶层对象都展开，避免只认识一层包装名。
    if field.key == "guiTypes" || !children.is_empty() {
        document.roots.extend(children);
        document.properties.extend(rest);
        return;
    }
    document.properties.push(field);
}

fn split_widgets(fields: Vec<UiField>, source: &ContentSource) -> (Vec<GuiNode>, Vec<UiField>) {
    let mut children = Vec::new();
    let mut rest = Vec::new();
    for field in fields {
        if is_widget_object(&field) {
            let UiValue::Object(body) = field.value else {
                continue;
            };
            children.push(normalize_widget(&field.key, body, source));
        } else {
            rest.push(field);
        }
    }
    (children, rest)
}

fn normalize_widget(type_key: &str, fields: Vec<UiField>, source: &ContentSource) -> GuiNode {
    let mut node = GuiNode {
        kind: GuiNodeKind::from_type_key(type_key),
        type_key: type_key.to_owned(),
        name: None,
        position: None,
        show_position: None,
        hide_position: None,
        size: None,
        fullscreen: None,
        orientation: None,
        sprite: None,
        quad_texture_sprite: None,
        text: None,
        font: None,
        button_text: None,
        button_font: None,
        max_width: None,
        max_height: None,
        format: None,
        moveable: None,
        dont_render: None,
        always_transparent: None,
        tooltip: None,
        click_sound: None,
        properties: Vec::new(),
        children: Vec::new(),
        source: source.clone(),
    };
    for field in fields {
        if is_widget_object(&field) {
            let UiValue::Object(body) = field.value else {
                continue;
            };
            node.children
                .push(normalize_widget(&field.key, body, source));
            continue;
        }
        if !apply_known(&mut node, &field) {
            node.properties.push(field);
        }
    }
    node
}

/// 以 `Type` 结尾的对象是控件。`spriteType` 在 `.gui` 里是精灵名，不是子控件。
fn is_widget_key(key: &str) -> bool {
    key.ends_with("Type") && key != "spriteType" && key != "quadTextureSprite"
}

fn is_widget_object(field: &UiField) -> bool {
    is_widget_key(&field.key) && matches!(field.value, UiValue::Object(_))
}

fn apply_known(node: &mut GuiNode, field: &UiField) -> bool {
    // 关键字表按小写查找，`fullScreen` 和 `fullscreen` 是同一个 token。
    match field.key.to_ascii_lowercase().as_str() {
        "name" => take_text(&mut node.name, &field.value),
        "position" => {
            let stored = take_vec2(&mut node.position, &field.value);
            if stored && matches!(node.kind, GuiNodeKind::ContainerWindow) {
                if let Some(position) = node.position.clone() {
                    node.show_position = Some(position.clone());
                    node.hide_position = Some(position);
                }
            }
            stored
        }
        "show_position" => put_vec2(&mut node.show_position, &field.value),
        "hide_position" => put_vec2(&mut node.hide_position, &field.value),
        "size" => take_vec2(&mut node.size, &field.value),
        "fullscreen" => take_flag(&mut node.fullscreen, &field.value, false),
        "orientation" => take_orientation(&mut node.orientation, &field.value),
        "spritetype" => take_text(&mut node.sprite, &field.value),
        "quadtexturesprite" => take_text(&mut node.quad_texture_sprite, &field.value),
        "text" => take_text(&mut node.text, &field.value),
        "font" => take_text(&mut node.font, &field.value),
        "buttontext" => take_text(&mut node.button_text, &field.value),
        "buttonfont" => take_text(&mut node.button_font, &field.value),
        "maxwidth" => take_scalar(&mut node.max_width, &field.value),
        "maxheight" => take_scalar(&mut node.max_height, &field.value),
        "format" => take_text(&mut node.format, &field.value),
        "moveable" => take_flag(&mut node.moveable, &field.value, false),
        "dontrender" => take_flag(&mut node.dont_render, &field.value, true),
        "alwaystransparent" => take_flag(&mut node.always_transparent, &field.value, false),
        "tooltip" => take_text(&mut node.tooltip, &field.value),
        "clicksound" => take_text(&mut node.click_sound, &field.value),
        _ => false,
    }
}

fn take_text(slot: &mut Option<String>, value: &UiValue) -> bool {
    if slot.is_some() {
        return false;
    }
    match value::scalar_text(value) {
        Some(text) => {
            *slot = Some(text);
            true
        }
        None => false,
    }
}

fn take_scalar(slot: &mut Option<UiScalar>, value: &UiValue) -> bool {
    if slot.is_some() {
        return false;
    }
    match UiScalar::from_value(value) {
        Some(scalar) => {
            *slot = Some(scalar);
            true
        }
        None => false,
    }
}

fn take_flag(slot: &mut Option<bool>, value: &UiValue, empty_string_is_false: bool) -> bool {
    if slot.is_some() {
        return false;
    }
    match value::parse_flag(value, empty_string_is_false) {
        Some(flag) => {
            *slot = Some(flag);
            true
        }
        None => false,
    }
}

fn take_orientation(slot: &mut Option<UiOrientation>, value: &UiValue) -> bool {
    if slot.is_some() {
        return false;
    }
    let Some(text) = value::scalar_text(value) else {
        return false;
    };
    *slot = Some(UiOrientation::parse(&text));
    true
}

fn put_vec2(slot: &mut Option<UiVec2>, value: &UiValue) -> bool {
    match vec2_from(value) {
        Some(parsed) => {
            *slot = Some(parsed);
            true
        }
        None => false,
    }
}

fn take_vec2(slot: &mut Option<UiVec2>, value: &UiValue) -> bool {
    if slot.is_some() {
        return false;
    }
    put_vec2(slot, value)
}

fn vec2_from(value: &UiValue) -> Option<UiVec2> {
    parse_vec2(value)
}

fn parse_vec2(value: &UiValue) -> Option<UiVec2> {
    let UiValue::Object(fields) = value else {
        return None;
    };
    let mut x = None;
    let mut y = None;
    let mut properties = Vec::new();
    for field in fields {
        match field.key.as_str() {
            "x" if x.is_none() => x = UiScalar::from_value(&field.value),
            "y" if y.is_none() => y = UiScalar::from_value(&field.value),
            _ => properties.push(field.clone()),
        }
    }
    Some(UiVec2 {
        x: x?,
        y: y?,
        properties,
    })
}
