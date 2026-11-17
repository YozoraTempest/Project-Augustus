// SPDX-License-Identifier: GPL-3.0-only

//! 只读检查输出。文本给命令行，JSON 给 Godot 预览。

use serde_json::{Map, Value, json};

use super::localisation::LocalisationCatalog;
use super::model::{GfxCatalog, GuiDocument, GuiNode, UiField, UiScalar, UiValue, UiVec2};

pub fn inspect_gui(
    document: &GuiDocument,
    sprites: Option<&GfxCatalog>,
    localisation: Option<&LocalisationCatalog>,
) -> String {
    let mut lines = Vec::new();
    lines.push(format!("file: {}", document.source.path.display()));
    lines.push("status: ok".to_owned());
    lines.push(format!("roots: {}", document.roots.len()));
    lines.push(format!("unknown_nodes: {}", document.unknown_node_count()));
    lines.push(format!(
        "unknown_fields: {}",
        document.unknown_field_count()
    ));
    for root in &document.roots {
        write_node(&mut lines, root, 0, localisation);
    }
    if let Some(sprites) = sprites {
        lines.push("sprites:".to_owned());
        let mut names = referenced_sprites(document);
        names.sort();
        names.dedup();
        for name in names {
            match sprites.resolve(&name) {
                Some(sprite) => lines.push(format!(
                    "- {name} texture {} frames {}",
                    sprite.texture_file.as_deref().unwrap_or("-"),
                    sprite
                        .frames
                        .map(|frames| frames.to_string())
                        .unwrap_or_else(|| "-".to_owned())
                )),
                None => lines.push(format!("- {name} unresolved")),
            }
        }
        for error in &sprites.errors {
            lines.push(format!("gfx_error: {error}"));
        }
    }
    lines.join("\n")
}

pub fn inspect_window(
    document: &GuiDocument,
    name: &str,
    localisation: Option<&LocalisationCatalog>,
) -> Result<String, String> {
    let Some(node) = document.find(name) else {
        return Err(format!(
            "{}: window `{name}` was not found",
            document.source.path.display()
        ));
    };
    let mut lines = Vec::new();
    lines.push(format!("file: {}", document.source.path.display()));
    lines.push("status: ok".to_owned());
    lines.push(format!("window: {name}"));
    write_node(&mut lines, node, 0, localisation);
    Ok(lines.join("\n"))
}

pub fn inspect_gfx(catalog: &GfxCatalog) -> String {
    let mut lines = vec![
        format!("sprites: {}", catalog.sprites.len()),
        format!("errors: {}", catalog.errors.len()),
    ];
    for sprite in &catalog.sprites {
        lines.push(format!(
            "- {} {} texture {} frames {}",
            sprite.kind,
            if sprite.name.is_empty() {
                "-"
            } else {
                sprite.name.as_str()
            },
            sprite.texture_file.as_deref().unwrap_or("-"),
            sprite
                .frames
                .map(|frames| frames.to_string())
                .unwrap_or_else(|| "-".to_owned())
        ));
    }
    for error in &catalog.errors {
        lines.push(format!("gfx_error: {error}"));
    }
    lines.join("\n")
}

pub fn document_json(document: &GuiDocument) -> String {
    render_json(&json!({
        "source": document.source.path.display().to_string(),
        "roots": document.roots.iter().map(node_json).collect::<Vec<_>>(),
        "unknown_nodes": document.unknown_node_count(),
        "unknown_fields": document.unknown_field_count(),
    }))
}

pub fn window_json(document: &GuiDocument, name: &str) -> Option<String> {
    document
        .find(name)
        .map(|node| render_json(&node_json(node)))
}

fn render_json(value: &Value) -> String {
    serde_json::to_string(value)
        .unwrap_or_else(|error| json!({"ok": false, "error": error.to_string()}).to_string())
}

