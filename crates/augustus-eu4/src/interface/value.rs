// SPDX-License-Identifier: GPL-3.0-only

//! 把 jomini 的借用读取器转换成拥有数据的值。注释不进入 IR。

use std::path::Path;

use jomini::{
    TextToken, Windows1252Encoding,
    text::{Operator, ValueReader},
};

use crate::Error;

use super::model::{UiField, UiOperator, UiValue};

pub(super) struct Context<'a> {
    pub path: &'a Path,
    stack: Vec<String>,
}

impl<'a> Context<'a> {
    pub(super) fn new(path: &'a Path) -> Self {
        Self {
            path,
            stack: Vec::new(),
        }
    }

    pub(super) fn error(&self, message: impl std::fmt::Display) -> Error {
        let message = if self.stack.is_empty() {
            message.to_string()
        } else {
            format!("{}: {message}", self.stack.join("."))
        };
        Error::invalid(self.path, message)
    }

    fn push(&mut self, key: &str) {
        self.stack.push(key.to_owned());
    }

    fn pop(&mut self) {
        self.stack.pop();
    }
}

pub(super) fn map_operator(operator: Option<Operator>) -> UiOperator {
    match operator {
        None | Some(Operator::Equal) => UiOperator::Equal,
        Some(Operator::LessThan) => UiOperator::LessThan,
        Some(Operator::GreaterThan) => UiOperator::GreaterThan,
        Some(operator) => UiOperator::Unknown(operator.symbol().to_owned()),
    }
}

pub(super) fn read_root_fields(
    tape: &jomini::TextTape<'_>,
    context: &mut Context<'_>,
) -> Result<Vec<UiField>, Error> {
    let root = tape.windows1252_reader();
    let mut fields = Vec::new();
    let mut iter = root.fields();
    while let Some((key, operator, child)) = iter.next() {
        let key = key.read_string();
        context.push(&key);
        let child_value = read_keyed_value(&key, &child, context)?;
        context.pop();
        fields.push(UiField {
            key,
            operator: map_operator(operator),
            value: child_value,
        });
    }
    let remainder = iter.remainder();
    if !remainder.is_empty() {
        let mut items = Vec::new();
        for child in remainder.values() {
            items.push(read_value_inner(&child, context, false)?);
        }
        fields.push(UiField {
            key: String::new(),
            operator: UiOperator::Equal,
            value: UiValue::Array(items),
        });
    }
    Ok(fields)
}

fn read_keyed_value(
    key: &str,
    value: &ValueReader<'_, '_, Windows1252Encoding>,
    context: &mut Context<'_>,
) -> Result<UiValue, Error> {
    // jomini 的 Windows-1252 解码会删掉反斜杠，路径要在解码前改成 `/`。
    if key == "texturefile" {
        if let Ok(scalar) = value.read_scalar() {
            let bytes: Vec<u8> = scalar
                .as_bytes()
                .iter()
                .map(|byte| if *byte == b'\\' { b'/' } else { *byte })
                .collect();
            let text = Windows1252Encoding::decode(&bytes).into_owned();
            return Ok(match value.token() {
                TextToken::Quoted(_) => UiValue::String(text),
                _ => classify_unquoted(&text),
            });
        }
    }
    read_value_inner(value, context, false)
}

fn read_value_inner(
    value: &ValueReader<'_, '_, jomini::Windows1252Encoding>,
    context: &mut Context<'_>,
    header_is_scalar: bool,
) -> Result<UiValue, Error> {
    match value.token() {
        TextToken::Quoted(_) => Ok(UiValue::String(read_text(value, context)?)),
        TextToken::Unquoted(_) => Ok(classify_unquoted(&read_text(value, context)?)),
        TextToken::Parameter(_) | TextToken::UndefinedParameter(_) => {
            Ok(UiValue::Atom(read_text(value, context)?))
        }
        TextToken::Header(_) if header_is_scalar => Ok(UiValue::Atom(read_text(value, context)?)),
        TextToken::Header(_) => {
            // `rgb { 1 2 3 }` 在磁带里是头标记加后续容器，整体仍是一个值。
            let array = value
                .read_array()
                .map_err(|error| context.error(error.to_string()))?;
            let mut items = Vec::new();
            for child in array.values() {
                items.push(read_value_inner(&child, context, true)?);
            }
            Ok(UiValue::Array(items))
        }
        TextToken::Object { .. } => Ok(UiValue::Object(read_fields(value, context)?)),
        TextToken::Array { .. } => {
            let array = value
                .read_array()
                .map_err(|error| context.error(error.to_string()))?;
            let mut items = Vec::new();
            for child in array.values() {
                items.push(read_value_inner(&child, context, false)?);
            }
            Ok(UiValue::Array(items))
        }
        TextToken::Operator(operator) => Ok(UiValue::Atom(operator.symbol().to_owned())),
        TextToken::End(_) | TextToken::MixedContainer => Ok(UiValue::Atom(String::new())),
    }
}

