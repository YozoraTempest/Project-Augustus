// SPDX-License-Identifier: GPL-3.0-only

//! 从欧陆安装目录读取地形贴图。文件留在原处，不复制进仓库。

use std::path::Path;

use godot::{
    classes::{Image, ImageTexture, image::Format},
    prelude::*,
};

pub struct TerrainTextures {
    pub color: Gd<ImageTexture>,
    pub atlas: Gd<ImageTexture>,
    pub indices: Gd<ImageTexture>,
    pub water_color: Gd<ImageTexture>,
    pub water_detail: Gd<ImageTexture>,
    pub country_border: Option<Gd<ImageTexture>>,
    pub province_border: Option<Gd<ImageTexture>>,
}

/// 欧陆地形着色用的三张图：季节色、图集、地形序号。
pub fn load(game_directory: &str) -> Result<TerrainTextures, String> {
    let root = Path::new(game_directory);
    let color = texture_from_dds(&root.join("map/terrain/colormap_summer.dds"), true)?;
    let atlas = texture_from_dds(&root.join("map/terrain/atlas0.dds"), true)?;
    let indices = index_texture(&root.join("map/terrain.bmp"))?;
    // 水面着色：水色铺满海面，海底贴图按地图尺寸重复。
    let water_color = texture_from_dds(&root.join("map/terrain/colormap_water.dds"), true)?;
    let water_detail = texture_from_dds(&root.join("map/terrain/underwater_terrain.dds"), true)?;
    // 政治地图的国界和省界。缺了不影响地形。
    let country_border = match texture_from_dds(&root.join("map/terrain/border0.dds"), true) {
        Ok(texture) => Some(texture),
        Err(error) => {
            godot_print!("国界纹理未载入：{error}");
            None
        }
    };
    let province_border = match texture_from_dds(&root.join("map/terrain/border1.dds"), true) {
        Ok(texture) => Some(texture),
        Err(error) => {
            godot_print!("省界纹理未载入：{error}");
            None
        }
    };
    Ok(TerrainTextures {
        color,
        atlas,
        indices,
        water_color,
        water_detail,
        country_border,
        province_border,
    })
}

fn texture_from_dds(path: &Path, mipmaps: bool) -> Result<Gd<ImageTexture>, String> {
    let bytes = std::fs::read(path).map_err(|error| format!("{}：{error}", path.display()))?;
    let (width, height, pixels) = decode_dds(&bytes)?;
    image_texture(width, height, &pixels, mipmaps)
        .ok_or_else(|| format!("无法创建纹理 {}", path.display()))
}

fn index_texture(path: &Path) -> Result<Gd<ImageTexture>, String> {
    let bytes = std::fs::read(path).map_err(|error| format!("{}：{error}", path.display()))?;
    let (width, height, indices) = augustus_eu4::map::palette_indices(&bytes)
        .map_err(|error| format!("{}：{error}", path.display()))?;
    let mut pixels = Vec::with_capacity(indices.len() * 4);
    for index in indices {
        pixels.extend([index, 0, 0, 255]);
    }
    image_texture(width, height, &pixels, false)
        .ok_or_else(|| format!("无法创建地形序号纹理 {}", path.display()))
}

fn image_texture(
    width: u32,
    height: u32,
    pixels: &[u8],
    mipmaps: bool,
) -> Option<Gd<ImageTexture>> {
    let (Ok(width), Ok(height)) = (i32::try_from(width), i32::try_from(height)) else {
        return None;
    };
    let bytes = PackedByteArray::from(pixels);
    let mut image = Image::create_from_data(width, height, false, Format::RGBA8, &bytes)?;
    if mipmaps {
        let _ = image.generate_mipmaps();
    }
    ImageTexture::create_from_image(&image)
}

fn decode_dds(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), String> {
    if bytes.len() < 128 || &bytes[0..4] != b"DDS " {
        return Err("不是 DDS".to_owned());
    }
    let height = u32::from_le_bytes(bytes[12..16].try_into().expect("高度四字节"));
    let width = u32::from_le_bytes(bytes[16..20].try_into().expect("宽度四字节"));
    if width == 0 || height == 0 || width % 4 != 0 || height % 4 != 0 {
        return Err("DDS 尺寸无法按 4×4 块解码".to_owned());
    }
    let dxt5 = match &bytes[84..88] {
        b"DXT1" => false,
        b"DXT5" => true,
        fourcc => return Err(format!("不支持的 DDS 格式 {fourcc:?}")),
    };
    let blocks_x = (width / 4) as usize;
    let blocks_y = (height / 4) as usize;
    let block_bytes = if dxt5 { 16 } else { 8 };
    let needed = 128 + blocks_x * blocks_y * block_bytes;
    if bytes.len() < needed {
        return Err("DDS 像素数据被截断".to_owned());
    }
    let mut pixels = vec![0; width as usize * height as usize * 4];
    let mut cursor = 128;
    for block_y in 0..blocks_y {
        for block_x in 0..blocks_x {
            let block = &bytes[cursor..cursor + block_bytes];
            cursor += block_bytes;
            write_block(
                block,
                dxt5,
                &mut pixels,
                width as usize,
                block_x * 4,
                block_y * 4,
            );
        }
    }
    Ok((width, height, pixels))
}

