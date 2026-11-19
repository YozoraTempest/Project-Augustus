// SPDX-License-Identifier: GPL-3.0-only

//! `interface/messagetypes.txt`。只解析类型，不生成消息日志。

use std::path::Path;

use jomini::TextTape;

use crate::Error;

use super::model::{UiField, UiValue};
use super::value::{self, Context};

#[derive(Debug, Clone, PartialEq)]
pub struct MessageChannel {
    pub audience: String,
    pub log: Option<bool>,
    pub on_map: Option<bool>,
    pub popup: Option<bool>,
    pub pause_popup: Option<bool>,
    pub icon: Option<bool>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MessageType {
    pub name: String,
    pub icon_type: Option<String>,
    pub category: Option<String>,
    pub option: Option<bool>,
    pub channels: Vec<MessageChannel>,
    pub properties: Vec<UiField>,
}

pub fn parse_message_types_bytes(path: &Path, bytes: &[u8]) -> Result<Vec<MessageType>, Error> {
    let tape = TextTape::from_slice(bytes).map_err(|error| Error::invalid(path, error.to_string()))?;
    let mut context = Context::new(path);
    let fields = value::read_root_fields(&tape, &mut context)?;
    let mut types = Vec::new();
    for field in fields {
        let UiValue::Object(body) = field.value else {
            continue;
        };
        if field.key.is_empty() {
            continue;
        }
        types.push(message_type(field.key, body));
    }
    Ok(types)
}

fn message_type(name: String, fields: Vec<UiField>) -> MessageType {
    let mut icon_type = None;
    let mut category = None;
    let mut option = None;
    let mut channels = Vec::new();
    let mut properties = Vec::new();
    for field in fields {
        let key = field.key.to_ascii_lowercase();
        match key.as_str() {
            "msg_icontype" if icon_type.is_none() => match value::scalar_text(&field.value) {
                Some(text) => icon_type = Some(text),
                None => properties.push(field),
            },
            "category" if category.is_none() => match value::scalar_text(&field.value) {
                Some(text) => category = Some(text),
                None => properties.push(field),
            },
            "option" if option.is_none() => match flag(&field.value) {
                Some(value) => option = Some(value),
                None => properties.push(field),
            },
            _ => match channel(&field) {
                Some(parsed) => channels.push(parsed),
                None => properties.push(field),
            },
        }
    }
    MessageType {
        name,
        icon_type,
        category,
        option,
        channels,
        properties,
    }
}

fn flag(value: &UiValue) -> Option<bool> {
    if let Some(parsed) = value::parse_flag(value, false) {
        return Some(parsed);
    }
    match value::scalar_text(value)?.as_str() {
        "yes" => Some(true),
        "no" => Some(false),
        _ => None,
    }
}
fn channel(field: &UiField) -> Option<MessageChannel> {
    let UiValue::Object(fields) = &field.value else {
        return None;
    };
    if field.key.is_empty() {
        return None;
    }
    let mut log = None;
    let mut on_map = None;
    let mut popup = None;
    let mut pause_popup = None;
    let mut icon = None;
    let mut recognized = false;
    for child in fields {
        let key = child.key.to_ascii_lowercase();
        let flag = flag(&child.value);
        match key.as_str() {
            "log" => {
                log = flag;
                recognized = true;
            }
            "onmap" => {
                on_map = flag;
                recognized = true;
            }
            "popup" => {
                popup = flag;
                recognized = true;
            }
            "pausepopup" => {
                pause_popup = flag;
                recognized = true;
            }
            "icon" => {
                icon = flag;
                recognized = true;
            }
            _ => {}
        }
    }
    if !recognized {
        return None;
    }
    Some(MessageChannel {
        audience: field.key.clone(),
        log,
        on_map,
        popup,
        pause_popup,
        icon,
    })
}
