// SPDX-License-Identifier: GPL-3.0-only

//! 界面中间表示。对象保留字段顺序和重复项，未知内容留在 properties。

use std::path::PathBuf;

/// 解析结果指向玩家机器上的源文件，不把 EU4 内容复制进仓库。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentSource {
    pub path: PathBuf,
}

impl ContentSource {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

/// 单个内容根。后续 DLC 和 Mod 再按优先级覆盖，不在第一版里模拟。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentMount {
    pub root: PathBuf,
    pub priority: i32,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentStack {
    pub mounts: Vec<ContentMount>,
}

impl ContentStack {
    pub fn single(root: impl Into<PathBuf>) -> Self {
        Self {
            mounts: vec![ContentMount {
                root: root.into(),
                priority: 0,
                label: "base".to_owned(),
            }],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiOperator {
    Equal,
    LessThan,
    GreaterThan,
    /// 保留 jomini 认得、但第一版布局不解释的操作符原文。
    Unknown(String),
}

impl UiOperator {
    pub fn symbol(&self) -> &str {
        match self {
            Self::Equal => "=",
            Self::LessThan => "<",
            Self::GreaterThan => ">",
            Self::Unknown(symbol) => symbol,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum UiValue {
    Atom(String),
    String(String),
    Bool(bool),
    Integer(i64),
    /// 保留小数的原始写法，避免提前变成浮点。
    Decimal(String),
    Object(Vec<UiField>),
    Array(Vec<UiValue>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct UiField {
    pub key: String,
    pub operator: UiOperator,
    pub value: UiValue,
}

/// 坐标和尺寸里的标量。`@` 宏第一版保持 Atom，不做替换。
#[derive(Debug, Clone, PartialEq)]
pub enum UiScalar {
    Integer(i64),
    Decimal(String),
    Atom(String),
    String(String),
}

impl UiScalar {
    pub fn from_value(value: &UiValue) -> Option<Self> {
        match value {
            UiValue::Integer(value) => Some(Self::Integer(*value)),
            UiValue::Decimal(value) => Some(Self::Decimal(value.clone())),
            UiValue::Atom(value) => Some(Self::Atom(value.clone())),
            UiValue::String(value) => Some(Self::String(value.clone())),
            _ => None,
        }
    }

    pub fn to_text(&self) -> String {
        match self {
            Self::Integer(value) => value.to_string(),
            Self::Decimal(value) | Self::Atom(value) | Self::String(value) => value.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct UiVec2 {
    pub x: UiScalar,
    pub y: UiScalar,
    pub properties: Vec<UiField>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiOrientation {
    UpperLeft,
    Center,
    LowerLeft,
    UpperRight,
    LowerRight,
    Unknown(String),
}

impl UiOrientation {
    pub fn parse(text: &str) -> Self {
        match text {
            "UPPER_LEFT" => Self::UpperLeft,
            "CENTER" => Self::Center,
            "LOWER_LEFT" => Self::LowerLeft,
            "UPPER_RIGHT" => Self::UpperRight,
            "LOWER_RIGHT" => Self::LowerRight,
            other => Self::Unknown(other.to_owned()),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::UpperLeft => "UPPER_LEFT",
            Self::Center => "CENTER",
            Self::LowerLeft => "LOWER_LEFT",
            Self::UpperRight => "UPPER_RIGHT",
            Self::LowerRight => "LOWER_RIGHT",
            Self::Unknown(text) => text,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuiNodeKind {
    Window,
    ContainerWindow,
    Button,
    Icon,
    InstantTextBox,
    TextBox,
    GridBox,
    ListBox,
    OverlappingElementsBox,
    Unknown(String),
}

impl GuiNodeKind {
    pub fn from_type_key(key: &str) -> Self {
        match key {
            "windowType" => Self::Window,
            "containerWindowType" => Self::ContainerWindow,
            "guiButtonType" => Self::Button,
            "iconType" => Self::Icon,
            "instantTextBoxType" => Self::InstantTextBox,
            "textBoxType" => Self::TextBox,
            "gridBoxType" => Self::GridBox,
            "listboxType" => Self::ListBox,
            "overlappingElementsBoxType" => Self::OverlappingElementsBox,
            other => Self::Unknown(other.to_owned()),
        }
    }

    pub fn id(&self) -> &str {
        match self {
            Self::Window => "window",
            Self::ContainerWindow => "container_window",
            Self::Button => "button",
            Self::Icon => "icon",
            Self::InstantTextBox => "instant_text_box",
            Self::TextBox => "text_box",
            Self::GridBox => "grid_box",
            Self::ListBox => "list_box",
            Self::OverlappingElementsBox => "overlapping_elements_box",
            Self::Unknown(_) => "unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct GuiNode {
    pub kind: GuiNodeKind,
    pub type_key: String,
    pub name: Option<String>,
    pub position: Option<UiVec2>,
    pub size: Option<UiVec2>,
    pub orientation: Option<UiOrientation>,
    pub sprite: Option<String>,
    pub quad_texture_sprite: Option<String>,
    pub text: Option<String>,
    pub font: Option<String>,
    pub button_text: Option<String>,
    pub button_font: Option<String>,
    pub max_width: Option<UiScalar>,
    pub max_height: Option<UiScalar>,
    pub format: Option<String>,
    pub moveable: Option<bool>,
    pub dont_render: Option<bool>,
    pub always_transparent: Option<bool>,
    pub tooltip: Option<String>,
    pub click_sound: Option<String>,
    pub properties: Vec<UiField>,
    pub children: Vec<GuiNode>,
    pub source: ContentSource,
}

impl GuiNode {
    pub fn is_unknown(&self) -> bool {
        matches!(self.kind, GuiNodeKind::Unknown(_))
    }

    pub fn find(&self, name: &str) -> Option<&GuiNode> {
        if self.name.as_deref() == Some(name) {
            return Some(self);
        }
        self.children.iter().find_map(|child| child.find(name))
    }

    fn unknown_node_count(&self) -> usize {
        usize::from(self.is_unknown())
            + self
                .children
                .iter()
                .map(GuiNode::unknown_node_count)
                .sum::<usize>()
    }

    fn unknown_field_count(&self) -> usize {
        self.properties.len()
            + self
                .position
                .as_ref()
                .map_or(0, |value| value.properties.len())
            + self.size.as_ref().map_or(0, |value| value.properties.len())
            + self
                .children
                .iter()
                .map(GuiNode::unknown_field_count)
                .sum::<usize>()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct GuiDocument {
    pub source: ContentSource,
    pub roots: Vec<GuiNode>,
    pub properties: Vec<UiField>,
}

impl GuiDocument {
    pub fn unknown_node_count(&self) -> usize {
        self.roots.iter().map(GuiNode::unknown_node_count).sum()
    }

    pub fn unknown_field_count(&self) -> usize {
        self.properties.len()
            + self
                .roots
                .iter()
                .map(GuiNode::unknown_field_count)
                .sum::<usize>()
    }

    pub fn find(&self, name: &str) -> Option<&GuiNode> {
        self.roots.iter().find_map(|node| node.find(name))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpriteDefinition {
    pub kind: String,
    pub name: String,
    pub texture_file: Option<String>,
    pub frames: Option<u32>,
    pub frame_duration: Option<String>,
    pub always_transparent: Option<bool>,
    pub properties: Vec<UiField>,
    pub source: ContentSource,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GfxCatalog {
    pub sprites: Vec<SpriteDefinition>,
    /// 单个 `.gfx` 失败时保留已读到的精灵，并带上来源说明。
    pub errors: Vec<String>,
}

impl GfxCatalog {
    pub fn new() -> Self {
        Self {
            sprites: Vec::new(),
            errors: Vec::new(),
        }
    }

    /// 同名精灵以后出现的定义为准。单个根目录里这只是文件内和文件间的读取顺序。
    pub fn resolve(&self, name: &str) -> Option<&SpriteDefinition> {
        self.sprites.iter().rev().find(|sprite| sprite.name == name)
    }
}

impl Default for GfxCatalog {
    fn default() -> Self {
        Self::new()
    }
}