fn write_block(
    block: &[u8],
    dxt5: bool,
    pixels: &mut [u8],
    width: usize,
    origin_x: usize,
    origin_y: usize,
) {
    let (alphas, color) = if dxt5 {
        (dxt5_alphas(block), &block[8..16])
    } else {
        ([255; 16], block)
    };
    let colors = dxt_colors(color);
    let indices = u32::from_le_bytes(color[4..8].try_into().expect("颜色索引四字节"));
    for pixel in 0..16 {
        let color_index = ((indices >> (pixel * 2)) & 3) as usize;
        let x = origin_x + pixel % 4;
        let y = origin_y + pixel / 4;
        let at = (y * width + x) * 4;
        let [red, green, blue] = colors[color_index];
        pixels[at] = red;
        pixels[at + 1] = green;
        pixels[at + 2] = blue;
        pixels[at + 3] = alphas[pixel];
    }
}

fn dxt5_alphas(block: &[u8]) -> [u8; 16] {
    let first = block[0];
    let second = block[1];
    let mut palette = [0; 8];
    palette[0] = first;
    palette[1] = second;
    if first > second {
        for index in 1..7 {
            let index = u16::try_from(index).expect("插值序号");
            palette[index as usize + 1] =
                (((7 - index) * u16::from(first) + index * u16::from(second)) / 7) as u8;
        }
    } else {
        for index in 1..5 {
            let index = u16::try_from(index).expect("插值序号");
            palette[index as usize + 1] =
                (((5 - index) * u16::from(first) + index * u16::from(second)) / 5) as u8;
        }
        palette[6] = 0;
        palette[7] = 255;
    }
    let mut bits = 0u64;
    for (place, byte) in block[2..8].iter().enumerate() {
        bits |= u64::from(*byte) << (place * 8);
    }
    let mut alphas = [0; 16];
    for pixel in 0..16 {
        alphas[pixel] = palette[((bits >> (pixel * 3)) & 7) as usize];
    }
    alphas
}

fn dxt_colors(block: &[u8]) -> [[u8; 3]; 4] {
    let first = u16::from_le_bytes([block[0], block[1]]);
    let second = u16::from_le_bytes([block[2], block[3]]);
    let mut colors = [rgb565(first), rgb565(second), [0, 0, 0], [0, 0, 0]];
    if first > second {
        colors[2] = mix(colors[0], colors[1], 2, 1);
        colors[3] = mix(colors[0], colors[1], 1, 2);
    } else {
        colors[2] = mix(colors[0], colors[1], 1, 1);
        colors[3] = [0, 0, 0];
    }
    colors
}

fn mix(first: [u8; 3], second: [u8; 3], first_weight: u16, second_weight: u16) -> [u8; 3] {
    let divisor = first_weight + second_weight;
    let mut mixed = [0; 3];
    for channel in 0..3 {
        mixed[channel] = ((u16::from(first[channel]) * first_weight
            + u16::from(second[channel]) * second_weight)
            / divisor) as u8;
    }
    mixed
}

fn rgb565(color: u16) -> [u8; 3] {
    [
        (((color >> 11) & 31) * 255 / 31) as u8,
        (((color >> 5) & 63) * 255 / 63) as u8,
        ((color & 31) * 255 / 31) as u8,
    ]
}

#[cfg(test)]
mod tests {
    use super::decode_dds;

    #[test]
    fn dxt1_solid_white_block() {
        let mut bytes = vec![0; 128 + 8];
        bytes[0..4].copy_from_slice(b"DDS ");
        bytes[12..16].copy_from_slice(&4u32.to_le_bytes());
        bytes[16..20].copy_from_slice(&4u32.to_le_bytes());
        bytes[84..88].copy_from_slice(b"DXT1");
        bytes[128..130].copy_from_slice(&0xffffu16.to_le_bytes());
        let (_width, _height, pixels) = decode_dds(&bytes).expect("解码");
        assert_eq!(&pixels[0..4], &[255, 255, 255, 255]);
    }
}