fn write_node(
    lines: &mut Vec<String>,
    node: &GuiNode,
    depth: usize,
    localisation: Option<&LocalisationCatalog>,
) {
    let indent = "  ".repeat(depth);
    let name = node.name.as_deref().unwrap_or("-");
    lines.push(format!(
        "{indent}- {} {name} type {}",
        node.kind.id(),
        node.type_key
    ));
    if let Some(position) = &node.position {
        lines.push(format!(
            "{indent}  position: {}, {}",
            position.x.to_text(),
            position.y.to_text()
        ));
    }
    if let Some(size) = &node.size {
        lines.push(format!(
            "{indent}  size: {}, {}",
            size.x.to_text(),
            size.y.to_text()
        ));
    }
    if let Some(orientation) = &node.orientation {
        lines.push(format!("{indent}  orientation: {}", orientation.as_str()));
    }
    if let Some(sprite) = &node.sprite {
        lines.push(format!("{indent}  sprite: {sprite}"));
    }
    if let Some(sprite) = &node.quad_texture_sprite {
        lines.push(format!("{indent}  quad: {sprite}"));
    }
    for (label, value) in [("text", &node.text), ("button", &node.button_text)] {
        if let Some(value) = value {
            lines.push(format!("{indent}  {label}: {value}"));
            if let Some(resolved) = localisation.and_then(|catalog| catalog.format_key(value)) {
                lines.push(format!("{indent}  localisation: {resolved}"));
            }
        }
    }
    if node.is_unknown() {
        lines.push(format!("{indent}  unknown: {}", node.type_key));
    }
    for child in &node.children {
        write_node(lines, child, depth + 1, localisation);
    }
}

fn referenced_sprites(document: &GuiDocument) -> Vec<String> {
    let mut names = Vec::new();
    for root in &document.roots {
        collect_sprites(root, &mut names);
    }
    names
}

fn collect_sprites(node: &GuiNode, names: &mut Vec<String>) {
    if let Some(sprite) = &node.sprite {
        push_sprite(names, sprite);
    }
    if let Some(sprite) = &node.quad_texture_sprite {
        push_sprite(names, sprite);
    }
    for child in &node.children {
        collect_sprites(child, names);
    }
}

fn push_sprite(names: &mut Vec<String>, sprite: &str) {
    if !sprite.is_empty() {
        names.push(sprite.to_owned());
    }
}

fn node_json(node: &GuiNode) -> Value {
    json!({
        "kind": node.kind.id(),
        "type_key": node.type_key,
        "name": node.name,
        "position": node.position.as_ref().map(vec2_json),
        "size": node.size.as_ref().map(vec2_json),
        "orientation": node.orientation.as_ref().map(|value| value.as_str()),
        "sprite": node.sprite,
        "quad_texture_sprite": node.quad_texture_sprite,
        "text": node.text,
        "font": node.font,
        "button_text": node.button_text,
        "button_font": node.button_font,
        "max_width": node.max_width.as_ref().map(scalar_json),
        "max_height": node.max_height.as_ref().map(scalar_json),
        "format": node.format,
        "moveable": node.moveable,
        "dont_render": node.dont_render,
        "always_transparent": node.always_transparent,
        "tooltip": node.tooltip,
        "click_sound": node.click_sound,
        "properties": node.properties.iter().map(field_json).collect::<Vec<_>>(),
        "children": node.children.iter().map(node_json).collect::<Vec<_>>(),
    })
}

fn vec2_json(value: &UiVec2) -> Value {
    json!({
        "x": scalar_json(&value.x),
        "y": scalar_json(&value.y),
    })
}

fn scalar_json(value: &UiScalar) -> Value {
    match value {
        UiScalar::Integer(value) => Value::from(*value),
        UiScalar::Decimal(value) | UiScalar::Atom(value) | UiScalar::String(value) => {
            Value::String(value.clone())
        }
    }
}

fn field_json(field: &UiField) -> Value {
    let mut object = Map::new();
    object.insert("key".to_owned(), Value::String(field.key.clone()));
    object.insert(
        "operator".to_owned(),
        Value::String(field.operator.symbol().to_owned()),
    );
    object.insert("value".to_owned(), value_json(&field.value));
    Value::Object(object)
}

fn value_json(value: &UiValue) -> Value {
    match value {
        UiValue::Atom(value) => json!({"type": "atom", "value": value}),
        UiValue::String(value) => json!({"type": "string", "value": value}),
        UiValue::Bool(value) => json!({"type": "bool", "value": value}),
        UiValue::Integer(value) => json!({"type": "integer", "value": value}),
        UiValue::Decimal(value) => json!({"type": "decimal", "value": value}),
        UiValue::Object(fields) => json!({
            "type": "object",
            "value": fields.iter().map(field_json).collect::<Vec<_>>(),
        }),
        UiValue::Array(values) => json!({
            "type": "array",
            "value": values.iter().map(value_json).collect::<Vec<_>>(),
        }),
    }
}
