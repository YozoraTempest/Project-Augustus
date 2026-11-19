// SPDX-License-Identifier: GPL-3.0-only

//! 按 key 查询英文界面文本。动态变量和复杂脚本化本地化留到后续。

use std::{collections::BTreeMap, path::Path};

use crate::{Error, content::ContentRoot};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalisationCatalog {
    pub language: String,
    pub values: BTreeMap<String, String>,
    pub errors: Vec<String>,
}

impl LocalisationCatalog {
    pub fn english() -> Self {
        Self {
            language: "l_english".to_owned(),
            values: BTreeMap::new(),
            errors: Vec::new(),
        }
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(String::as_str)
    }

    pub fn format_key(&self, key: &str) -> Option<String> {
        self.get(key).map(format_eu4_text)
    }

    pub fn format_key_with(&self, key: &str, colors: &super::colors::InterfaceColors) -> Option<String> {
        self.get(key)
            .map(|text| format_eu4_text_with(text, |code| colors.resolve(&code.to_string())))
    }
}

pub fn format_eu4_text(text: &str) -> String {
    format_eu4_text_with(text, |_| None)
}

pub fn format_eu4_text_with(
    text: &str,
    named: impl Fn(char) -> Option<super::colors::Rgb>,
) -> String {
    let mut formatted = String::with_capacity(text.len());
    let mut characters = text.chars().peekable();
    let mut open_colors = 0;
    while let Some(character) = characters.next() {
        if character != '§' {
            formatted.push(character);
            continue;
        }
        match characters.next() {
            Some('!') => {
                if open_colors > 0 {
                    formatted.push_str("[/color]");
                    open_colors -= 1;
                }
            }
            Some(code) => {
                let from_name = named(code).map(|color| color.hex());
                let color = from_name.or_else(|| color_code(code).map(str::to_owned));
                if let Some(color) = color {
                    formatted.push_str("[color=");
                    formatted.push_str(&color);
                    formatted.push(']');
                    open_colors += 1;
                }
            }
            None => formatted.push('§'),
        }
    }
    for _ in 0..open_colors {
        formatted.push_str("[/color]");
    }
    formatted
}

fn color_code(code: char) -> Option<&'static str> {
    match code {
        'Y' | 'y' => Some("#d4af37"),
        'R' => Some("#c44536"),
        'G' => Some("#6fbf73"),
        'B' => Some("#6aa8d6"),
        'W' => Some("#f4f1ea"),
        'g' => Some("#a0a7b0"),
        'b' => Some("#1c1a17"),
        _ => None,
    }
}

pub(super) fn load(content: &ContentRoot) -> Result<LocalisationCatalog, Error> {
    let mut catalog = LocalisationCatalog::english();
    for directory in ["localisation", "localisation/replace"] {
        let files = match content.files(directory, "yml") {
            Ok(files) => files,
            Err(Error::Read { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
                continue;
            }
            Err(error) => return Err(error),
        };
        for path in files
            .into_iter()
            .filter(|path| path.to_string_lossy().ends_with("_l_english.yml"))
        {
            let file = match content.read(&path) {
                Ok(file) => file,
                Err(error) => {
                    catalog.errors.push(error.to_string());
                    continue;
                }
            };
            if let Err(error) = parse_localisation_bytes(&file.path, &file.bytes, &mut catalog) {
                catalog.errors.push(error.to_string());
            }
        }
    }
    Ok(catalog)
}

pub fn parse_localisation_bytes(
    path: &Path,
    bytes: &[u8],
    catalog: &mut LocalisationCatalog,
) -> Result<(), Error> {
    let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes);
    let text =
        std::str::from_utf8(bytes).map_err(|error| Error::invalid(path, error.to_string()))?;
    let mut english = false;
    for (line_number, line) in text.lines().enumerate() {
        let line = line.trim_start();
        if line.starts_with("l_english:") {
            english = true;
            continue;
        }
        if !english || line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, rest)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim();
        if key.is_empty() {
            continue;
        }
        // EU4 把第一个引号到行尾注释前的最后一个引号当成本地化值，引号里还可以再写引号。
        match quoted_localisation_value(rest) {
            Some(value) => {
                catalog.values.insert(key.to_owned(), decode_escapes(value));
            }
            None => catalog.errors.push(
                Error::invalid(
                    path,
                    format!(
                        "line {}: {key}: expected a quoted localisation value",
                        line_number + 1
                    ),
                )
                .to_string(),
            ),
        }
    }
    Ok(())
}

fn quoted_localisation_value(text: &str) -> Option<&str> {
    let start = text.find('"')?;
    let body = &text[start + 1..];
    let mut characters = body.char_indices();
    while let Some((index, character)) = characters.next() {
        if character == '\\' {
            characters.next();
            continue;
        }
        if character != '"' {
            continue;
        }
        let after = body[index + 1..].trim_start();
        if after.is_empty() || after.starts_with('#') {
            return Some(&body[..index]);
        }
    }
    None
}

fn decode_escapes(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut characters = text.chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            result.push(character);
            continue;
        }
        match characters.next() {
            Some('"') => result.push('"'),
            Some('\\') => result.push('\\'),
            Some('n') => result.push('\n'),
            Some('r') => result.push('\r'),
            Some('t') => result.push('\t'),
            Some(other) => {
                result.push('\\');
                result.push(other);
            }
            None => result.push('\\'),
        }
    }
    result
}
