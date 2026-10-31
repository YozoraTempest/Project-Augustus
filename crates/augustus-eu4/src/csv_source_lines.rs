// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

/// Physical source lines for the CSV reader's record boundary positions.
pub(crate) struct CsvSourceLines<'a> {
    bytes: &'a [u8],
    starts: Vec<usize>,
}

impl<'a> CsvSourceLines<'a> {
    pub(crate) fn new(bytes: &'a [u8]) -> Self {
        let mut starts = vec![0];
        let mut offset = 0;
        while offset < bytes.len() {
            match bytes[offset] {
                b'\r' => {
                    offset += 1;
                    if bytes.get(offset) == Some(&b'\n') {
                        offset += 1;
                    }
                    starts.push(offset);
                }
                b'\n' => {
                    offset += 1;
                    starts.push(offset);
                }
                _ => offset += 1,
            }
        }
        Self { bytes, starts }
    }

    pub(crate) fn line(&self, position: &csv::Position) -> usize {
        let mut offset = position.byte().min(self.bytes.len() as u64) as usize;
        // The reader can stop after CR, leaving LF or empty lines before the
        // next record. Locate its first byte without changing field contents.
        while self
            .bytes
            .get(offset)
            .is_some_and(|byte| matches!(*byte, b'\r' | b'\n'))
        {
            offset += 1;
        }
        self.starts.partition_point(|&start| start <= offset)
    }
}
