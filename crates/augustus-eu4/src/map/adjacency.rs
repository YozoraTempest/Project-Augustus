// SPDX-License-Identifier: GPL-3.0-only

use std::collections::{BTreeSet, HashSet};

use crate::{Error, content::ContentFile};

pub(super) fn calculate(pixels: &[u32], width: u32) -> BTreeSet<(u32, u32)> {
    let width = width as usize;
    let mut pairs = HashSet::new();
    let mut connect = |a: u32, b: u32| {
        if a != 0 && b != 0 && a != b {
            pairs.insert((a.min(b), a.max(b)));
        }
    };
    for (y, row) in pixels.chunks_exact(width).enumerate() {
        for x in 0..width {
            // 只认共边，不把对角接触视作相邻；世界地图左右边缘相接。
            connect(row[x], row[(x + 1) % width]);
            if y > 0 {
                connect(row[x], pixels[(y - 1) * width + x]);
            }
        }
    }
    pairs.into_iter().collect()
}

pub(super) fn apply_impassable(
    pairs: &mut BTreeSet<(u32, u32)>,
    file: &ContentFile,
) -> Result<(), Error> {
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(b';')
        .flexible(true)
        .trim(csv::Trim::All)
        .from_reader(file.bytes.as_slice());
    for row in reader.byte_records() {
        let row = row.map_err(|error| Error::invalid(&file.path, error.to_string()))?;
        if row.get(2) != Some(b"impassable".as_slice()) {
            continue;
        }
        let id = |index| {
            row.get(index)
                .and_then(|text| std::str::from_utf8(text).ok())
                .and_then(|text| text.parse::<u32>().ok())
                .ok_or_else(|| Error::invalid(&file.path, "invalid impassable province ID"))
        };
        let (a, b) = (id(0)?, id(1)?);
        pairs.remove(&(a.min(b), a.max(b)));
    }
    Ok(())
}
