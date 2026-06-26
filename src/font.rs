use std::fs;

use anyhow::Result;
use fontdb::{Database, Family, Query, Source, Stretch, Style, Weight};
use fontdue::Font;

/// Loads the default monospace font from the system font database.
/// Loads the default monospace font and fallback fonts from the system font database.
pub(crate) fn load_system_fonts() -> Result<Vec<Font>> {
    let mut db = Database::new();
    db.load_system_fonts();

    let mut target_face_ids = Vec::new();

    // 1. Detect monospaced Nerd Font
    for face in db.faces() {
        let is_nerd = face.families.iter().any(|(family, _)| {
            let family_lower = family.to_lowercase();
            family_lower.contains("nerd font") || family_lower.contains(" nf")
        });
        if face.monospaced && is_nerd {
            target_face_ids.push(face.id);
            break;
        }
    }

    // 2. Query default Monospace font
    if let Some(id) = db.query(&Query {
        families: &[Family::Monospace],
        weight: Weight::NORMAL,
        stretch: Stretch::Normal,
        style: Style::Normal,
    }) {
        if !target_face_ids.contains(&id) {
            target_face_ids.push(id);
        }
    }

    // 3. Find system Emoji font
    for face in db.faces() {
        let is_emoji = face.families.iter().any(|(family, _)| {
            let family_lower = family.to_lowercase();
            family_lower.contains("emoji")
        });
        if is_emoji {
            if !target_face_ids.contains(&face.id) {
                target_face_ids.push(face.id);
            }
            break;
        }
    }

    if target_face_ids.is_empty() {
        anyhow::bail!("no system fonts found");
    }

    let mut loaded_fonts = Vec::new();
    for id in target_face_ids {
        if let Some(face) = db.face(id) {
            // fontdb stores only the font source; normalize it to bytes for fontdue.
            let bytes = match &face.source {
                Source::File(path) => {
                    match fs::read(path) {
                        Ok(b) => b,
                        Err(error) => {
                            tracing::warn!("failed to read font file {}: {error}", path.display());
                            continue;
                        }
                    }
                }
                Source::Binary(bytes) => bytes.as_ref().as_ref().to_vec(),
                Source::SharedFile(path, _) => {
                    match fs::read(path) {
                        Ok(b) => b,
                        Err(error) => {
                            tracing::warn!("failed to read font file {}: {error}", path.display());
                            continue;
                        }
                    }
                }
            };

            match Font::from_bytes(bytes, fontdue::FontSettings::default()) {
                Ok(font) => {
                    loaded_fonts.push(font);
                }
                Err(error) => {
                    tracing::warn!("parse system font '{}': {error}", face.post_script_name);
                }
            }
        }
    }

    if loaded_fonts.is_empty() {
        anyhow::bail!("failed to load any system fonts");
    }

    Ok(loaded_fonts)
}
