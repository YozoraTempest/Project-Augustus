// SPDX-License-Identifier: GPL-3.0-only

//! 读取 8 位索引和 24 位 BMP。地形图要保留调色板序号，不能先转成 RGB。

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IndexedBitmap {
    pub width: u32,
    pub height: u32,
    /// 从上到下、从左到右的调色板序号。
    pub indices: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RgbBitmap {
    pub width: u32,
    pub height: u32,
    /// 从上到下、从左到右的 RGB。
    pub pixels: Vec<[u8; 3]>,
}

pub(crate) fn read_indexed(bytes: &[u8]) -> Result<IndexedBitmap, String> {
    let image = decode(bytes)?;
    if image.bits != 8 {
        return Err("terrain bitmap must be 8-bit indexed".to_owned());
    }
    Ok(IndexedBitmap {
        width: image.width,
        height: image.height,
        indices: image.indices,
    })
}

pub(crate) fn read_rgb(bytes: &[u8]) -> Result<RgbBitmap, String> {
    let image = decode(bytes)?;
    let pixels = if image.bits == 24 {
        image
            .indices
            .chunks_exact(3)
            .map(|pixel| [pixel[2], pixel[1], pixel[0]])
            .collect()
    } else {
        image
            .indices
            .iter()
            .map(|index| {
                let entry = image
                    .palette
                    .get(usize::from(*index))
                    .ok_or_else(|| format!("palette index {index} is outside the BMP palette"))?;
                Ok(*entry)
            })
            .collect::<Result<Vec<_>, String>>()?
    };
    Ok(RgbBitmap {
        width: image.width,
        height: image.height,
        pixels,
    })
}

struct Decoded {
    width: u32,
    height: u32,
    bits: u16,
    indices: Vec<u8>,
    palette: Vec<[u8; 3]>,
}

fn decode(bytes: &[u8]) -> Result<Decoded, String> {
    if bytes.len() < 54 || bytes[0] != b'B' || bytes[1] != b'M' {
        return Err("invalid BMP header".to_owned());
    }
    let offset = u32_at(bytes, 10)?;
    let header_size = u32_at(bytes, 14)?;
    if header_size < 40 {
        return Err("unsupported BMP header".to_owned());
    }
    let width = i32_at(bytes, 18)?;
    let height_raw = i32_at(bytes, 22)?;
    if width <= 0 || height_raw == 0 || height_raw == i32::MIN {
        return Err("invalid BMP dimensions".to_owned());
    }
    let planes = u16_at(bytes, 26)?;
    let bits = u16_at(bytes, 28)?;
    let compression = u32_at(bytes, 30)?;
    if planes != 1 || compression != 0 || !matches!(bits, 8 | 24) {
        return Err("only uncompressed 8-bit and 24-bit BMP files are supported".to_owned());
    }
    let width = u32::try_from(width).expect("宽度已确认为正数");
    let top_down = height_raw < 0;
    let height = height_raw.unsigned_abs();
    let count = (width as usize)
        .checked_mul(height as usize)
        .ok_or_else(|| "bitmap is too large".to_owned())?;
    let stride = (u64::from(width) * u64::from(bits)).div_ceil(32) * 4;
    let data_end = u64::from(offset)
        .checked_add(
            stride
                .checked_mul(u64::from(height))
                .ok_or_else(|| "bitmap is too large".to_owned())?,
        )
        .ok_or_else(|| "bitmap is too large".to_owned())?;
    if data_end > bytes.len() as u64 {
        return Err("truncated BMP pixel data".to_owned());
    }

    let mut palette = Vec::new();
    if bits == 8 {
        let colors_used = u32_at(bytes, 46)?;
        let colors = if colors_used == 0 { 256 } else { colors_used };
        if colors > 256 {
            return Err("8-bit BMP palette is too large".to_owned());
        }
        let palette_at = 14 + header_size;
        let palette_end = u64::from(palette_at)
            .checked_add(u64::from(colors) * 4)
            .ok_or_else(|| "bitmap is too large".to_owned())?;
        if palette_end > bytes.len() as u64 {
            return Err("truncated BMP palette".to_owned());
        }
        palette.reserve(colors as usize);
        for index in 0..colors {
            let at = (palette_at + index * 4) as usize;
            palette.push([bytes[at + 2], bytes[at + 1], bytes[at]]);
        }
    }

    let bytes_per_pixel = usize::from(bits / 8);
    let mut indices = vec![0; count * bytes_per_pixel];
    for file_row in 0..height {
        let y = if top_down {
            file_row
        } else {
            height - 1 - file_row
        };
        let row = (u64::from(offset) + u64::from(file_row) * stride) as usize;
        let target = (y as usize) * (width as usize) * bytes_per_pixel;
        let source = &bytes[row..row + (width as usize) * bytes_per_pixel];
        indices[target..target + source.len()].copy_from_slice(source);
    }
    Ok(Decoded {
        width,
        height,
        bits,
        indices,
        palette,
    })
}

fn u16_at(bytes: &[u8], at: usize) -> Result<u16, String> {
    let bytes = bytes.get(at..at + 2).ok_or("truncated BMP header")?;
    Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn u32_at(bytes: &[u8], at: usize) -> Result<u32, String> {
    let bytes = bytes.get(at..at + 4).ok_or("truncated BMP header")?;
    Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn i32_at(bytes: &[u8], at: usize) -> Result<i32, String> {
    Ok(i32::from_le_bytes(u32_at(bytes, at)?.to_le_bytes()))
}

#[cfg(test)]
mod tests {
    use super::{read_indexed, read_rgb};

    fn encode(width: u32, height: i32, bits: u16, rows: &[u8], palette: &[[u8; 4]]) -> Vec<u8> {
        let abs_height = height.unsigned_abs();
        let stride = ((u64::from(width) * u64::from(bits)).div_ceil(32) * 4) as usize;
        let pixel_bytes = usize::from(bits / 8);
        let top_down = height < 0;
        let mut pixels = vec![0; stride * abs_height as usize];
        for y in 0..abs_height {
            let source_y = if top_down { y } else { abs_height - 1 - y };
            let row = y as usize * stride;
            for x in 0..width {
                let from = ((source_y * width + x) as usize) * pixel_bytes;
                let to = row + x as usize * pixel_bytes;
                pixels[to..to + pixel_bytes].copy_from_slice(&rows[from..from + pixel_bytes]);
            }
        }
        let palette_bytes = palette.len() * 4;
        let offset = 14 + 40 + palette_bytes;
        let mut file = Vec::new();
        file.extend(b"BM");
        file.extend(u32::try_from(offset + pixels.len()).unwrap().to_le_bytes());
        file.extend([0, 0, 0, 0]);
        file.extend(u32::try_from(offset).unwrap().to_le_bytes());
        file.extend(40u32.to_le_bytes());
        file.extend(width.to_le_bytes());
        file.extend(height.to_le_bytes());
        file.extend(1u16.to_le_bytes());
        file.extend(bits.to_le_bytes());
        file.extend(0u32.to_le_bytes());
        file.extend(u32::try_from(pixels.len()).unwrap().to_le_bytes());
        file.extend([0; 8]);
        file.extend(u32::try_from(palette.len()).unwrap().to_le_bytes());
        file.extend(0u32.to_le_bytes());
        for entry in palette {
            file.extend(entry);
        }
        file.extend(pixels);
        file
    }

    #[test]
    fn eight_bit_bottom_up_rows_become_top_down() {
        let palette = vec![[0, 0, 0, 0]; 4];
        let file = encode(2, 2, 8, &[1, 2, 3, 4], &palette);
        assert_eq!(file.len(), 14 + 40 + 16 + 8);
        let image = read_indexed(&file).expect("位图有效");
        assert_eq!(image.indices, vec![1, 2, 3, 4]);
        assert_eq!(image.indices[0], 1);
    }

    #[test]
    fn top_down_and_palette_rgb_keep_their_corners() {
        let mut palette = vec![[0, 0, 0, 0]; 2];
        palette[1] = [10, 20, 30, 0];
        let indexed = encode(1, -1, 8, &[1], &palette);
        let rgb = read_rgb(&indexed).expect("调色板可展开");
        assert_eq!(rgb.pixels, vec![[30, 20, 10]]);

        let file = encode(1, 1, 24, &[1, 2, 3], &[]);
        let rgb = read_rgb(&file).expect("24 位有效");
        assert_eq!(rgb.pixels, vec![[3, 2, 1]]);
    }

    #[test]
    fn truncated_and_compressed_files_are_rejected() {
        let file = encode(2, 2, 8, &[1, 2, 3, 4], &[[0; 4]]);
        assert!(read_indexed(&file[..file.len() - 1]).is_err());
        let mut compressed = file.clone();
        compressed[30] = 1;
        assert!(read_indexed(&compressed).is_err());
        let mut negative = file.clone();
        negative[18] = 0xff;
        negative[19] = 0xff;
        negative[20] = 0xff;
        negative[21] = 0xff;
        assert!(read_indexed(&negative).is_err());
    }
}
