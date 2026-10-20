// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use jomini::{Encoding, TextTape, Utf8Encoding, Windows1252Encoding, text::ObjectReader};
use std::{
    borrow::Cow,
    fs,
    path::{Path, PathBuf},
};

use crate::{Error, SourceLocation};

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
    bytes: &'data [u8],
    line_starts: Vec<usize>,
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

    pub(crate) fn bytes(&self) -> &[u8] {
        &self.bytes
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
            bytes: &self.bytes,
            line_starts: std::iter::once(0)
                .chain(
                    self.bytes
                        .iter()
                        .enumerate()
                        .filter_map(|(offset, byte)| (*byte == b'\n').then_some(offset + 1)),
                )
                .collect(),
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

    /// Normalizes EU4 path separators before decoding scalar escape syntax.
    pub(crate) fn decode_path_scalar(&self, scalar: jomini::Scalar<'_>) -> String {
        let bytes = scalar
            .as_bytes()
            .iter()
            .map(|byte| if *byte == b'\\' { b'/' } else { *byte })
            .collect::<Vec<_>>();
        self.encoding.decode(&bytes).into_owned()
    }

    pub(crate) fn field_location(
        &self,
        key: &jomini::text::ScalarReader<'_, TextEncoding>,
    ) -> SourceLocation {
        // Parser scalars borrow the input bytes; decoded strings are not used for offsets.
        let offset = key.read_scalar().as_bytes().as_ptr() as usize - self.bytes.as_ptr() as usize;
        let offset = if matches!(key.token(), jomini::TextToken::Quoted(_)) {
            offset.saturating_sub(1)
        } else {
            offset
        };
        let line_index = self.line_starts.partition_point(|&start| start <= offset) - 1;
        SourceLocation {
            line: line_index + 1,
            column: offset - self.line_starts[line_index] + 1,
        }
    }
}
