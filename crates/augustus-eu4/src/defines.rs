// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::{
    collections::BTreeMap,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use jomini::Encoding;
use mlua::{HookTriggers, Lua, LuaOptions, StdLib, Value, VmState, chunk::ChunkMode};
use serde::Serialize;

mod sources;

use crate::{
    ContentSet, ContentView, Error, FileEntry, LoadedSource, SourceLocation, TextEncoding,
};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DefineDifficulty {
    VeryEasy,
    Easy,
    #[default]
    Normal,
    Hard,
    VeryHard,
}

impl DefineDifficulty {
    fn filename(self) -> &'static str {
        match self {
            Self::VeryEasy => "difficulty_very_easy.lua",
            Self::Easy => "difficulty_easy.lua",
            Self::Normal => "difficulty_normal.lua",
            Self::Hard => "difficulty_hard.lua",
            Self::VeryHard => "difficulty_very_hard.lua",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum DefineValue {
    Number(f64),
    String(String),
    Boolean(bool),
    Unsupported(String),
}

impl DefineValue {
    fn same_bits(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Number(first), Self::Number(second)) => first.to_bits() == second.to_bits(),
            _ => self == other,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct EvaluatedDefine {
    pub value: DefineValue,
    /// Last effective source, with field locations for straight assignments and chunk locations for dynamic execution.
    pub source: LoadedSource,
}

/// Evaluated input data only. The interpreter is discarded before world construction.
#[derive(Debug, Serialize)]
pub struct Defines {
    pub difficulty: DefineDifficulty,
    pub executed_files: Vec<LoadedSource>,
    pub values: BTreeMap<String, EvaluatedDefine>,
    #[serde(skip)]
    base_source: std::path::PathBuf,
}

impl Defines {
    pub fn from_content(content: &ContentSet, difficulty: DefineDifficulty) -> Result<Self, Error> {
        Self::load(content, &content.files()?, difficulty)
    }

    pub(super) fn load(
        root: &impl ContentView,
        files: &[FileEntry],
        difficulty: DefineDifficulty,
    ) -> Result<Self, Error> {
        let base = Path::new("common/defines.lua");
        let mut result = Self {
            difficulty,
            executed_files: Vec::new(),
            values: BTreeMap::new(),
            base_source: root.physical_path(base),
        };
        if !files.iter().any(|file| file.relative_path == base) {
            return Ok(result);
        }
        let mut paths = vec![base.to_path_buf()];
        let mut selected = None;
        // Effective files already use the content loader's deterministic filename order.
        for file in files.iter().filter(|file| {
            file.relative_path.parent() == Some(Path::new("common/defines"))
                && file
                    .relative_path
                    .extension()
                    .is_some_and(|ext| ext == "lua")
        }) {
            let name = file
                .relative_path
                .file_name()
                .expect("file entry")
                .to_string_lossy();
            if name.starts_with("difficulty_") {
                if name == difficulty.filename() {
                    selected = Some(file.relative_path.clone());
                }
            } else {
                paths.push(file.relative_path.clone());
            }
        }
        paths.extend(selected);
        let lua = Lua::new_with(
            StdLib::TABLE | StdLib::STRING | StdLib::MATH,
            LuaOptions::default(),
        )
        .map_err(|error| lua_error(&result.base_source, error))?;
        lua.set_memory_limit(64 * 1024 * 1024)
            .map_err(|error| lua_error(&result.base_source, error))?;
        // Defines compute input data without filesystem, process or random services.
        for name in [
            "dofile",
            "loadfile",
            "load",
            "loadstring",
            "print",
            "collectgarbage",
            "pcall",
            "xpcall",
            "coroutine",
        ] {
            lua.globals()
                .raw_set(name, Value::Nil)
                .map_err(|error| lua_error(&result.base_source, error))?;
        }
        let math: mlua::Table = lua
            .globals()
            .raw_get("math")
            .map_err(|error| lua_error(&result.base_source, error))?;
        for name in ["random", "randomseed"] {
            math.raw_set(name, Value::Nil)
                .map_err(|error| lua_error(&result.base_source, error))?;
        }
        let budget = Arc::new(AtomicU64::new(10_000_000));
        lua.set_hook(
            HookTriggers::new().every_nth_instruction(1000),
            move |_, _| {
                if budget
                    .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |left| {
                        left.checked_sub(1000)
                    })
                    .is_err()
                {
                    return Err(mlua::Error::runtime("define instruction limit exceeded"));
                }
                Ok(VmState::Continue)
            },
        )
        .map_err(|error| lua_error(&result.base_source, error))?;
        for path in paths {
            let doc = root.read_text(&path, TextEncoding::Windows1252)?;
            if doc.bytes().len() > 8 * 1024 * 1024 {
                return Err(Error::message(doc.source(), "define source exceeds 8 MiB"));
            }
            let text = TextEncoding::Windows1252.decode(doc.bytes());
            lua.load(text.as_ref())
                .set_name(format!("@{}", doc.source().display()))
                .set_mode(ChunkMode::Text)
                .exec()
                .map_err(|error| lua_error(doc.source(), error))?;
            let source = LoadedSource {
                path: doc.source().into(),
                line: 1,
                column: 1,
            };
            let snapshot = snapshot(&lua).map_err(|error| lua_error(doc.source(), error))?;
            let assignments = sources::assignments(&text, doc.source());
            result.values.retain(|key, _| snapshot.contains_key(key));
            for (key, value) in snapshot {
                if assignments.contains_key(&key)
                    || result
                        .values
                        .get(&key)
                        .is_none_or(|old| !old.value.same_bits(&value))
                {
                    let source = assignments.get(&key).unwrap_or(&source).clone();
                    result.values.insert(
                        key,
                        EvaluatedDefine {
                            value,
                            source: source.clone(),
                        },
                    );
                }
            }
            result.executed_files.push(source);
        }
        Ok(result)
    }

    pub(super) fn required(&self, section: &str, key: &str) -> Result<(i64, LoadedSource), Error> {
        self.optional(section, key)?
            .ok_or_else(|| Error::message(&self.base_source, format!("missing define {key}")))
    }
    pub(super) fn optional(
        &self,
        section: &str,
        key: &str,
    ) -> Result<Option<(i64, LoadedSource)>, Error> {
        self.number(section, key, false)
    }
    pub(super) fn optional_signed(
        &self,
        section: &str,
        key: &str,
    ) -> Result<Option<(i64, LoadedSource)>, Error> {
        self.number(section, key, true)
    }
    fn number(
        &self,
        section: &str,
        key: &str,
        signed: bool,
    ) -> Result<Option<(i64, LoadedSource)>, Error> {
        let Some(entry) = self.values.get(&format!("NDefines.{section}.{key}")) else {
            return Ok(None);
        };
        let fail = |detail: &str| {
            Error::message(&entry.source.path, format!("{key}: {detail}")).with_location(
                SourceLocation {
                    line: entry.source.line,
                    column: entry.source.column,
                },
            )
        };
        let DefineValue::Number(value) = entry.value else {
            return Err(fail("expected numeric define"));
        };
        if !signed && value < 0.0 {
            return Err(fail("expected nonnegative define"));
        }
        // Existing consumers use millionths. Round evaluation noise at that boundary.
        let scaled = (value * 1_000_000.0).round();
        if !scaled.is_finite() || !(-((1_u64 << 63) as f64)..(1_u64 << 63) as f64).contains(&scaled)
        {
            return Err(fail("define is outside fixed-point range"));
        }
        Ok(Some((scaled as i64, entry.source.clone())))
    }
}

fn snapshot(lua: &Lua) -> mlua::Result<BTreeMap<String, DefineValue>> {
    let root: mlua::Table = lua.globals().raw_get("NDefines")?;
    let mut result = BTreeMap::new();
    for section in root.pairs::<mlua::LuaString, mlua::Table>() {
        let (name, values) = section?;
        let name = name.to_str()?;
        for entry in values.pairs::<mlua::LuaString, Value>() {
            let (key, value) = entry?;
            let value = match value {
                Value::Integer(value) => DefineValue::Number(value as f64),
                Value::Number(value) if value.is_finite() => DefineValue::Number(value),
                Value::Number(_) => return Err(mlua::Error::runtime("non-finite define value")),
                Value::String(value) => DefineValue::String(value.to_str()?.to_owned()),
                Value::Boolean(value) => DefineValue::Boolean(value),
                other => DefineValue::Unsupported(other.type_name().into()),
            };
            result.insert(format!("NDefines.{name}.{}", key.to_str()?), value);
            if result.len() > 50_000 {
                return Err(mlua::Error::runtime("define count exceeds 50000"));
            }
        }
    }
    Ok(result)
}

fn lua_error(path: &Path, error: mlua::Error) -> Error {
    let message = error.to_string();
    let line = message
        .split(':')
        .find_map(|part| part.trim().parse::<usize>().ok())
        .unwrap_or(1);
    Error::message(path, format!("Lua define evaluation failed: {message}"))
        .with_location(SourceLocation { line, column: 1 })
}
