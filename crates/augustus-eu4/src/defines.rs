// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use jomini::Encoding;

use crate::setup::micros;
use crate::{ContentRoot, Error, FileEntry, LoadedSource, SourceLocation, TextEncoding};

/// Selected base-file literals only; additional assignments need Lua evaluation.
pub(super) fn literal(
    root: &ContentRoot,
    files: &[FileEntry],
    section: &str,
    key: &str,
) -> Result<(i64, LoadedSource), Error> {
    optional_literal(root, files, section, key)?.ok_or_else(|| {
        Error::message(
            root.path().join("common/defines.lua"),
            format!("missing literal define {key}"),
        )
    })
}

pub(super) fn optional_literal(
    root: &ContentRoot,
    files: &[FileEntry],
    section: &str,
    key: &str,
) -> Result<Option<(i64, LoadedSource)>, Error> {
    let encoding = TextEncoding::Windows1252;
    let doc = root.read_text("common/defines.lua", encoding)?;
    let text = encoding.decode(doc.bytes());
    let header = format!("{section} = {{");
    let mut inside = false;
    let mut sections = 0;
    let mut amount = None;
    for (index, line) in text.lines().enumerate() {
        let clean = line.split_once("--").map_or(line, |(code, _)| code).trim();
        if clean == header {
            inside = true;
            sections += 1;
        } else if inside && clean == "}," {
            inside = false;
        }
        if !clean.contains(key) {
            continue;
        }
        let at = SourceLocation {
            line: index + 1,
            column: line.find(key).unwrap_or(0) + 1,
        };
        let fail = |m: &str| Error::message(doc.source(), format!("{key}: {m}")).with_location(at);
        if !inside || sections != 1 || amount.is_some() {
            return Err(fail(
                "define must be a unique literal in the selected section",
            ));
        }
        let (name, value) = clean
            .split_once('=')
            .ok_or_else(|| fail("expected literal define"))?;
        if name.trim() != key {
            return Err(fail("unsupported define assignment"));
        }
        let value = value
            .trim()
            .strip_suffix(',')
            .unwrap_or(value.trim())
            .trim();
        let value = micros(value)
            .filter(|v| *v >= 0)
            .ok_or_else(|| fail("expected nonnegative literal define"))?;
        amount = Some((
            value,
            LoadedSource {
                path: doc.source().into(),
                line: at.line,
                column: at.column,
            },
        ));
    }
    for file in files.iter().filter(|f| {
        f.relative_path
            .parent()
            .is_some_and(|p| p == std::path::Path::new("common/defines"))
            && f.relative_path.extension().is_some_and(|e| e == "lua")
    }) {
        let doc = root.read_text(&file.relative_path, encoding)?;
        let text = encoding.decode(doc.bytes());
        for (index, line) in text.lines().enumerate() {
            let code = line.split_once("--").map_or(line, |(code, _)| code);
            if code.contains(key) {
                return Err(Error::message(
                    doc.source(),
                    format!("{key}: define override requires unsupported Lua evaluation"),
                )
                .with_location(SourceLocation {
                    line: index + 1,
                    column: code.find(key).unwrap_or(0) + 1,
                }));
            }
        }
    }
    Ok(amount)
}
