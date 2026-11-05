// SPDX-License-Identifier: GPL-3.0-only

//! 读取信息面板需要的原版英文名称；EU4 的带版本号格式并非普通 YAML。

use std::collections::{BTreeMap, BTreeSet};

use jomini::text::{Token, TokenReader};

use crate::{Error, content::ContentRoot};

pub(super) fn load(
    content: &ContentRoot,
    wanted: &BTreeSet<String>,
) -> Result<BTreeMap<String, String>, Error> {
    let mut names = BTreeMap::new();
    // replace 层按游戏约定覆盖普通本地化，与文件名排序相互独立。
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
            let file = content.read(path)?;
            let bytes = file
                .bytes
                .strip_prefix(&[0xef, 0xbb, 0xbf])
                .unwrap_or(&file.bytes);
            let text = std::str::from_utf8(bytes)
                .map_err(|error| Error::invalid(&file.path, error.to_string()))?;
            let mut english = false;
            for (line_number, line) in text.lines().enumerate() {
                let line = line.trim_start();
                if line.starts_with("l_english:") {
                    english = true;
                    continue;
                }
                if !english || line.starts_with('#') {
                    continue;
                }
                let Some((key, rest)) = line.split_once(':') else {
                    continue;
                };
                let key = key.trim();
                if !wanted.contains(key) {
                    continue;
                }
                let fail = |message: String| {
                    Error::invalid(&file.path, format!("line {}: {message}", line_number + 1))
                };
                // 版本号由此格式定义，字符串、引号和行尾注释交给已有的 jomini 读取器。
                let value = rest
                    .trim_start()
                    .trim_start_matches(|c: char| c.is_ascii_digit())
                    .trim_start();
                let mut reader = TokenReader::from_slice(value.as_bytes());
                let decoded = match reader.next().map_err(|error| fail(error.to_string()))? {
                    Some(Token::Quoted(value)) => decode_escapes(
                        std::str::from_utf8(value.as_bytes())
                            .map_err(|error| fail(error.to_string()))?,
                    ),
                    _ => return Err(fail(format!("{key}: expected a quoted localisation value"))),
                };
                if reader
                    .next()
                    .map_err(|error| fail(error.to_string()))?
                    .is_some()
                {
                    return Err(fail(format!(
                        "{key}: unexpected trailing localisation data"
                    )));
                }
                names.insert(key.to_owned(), decoded);
            }
        }
    }
    Ok(names)
}

fn decode_escapes(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(character) = chars.next() {
        if character != '\\' {
            result.push(character);
            continue;
        }
        match chars.next() {
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
