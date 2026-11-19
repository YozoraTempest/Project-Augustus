// SPDX-License-Identifier: GPL-3.0-only

//! 宗教名到 GFX_icon_religion 帧号。帧从 1 起，catholic 是第 1 帧。

use std::collections::BTreeMap;

use jomini::Windows1252Encoding;
use jomini::text::{ObjectReader, ValueReader};

use crate::{
    Error,
    content::{ContentFile, ContentRoot},
    script,
};

pub(super) fn load(content: &ContentRoot) -> Result<BTreeMap<String, u32>, Error> {
    let files = match content.text_files("common/religions") {
        Ok(files) => files,
        Err(Error::Read { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
            return Ok(BTreeMap::new());
        }
        Err(error) => return Err(error),
    };
    let mut icons = BTreeMap::new();
    for path in files {
        let file = content.read(path)?;
        let tape = script::parse(&file)?;
        for (key, operator, value) in tape.windows1252_reader().fields() {
            let name = key.read_str();
            script::equal(&file, operator, &name)?;
            let Ok(object) = value.read_object() else {
                continue;
            };
            read_religion(&file, &name, &object, &mut icons)?;
        }
    }
    Ok(icons)
}

fn read_religion(
    file: &ContentFile,
    name: &str,
    object: &ObjectReader<'_, '_, Windows1252Encoding>,
    icons: &mut BTreeMap<String, u32>,
) -> Result<(), Error> {
    // 组本身没有 icon。有 icon 的是宗教；组下面再扫一层，不把流派和修正当成宗教。
    if let Some(frame) = direct_icon(file, object)? {
        return insert(file, icons, name, frame);
    }
    for (key, operator, value) in object.fields() {
        let child = key.read_str();
        let Ok(body) = value.read_object() else {
            continue;
        };
        script::equal(file, operator, &child)?;
        if let Some(frame) = direct_icon(file, &body)? {
            insert(file, icons, &child, frame)?;
        }
    }
    Ok(())
}

fn direct_icon(
    file: &ContentFile,
    object: &ObjectReader<'_, '_, Windows1252Encoding>,
) -> Result<Option<u32>, Error> {
    let mut frame = None;
    for (key, operator, value) in object.fields() {
        if key.read_str() != "icon" {
            continue;
        }
        script::equal(file, operator, "icon")?;
        let parsed = icon_frame(file, &value)?;
        if frame.replace(parsed).is_some() {
            return Err(Error::invalid(&file.path, "religion icon is specified twice"));
        }
    }
    Ok(frame)
}

fn icon_frame(
    file: &ContentFile,
    value: &ValueReader<'_, '_, Windows1252Encoding>,
) -> Result<u32, Error> {
    let scalar = value
        .read_scalar()
        .map_err(|error| Error::invalid(&file.path, error.to_string()))?;
    let frame = scalar
        .to_u64()
        .ok()
        .and_then(|frame| u32::try_from(frame).ok())
        .filter(|frame| *frame >= 1)
        .ok_or_else(|| Error::invalid(&file.path, "religion icon must be a frame number from 1"))?;
    Ok(frame)
}

fn insert(
    file: &ContentFile,
    icons: &mut BTreeMap<String, u32>,
    name: &str,
    frame: u32,
) -> Result<(), Error> {
    if let Some(previous) = icons.insert(name.to_owned(), frame)
        && previous != frame
    {
        return Err(Error::invalid(
            &file.path,
            format!("{name}: conflicting religion icon"),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    use crate::content::ContentRoot;

    use super::load;

    #[test]
    fn icon_numbers_belong_to_religions_not_groups() {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "augustus-religions-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("common/religions")).unwrap();
        fs::write(
            root.join("common/religions/00_religion.txt"),
            r#"
                christian = {
                    defender_of_faith = yes
                    catholic = {
                        color = { 204 204 0 }
                        icon = 1
                        country = { tolerance_own = 1 }
                    }
                    protestant = { icon = 2 }
                }
                muslim = {
                    sunni = { icon = 5 }
                    religious_schools = {
                        hanafi_school = { potential = { religion = sunni } }
                    }
                }
            "#,
        )
        .unwrap();
        let icons = load(&ContentRoot::new(&root)).unwrap();
        let _ = fs::remove_dir_all(&root);
        assert_eq!(icons.get("catholic"), Some(&1));
        assert_eq!(icons.get("protestant"), Some(&2));
        assert_eq!(icons.get("sunni"), Some(&5));
        assert!(!icons.contains_key("christian"));
        assert!(!icons.contains_key("muslim"));
        assert!(!icons.contains_key("religious_schools"));
        assert!(!icons.contains_key("hanafi_school"));
    }

    #[test]
    fn vanilla_frames_when_the_game_directory_is_set() {
        let Ok(dir) = std::env::var("AUGUSTUS_EU4_DIR") else {
            return;
        };
        let icons = load(&ContentRoot::new(dir)).expect("religions");
        assert_eq!(icons.get("catholic"), Some(&1));
        assert_eq!(icons.get("sunni"), Some(&5));
        assert_eq!(icons.get("confucianism"), Some(&9));
        assert_eq!(icons.get("anglican"), Some(&28));
        assert_eq!(icons.get("dreamtime"), Some(&29));
        assert!(!icons.contains_key("christian"));
    }
}
