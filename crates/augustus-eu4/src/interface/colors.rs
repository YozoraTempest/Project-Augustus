// SPDX-License-Identifier: GPL-3.0-only

//! `interface/colors.txt` 是按顺序排列的颜色。界面里的颜色名也能查到这里，
//! 以及 `.gfx` 里 `textcolors` 写下的名字。

use std::collections::BTreeMap;
use std::path::Path;

use jomini::TextTape;

use crate::Error;

use super::model::{GfxCatalog, UiValue};
use super::value::{self, Context};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub fn hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterfaceColors {
    pub entries: Vec<Rgb>,
    named: BTreeMap<String, Rgb>,
}

impl InterfaceColors {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            named: BTreeMap::new(),
        }
    }

    pub fn push(&mut self, color: Rgb) {
        let index = self.entries.len();
        self.entries.push(color);
        self.named.insert(index.to_string(), color);
    }

    pub fn insert_name(&mut self, name: impl Into<String>, color: Rgb) {
        self.named.insert(name.into(), color);
    }

    pub fn get(&self, index: usize) -> Option<Rgb> {
        self.entries.get(index).copied()
    }

    /// 颜色名区分大小写。`colors.txt` 的条目用十进制下标，`textcolors` 用文件里的名字。
    pub fn resolve(&self, name: &str) -> Option<Rgb> {
        self.named.get(name).copied()
    }
}

impl Default for InterfaceColors {
    fn default() -> Self {
        Self::new()
    }
}

pub fn parse_colors_bytes(path: &Path, bytes: &[u8]) -> Result<InterfaceColors, Error> {
    let tape = TextTape::from_slice(bytes).map_err(|error| Error::invalid(path, error.to_string()))?;
    let mut context = Context::new(path);
    let fields = value::read_root_fields(&tape, &mut context)?;
    let mut colors = InterfaceColors::new();
    for field in fields {
        if !field.key.eq_ignore_ascii_case("color") {
            continue;
        }
        let Some(color) = rgb_from(&field.value) else {
            return Err(Error::invalid(path, "color entries need three channels"));
        };
        colors.push(color);
    }
    Ok(colors)
}

pub fn absorb_text_colors(catalog: &GfxCatalog, colors: &mut InterfaceColors) {
    for sprite in &catalog.sprites {
        if !sprite.kind.eq_ignore_ascii_case("textcolors") {
            continue;
        }
        for field in &sprite.properties {
            if field.key.is_empty() {
                continue;
            }
            if let Some(color) = rgb_from(&field.value) {
                colors.insert_name(field.key.clone(), color);
            }
        }
    }
}

pub(super) fn rgb_from(value: &UiValue) -> Option<Rgb> {
    match value {
        UiValue::Array(items) => channels(items),
        UiValue::Object(fields) => fields.iter().find_map(|field| {
            if field.key.is_empty() {
                if let UiValue::Array(items) = &field.value {
                    return channels(items);
                }
            }
            None
        }),
        _ => None,
    }
}

fn channels(items: &[UiValue]) -> Option<Rgb> {
    let numbers: Vec<u8> = items.iter().filter_map(channel).take(3).collect();
    if numbers.len() == 3 {
        Some(Rgb {
            r: numbers[0],
            g: numbers[1],
            b: numbers[2],
        })
    } else {
        None
    }
}

fn channel(value: &UiValue) -> Option<u8> {
    match value {
        UiValue::Integer(number) => u8::try_from(*number).ok(),
        UiValue::Decimal(text) | UiValue::Atom(text) | UiValue::String(text) => text.parse().ok(),
        _ => None,
    }
}
