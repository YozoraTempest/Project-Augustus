// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use jomini::{Encoding, TextTape, Utf8Encoding, Windows1252Encoding, text::ObjectReader};
use std::{
    borrow::Cow,
    fs,
    path::{Path, PathBuf},
};

use crate::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextEncoding {
    Utf8,
    Windows1252,
}

impl Encoding for TextEncoding {
    fn decode<'a>(&self, data: &'a [u8]) -> Cow<'a, str> {
        match self {
            Self::Utf8 => Utf8Encoding::new().decode(data),
            Self::Windows1252 => Windows1252Encoding::new().decode(data),
        }
    }
}

/// Owns input bytes; it never writes or normalizes the source file.
#[derive(Debug)]
pub struct TextDocument {
    source: PathBuf,
    bytes: Vec<u8>,
    encoding: TextEncoding,
}

/// The parser's tape is private to the EU4 adapter.
#[derive(Debug)]
pub struct ParsedText<'data> {
    source: &'data Path,
    tape: TextTape<'data>,
    encoding: TextEncoding,
}

/// An inspection view, not a game-state or native-mod representation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldSummary {
    pub name: String,
    pub operator: String,
    /// None means the field contains a block rather than a scalar.
    pub scalar: Option<String>,
}

impl TextDocument {
    pub fn read(path: impl AsRef<Path>, encoding: TextEncoding) -> Result<Self, Error> {
        let path = path.as_ref();
        let bytes = fs::read(path).map_err(|cause| Error::caused_by(path, cause))?;
        Ok(Self::from_bytes(path, bytes, encoding))
    }

    pub fn from_bytes(source: impl Into<PathBuf>, bytes: Vec<u8>, encoding: TextEncoding) -> Self {
        Self {
            source: source.into(),
            bytes,
            encoding,
        }
    }

    pub fn source(&self) -> &Path {
        &self.source
    }

    pub fn byte_len(&self) -> usize {
        self.bytes.len()
    }

    pub fn parse(&self) -> Result<ParsedText<'_>, Error> {
        if self.encoding == TextEncoding::Utf8 {
            std::str::from_utf8(&self.bytes).map_err(|cause| {
                let offset = cause.valid_up_to();
                Error::caused_by(&self.source, cause).at_offset(&self.bytes, offset)
            })?;
        }
        let tape = TextTape::from_slice(&self.bytes).map_err(|cause| {
            let offset = cause.offset().or_else(|| {
                matches!(cause.kind(), jomini::ErrorKind::Eof).then_some(self.bytes.len())
            });
            let error = Error::caused_by(&self.source, cause);
            match offset {
                Some(offset) => error.at_offset(&self.bytes, offset),
                None => error,
            }
        })?;
        Ok(ParsedText {
            source: &self.source,
            tape,
            encoding: self.encoding,
        })
    }
}

impl ParsedText<'_> {
    pub fn source(&self) -> &Path {
        self.source
    }

    pub fn token_count(&self) -> usize {
        self.tape.tokens().len()
    }

    /// Preserves root field order and repeated names.
    pub fn root_fields(&self) -> Vec<FieldSummary> {
        self.reader()
            .fields()
            .map(|(key, operator, value)| FieldSummary {
                name: key.read_string(),
                operator: operator
                    .unwrap_or(jomini::text::Operator::Equal)
                    .to_string(),
                scalar: value.read_string().ok(),
            })
            .collect()
    }

    pub(crate) fn reader(&self) -> ObjectReader<'_, '_, TextEncoding> {
        ObjectReader::new(&self.tape, self.encoding)
    }
}
