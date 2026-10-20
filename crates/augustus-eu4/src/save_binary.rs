// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::{borrow::Cow, fmt::Write as _, path::Path};

use jomini::{
    Encoding, Windows1252Encoding,
    binary::{BinaryFlavor, Token, TokenReader, TokenResolver},
    common::{Date, PdsDate},
};

use crate::{Error, TextDocument, TextEncoding};

#[derive(Debug, Default)]
struct Eu4BinaryFlavor(Windows1252Encoding);

impl Encoding for Eu4BinaryFlavor {
    fn decode<'a>(&self, data: &'a [u8]) -> Cow<'a, str> {
        self.0.decode(data)
    }
}

impl BinaryFlavor for Eu4BinaryFlavor {
    fn visit_f32(&self, data: [u8; 4]) -> f32 {
        i32::from_le_bytes(data) as f32 / 1000.0
    }

    fn visit_f64(&self, data: [u8; 8]) -> f64 {
        let value = i64::from_le_bytes(data) as f64 / 32768.0;
        (value * 100_000.0).round() / 100_000.0
    }
}

enum ScalarValue {
    Text(String),
    Plain(String),
}

impl ScalarValue {
    fn field_name(&self) -> &str {
        match self {
            Self::Text(value) | Self::Plain(value) => value,
        }
    }
}

pub(crate) fn decode_eu4_binary_save(
    path: &Path,
    bytes: &[u8],
    resolver: &impl TokenResolver,
) -> Result<TextDocument, Error> {
    if !bytes.starts_with(b"EU4bin") {
        return Err(Error::message(path, "expected an EU4bin save"));
    }

    let flavor = Eu4BinaryFlavor::default();
    let mut reader = TokenReader::from_slice(&bytes[6..]);
    let mut output = String::with_capacity(bytes.len().saturating_sub(6));
    let mut candidate = None;
    let mut pending_field: Option<String> = None;

    while let Some(token) = reader
        .next()
        .map_err(|error| Error::caused_by(path, error))?
    {
        match token {
            Token::Equal => {
                let Some(key) = candidate.take() else {
                    return Err(Error::message(
                        path,
                        "invalid EU4 binary save: '=' without a field name",
                    ));
                };
                write_scalar(&mut output, &key);
                output.push_str(" = ");
                pending_field = Some(key.field_name().to_owned());
            }
            Token::Open => {
                flush_candidate(&mut output, &mut candidate, &mut pending_field, path)?;
                pending_field = None;
                output.push_str("{ ");
            }
            Token::Close => {
                flush_candidate(&mut output, &mut candidate, &mut pending_field, path)?;
                pending_field = None;
                output.push_str(" } ");
            }
            Token::Rgb(rgb) => {
                flush_candidate(&mut output, &mut candidate, &mut pending_field, path)?;
                pending_field = None;
                let _ = write!(output, "rgb {{ {} {} {}", rgb.r, rgb.g, rgb.b);
                if let Some(alpha) = rgb.a {
                    let _ = write!(output, " {alpha}");
                }
                output.push_str(" } ");
            }
            Token::Id(id) => {
                flush_candidate(&mut output, &mut candidate, &mut pending_field, path)?;
                let value = resolver.resolve(id).ok_or_else(|| {
                    Error::message(
                        path,
                        format!("EU4 binary save uses unmapped token 0x{id:04x}"),
                    )
                })?;
                candidate = Some(ScalarValue::Text(value.to_owned()));
            }
            Token::Lookup(index) => {
                flush_candidate(&mut output, &mut candidate, &mut pending_field, path)?;
                let value = resolver.lookup(index).ok_or_else(|| {
                    Error::message(
                        path,
                        format!("EU4 binary save uses unmapped string lookup {index}"),
                    )
                })?;
                candidate = Some(ScalarValue::Text(value.to_owned()));
            }
            Token::Quoted(value) | Token::Unquoted(value) => {
                flush_candidate(&mut output, &mut candidate, &mut pending_field, path)?;
                candidate = Some(ScalarValue::Text(
                    flavor.decode(value.as_bytes()).into_owned(),
                ));
            }
            Token::U32(value) => set_plain_candidate(
                &mut output,
                &mut candidate,
                &mut pending_field,
                value.to_string(),
                path,
            )?,
            Token::U64(value) => set_plain_candidate(
                &mut output,
                &mut candidate,
                &mut pending_field,
                value.to_string(),
                path,
            )?,
            Token::I32(value) => set_plain_candidate(
                &mut output,
                &mut candidate,
                &mut pending_field,
                value.to_string(),
                path,
            )?,
            Token::I64(value) => set_plain_candidate(
                &mut output,
                &mut candidate,
                &mut pending_field,
                value.to_string(),
                path,
            )?,
            Token::Bool(value) => set_plain_candidate(
                &mut output,
                &mut candidate,
                &mut pending_field,
                if value { "yes" } else { "no" }.to_owned(),
                path,
            )?,
            Token::F32(value) => set_plain_candidate(
                &mut output,
                &mut candidate,
                &mut pending_field,
                flavor.visit_f32(value).to_string(),
                path,
            )?,
            Token::F64(value) => set_plain_candidate(
                &mut output,
                &mut candidate,
                &mut pending_field,
                flavor.visit_f64(value).to_string(),
                path,
            )?,
        }
    }

    flush_candidate(&mut output, &mut candidate, &mut pending_field, path)?;
    Ok(TextDocument::from_bytes(
        path,
        output.into_bytes(),
        TextEncoding::Utf8,
    ))
}

fn set_plain_candidate(
    output: &mut String,
    candidate: &mut Option<ScalarValue>,
    pending_field: &mut Option<String>,
    value: String,
    path: &Path,
) -> Result<(), Error> {
    flush_candidate(output, candidate, pending_field, path)?;
    *candidate = Some(ScalarValue::Plain(value));
    Ok(())
}

fn flush_candidate(
    output: &mut String,
    candidate: &mut Option<ScalarValue>,
    pending_field: &mut Option<String>,
    path: &Path,
) -> Result<(), Error> {
    let Some(value) = candidate.take() else {
        return Ok(());
    };
    let field = pending_field.take();
    if field.as_deref() == Some("date")
        && let ScalarValue::Plain(raw_date) = &value
    {
        let raw_date = raw_date
            .parse::<i32>()
            .map_err(|error| Error::caused_by(path, error))?;
        let date = Date::from_binary(raw_date)
            .ok_or_else(|| Error::message(path, format!("invalid EU4 binary date {raw_date}")))?;
        write_quoted(output, &date.game_fmt().to_string());
    } else {
        write_scalar(output, &value);
    }
    Ok(())
}

fn write_scalar(output: &mut String, value: &ScalarValue) {
    output.push(' ');
    match value {
        ScalarValue::Text(value) => write_quoted(output, value),
        ScalarValue::Plain(value) => output.push_str(value),
    }
    output.push(' ');
}

fn write_quoted(output: &mut String, value: &str) {
    output.push('"');
    for character in value.chars() {
        match character {
            '\\' => output.push_str("\\\\"),
            '"' => output.push_str("\\\""),
            other => output.push(other),
        }
    }
    output.push('"');
}
