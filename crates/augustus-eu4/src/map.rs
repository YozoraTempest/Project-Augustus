// SPDX-License-Identifier: GPL-3.0-only

//! 组合地图配置、省份定义和位图，提供与运行入口无关的地图查询。

mod adjacency;
mod anchors;
mod bitmap;
mod config;
mod definitions;
pub(crate) mod indexed;

use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use crate::{Error, content::ContentRoot};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MapSummary {
    pub width: u32,
    pub height: u32,
    /// 编号位于配置范围内的省份定义数量，包含没有出现在位图上的省份。
    pub defined_provinces: usize,
    /// 位图中实际出现且能唯一对应到编号的省份数量。
    pub mapped_provinces: usize,
    /// 颜色没有对应省份定义的像素数量。
    pub unassigned_pixels: usize,
}

#[derive(Debug)]
pub struct ProvinceMap {
    summary: MapSummary,
    defined_ids: BTreeSet<u32>,
    mapped_ids: BTreeSet<u32>,
    names: BTreeMap<u32, String>,
    // 按行保存省份编号，行内从左到右；0 表示该像素没有对应省份。
    pixels: Vec<u32>,
    anchors: BTreeMap<u32, [u32; 2]>,
    neighbors: BTreeSet<(u32, u32)>,
}

impl ProvinceMap {
    /// 从游戏目录读取地图配置，并按配置中的引用加载省份定义和位图。
    pub fn load(game_dir: impl AsRef<Path>) -> Result<Self, Error> {
        let content = ContentRoot::new(game_dir);
        let config = config::MapConfig::parse(&content.read("map/default.map")?)?;
        let definitions = definitions::Definitions::parse(
            &content.read(&config.definitions)?,
            config.max_provinces,
        )?;
        let raster = bitmap::load(
            &content.read(&config.provinces)?,
            config.width,
            config.height,
            &definitions,
        )?;
        let mut neighbors = adjacency::calculate(&raster.pixels, config.width);
        if let Some(path) = &config.adjacencies {
            adjacency::apply_impassable(&mut neighbors, &content.read(path)?)?;
        }

        Ok(Self {
            summary: MapSummary {
                width: config.width,
                height: config.height,
                defined_provinces: definitions.len(),
                mapped_provinces: raster.mapped_ids.len(),
                unassigned_pixels: raster.unassigned_pixels,
            },
            anchors: anchors::calculate(&raster.pixels, config.width),
            pixels: raster.pixels,
            defined_ids: definitions.ids,
            mapped_ids: raster.mapped_ids,
            names: definitions.names,
            neighbors,
        })
    }

    pub fn summary(&self) -> &MapSummary {
        &self.summary
    }

    /// 包含未绘制的省份定义，用于校对历史文件和地理分类的引用。
    pub fn defined_province_ids(&self) -> &BTreeSet<u32> {
        &self.defined_ids
    }

    pub fn mapped_province_ids(&self) -> &BTreeSet<u32> {
        &self.mapped_ids
    }

    /// 定义表名称只作为本地化缺失时的回退，不参与省份身份识别。
    pub fn definition_name(&self, id: u32) -> Option<&str> {
        self.names.get(&id).map(String::as_str)
    }

    /// 按从上到下、从左到右的顺序借用像素编号，供显示层生成纹理。
    /// 编号 0 表示没有对应省份；显示层与坐标查询共用这一份数据。
    pub fn province_ids(&self) -> &[u32] {
        &self.pixels
    }

    pub fn province_anchor(&self, id: u32) -> Option<[u32; 2]> {
        self.anchors.get(&id).copied()
    }

    pub fn adjacent_pairs(&self) -> &BTreeSet<(u32, u32)> {
        &self.neighbors
    }

    /// 按像素坐标查询省份：左上角为 (0, 0)，向右、向下递增。
    /// 无省份的像素返回 `Ok(None)`，坐标越界返回错误。
    pub fn province_at(&self, x: u32, y: u32) -> Result<Option<u32>, Error> {
        let MapSummary { width, height, .. } = self.summary;
        if x >= width || y >= height {
            return Err(Error::OutOfBounds {
                x,
                y,
                width,
                height,
            });
        }
        let id = self.pixels[y as usize * width as usize + x as usize];
        Ok((id != 0).then_some(id))
    }
}
