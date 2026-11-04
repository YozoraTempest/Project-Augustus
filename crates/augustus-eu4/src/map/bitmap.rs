// SPDX-License-Identifier: GPL-3.0-only

//! 将省份位图转换为按行排列的编号数组，并统计实际出现的省份。

use std::collections::HashSet;

use crate::{Error, content::ContentFile};

use super::definitions::Definitions;

pub(super) struct Raster {
    pub pixels: Vec<u32>,
    pub mapped_provinces: usize,
    pub unassigned_pixels: usize,
}

pub(super) fn load(
    file: &ContentFile,
    width: u32,
    height: u32,
    definitions: &Definitions,
) -> Result<Raster, Error> {
    // 配置可以引用任意文件名，显式指定 BMP 格式，不依赖扩展名识别。
    let bitmap = image::load_from_memory_with_format(&file.bytes, image::ImageFormat::Bmp)
        .map_err(|error| Error::invalid(&file.path, error.to_string()))?
        .into_rgb8();
    if bitmap.dimensions() != (width, height) {
        return Err(Error::invalid(
            &file.path,
            format!(
                "bitmap is {} x {}, but default.map specifies {width} x {height}",
                bitmap.width(),
                bitmap.height()
            ),
        ));
    }

    let mut pixels = Vec::with_capacity(bitmap.as_raw().len() / 3);
    let mut mapped = HashSet::new();
    let mut unassigned_pixels = 0;
    // 解码后的像素从上到下、从左到右排列，与坐标查询使用相同顺序。
    for pixel in bitmap.pixels() {
        if let Some(id) = definitions.province_for_color(pixel.0)? {
            pixels.push(id);
            mapped.insert(id);
        } else {
            pixels.push(0);
            unassigned_pixels += 1;
        }
    }
    Ok(Raster {
        pixels,
        mapped_provinces: mapped.len(),
        unassigned_pixels,
    })
}
