// SPDX-License-Identifier: GPL-3.0-only

//! 国家理念。带 `free = yes` 的组按开局日期的触发条件分配。修正只用于显示。

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use jomini::Windows1252Encoding;
use jomini::text::ValueReader;

use crate::{
    Error,
    content::{ContentFile, ContentRoot},
    script,
};
use augustus_core::Country;

use super::history::CountryHistory;

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

/// 开局时拿来对照理念触发条件的国家。旗帜只含历史在这一天之前立下的。
pub(super) struct IdeaSubject<'a> {
    pub tag: &'a str,
    pub culture: Option<&'a str>,
    pub religion: Option<&'a str>,
    pub government: Option<&'a str>,
    pub reforms: &'a [String],
    pub technology_group: Option<&'a str>,
    pub flags: &'a BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Cond {
    All(Vec<Cond>),
    Any(Vec<Cond>),
    Not(Box<Cond>),
    Tag(String),
    Flag(String),
    Culture(String),
    Religion(String),
    Government(String),
    TechGroup(String),
    Reform(String),
    /// 1444 年还没有的身份，按字面 yes/no 判断。
    StartFalse(bool),
    Unknown,
}

struct IdeaGroup {
    ideas: NationalIdeas,
    trigger: Cond,
    tags: BTreeSet<String>,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Rank {
    specific: bool,
    narrow: u32,
}

pub(super) fn load(
    content: &ContentRoot,
    countries: &BTreeMap<String, Country>,
    history: &CountryHistory,
) -> Result<BTreeMap<String, NationalIdeas>, Error> {
    let files = match content.text_files("common/ideas") {
        Ok(files) => files,
        Err(Error::Read { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
            return Ok(BTreeMap::new());
        }
        Err(error) => return Err(error),
    };
    let mut groups = Vec::new();
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
            let Some(trigger) = trigger_of(&file, &object)? else {
                continue;
            };
            let mut tags = BTreeSet::new();
            positive_tags(&trigger, false, &mut tags);
            groups.push(IdeaGroup {
                ideas: NationalIdeas {
                    id: id.clone(),
                    ideas: idea_entries(content, &file, &object)?,
                },
                trigger,
                tags,
            });
        }
    }
    let mut assigned = BTreeMap::new();
    let empty_flags = BTreeSet::new();
    for country in countries.values() {
        let subject = IdeaSubject {
            tag: country.tag(),
            culture: country.primary_culture.as_deref(),
            religion: country.religion.as_deref(),
            government: country.government.as_deref(),
            reforms: &country.government_reforms,
            technology_group: history
                .groups
                .get(country.tag())
                .and_then(|group| group.technology_group.as_deref()),
            flags: history.flags.get(country.tag()).unwrap_or(&empty_flags),
        };
        let mut best: Option<(Rank, usize)> = None;
        for (index, group) in groups.iter().enumerate() {
            if !matches_trigger(&group.trigger, &subject) {
                continue;
            }
            let rank = rank_for(group, subject.tag);
            let replace = best.as_ref().is_none_or(|(previous, _)| rank > *previous);
            if replace {
                best = Some((rank, index));
            }
        }
        if let Some((_, index)) = best {
            assigned.insert(country.tag().to_owned(), groups[index].ideas.clone());
        }
    }
    Ok(assigned)
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

fn trigger_of(
    file: &ContentFile,
    group: &jomini::text::ObjectReader<'_, '_, Windows1252Encoding>,
) -> Result<Option<Cond>, Error> {
    for (key, operator, value) in group.fields() {
        if key.read_str() != "trigger" {
            continue;
        }
        script::equal(file, operator, "trigger")?;
        let trigger = value
            .read_object()
            .map_err(|error| Error::invalid(&file.path, error.to_string()))?;
        return Ok(Some(parse_all(file, &trigger)?));
    }
    Ok(None)
}

fn parse_all(
    file: &ContentFile,
    object: &jomini::text::ObjectReader<'_, '_, Windows1252Encoding>,
) -> Result<Cond, Error> {
    let mut parts = Vec::new();
    for (key, operator, value) in object.fields() {
        let name = key.read_str();
        script::equal(file, operator, &name)?;
        parts.push(parse_one(file, &name, &value)?);
    }
    Ok(Cond::All(parts))
}

fn parse_one(
    file: &ContentFile,
    name: &str,
    value: &ValueReader<'_, '_, Windows1252Encoding>,
) -> Result<Cond, Error> {
    match name {
        "AND" => {
            let nested = value
                .read_object()
                .map_err(|error| Error::invalid(&file.path, error.to_string()))?;
            parse_all(file, &nested)
        }
        "OR" => {
            let nested = value
                .read_object()
                .map_err(|error| Error::invalid(&file.path, error.to_string()))?;
            let mut parts = Vec::new();
            for (key, operator, value) in nested.fields() {
                let name = key.read_str();
                script::equal(file, operator, &name)?;
                parts.push(parse_one(file, &name, &value)?);
            }
            Ok(Cond::Any(parts))
        }
        "NOT" => {
            let nested = value
                .read_object()
                .map_err(|error| Error::invalid(&file.path, error.to_string()))?;
            Ok(Cond::Not(Box::new(parse_all(file, &nested)?)))
        }
        "tag" => Ok(Cond::Tag(text(file, value)?)),
        "has_country_flag" => Ok(Cond::Flag(text(file, value)?)),
        "primary_culture" => Ok(Cond::Culture(text(file, value)?)),
        "religion" => Ok(Cond::Religion(text(file, value)?)),
        "government" => Ok(Cond::Government(text(file, value)?)),
        "technology_group" => Ok(Cond::TechGroup(text(file, value)?)),
        "has_reform" => Ok(Cond::Reform(text(file, value)?)),
        "is_revolutionary" | "is_revolution_target" | "is_colonial_nation" | "is_client_nation" => {
            match text(file, value)?.as_str() {
                "yes" => Ok(Cond::StartFalse(true)),
                "no" => Ok(Cond::StartFalse(false)),
                _ => Ok(Cond::Unknown),
            }
        }
        _ => Ok(Cond::Unknown),
    }
}

fn matches_trigger(cond: &Cond, country: &IdeaSubject<'_>) -> bool {
    match cond {
        Cond::All(parts) => parts.iter().all(|part| matches_trigger(part, country)),
        Cond::Any(parts) => parts.iter().any(|part| matches_trigger(part, country)),
        Cond::Not(inner) => !matches_trigger(inner, country),
        Cond::Tag(tag) => country.tag == tag,
        Cond::Flag(flag) => country.flags.contains(flag),
        Cond::Culture(culture) => country.culture == Some(culture.as_str()),
        Cond::Religion(religion) => country.religion == Some(religion.as_str()),
        Cond::Government(government) => country.government == Some(government.as_str()),
        Cond::TechGroup(group) => country.technology_group == Some(group.as_str()),
        Cond::Reform(reform) => country.reforms.iter().any(|owned| owned == reform),
        // 开局还没有革命、殖民或附庸国这类身份。
        Cond::StartFalse(expect_yes) => !expect_yes,
        Cond::Unknown => false,
    }
}

fn positive_tags(cond: &Cond, negated: bool, tags: &mut BTreeSet<String>) {
    match cond {
        Cond::Tag(tag) if !negated => {
            tags.insert(tag.clone());
        }
        Cond::Not(inner) => positive_tags(inner, !negated, tags),
        Cond::All(parts) | Cond::Any(parts) => {
            for part in parts {
                positive_tags(part, negated, tags);
            }
        }
        _ => {}
    }
}

fn rank_for(group: &IdeaGroup, tag: &str) -> Rank {
    if group.tags.contains(tag) {
        Rank {
            specific: true,
            narrow: u32::MAX - u32::try_from(group.tags.len()).unwrap_or(u32::MAX),
        }
    } else {
        Rank {
            specific: false,
            narrow: 0,
        }
    }
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
    use std::collections::{BTreeMap, BTreeSet};
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    use augustus_core::Country;

    use crate::content::ContentRoot;
    use crate::world::history::CountryHistory;

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
                LIT_ideas = {
                    trigger = {
                        tag = LIT
                        NOT = { has_country_flag = lit_new_magdeburg_laws }
                    }
                    free = yes
                    warlord_dukes = { leader_land_shock = 1 }
                }
                LIT2_ideas = {
                    trigger = {
                        tag = LIT
                        has_country_flag = lit_new_magdeburg_laws
                    }
                    free = yes
                    lit_new_magdeburg_laws = { development_cost = -0.1 }
                }
                PLC_LIT_ideas = {
                    trigger = {
                        tag = PLC
                        has_country_flag = is_LPC_flag
                    }
                    free = yes
                    warlord_dukes = { leader_land_shock = 1 }
                }
                FRA_ideas = {
                    trigger = {
                        tag = FRA
                        is_revolutionary = no
                        is_revolution_target = no
                    }
                    free = yes
                    royal_army = { discipline = 0.05 }
                }
            "#,
        )
        .unwrap();
        let mut countries = BTreeMap::new();
        for tag in ["TUR", "LIT", "PLC", "FRA"] {
            countries.insert(tag.to_owned(), Country::new(tag));
        }
        let history = CountryHistory {
            conflicts: BTreeSet::new(),
            groups: BTreeMap::new(),
            flags: BTreeMap::new(),
        };
        let ideas = load(&ContentRoot::new(&root), &countries, &history).unwrap();
        let mut flags = BTreeMap::new();
        let mut lit_flag = BTreeSet::new();
        lit_flag.insert("lit_new_magdeburg_laws".to_owned());
        flags.insert("LIT".to_owned(), lit_flag);
        let flagged = CountryHistory {
            conflicts: BTreeSet::new(),
            groups: BTreeMap::new(),
            flags,
        };
        let with_flag = load(&ContentRoot::new(&root), &countries, &flagged).unwrap();
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
        assert!(ideas.get("LIT").is_some_and(|group| group.id == "LIT_ideas"));
        assert!(with_flag.get("LIT").is_some_and(|group| group.id == "LIT2_ideas"));
        assert!(!ideas.contains_key("PLC"));
        assert!(ideas.get("FRA").is_some_and(|group| group.id == "FRA_ideas"));
        assert!(!ideas.values().any(|group| group.id == "norse_ideas"));
    }
}
