// SPDX-License-Identifier: GPL-3.0-only

//! 开局科技等级。只认等级序号，不读每级效果。

use std::collections::BTreeMap;

use crate::{Error, content::ContentRoot, script};

#[derive(Debug)]
pub(super) struct TechTables {
    start_levels: BTreeMap<String, u32>,
    /// `groups` 的书写顺序。帧号从 1 起，不按字母排序。
    group_order: Vec<String>,
    adm_levels: u32,
    dip_levels: u32,
    mil_levels: u32,
}

impl TechTables {
    pub(super) fn start_level(&self, group: &str) -> Result<u32, Error> {
        let level = self.start_levels.get(group).copied().ok_or_else(|| {
            Error::invalid(
                "common/technology.txt",
                format!("{group} has no start_level"),
            )
        })?;
        if level >= self.adm_levels || level >= self.dip_levels || level >= self.mil_levels {
            return Err(Error::invalid(
                "common/technologies",
                format!("start_level {level} is outside the technology tables"),
            ));
        }
        Ok(level)
    }

    pub(super) fn group_order(&self) -> &[String] {
        &self.group_order
    }
}

pub(super) fn load(content: &ContentRoot) -> Result<TechTables, Error> {
    let (start_levels, group_order) = start_levels(content)?;
    Ok(TechTables {
        start_levels,
        group_order,
        adm_levels: level_count(content, "common/technologies/adm.txt")?,
        dip_levels: level_count(content, "common/technologies/dip.txt")?,
        mil_levels: level_count(content, "common/technologies/mil.txt")?,
    })
}

fn start_levels(content: &ContentRoot) -> Result<(BTreeMap<String, u32>, Vec<String>), Error> {
    let file = content.read("common/technology.txt")?;
    let tape = script::parse(&file)?;
    let mut groups = None;
    for (key, operator, value) in tape.windows1252_reader().fields() {
        if key.read_str() != "groups" {
            continue;
        }
        script::equal(&file, operator, "groups")?;
        if groups.is_some() {
            return Err(Error::invalid(&file.path, "duplicate technology groups"));
        }
        groups = Some(
            value
                .read_object()
                .map_err(|error| Error::invalid(&file.path, error.to_string()))?,
        );
    }
    let Some(groups) = groups else {
        return Err(Error::invalid(&file.path, "missing technology groups"));
    };
    let mut levels = BTreeMap::new();
    let mut order = Vec::new();
    for (key, operator, value) in groups.fields() {
        let name = key.read_str().into_owned();
        script::equal(&file, operator, &name)?;
        let object = value
            .read_object()
            .map_err(|error| Error::invalid(&file.path, error.to_string()))?;
        let mut start = None;
        for (key, operator, value) in object.fields() {
            if key.read_str() != "start_level" {
                continue;
            }
            script::equal(&file, operator, "start_level")?;
            if start.is_some() {
                return Err(Error::invalid(
                    &file.path,
                    format!("duplicate start_level for {name}"),
                ));
            }
            start = Some(whole(&file, &value, "start_level")?);
        }
        let start = start
            .ok_or_else(|| Error::invalid(&file.path, format!("missing start_level for {name}")))?;
        levels.insert(name.clone(), start);
        order.push(name);
    }
    Ok((levels, order))
}

fn level_count(content: &ContentRoot, relative: &str) -> Result<u32, Error> {
    let file = content.read(relative)?;
    let tape = script::parse(&file)?;
    let mut count = 0u32;
    for (key, operator, value) in tape.windows1252_reader().fields() {
        if key.read_str() != "technology" {
            continue;
        }
        script::equal(&file, operator, "technology")?;
        value
            .read_object()
            .map_err(|error| Error::invalid(&file.path, error.to_string()))?;
        count = count
            .checked_add(1)
            .ok_or_else(|| Error::invalid(&file.path, "too many technology levels"))?;
    }
    if count == 0 {
        return Err(Error::invalid(&file.path, "technology table is empty"));
    }
    Ok(count)
}

fn whole(
    file: &crate::content::ContentFile,
    value: &jomini::text::ValueReader<'_, '_, jomini::Windows1252Encoding>,
    field: &str,
) -> Result<u32, Error> {
    let scalar = value
        .read_scalar()
        .map_err(|error| Error::invalid(&file.path, error.to_string()))?;
    scalar
        .to_u64()
        .ok()
        .and_then(|level| u32::try_from(level).ok())
        .ok_or_else(|| Error::invalid(&file.path, format!("{field} must be a nonnegative integer")))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    use crate::content::ContentRoot;

    use super::load;

    #[test]
    fn levels_are_the_order_of_technology_blocks() {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "augustus-tech-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("common/technologies")).unwrap();
        fs::write(
            root.join("common/technology.txt"),
            "groups = { western = { start_level = 1 } ottoman = { start_level = 3 } }",
        )
        .unwrap();
        let table = "technology = { year = 1 }\ntechnology = { year = 2 }\ntechnology = { year = 3 }\ntechnology = { year = 4 }\n";
        for name in ["adm.txt", "dip.txt", "mil.txt"] {
            fs::write(root.join("common/technologies").join(name), table).unwrap();
        }
        let tables = load(&ContentRoot::new(&root)).unwrap();
        let _ = fs::remove_dir_all(&root);
        assert_eq!(tables.start_level("western").unwrap(), 1);
        assert_eq!(tables.start_level("ottoman").unwrap(), 3);
        assert_eq!(
            tables.group_order(),
            ["western".to_owned(), "ottoman".to_owned()]
        );
        assert!(tables.start_level("missing").is_err());
    }
}
