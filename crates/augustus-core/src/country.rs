// SPDX-License-Identifier: GPL-3.0-only

//! 国家实体的标识与状态，由 World 统一持有。

#[derive(Debug)]
pub struct Country {
    // 标识不可在创建后修改，保持 World 索引与省份归属引用一致。
    tag: String,
}

impl Country {
    pub fn new(tag: impl Into<String>) -> Self {
        Self { tag: tag.into() }
    }

    pub fn tag(&self) -> &str {
        &self.tag
    }
}
