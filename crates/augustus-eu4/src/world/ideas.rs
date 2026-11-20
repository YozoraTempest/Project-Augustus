// SPDX-License-Identifier: GPL-3.0-only

//! 国家理念。只认带 `free = yes`、触发条件就是国家标签的组。

use std::collections::BTreeMap;
use std::path::Path;

use jomini::Windows1252Encoding;
use jomini::text::ValueReader;

use crate::{
    Error,
    content::{ContentFile, ContentRoot},
    script,
};

#[derive(Debug, Clone, PartialEq)]
pub(super) struct NationalIdeas {
    pub id: String,
    pub ideas: Vec<NationalIdea>,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct NationalIdea {
    pub id: String,
    pub icon: String,
    pub modifiers: Vec<IdeaModifier>,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct IdeaModifier {
    pub key: String,
    pub amount: ModifierAmount,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum ModifierAmount {
    Number(f64),
    Yes,
}

pub(super) fn load(content: &ContentRoot) -> Result<BTreeMap<String, NationalIdeas>, Error> {
    let files = match content.text_files("common/ideas") {
        Ok(files) => files,
        Err(Error::Read { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
            return Ok(BTreeMap::new());
        }
        Err(error) => return Err(error),
    };
    let mut claims: BTreeMap<String, (usize, NationalIdeas)> = BTreeMap::new();
    for path in files {
        let file = content.read(path)?;
        let tape = script::parse(&file)?;
        for (key, operator, value) in tape.windows1252_reader().fields() {
            let id = key.read_str().into_owned();
            script::equal(&file, operator, &id)?;
            let Ok(object) = value.read_object() else {
                continue;
            };
            if !is_free(&file, &object)? {
                continue;
            }
            let Some(tags) = trigger_tags(&file, &object)? else {
                continue;
            };
            let ideas = NationalIdeas {
                id: id.clone(),
                ideas: idea_entries(content, &file, &object)?,
            };
            let specificity = tags.len();
            for tag in tags {
                match claims.get(&tag) {
                    Some((previous, _)) if *previous <= specificity => {}
                    _ => {
                        claims.insert(tag, (specificity, ideas.clone()));
                    }
                }
            }
        }
    }
    Ok(claims
        .into_iter()
        .map(|(tag, (_, ideas))| (tag, ideas))
        .collect())
}

fn is_free(
    file: &ContentFile,
    object: &jomini::text::ObjectReader<'_, '_, Windows1252Encoding>,
) -> Result<bool, Error> {
    for (key, operator, value) in object.fields() {
        if key.read_str() != "free" {
            continue;
        }
        script::equal(file, operator, "free")?;
        let text = value
            .read_string()
            .map_err(|error| Error::invalid(&file.path, error.to_string()))?;
        return Ok(text == "yes");
    }
    Ok(false)
}

/// 触发条件里只有国家标签（可以包在 OR / AND 里）时，返回这些标签。
fn trigger_tags(
    file: &ContentFile,
    group: &jomini::text::ObjectReader<'_, '_, Windows1252Encoding>,
) -> Result<Option<Vec<String>>, Error> {
    for (key, operator, value) in group.fields() {
        if key.read_str() != "trigger" {
            continue;
        }
        script::equal(file, operator, "trigger")?;
        let trigger = value
            .read_object()
            .map_err(|error| Error::invalid(&file.path, error.to_string()))?;
        return pure_tags(file, &trigger);
    }
    Ok(None)
}

fn pure_tags(
    file: &ContentFile,
    object: &jomini::text::ObjectReader<'_, '_, Windows1252Encoding>,
) -> Result<Option<Vec<String>>, Error> {
    let mut tags = Vec::new();
    for (key, operator, value) in object.fields() {
        let name = key.read_str();
        script::equal(file, operator, &name)?;
        match name.as_ref() {
            "tag" => tags.push(text(file, &value)?),
            "OR" | "AND" => {
                let nested = value
                    .read_object()
                    .map_err(|error| Error::invalid(&file.path, error.to_string()))?;
                let Some(inner) = pure_tags(file, &nested)? else {
                    return Ok(None);
                };
                tags.extend(inner);
            }
            _ => return Ok(None),
        }
    }
    Ok((!tags.is_empty()).then_some(tags))
}

fn idea_entries(
    content: &ContentRoot,
    file: &ContentFile,
    group: &jomini::text::ObjectReader<'_, '_, Windows1252Encoding>,
) -> Result<Vec<NationalIdea>, Error> {
    let mut ideas = Vec::new();
    for (key, operator, value) in group.fields() {
        let name = key.read_str();
        if matches!(
            name.as_ref(),
            "start" | "bonus" | "trigger" | "ai_will_do" | "free" | "category"
        ) {
            continue;
        }
        script::equal(file, operator, &name)?;
        let Ok(idea) = value.read_object() else {
            continue;
        };
        ideas.push(NationalIdea {
            id: name.into_owned(),
            icon: icon_for(content, file, &idea)?,
            modifiers: idea_modifiers(file, &idea)?,
        });
    }
    Ok(ideas)
}

fn icon_for(
    content: &ContentRoot,
    file: &ContentFile,
    idea: &jomini::text::ObjectReader<'_, '_, Windows1252Encoding>,
) -> Result<String, Error> {
    for (key, operator, value) in idea.fields() {
        if value.read_object().is_ok() {
            continue;
        }
        let modifier = key.read_str();
        script::equal(file, operator, &modifier)?;
        let relative = format!("gfx/interface/ideas_EU4/{modifier}.dds");
        if content.path().join(Path::new(&relative)).is_file() {
            return Ok(relative);
        }
    }
    Ok("gfx/interface/ideas_EU4/unknown.dds".to_owned())
}

fn idea_modifiers(
    file: &ContentFile,
    idea: &jomini::text::ObjectReader<'_, '_, Windows1252Encoding>,
) -> Result<Vec<IdeaModifier>, Error> {
    let mut modifiers = Vec::new();
    for (key, operator, value) in idea.fields() {
        if value.read_object().is_ok() {
            continue;
        }
        let name = key.read_str().into_owned();
        script::equal(file, operator, &name)?;
        let scalar = value
            .read_scalar()
            .map_err(|error| Error::invalid(&file.path, error.to_string()))?;
        let amount = if let Ok(number) = scalar.to_f64() {
            ModifierAmount::Number(number)
        } else {
            let text = scalar.to_string();
            if text == "yes" {
                ModifierAmount::Yes
            } else {
                continue;
            }
        };
        modifiers.push(IdeaModifier { key: name, amount });
    }
    Ok(modifiers)
}

impl IdeaModifier {
    pub(super) fn value_text(&self) -> String {
        match self.amount {
            ModifierAmount::Yes => "Yes".to_owned(),
            ModifierAmount::Number(value) => {
                if is_percent(&self.key, value) {
                    format_percent(value)
                } else {
                    format_number(value)
                }
            }
        }
    }
}

fn is_percent(key: &str, value: f64) -> bool {
    // 这些小数是每年点数，不是百分比。其余小于 1 的修正按百分比显示。
    const FLAT: &[&str] = &[
        "army_tradition",
        "navy_tradition",
        "prestige",
        "legitimacy",
        "devotion",
        "republican_tradition",
        "horde_unity",
        "meritocracy",
        "inflation_reduction",
        "war_exhaustion",
        "global_autonomy",
        "papal_influence",
        "yearly_corruption",
        "monthly_piety",
        "monthly_fervor_increase",
        "diplomatic_reputation",
        "interest",
    ];
    if FLAT.contains(&key) {
        return false;
    }
    value.abs() < 1.0
        || key.ends_with("_modifier")
        || key.ends_with("_power")
        || key.ends_with("_cost")
        || key.ends_with("_efficiency")
        || key.ends_with("_speed")
        || key.ends_with("_ability")
}

fn format_percent(value: f64) -> String {
    let percent = value * 100.0;
    if (percent - percent.round()).abs() < 0.05 {
        format!("{:+.0}%", percent.round())
    } else {
        format!("{:+.1}%", percent)
    }
}

fn format_number(value: f64) -> String {
    if (value - value.round()).abs() < 0.001 {
        format!("{:+.0}", value.round())
    } else {
        let text = format!("{value:+.2}");
        text.trim_end_matches('0').trim_end_matches('.').to_owned()
    }
}

fn text(
    file: &ContentFile,
    value: &ValueReader<'_, '_, Windows1252Encoding>,
) -> Result<String, Error> {
    value
        .read_string()
        .map_err(|error| Error::invalid(&file.path, error.to_string()))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    use crate::content::ContentRoot;

    use super::load;

    #[test]
    fn tag_trigger_keeps_idea_order_and_skips_traditions() {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "augustus-ideas-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("common/ideas")).unwrap();
        fs::create_dir_all(root.join("gfx/interface/ideas_EU4")).unwrap();
        fs::write(root.join("gfx/interface/ideas_EU4/core_creation.dds"), []).unwrap();
        fs::write(
            root.join("gfx/interface/ideas_EU4/manpower_recovery_speed.dds"),
            [],
        )
        .unwrap();
        fs::write(
            root.join("common/ideas/00_country_ideas.txt"),
            r#"
                TUR_ideas = {
                    start = { discipline = 0.05 }
                    bonus = { land_forcelimit_modifier = 0.25 }
                    trigger = { tag = TUR }
                    free = yes
                    millets = { core_creation = -0.25 }
                    ghazi = { manpower_recovery_speed = 0.2 }
                }
                norse_ideas = {
                    trigger = { has_country_flag = accepted_norse_ideas_flag }
                    free = yes
                    raid = { discipline = 0.05 }
                }
            "#,
        )
        .unwrap();
        let ideas = load(&ContentRoot::new(&root)).unwrap();
        let _ = fs::remove_dir_all(&root);
        let group = ideas.get("TUR").unwrap();
        assert_eq!(group.id, "TUR_ideas");
        assert_eq!(
            group
                .ideas
                .iter()
                .map(|idea| idea.icon.as_str())
                .collect::<Vec<_>>(),
            vec![
                "gfx/interface/ideas_EU4/core_creation.dds",
                "gfx/interface/ideas_EU4/manpower_recovery_speed.dds",
            ]
        );
        assert_eq!(group.ideas[0].id, "millets");
        assert!(!ideas.contains_key("accepted_norse_ideas_flag"));
    }
}