fn read_text(
    value: &ValueReader<'_, '_, jomini::Windows1252Encoding>,
    context: &mut Context<'_>,
) -> Result<String, Error> {
    value
        .read_string()
        .map_err(|error| context.error(error.to_string()))
}

fn read_fields(
    value: &ValueReader<'_, '_, jomini::Windows1252Encoding>,
    context: &mut Context<'_>,
) -> Result<Vec<UiField>, Error> {
    let object = value
        .read_object()
        .map_err(|error| context.error(error.to_string()))?;
    let mut fields = Vec::new();
    let mut iter = object.fields();
    while let Some((key, operator, child)) = iter.next() {
        let key = key.read_string();
        context.push(&key);
        let child_value = read_keyed_value(&key, &child, context)?;
        context.pop();
        fields.push(UiField {
            key,
            operator: map_operator(operator),
            value: child_value,
        });
    }
    let remainder = iter.remainder();
    if !remainder.is_empty() {
        let mut items = Vec::new();
        for child in remainder.values() {
            items.push(read_value_inner(&child, context, false)?);
        }
        fields.push(UiField {
            key: String::new(),
            operator: UiOperator::Equal,
            value: UiValue::Array(items),
        });
    }
    Ok(fields)
}

/// `yes`/`no` 才是布尔值。整数、小数和普通 atom 分开保存。
pub(super) fn classify_unquoted(text: &str) -> UiValue {
    match text {
        "yes" => return UiValue::Bool(true),
        "no" => return UiValue::Bool(false),
        _ => {}
    }
    if is_integer(text) {
        if let Ok(value) = text.parse::<i64>() {
            return UiValue::Integer(value);
        }
    }
    if is_decimal(text) {
        return UiValue::Decimal(text.to_owned());
    }
    UiValue::Atom(text.to_owned())
}

fn is_integer(text: &str) -> bool {
    let digits = text.strip_prefix('-').unwrap_or(text);
    !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
}

fn is_decimal(text: &str) -> bool {
    let body = text.strip_suffix(['f', 'F']).unwrap_or(text);
    let body = body.strip_prefix(['-', '+']).unwrap_or(body);
    let Some((whole, fraction)) = body.split_once('.') else {
        return false;
    };
    !fraction.is_empty()
        && !fraction.contains('.')
        && whole.bytes().all(|byte| byte.is_ascii_digit())
        && fraction.bytes().all(|byte| byte.is_ascii_digit())
        && (!whole.is_empty() || !fraction.is_empty())
}

pub(super) fn scalar_text(value: &UiValue) -> Option<String> {
    match value {
        UiValue::String(text) | UiValue::Atom(text) | UiValue::Decimal(text) => Some(text.clone()),
        UiValue::Integer(value) => Some(value.to_string()),
        UiValue::Bool(true) => Some("yes".to_owned()),
        UiValue::Bool(false) => Some("no".to_owned()),
        _ => None,
    }
}

/// EU4 界面里 `0`/`1` 和 `yes`/`no` 都会出现。空字符串只用于 `dontRender = ""` 这种占位。
pub(super) fn parse_flag(value: &UiValue, empty_string_is_false: bool) -> Option<bool> {
    match value {
        UiValue::Bool(value) => Some(*value),
        UiValue::Integer(0) => Some(false),
        UiValue::Integer(1) => Some(true),
        UiValue::String(text) if empty_string_is_false && text.is_empty() => Some(false),
        UiValue::Object(fields) if fields.is_empty() => Some(true),
        _ => None,
    }
}
